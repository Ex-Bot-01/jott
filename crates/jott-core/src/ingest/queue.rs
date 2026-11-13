//! The ingest queue (§11): a save changed bytes, so the vault gets indexed and then embedded, in that
//! order, once per batch rather than once per note — np's `scan` and `embed` take a workspace and
//! nothing else (§18.2), so batching is not an optimisation someone could skip; it is the only shape
//! there is.
//!
//! What this module owns:
//!
//! * **Batching.** ~1 s after typing stops, one pass runs. Ten saves in that window are one pass.
//! * **Honest stamps.** A file's stamp moves to *indexed* when a scan committed and to *embedded* only
//!   when an embed run finished with nothing failing. Nothing is marked ahead of the work: stamps come
//!   from reports, never from hopes.
//! * **States, not errors.** `locked` and `offline` requeue with backoff and say "index behind"; a
//!   refusal carries np's sentence unchanged (§11).
//! * **Consent before download.** The gate is decided from what np's config names and what `models.list`
//!   says is on disk — never by starting a download to find out (§11).
//! * **A close that does not wait for a model.** [`IngestQueue::drain`] finishes what it started, gives
//!   up after five seconds, and leaves the file waiting. The vault is files; nothing owed but time.

use crate::diagnostics::{Journal, RequestRecord};
use crate::hashes::sha256_file;
use crate::ingest::consent::{
    consent_file_is_unreadable, default_bytes_for, embedding_model_named, set_auto_fetch,
};
use crate::np::outcomes::{EmbedOutcome, EmbedReport, ScanOutcome, ScanReport};
use crate::np::pool::{Connection, NpPool, PoolNote};
use crate::np::ProgressNote;
use crate::vault::files::PathGuard;
use crate::vault::stamps::{StampState, Stamps};
use crate::vault::store::Consent;
use serde::Serialize;
use serde_json::Value;
use std::collections::{HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::{broadcast, mpsc, oneshot};

/// The plan's retry ladder, in milliseconds: 2 s, then 5 s, then 15 s, then never higher. A vault
/// locked by a terminal `np scan` (§18.9) should not be knocked on every two seconds for an hour, and a
/// state still there after fifteen is worth waiting a minute for.
pub fn backoff_ms(attempt: u32) -> u64 {
    match attempt {
        0 => 2_000,
        1 => 5_000,
        2 => 15_000,
        _ => 60_000,
    }
}

/// Why the index is behind — a state the status line shows, never an error dialog (§11).
///
/// `camelCase` on the way out: this enum exists to be read by the window, and a payload that arrives as
/// `waited_ms` in one place and `retryInMs` in another is how a status line gets a `undefined`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum StateReason {
    // The field is renamed by hand because serde's container-level `rename_all` on an enum covers the
    // *variant* names and leaves struct-variant fields alone. Getting that wrong is how `waited_ms`
    // arrives in one place and `retryInMs` in another — exactly what the note above warns about — so
    // `tests/ipc_surface.rs` pins every key the window reads.
    Locked { #[serde(rename = "waitedMs")] waited_ms: u64 },
    Offline,
}

/// What the queue tells the window. The shell emits these as `ingest://event` (Task 10).
///
/// Variant and field names are `camelCase` for the webview; the reports inside `Indexed` and `Embedded`
/// keep np's own field spellings on purpose — they are np's documents, and §18 says to show np's words
/// rather than a translation of them.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum IngestEvent {
    /// A line from np's own progress stream. Phases that carry no counters (`locking`, `indexing`, and
    /// any build with no progress at all, §18.12) arrive as `done: 0, total: None`, which the status
    /// line renders as its label alone — never as a bar sitting at zero.
    Working { phase: String, done: u64, total: Option<u64> },
    /// A scan committed. The report is np's, aggregates and all (§18.2).
    Indexed(ScanReport),
    /// An embed run finished. Its `errors` count travels with it, so the window can say "3 chunks
    /// failed" from np's number rather than from this module deciding what a failure means.
    Embedded(EmbedReport),
    Behind { reason: StateReason, #[serde(rename = "retryInMs")] retry_in_ms: u64 },
    /// The prompt (§8): the model np's config names and the folder its bytes would land in. `bytes` is
    /// `None` because np 0.3.1 sends no artifact size anywhere — `models.list` rows have none (measured
    /// against `main.rs`'s `model_json`) — and Jott does not invent a download size to make a dialog
    /// look complete (§4).
    NeedsConsent { #[serde(rename = "modelId")] model_id: Option<String>, bytes: Option<u64>, #[serde(rename = "dataDir")] data_dir: PathBuf },
    /// A refusal, in np's words where np sent words. When it sent only a state token
    /// (`artifact-absent`), that token is the message: np's word, not ours (§18.5).
    Refused { message: String },
    /// Embedding is off in np's configuration, so nothing was embedded and keyword search carries on
    /// (§11). `hint` is np's own suggested command, when it sent one.
    Disabled { key: Option<String>, #[serde(rename = "modelId")] model_id: Option<String>, hint: Option<String> },
}

#[derive(Debug)]
enum Msg {
    Saved(String),
    Flush,
    /// The user answered the consent prompt while work was held at the gate.
    ConsentChanged,
    Drain(oneshot::Sender<()>),
    /// "Is there still a download to talk about?" — asked by the window when it wants to offer embeddings
    /// again after an earlier no. Answers `None` when there is nothing to offer.
    Offer(oneshot::Sender<Option<ConsentOffer>>),
}

struct Options {
    debounce: Duration,
    drain_cap: Duration,
    /// Tests shrink the retry wait so a requeue is observable inside a second. Production leaves it
    /// `None` and waits the ladder's real time; the *event* always reports the ladder's number, because
    /// that is the number the status line quotes.
    backoff_floor: Option<Duration>,
    np_config: Option<PathBuf>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            debounce: IngestQueue::DEBOUNCE,
            drain_cap: IngestQueue::DRAIN_CAP,
            backoff_floor: None,
            np_config: None,
        }
    }
}

struct Shared {
    options: Mutex<Options>,
    consent: Mutex<Consent>,
    /// Where to write a line about each np request (Task 11). `None` until the shell says where, and a
    /// journal that has given up writing is treated the same way as `None`.
    journal: Mutex<Option<Arc<Journal>>>,
    /// Set when the app is closing: no new work starts, and no events go out — a child we killed at the
    /// drain cap would otherwise arrive as a refusal the user did not do anything to deserve.
    closing: AtomicBool,
}

impl Default for Shared {
    fn default() -> Self {
        // A vault nobody has asked about yet is undecided — the same answer the registry gives (§8).
        Self {
            options: Mutex::new(Options::default()),
            consent: Mutex::new(Consent::Undecided),
            journal: Mutex::new(None),
            closing: AtomicBool::new(false),
        }
    }
}

/// The queue. One per vault, and deliberately not `Clone`: two queues on one vault would race over the
/// same workspace lock for no benefit.
pub struct IngestQueue {
    tx: mpsc::UnboundedSender<Msg>,
    events: broadcast::Sender<IngestEvent>,
    pool: Arc<NpPool>,
    shared: Arc<Shared>,
}

impl IngestQueue {
    /// §11: debounce ~1 s after typing stops.
    pub const DEBOUNCE: Duration = Duration::from_secs(1);
    /// §11: closing flushes what it can and never blocks on embedding.
    pub const DRAIN_CAP: Duration = Duration::from_secs(5);

    /// Start the worker. Call it from inside the async runtime (Tauri's `async_runtime`, or a
    /// `#[tokio::test]`), because the worker is a task on whatever runtime is current.
    pub fn spawn(pool: Arc<NpPool>, stamps: Arc<Mutex<Stamps>>, vault: PathBuf) -> IngestQueue {
        let (tx, rx) = mpsc::unbounded_channel();
        let (events, _) = broadcast::channel(128);
        let shared = Arc::new(Shared::default());
        let worker = Worker {
            pool: pool.clone(),
            stamps,
            vault,
            events: events.clone(),
            shared: shared.clone(),
            state: State::default(),
        };
        tokio::spawn(worker.run(rx));
        IngestQueue { tx, events, pool, shared }
    }

    /// §11's number, shortened for tests that are not measuring it.
    pub fn with_debounce(self, debounce: Duration) -> Self {
        self.shared.options.lock().unwrap().debounce = debounce;
        self
    }

    pub fn with_drain_cap(self, cap: Duration) -> Self {
        self.shared.options.lock().unwrap().drain_cap = cap;
        self
    }

    pub fn with_backoff_floor(self, floor: Duration) -> Self {
        self.shared.options.lock().unwrap().backoff_floor = Some(floor);
        self
    }

    /// np's `config.toml` — the file §11 says consent is written to. Unset means a decision lives in
    /// memory only, and the gate cannot tell which model is configured, so it asks for nothing.
    pub fn with_np_config(self, path: Option<PathBuf>) -> Self {
        self.shared.options.lock().unwrap().np_config = path;
        self
    }

    /// Where the log goes. Takes `&self` where its siblings take `self`, because the shell attaches it to
    /// a queue it already holds behind an `Arc`.
    pub fn set_journal(&self, journal: Arc<Journal>) {
        *self.shared.journal.lock().unwrap() = Some(journal);
    }

    /// A live view of what the queue says. Broadcast, so a window can come and go; a receiver that
    /// falls behind gets the lag signal rather than stalling the worker.
    pub fn events(&self) -> broadcast::Receiver<IngestEvent> {
        self.events.subscribe()
    }

    /// A note was saved. Whether it is *owed* is decided when the batch runs, from its bytes and its
    /// stamp — so a Ctrl+S that changed nothing costs no round trip (§11).
    pub fn note_saved(&self, rel: &str) {
        let _ = self.tx.send(Msg::Saved(rel.to_string()));
    }

    /// Run the batch now: the save-on-close path, and "ingest everything waiting" (§11's manual action).
    pub fn flush_now(&self) {
        let _ = self.tx.send(Msg::Flush);
    }

    /// What the registry already remembers, applied at open. Memory only: re-opening a vault is not
    /// re-consenting, and writing np's file on every launch would overwrite an edit someone made in a text
    /// editor between sessions (§12 says show a diff before that; here there is no reason to write at all).
    pub fn seed_consent(&self, consent: Consent) {
        *self.shared.consent.lock().unwrap() = consent;
    }

    /// The user answered the consent prompt. Granted writes `auto_fetch = true` and runs what was held at
    /// the gate; declined writes `false` and keeps indexing, because keyword search must not stop working
    /// (§11).
    pub fn set_consent(&self, consent: Consent) {
        // Always writes, even when the memory already holds this answer: what is remembered from a earlier
        // session is not the same fact as somebody pressing a button now (§11), and the file is where np
        // reads it. Writing the same value twice leaves the file unchanged anyway — `toml_edit` keeps the
        // rest of it as it was.
        let config = self.shared.options.lock().unwrap().np_config.clone();
        // The file first, then the in-memory answer: the file is where np reads it, so if writing failed,
        // the answer has not been given anywhere that matters and saying so beats promising a download
        // the child will refuse.
        if let Some(path) = config {
            let allow = match consent {
                Consent::Granted => Some(true),
                Consent::Declined => Some(false),
                // "Not yet" changes nothing in someone's config file; only an answer does.
                Consent::Undecided => None,
            };
            if let Some(allow) = allow {
                if let Err(e) = set_auto_fetch(&path, allow) {
                    self.emit(IngestEvent::Refused { message: e.to_string() });
                }
            }
        }
        *self.shared.consent.lock().unwrap() = consent;
        let _ = self.tx.send(Msg::ConsentChanged);
    }

    pub fn consent(&self) -> Consent {
        *self.shared.consent.lock().unwrap()
    }

    /// Whether there is still a download to talk about, for a window that wants to ask again after an
    /// earlier no. `None` means nothing to offer: the decision is yes (np will fetch when it embeds), or
    /// the bytes are already on disk and embedding is merely owed work, not a question.
    ///
    /// This asks np — `models.list`, on the reader connection — because guessing from the config file alone
    /// would offer a download of an artifact that is already installed.
    pub async fn offer(&self) -> Option<ConsentOffer> {
        let (tx, rx) = oneshot::channel();
        if self.tx.send(Msg::Offer(tx)).is_err() {
            return None;
        }
        rx.await.ok().flatten()
    }

    /// Finish what is in flight, save the stamps, shut the children down — and give up once
    /// [`Self::DRAIN_CAP`] has passed, because closing must not wait for a model (§11). A note whose
    /// pass did not finish stays *waiting*, which is what "owed at next launch" means.
    pub async fn drain(self) {
        self.drain_now().await
    }

    /// The same close-out through a borrow, for an owner that holds the queue behind an `Arc` (the shell
    /// keeps one per open vault). Same cap, same forced kill, same nothing-left-behind.
    pub async fn drain_now(&self) {
        let (done, waited) = oneshot::channel::<()>();
        self.shared.closing.store(true, Ordering::SeqCst);
        let cap = self.shared.options.lock().unwrap().drain_cap;
        if self.tx.send(Msg::Drain(done)).is_ok() {
            if tokio::time::timeout(cap, waited).await.is_err() {
                // Still embedding at the cap. Shutting the pool from here is what makes the wait end —
                // with a short grace, because np serves one request at a time (§18.1) and cannot even
                // read the ask while it is mid-run: past it the child is killed. `closing` keeps the
                // resulting error from being reported as a refusal the user did nothing to deserve.
                let _ = self.pool.close_within(Duration::from_millis(250)).await;
            }
        } else {
            // The worker is already gone; the children still deserve the ask.
            let _ = self.pool.close().await;
        }
    }

    fn emit(&self, event: IngestEvent) {
        if !self.shared.closing.load(Ordering::SeqCst) {
            let _ = self.events.send(event);
        }
    }
}

/// What the worker carries between messages.
#[derive(Default)]
struct State {
    /// Rels waiting for their pass, in save order (§11: the queue drains in save order).
    pending: VecDeque<String>,
    /// So ten saves of one file are one entry rather than ten.
    queued: HashSet<String>,
    /// Scanned and held at the consent gate for their embed, kept as `(rel, hash)` so granting does not
    /// scan the vault a second time.
    held_for_consent: Vec<(String, String)>,
    attempt: u32,
    retry_at: Option<Instant>,
    last_save: Option<Instant>,
    flush: bool,
    consent_asked: bool,
}

struct Worker {
    pool: Arc<NpPool>,
    stamps: Arc<Mutex<Stamps>>,
    vault: PathBuf,
    events: broadcast::Sender<IngestEvent>,
    shared: Arc<Shared>,
    state: State,
}

impl Worker {
    /// One line in the hand-off log for one np request. Never questioned: a diagnostic that can make a
    /// save fail is worse than no diagnostic (`Journal` already stops trying once writing fails).
    fn record(&self, method: &str, elapsed: Duration, outcome: &'static str) {
        if let Some(journal) = self.shared.journal.lock().unwrap().clone() {
            let _ = journal.record(&RequestRecord::new(method, &self.vault, elapsed, outcome));
        }
    }

    fn record_fault(&self, method: &str, elapsed: Duration, code: i32, message: &str) {
        if let Some(journal) = self.shared.journal.lock().unwrap().clone() {
            let _ = journal.record(&RequestRecord::fault(method, &self.vault, elapsed, code as i64, message));
        }
    }

    async fn run(mut self, mut rx: mpsc::UnboundedReceiver<Msg>) {
        let mut progress = Some(self.pool.progress());
        let mut drain_reply: Option<oneshot::Sender<()>> = None;
        loop {
            let due = self.due();
            tokio::select! {
                biased;

                maybe = rx.recv() => match maybe {
                    Some(Msg::Saved(rel)) => self.saved(rel),
                    Some(Msg::Flush) => {
                        if !self.state.pending.is_empty() {
                            self.state.flush = true;
                        }
                    }
                    Some(Msg::Offer(tx)) => {
                        let np_config = self.shared.options.lock().unwrap().np_config.clone();
                        let offer = match self.consent() {
                            // Yes was answered: np fetches when it embeds, so there is nothing left to offer.
                            Consent::Granted => None,
                            Consent::Undecided | Consent::Declined => match self.gate(&np_config).await {
                                Gate::Proceed | Gate::NothingToAsk => None,
                                Gate::Blocked { model_id, bytes } => Some(ConsentOffer {
                                    model_id,
                                    bytes,
                                    data_dir: PathBuf::from(self.pool.info().data_dir.as_str()),
                                }),
                            },
                        };
                        let _ = tx.send(offer);
                    }
                    Some(Msg::ConsentChanged) => {
                        let consent = self.consent();
                        if consent == Consent::Granted && !self.state.held_for_consent.is_empty() {
                            self.embed_held().await;
                        } else if consent == Consent::Declined {
                            // Their notes stay indexed and unembedded: that is what "not now" means, and
                            // the stamps already say so.
                            self.state.held_for_consent.clear();
                        }
                    }
                    Some(Msg::Drain(done)) => {
                        if self.state.pending.is_empty() {
                            // Nothing that a pass could run: closing is stopping, not waiting. Held-for-
                            // consent work does not count as owed here — it is parked precisely because a
                            // human has not answered, its notes are already `Indexed`, and a drain cannot
                            // answer on their behalf. So quitting with the banner up quits promptly; §11's
                            // cap is for a pass in flight, which is what `pending` means.
                            let _ = done.send(());
                        } else {
                            drain_reply = Some(done);
                            self.state.flush = true;
                        }
                    }
                    // The queue went away with the app: start nothing new.
                    None => break,
                },

                note = recv_progress(&mut progress) => match note {
                    Ok(Some(note)) => {
                        // Only the writer's lines belong to ingest: a reader's progress is a search's
                        // (§10), and mixing them would put somebody else's counters in this status line.
                        if note.connection == Connection::Writer && !self.shared.closing.load(Ordering::SeqCst) {
                            let _ = self.events.send(working(&note.note));
                        }
                    }
                    // Lagged: the window was not reading; nothing this time.
                    Ok(None) => {},
                    Err(_) => progress = None,
                },

                _ = tokio::time::sleep_until(due.unwrap_or_else(|| Instant::now() + Duration::from_secs(3600)).into()), if due.is_some() => {
                    self.cycle().await;
                    if let Some(done) = drain_reply.take() {
                        let _ = done.send(());
                        break;
                    }
                }
            }
        }
        // Whatever the loop did, the stamps are what we last learned. Saving here is what stops closing
        // from losing an index pass that finished a moment ago.
        if let Err(e) = self.stamps.lock().unwrap().save() {
            self.emit(IngestEvent::Refused { message: e.to_string() });
        }
        let _ = self.pool.close().await;
    }

    /// When the next pass is due, or `None` when there is nothing to do.
    fn due(&self) -> Option<Instant> {
        if self.state.pending.is_empty() {
            return None;
        }
        if let Some(retry_at) = self.state.retry_at {
            return Some(retry_at);
        }
        let options = self.shared.options.lock().unwrap();
        if self.state.flush || options.debounce.is_zero() {
            return Some(Instant::now());
        }
        let debounce = options.debounce;
        drop(options);
        Some(self.state.last_save.unwrap_or_else(Instant::now) + debounce)
    }

    fn saved(&mut self, rel: String) {
        // Not gated on `closing`: §11 says the save that happens when the app closes takes the same
        // path as every other save, and `drain` is what runs it.
        if self.state.queued.insert(rel.clone()) {
            self.state.pending.push_back(rel);
        }
        // Every save restarts the clock: the debounce is "after typing stops" (§11), not "one second
        // after the first keystroke saved".
        self.state.last_save = Some(Instant::now());
    }

    fn consent(&self) -> Consent {
        *self.shared.consent.lock().unwrap()
    }

    fn emit(&self, event: IngestEvent) {
        if !self.shared.closing.load(Ordering::SeqCst) {
            let _ = self.events.send(event);
        }
    }

    /// One pass: index what is owed, then embed it. Every early return leaves the stamps saying what is
    /// true now rather than what was hoped for.
    async fn cycle(&mut self) {
        self.state.flush = false;
        self.state.retry_at = None;
        let np_config = self.shared.options.lock().unwrap().np_config.clone();

        let mut owed: Vec<(String, String)> = Vec::new();
        while let Some(rel) = self.state.pending.pop_front() {
            self.state.queued.remove(&rel);
            let Some(path) = self.path_of(&rel) else { continue };
            let Ok(hash) = sha256_file(&path) else {
                // Not on disk any more: the next walk will show it gone, and there is nothing here for
                // np to read. Marking a file that is not there as refused would be a claim about
                // something we cannot look at (§4).
                continue;
            };
            let already = {
                let stamps = self.stamps.lock().unwrap();
                // §11: a write that left the bytes exactly as np last embedded them triggers nothing.
                // Anything else — never seen, indexed but not embedded, refused before — is owed.
                stamps.state_of(&rel, &hash) == StampState::Embedded
            };
            if !already {
                owed.push((rel, hash));
            }
        }
        if owed.is_empty() {
            return;
        }

        // ---- index. Never gated: keyword search has to work whatever someone decided about models.
        let started = std::time::Instant::now();
        let scan_result = self.pool.scan(&self.vault).await;
        match &scan_result {
            Ok(outcome) => self.record("scan", started.elapsed(), outcome.label()),
            Err(fault) => self.record_fault("scan", started.elapsed(), fault.code, &fault.message),
        }
        let scanned = match scan_result {
            Ok(ScanOutcome::Locked { waited_ms }) => {
                self.requeue(&owed, StateReason::Locked { waited_ms });
                return;
            }
            Ok(ScanOutcome::Applied(report)) => report,
            Err(fault) => {
                // A fault is not a state. The work goes back and the note stays *waiting*; np's sentence
                // is shown as it arrived (§11). No timer retries it: a refusal that will answer the same
                // way tomorrow should not be asked a hundred times today, and the next save or launch
                // asks again honestly.
                self.put_back(&owed);
                self.emit(IngestEvent::Refused { message: fault.message });
                return;
            }
        };

        let skipped: HashSet<&str> = scanned.skipped.iter().map(String::as_str).collect();
        let unindexable: HashSet<&str> = scanned
            .unindexable
            .as_ref()
            .map(|loss| loss.names.iter().map(String::as_str).collect())
            .unwrap_or_default();
        {
            let mut stamps = self.stamps.lock().unwrap();
            for (rel, hash) in &owed {
                if skipped.contains(rel.as_str()) {
                    // np's own word for it. The reason lives in a diagnostic line with a rule token
                    // (§18.5), which this build does not read — so the marker says refused and stops.
                    stamps.mark_refused(rel, "skipped");
                } else if unindexable.contains(rel.as_str()) {
                    stamps.mark_refused(rel, "unindexable");
                } else {
                    stamps.mark_indexed(rel, hash);
                }
            }
        }
        // Saved here rather than only at the end of the loop: a pass that indexed and then stopped at
        // the consent gate should not lose its index stamp because someone closed the window.
        self.save_stamps();
        self.emit(IngestEvent::Indexed(scanned));

        // ---- embed, if it may.
        match self.consent() {
            Consent::Undecided => match self.gate(&np_config).await {
                Gate::Blocked { model_id, bytes } => {
                    // Added, not assigned: two saves that both stop at the gate are two batches of work,
                    // and replacing the first would leave those notes `Indexed` with nothing left to
                    // requeue them — the queue would consider them done (§11: owed work waits, it does not
                    // disappear).
                    for item in owed {
                        // The newest hash wins. Held twice and then granted means np embedded what is on
                        // disk *now*; stamping the older bytes would leave the note answering `Indexed`
                        // for its own current content, and the tree would say "waiting to embed" forever.
                        match self.state.held_for_consent.iter_mut().find(|held| held.0 == item.0) {
                            Some(held) => *held = item,
                            None => self.state.held_for_consent.push(item),
                        }
                    }
                    if !self.state.consent_asked {
                        // Once per vault per session (§11), not once per save.
                        self.state.consent_asked = true;
                        self.emit(IngestEvent::NeedsConsent {
                            model_id,
                            bytes,
                            data_dir: PathBuf::from(self.pool.info().data_dir.as_str()),
                        });
                    }
                    return;
                }
                Gate::Proceed | Gate::NothingToAsk => {}
            },
            // "Not now" was answered once and stands: index only, quietly (§11).
            Consent::Declined => return,
            Consent::Granted => {}
        }

        self.embed(&owed).await;
    }

    /// The embed half — shared by a normal pass and by an answer that arrived after a pass was held.
    async fn embed(&mut self, owed: &[(String, String)]) {
        let started = std::time::Instant::now();
        let embed_result = self.pool.embed(&self.vault).await;
        match &embed_result {
            Ok(outcome) => self.record("embed", started.elapsed(), outcome.label()),
            Err(fault) => self.record_fault("embed", started.elapsed(), fault.code, &fault.message),
        }
        match embed_result {
            Ok(EmbedOutcome::Applied(report)) => {
                if report.errors == 0 {
                    let mut stamps = self.stamps.lock().unwrap();
                    for (rel, hash) in owed {
                        stamps.mark_embedded(rel, hash);
                    }
                }
                // With `errors` above zero np said some chunks failed and did not say which (§18.2 says
                // its embed report is aggregates), so these notes are not promised vectors. The report
                // still travels: its own count is how the window says what happened.
                self.save_stamps();
                self.state.attempt = 0;
                self.emit(IngestEvent::Embedded(report));
            }
            Ok(EmbedOutcome::Locked { waited_ms }) => self.requeue(owed, StateReason::Locked { waited_ms }),
            // The vectors are still owed and the index is not behind: keyword search works meanwhile,
            // and the stamps say *indexed*, never *embedded*.
            Ok(EmbedOutcome::Offline { .. }) => self.requeue(owed, StateReason::Offline),
            Ok(EmbedOutcome::Disabled { key, model_id, hint }) => {
                self.emit(IngestEvent::Disabled { key, model_id, hint });
            }
            Ok(EmbedOutcome::Refused { state, detail, .. }) => {
                let message = detail.unwrap_or(state);
                self.emit(IngestEvent::Refused { message });
            }
            Err(fault) => {
                self.put_back(owed);
                self.emit(IngestEvent::Refused { message: fault.message });
            }
        }
    }

    async fn embed_held(&mut self) {
        let held = std::mem::take(&mut self.state.held_for_consent);
        self.embed(&held).await;
    }

    /// A state that stopped the pass requeues the same work and waits (§11). The event names the
    /// ladder's rung, so the status line can say "retrying in 15 s" from the number it was handed.
    fn requeue(&mut self, owed: &[(String, String)], reason: StateReason) {
        let attempt = self.state.attempt;
        let wait = backoff_ms(attempt);
        self.state.attempt = attempt.saturating_add(1);
        let floor = self.shared.options.lock().unwrap().backoff_floor;
        self.put_back(owed);
        self.state.retry_at = Some(Instant::now() + floor.unwrap_or_else(|| Duration::from_millis(wait)));
        self.emit(IngestEvent::Behind { reason, retry_in_ms: wait });
    }

    /// Put a batch back at the front of the queue, in save order, without losing anything that arrived
    /// while the pass was running.
    fn put_back(&mut self, owed: &[(String, String)]) {
        let mut restored: VecDeque<String> = owed.iter().map(|(rel, _)| rel.clone()).collect();
        restored.append(&mut self.state.pending);
        for rel in &restored {
            self.state.queued.insert(rel.clone());
        }
        self.state.pending = restored;
    }

    fn save_stamps(&self) {
        // A stamp file that cannot be written is worth saying out loud: the tree will look right until
        // the restart, and then it will not.
        if let Err(e) = self.stamps.lock().unwrap().save() {
            self.emit(IngestEvent::Refused { message: e.to_string() });
        }
    }

    fn path_of(&self, rel: &str) -> Option<PathBuf> {
        // The rel came from our own tree or watcher, but a queue is not the place to assume that: the
        // same containment rule as everywhere else.
        PathGuard::new(&self.vault).resolve(rel).ok()
    }

    /// Whether embedding would need bytes nobody has downloaded — decided from np's config and
    /// `models.list`, never by starting a fetch (§11).
    async fn gate(&mut self, np_config: &Option<PathBuf>) -> Gate {
        // No path at all means no file to write the answer into either, so there is nothing Jott could do
        // with a reply. Every real environment gives us one (np's own rule), which is why this arm is a
        // comment rather than a feature.
        let Some(path) = np_config.clone() else { return Gate::NothingToAsk };
        if consent_file_is_unreadable(&path) {
            // A file we cannot parse is not a file that names no model. np reads its own config and may
            // well fetch on its default; asking without knowing which model is at stake is the honest
            // answer, and embedding blind is how §4's surprise download happens.
            return Gate::Blocked { model_id: None, bytes: None };
        }
        // A fresh np names no model in its config and still embeds: 0.3.1 carries a built-in pin and will
        // fetch it — measured here, on the first *search*, 309,855,456 bytes, with nobody asked. So "nothing
        // named" is not "nothing to ask"; it is a question about np's own default, which Jott can name from
        // `builtin_default_model` until np reports one itself.
        let model_id = embedding_model_named(&path).unwrap_or_else(default_id);
        let started = std::time::Instant::now();
        let listing_result = self.pool.models_list(&self.vault).await;
        match &listing_result {
            Ok(_) => self.record("models.list", started.elapsed(), "applied"),
            Err(fault) => self.record_fault("models.list", started.elapsed(), fault.code, &fault.message),
        };
        match listing_result {
            Ok(listing) if listing.artifact_present(&model_id) => Gate::Proceed,
            // An unreadable models directory (`models_dir_error`) is not "no artifact", but it is "we
            // cannot promise a download we may already have" — so the gate holds and asks rather than
            // embedding blind (§18.6). A failed `models.list` holds for the same reason.
            Ok(_) => Gate::Blocked { bytes: default_bytes_for(&model_id), model_id: Some(model_id) },
            Err(_) => Gate::Blocked { bytes: default_bytes_for(&model_id), model_id: Some(model_id) },
        }
    }
}

/// The model np embeds with when its config names none.
fn default_id() -> String {
    crate::ingest::consent::builtin_default_model().id.to_string()
}

// No `listed_bytes` here, though it would be the nicer source for the size in a prompt: np 0.3.1's
// `models.list` rows carry no member sizes at all (measured — `"members": []` for an installed model), and
// the weight of a fetch only appears in a report from the fetch that already happened.

/// What the window shows when it offers to set embeddings up: np's model id when known, the size when
/// anybody named one, and the folder the bytes would land in. Built here rather than in the frontend so the
/// sentence a tester reads is built from measured facts (§4) and not from strings the webview invented.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsentOffer {
    #[serde(rename = "modelId")]
    pub model_id: Option<String>,
    pub bytes: Option<u64>,
    #[serde(rename = "dataDir")]
    pub data_dir: PathBuf,
}

enum Gate {
    Proceed,
    /// What to show in the prompt: np's model id when we know it, and the size when anybody — np or the pin —
    /// said one. Both are optional because a config that cannot be read names neither.
    Blocked { model_id: Option<String>, bytes: Option<u64> },
    NothingToAsk,
}

/// The progress stream, or a future that never wakes once it is closed — so a dead stream cannot spin
/// the loop.
async fn recv_progress(progress: &mut Option<broadcast::Receiver<PoolNote>>) -> Result<Option<PoolNote>, ()> {
    let Some(receiver) = progress.as_mut() else {
        std::future::pending::<()>().await;
        return Ok(None);
    };
    match receiver.recv().await {
        Ok(note) => Ok(Some(note)),
        // Lagged: the window was not reading, and there is another line coming. Not our business (§11's
        // status line shows the latest counters it got).
        Err(RecvError::Lagged(_)) => Ok(None),
        Err(RecvError::Closed) => Err(()),
    }
}

/// np's progress line, in the shape a status line can render. The counter keys are np's: `chunks_done`
/// and `chunks_total` while embedding or writing, `bytes_done`/`bytes_total` while fetching — and absent
/// means unknown, which is why `total` is an `Option` rather than a zero (§18.12).
fn working(note: &ProgressNote) -> IngestEvent {
    let (done_key, total_key): (&str, &str) = match note.phase.as_str() {
        "embedding" | "writing" => ("chunks_done", "chunks_total"),
        "fetching" => ("bytes_done", "bytes_total"),
        _ => ("", ""),
    };
    let done = if done_key.is_empty() {
        0
    } else {
        note.extra.get(done_key).and_then(Value::as_u64).unwrap_or(0)
    };
    let total = if total_key.is_empty() {
        None
    } else {
        note.extra.get(total_key).and_then(Value::as_u64)
    };
    IngestEvent::Working { phase: note.phase.clone(), done, total }
}
