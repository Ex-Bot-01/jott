//! The command surface the webview talks to (§9, §12, §13). It lives in the core rather than in the
//! shell for one reason: everything here is testable headless, and a shell that compiles only on a
//! desktop cannot be the place where "does the webview get told about an escaping path?" is decided.
//! `apps/shell/src/ipc.rs` is a thin `#[tauri::command]` layer over these functions.
//!
//! Two standing rules:
//!
//! * **The webview is not a trusted caller.** Every `rel` goes through the same [`PathGuard`] as
//!   everywhere else, and its verdict reaches the front end as a `kind` it can branch on.
//! * **Answers carry facts, not adjectives.** A save returns the hash that landed; a delete says where
//!   the file went; a search says which halves answered. The window never has to guess what happened.

use crate::sync::guarded;
use crate::diagnostics::{now_ms, Journal, LocalRecord, RequestRecord, Trace, TraceEvent, TraceKind};
use crate::ingest::queue::{IngestEvent, IngestQueue};
use crate::np::handshake::NpInfo;
use crate::np::pool::NpPool;
use crate::np::resolve::{install_hint, NpMissing, NpSource, Os};
use crate::vault::buffers::{self, MirroredBuffer};
use crate::vault::files::{self, FileError};
use crate::vault::stamps::{StampState, Stamps};
use crate::vault::store::Consent;
use crate::vault::walk::{self, EntryKind, TreeEntry};
use crate::vault::watch::{SelfWrites, VaultWatch};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// What a command says when it refuses. `kind` is for the front end to branch on; `message` is np's or
/// the walk's own sentence, shown as it came (§4).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IpcError {
    pub kind: ErrorKind,
    pub message: String,
}

// `camelCase` for the webview's union in `src/lib/ipc.ts`; the contract test pins every spelling, because a
// kind that arrives as `EscapesRoot` while the front end matches on `escapesRoot` is a silent fall-through.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ErrorKind {
    EscapesRoot,
    NotFound,
    AlreadyExists,
    IntoItself,
    TooLarge,
    NotUtf8,
    NpGenerated,
    NoVault,
    /// np could not be reached, or never answered: not installed, not runnable, killed mid-request, or silent past
    /// its deadline. Nothing about the *content* of a request.
    NpGone,
    /// np is running and refused this request — `-32000` "workspace … is not a directory that exists", `-32602`
    /// bad params, anything else it chose to answer with. Its sentence is the message (§4).
    NpRefused,
    /// np answered `-32601`: this build does not implement what Jott asks for, which in practice means too old.
    NpTooOld,
    Io,
}

impl From<FileError> for IpcError {
    fn from(source: FileError) -> IpcError {
        let (kind, message) = match source {
            FileError::EscapesRoot { .. } => (ErrorKind::EscapesRoot, source.to_string()),
            FileError::NotFound { .. } => (ErrorKind::NotFound, source.to_string()),
            FileError::AlreadyExists { .. } => (ErrorKind::AlreadyExists, source.to_string()),
            FileError::IntoItself { .. } => (ErrorKind::IntoItself, source.to_string()),
            FileError::TooLarge { .. } => (ErrorKind::TooLarge, source.to_string()),
            FileError::NpGenerated { .. } => (ErrorKind::NpGenerated, source.to_string()),
            FileError::Io { .. } => (ErrorKind::Io, source.to_string()),
        };
        IpcError { kind, message }
    }
}

impl IpcError {
    /// For the shell's own commands when no vault is open (§9's empty state asks for a folder instead).
    pub fn no_vault() -> IpcError {
        IpcError { kind: ErrorKind::NoVault, message: "no vault is open".into() }
    }

    /// The one refusal with no more specific kind: the filesystem said no, in its own words.
    pub fn io(rel: &str, source: impl std::fmt::Display) -> IpcError {
        IpcError { kind: ErrorKind::Io, message: format!("could not use {rel}: {source}") }
    }

    /// A JSON-RPC fault, sorted into the kind that says what actually happened.
    ///
    /// The distinction is worth two new kinds because every fault used to arrive as `npGone`, which told a person
    /// to go install something that was running fine and had just refused them — the same class of mislabelling
    /// that cost the v0.0.6 release ("np ended before answering", which was Jott forgetting to say `serve`).
    ///
    /// `-32603` stays `npGone` on purpose: our own transport failures flatten to that code, so it means "no answer
    /// came", whether np's doing or ours.
    pub fn np_fault(fault: &crate::np::NpFault) -> IpcError {
        let message = fault.message.clone();
        let kind = match fault.code {
            -32601 => return IpcError {
                kind: ErrorKind::NpTooOld,
                message: format!("{message} — Jott needs np {} or newer", version_text()),
            },
            -32603 => ErrorKind::NpGone,
            _ => ErrorKind::NpRefused,
        };
        IpcError { kind, message }
    }
}

/// One row of the tree, as the window sees it. `mtimeMs` rather than a timestamp object because a
/// webview has one shape for time and it is not `{"secs":…,"nanos":…}`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NodeEntry {
    pub rel: String,
    pub kind: EntryKind,
    pub size: u64,
    pub mtime_ms: Option<u64>,
    pub state: Option<StampState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VaultListing {
    pub vault: String,
    pub nodes: Vec<NodeEntry>,
    /// Folders the walk could not read, named. A tree that silently omits a folder is a lie by
    /// omission (§4), so the window says this out loud.
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NoteContent {
    pub rel: String,
    pub text: String,
    /// Hash of these bytes — what a save has to produce for the buffer to count as saved (§11).
    pub hash: String,
    pub disk_hash: String,
    pub disk_mtime_ms: u64,
    /// §18.13: unsaved text mirrored from an earlier session, newer than the file. The window offers it;
    /// it does not paste it over the buffer silently.
    pub mirrored: Option<MirroredText>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MirroredText {
    pub text: String,
    pub mtime_ms: u64,
    /// The hash the buffer had when it was mirrored, so "same as what is on screen" is checkable.
    pub hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WriteResult {
    /// The hash of the bytes that landed — read back from the file, not computed from the request.
    pub hash: String,
    pub mtime_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RenameOutcome {
    /// Detection only in M1 (§13): the window warns before the rename or reports after it, and Plan 2
    /// rewrites them.
    pub incoming_links: Vec<LinkRefDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LinkRefDto {
    pub src: String,
    pub line: u32,
    pub target: String,
}

// No `Eq`: np's scores are floats, and pretending otherwise would mean deciding what `NaN` equals.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SearchResults {
    pub hits: Vec<HitDto>,
    /// np's own word for which half answered — `hybrid`, `keyword`, `vector` (§18.11).
    pub mode: String,
    pub vectors: bool,
    pub offline: bool,
    pub reason: Option<String>,
    /// 0.4.0+: has this folder ever been scanned? `None` from an older np, which cannot say — the window must not
    /// read that as "no", because it is the difference between two different sentences to a person (§4).
    pub indexed: Option<bool>,
    /// Did the keyword half run? …and the vector half. np's `mode` word said `hybrid` for keyword-only searches
    /// until 0.4.0 fixed it (#5), so these are what the window is allowed to claim about a result.
    pub keyword_used: Option<bool>,
    pub semantic_used: Option<bool>,
    /// np's own sentence about this answer ("never been scanned", "the model was fetched by this search"). Named
    /// `notice` rather than `note` because everywhere else in Jott a note is somebody's file.
    pub notice: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HitDto {
    pub path: String,
    /// np's snippet with its `[` `]` marks left in: they are not escaped, so the window renders this as
    /// text and highlights its own matches (§18.11).
    /// np's snippet with its `[` `]` marks left in — they are not escaped, so the window renders this
    /// as text and highlights its own matches (§18.11). Absent when np returned none.
    pub snippet: Option<String>,
    pub kind: String,
    pub score: f64,
    pub bm25: Option<f64>,
    pub cosine: Option<f64>,
}

/// What `np://missing` carries: the reason, and the instructions for *this* platform (§2).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NpMissingPayload {
    pub message: String,
    pub install_hint: String,
}

// ---- the diagnostics snapshot (specs/2026-11-13-diagnostics-drawer-design.md) ----
//
// Four small DTOs with closed key sets, because the drawer is TypeScript written against shapes Rust does not
// check. Same rule as `VaultListing`: the keys are the contract, and a test compares them to the TS types.

/// Whether a slot holds a live np child. Three answers, because "not needed yet" is normal at startup while
/// "was there, now gone" usually means np died — a drawer that blurs the two teaches people to ignore it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionState {
    Up,
    Idle,
    Down,
}

/// What this run is talking to, and where its own files are. `np_version` is what the handshake answered; in a
/// running app that is always at or above the floor, because the version check happened before this session
/// existed.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeInfo {
    pub np_version: Option<String>,
    /// Which np binary this run resolved. On a machine with two installs, this is the fastest sentence on screen.
    pub np_path: Option<String>,
    pub protocol: Option<u32>,
    pub writer: ConnectionState,
    pub reader: ConnectionState,
    /// np's own folder for models and index files.
    pub data_dir: String,
    /// Where *Jott* keeps its state — stamps, mirrored unsaved buffers, this log. Different from `dataDir`, and
    /// worth naming separately when the two are on different drives or one of them is a fallback.
    pub state_dir: String,
    /// True when that is a temporary folder rather than the real app data dir, which happens when the real one is
    /// not writable. Unsaved drafts then live somewhere nobody chose, so the window says so instead of letting
    /// someone discover it by losing a draft.
    pub state_fallback: bool,
    pub log_path: Option<String>,
    /// Bytes in that file, so "send the log" arrives with a sense of how much. `None` when there is no log —
    /// never zero standing in for unknown (§4).
    pub log_bytes: Option<u64>,
    pub consent: Consent,
    /// What np's config says right now about fetching models on its own. `None` when Jott does not know which
    /// config file it is reading.
    pub auto_fetch: Option<bool>,
}

impl RuntimeInfo {
    pub const KEYS: [&'static str; 12] = [
        "npVersion", "npPath", "protocol", "writer", "reader", "dataDir", "stateDir", "stateFallback", "logPath",
        "logBytes", "consent", "autoFetch",
    ];
}

/// What the ingest queue is holding, in words: `backoff` carries `locked` / `offline`, the same two the status
/// line uses. One vocabulary across three surfaces is the point of §11.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueState {
    pub waiting: usize,
    pub held_for_consent: usize,
    pub backoff: Option<String>,
    #[serde(rename = "retryInMs")]
    pub retry_in_ms: Option<u64>,
}

impl QueueState {
    pub const KEYS: [&'static str; 4] = ["waiting", "heldForConsent", "backoff", "retryInMs"];
}

/// Notes by state, walked from disk rather than remembered — a remembered count is exactly what nobody trusts.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultCounts {
    pub notes: usize,
    pub embedded: usize,
    pub indexed: usize,
    pub waiting: usize,
    pub refused: usize,
    /// Folders Jott could not read, counted apart from refused notes because the sentence differs: one is about a
    /// folder's permissions, the other about a note np would not take.
    pub warnings: usize,
}

impl VaultCounts {
    pub const KEYS: [&'static str; 6] = ["notes", "embedded", "indexed", "waiting", "refused", "warnings"];
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostics {
    pub runtime: RuntimeInfo,
    /// Oldest first, like the log file and every other record here; the window renders newest-first itself.
    pub recent: Vec<TraceEvent>,
    pub queue: QueueState,
    pub vault: VaultCounts,
}

impl Diagnostics {
    pub const KEYS: [&'static str; 4] = ["runtime", "recent", "queue", "vault"];
}

/// The session for one open vault: its np pool, its ingest queue, its external-change watcher.
pub struct VaultSession {
    vault: PathBuf,
    /// One line per np request, for the hand-off (§19). `void()` until the shell says where the log lives;
    /// a diagnostic never fails a command, so this is an attachment and not a dependency.
    journal: Arc<Mutex<Arc<Journal>>>,
    /// Which np config file governs fetching, so the drawer can report what it says. `None` when Jott could not
    /// work out where np keeps its config — reported as unknown rather than guessed (§4).
    np_config: Option<PathBuf>,
    /// This run's backend activity, in memory, for the diagnostics drawer. Owned by the session so a second
    /// vault opened in the same run gets its own honest history rather than a shared one it cannot interpret.
    trace: Arc<Trace>,
    /// Where this vault's state lives: stamps, and the mirrored unsaved buffers (§12, §18.13). Named at
    /// open rather than read from the environment, so a test — or a portable build with its data beside
    /// the vault — decides, and nothing in one process depends on another's `$XDG_DATA_HOME`.
    data: PathBuf,
    pool: Arc<NpPool>,
    stamps: Arc<Mutex<Stamps>>,
    queue: Arc<IngestQueue>,
    self_writes: Arc<SelfWrites>,
    changed: tokio::sync::broadcast::Sender<String>,
    watch_task: Mutex<Option<tokio::task::JoinHandle<()>>>,
    /// Whether `data` is the temp fallback rather than the real app data dir. Set by [`VaultSession::open`],
    /// which is the only place that decides to fall back; every other constructor passes a folder somebody named.
    state_fallback: std::sync::atomic::AtomicBool,
    /// The watcher pump is a blocking thread that outlives no one else, so it needs telling to stop.
    /// Nothing waits for it on purpose — but a runtime being dropped does wait for its threads, and a
    /// session nobody holds must not hold one open forever.
    stop: Arc<std::sync::atomic::AtomicBool>,
}

impl VaultSession {
    /// Open a vault. The pool is not spawned yet — np children appear when they are first asked for
    /// something (Task 5) — but the ingest queue and the watcher start now, because a save should be able
    /// to arrive at any moment and an outside edit should be noticed from the first second.
    pub fn open(np: NpSource, info: NpInfo, vault: &Path, consent: Consent, np_config: Option<PathBuf>) -> Arc<VaultSession> {
        match crate::app_paths::app_data_dir() {
            Ok(data) => VaultSession::open_in(np, info, vault, consent, np_config, data),
            // No writable app data dir means no stamps and no mirrors. The window still opens the folder:
            // reading and writing notes does not need them (§1), and the tree simply has nothing to mark.
            Err(_) => {
                let session =
                    VaultSession::open_in(np, info, vault, consent, np_config, std::env::temp_dir().join("jott-unwritable"));
                // Named in the drawer and in "copy details", because a draft saved into a temp folder that nobody
                // was told about is a lost draft with an extra step.
                session.state_fallback.store(true, std::sync::atomic::Ordering::Relaxed);
                session.local(LocalRecord::new("state-root-fallback").with_detail(std::env::temp_dir().join("jott-unwritable").display().to_string()));
                session
            }
        }
    }

    /// The same, with the state root named. See [`VaultSession::data`].
    pub fn open_in(
        np: NpSource,
        info: NpInfo,
        vault: &Path,
        consent: Consent,
        np_config: Option<PathBuf>,
        data: PathBuf,
    ) -> Arc<VaultSession> {
        VaultSession::open_in_with(np, info, None, vault, consent, np_config, data)
    }

    /// `open_in`, with the child the app already started at startup handed to the pool as its writer.
    /// Passing `None` is exactly `open_in`.
    pub fn open_in_with(
        np: NpSource,
        info: NpInfo,
        warm: Option<Arc<crate::np::client::NpClient>>,
        vault: &Path,
        consent: Consent,
        np_config: Option<PathBuf>,
        data: PathBuf,
    ) -> Arc<VaultSession> {
        let adopted = warm.is_some();
        let pool = Arc::new(match warm {
            Some(client) => NpPool::with_writer(np, info, client),
            None => NpPool::new(np, info),
        });
        let stamps = Arc::new(Mutex::new(Stamps::with_root(data.clone(), vault)));
        let trace = Arc::new(Trace::default());
        let queue = Arc::new(
            IngestQueue::spawn(pool.clone(), stamps.clone(), vault.to_path_buf()).with_np_config(np_config.clone()),
        );
        // The registry's remembered answer, applied without touching np's config file (§12): opening a
        // vault is not the same event as deciding about a download.
        queue.seed_consent(consent);
        queue.set_trace(trace.clone());
        hold_np_fetching_until_asked(consent, np_config.as_deref());
        // Connection facts belong to the pool, which knows nothing about windows; the session is where that
        // sentence gets translated into a row. A warm child counts as started — it did, at launch.
        let observer_trace = trace.clone();
        pool.set_connection_observer(Box::new(move |which, event| {
            use crate::np::pool::{Connection, ConnectionEvent};
            let name = match which {
                Connection::Writer => "writer",
                Connection::Reader => "reader",
            };
            let detail = match event {
                ConnectionEvent::Started => format!("{name} started"),
                ConnectionEvent::StartFailed(sentence) => format!("{name} could not start: {sentence}"),
                ConnectionEvent::Closed => format!("{name} closed"),
            };
            observer_trace.push(TraceEvent::connection(detail));
        }));
        if adopted {
            trace.push(TraceEvent::connection(
                "writer adopted from the child Jott started at launch".to_string(),
            ));
        }
        let session = Arc::new(VaultSession {
            // One folder, one spelling. The picker can hand back `D:\Notes\` where the registry holds
            // `D:\Notes`, and np keys a workspace by the exact string it is sent — measured, then filed as
            // note-processor#4: alternating spellings cost a full re-add of the vault once. Normalising here
            // covers every downstream user at once: what np is asked about, what the tree header shows, and the
            // keys for stamps and mirrored buffers (which already normalised, so they were disagreeing with np).
            // Lexical, like `to_abs` always is: a symlinked vault stays the folder somebody chose.
            vault: crate::vault::store::to_abs(vault),
            journal: Arc::new(Mutex::new(Arc::new(Journal::void()))),
            np_config: np_config.clone(),
            trace,
            data: data.clone(),
            pool,
            stamps,
            queue,
            // The same registry the write paths record into. A fresh one here would make every save by
            // Jott arrive back as `vault://changed`, and the window would reload the note under the
            // person typing in it.
            self_writes: crate::vault::files::default_self_writes().clone(),
            changed: tokio::sync::broadcast::channel(64).0,
            state_fallback: std::sync::atomic::AtomicBool::new(false),
            watch_task: Mutex::new(None),
            stop: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        });
        session.start_watching();
        session
    }

    /// Attach the hand-off log. Also reaches the ingest queue, so a scan or an embed is recorded where it
    /// happened rather than being reconstructed later from what the window was told.
    pub fn with_journal(self: &Arc<Self>, journal: Arc<Journal>) -> Arc<VaultSession> {
        *guarded(&self.journal) = journal.clone();
        self.queue.set_journal(journal);
        Arc::clone(self)
    }

    /// Where the log goes, for whoever opens a second window on the same vault.
    pub fn journal(&self) -> Arc<Journal> {
        guarded(&self.journal).clone()
    }

    pub fn vault(&self) -> &Path {
        &self.vault
    }

    pub fn pool(&self) -> &Arc<NpPool> {
        &self.pool
    }

    pub fn queue(&self) -> &Arc<IngestQueue> {
        &self.queue
    }

    /// `vault://changed` for the window: paths somebody outside Jott touched, our own saves excluded.
    pub fn subscribe_changes(&self) -> tokio::sync::broadcast::Receiver<String> {
        self.changed.subscribe()
    }

    /// `ingest://event`: everything the status line has to say (§11). The queue turns np's progress into
    /// these, so the webview never sees a raw np frame and never has to interpret one.
    /// Everything the diagnostics drawer shows, in one call. Read-only and cheap: it never spawns an np child,
    /// never re-hashes a note that has not moved, and answers whether np is well or not — which is precisely when
    /// the drawer gets opened (§4).
    pub async fn diagnostics(&self) -> Diagnostics {
        use crate::np::pool::SlotState;
        let (writer, reader) = self.pool.slot_states();
        let state = |slot| match slot {
            SlotState::Open => ConnectionState::Up,
            SlotState::Unopened => ConnectionState::Idle,
            SlotState::Gone => ConnectionState::Down,
        };

        let journal = guarded(&self.journal).clone();
        let report = self.queue.report().await;

        // Walked rather than remembered. The digest cache makes this cheap — a vault whose notes have not moved
        // costs a folder read and no hashing.
        let mut counts = VaultCounts::default();
        // A vault that cannot be walked at all reports zeros. That is what is true; inventing a number, or
        // failing the whole snapshot over it, would both be worse than an honest empty panel (§4).
        if let Ok(tree) = crate::vault::walk::walk_with(
            &self.vault,
            &guarded(&self.stamps),
            crate::vault::walk::default_hash_cache(),
        ) {
        for entry in &tree {
            match &entry.state {
                Some(StampState::Embedded) => {
                    counts.notes += 1;
                    counts.embedded += 1;
                }
                Some(StampState::Indexed) => {
                    counts.notes += 1;
                    counts.indexed += 1;
                }
                Some(StampState::Waiting) => {
                    counts.notes += 1;
                    counts.waiting += 1;
                }
                Some(StampState::Refused(reason)) => {
                    counts.notes += 1;
                    // A folder Jott could not read is a warning about a folder, not a note np refused.
                    if reason == "unreadable-dir" {
                        counts.warnings += 1;
                    } else {
                        counts.refused += 1;
                    }
                }
                None => {}
            }
        }
        }

        Diagnostics {
            runtime: RuntimeInfo {
                np_version: Some(self.pool.info().version.clone()),
                np_path: Some(self.pool.binary().to_string_lossy().into_owned()),
                protocol: Some(u32::from(self.pool.info().protocol)),
                writer: state(writer),
                reader: state(reader),
                data_dir: self.pool.info().data_dir.clone(),
                state_dir: self.data.to_string_lossy().into_owned(),
                state_fallback: self.state_fallback.load(std::sync::atomic::Ordering::Relaxed),
                log_path: journal.file().map(|p| p.to_string_lossy().into_owned()),
                log_bytes: journal.size(),
                consent: self.queue.consent(),
                auto_fetch: self.np_config.as_deref().map(crate::ingest::consent::auto_fetch_allowed),
            },
            recent: self.trace.recent(),
            queue: QueueState {
                waiting: report.waiting,
                held_for_consent: report.held_for_consent,
                backoff: report.backoff,
                retry_in_ms: report.retry_in_ms,
            },
            vault: counts,
        }
    }

    /// The drawer's history: this session's backend activity so far. Kept per session on purpose — two vaults
    /// open in one run have different np children and different reasons to be slow.
    pub fn trace(&self) -> Arc<Trace> {
        self.trace.clone()
    }

    /// ...and its live feed, so a drawer opened mid-run shows what already happened *and* keeps up.
    pub fn trace_events(&self) -> tokio::sync::broadcast::Receiver<TraceEvent> {
        self.trace.subscribe()
    }

    pub fn subscribe_events(&self) -> tokio::sync::broadcast::Receiver<IngestEvent> {
        self.queue.events()
    }

    fn start_watching(&self) {
        // No watcher backend is a degraded session, not a failed one: the tree stays as walked and the next save
        // still ingests. The status line says nothing about it, because "no live watch" is not something a user can
        // act on (§4) — but the log says it, because that is the difference between "Jott never noticed my edit"
        // and an unexplained silence.
        let watch = match VaultWatch::start(&self.vault, self.self_writes.clone()) {
            Ok(watch) => watch,
            Err(e) => {
                self.local(LocalRecord::new("watcher-unavailable").with_detail(e.to_string()));
                return;
            }
        };
        let sink = self.changed.clone();
        let stop = self.stop.clone();
        // `next_change` waits on an OS channel with a deadline, so it belongs on a blocking thread: an
        // async task that slept a second at a time would hold a worker hostage. One second is also the
        // longest the pump can be told to stop — a task aborted mid-wait loses that one event, which the
        // next walk reports anyway.
        let handle = tokio::task::spawn_blocking(move || {
            let mut watch = watch;
            loop {
                if stop.load(std::sync::atomic::Ordering::Relaxed) {
                    break;
                }
                // 100 ms is both the longest the pump can be told to stop and the worst delay before the
                // window hears about an outside edit. Task 8's watcher coalesces at a longer interval than
                // this anyway, so it never adds a visible lag of its own.
                if let Some(rel) = watch.next_change(Duration::from_millis(100)) {
                    let _ = sink.send(rel);
                }
            }
        });
        *guarded(&self.watch_task) = Some(handle);
    }

    /// The tree, from our own walk (Task 7) with the stamps that make §9's markers answerable.
    pub fn listing(&self) -> Result<VaultListing, IpcError> {
        // A read-only copy: the walk asks questions of the stamps and must not hold the shared store.
        let stamps = (*guarded(&self.stamps)).clone();
        let entries: Vec<TreeEntry> = walk::walk(&self.vault, &stamps).map_err(|e| IpcError { kind: ErrorKind::Io, message: e.to_string() })?;
        // Sentences, not codes: `warnings` holds more than walk trouble (the shell adds the nested-vault
        // notice from §8.2 here too), and a list of bare paths would force the window to guess which kind
        // of warning each one is.
        let warnings = entries
            .iter()
            .filter_map(|entry| match &entry.state {
                Some(StampState::Refused(reason)) if reason == "unreadable-dir" => {
                    Some(format!("{} could not be read", entry.rel))
                }
                _ => None,
            })
            .collect();
        Ok(VaultListing {
            vault: self.vault.display().to_string(),
            nodes: entries.into_iter().map(NodeEntry::from).collect(),
            warnings,
        })
    }

    /// A note's text, its hash, what the disk says now, and any mirrored unsaved buffer that is newer
    /// than the file (§18.13).
    pub fn read_note(&self, rel: &str) -> Result<NoteContent, IpcError> {
        let bytes = files::read_note(&self.vault, rel).map_err(IpcError::from)?;
        let path = self.vault.join(rel);
        let disk_hash = crate::hashes::sha256_hex(&bytes);
        let mtime_ms = path
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .map(|when| to_millis(&when))
            .unwrap_or(0);
        let text = String::from_utf8(bytes.clone()).map_err(|_| IpcError {
            kind: ErrorKind::NotUtf8,
            // Jott's limit, not np's verdict: np lossily decodes such bytes and indexes them (Task 7).
            message: format!("{rel} is not text Jott can edit"),
        })?;
        let mirrored = buffers::read_mirror_in(&self.data, &self.vault, rel)
            .filter(|mirror| {
                path.metadata()
                    .and_then(|m| m.modified())
                    .ok()
                    .is_some_and(|file_mtime| mirror.newer_than(file_mtime))
            })
            .map(MirroredText::from);
        if mirrored.is_some() {
            // "My note vanished" is usually this: a draft was replayed over what the file says. It has to be in
            // the log, because the person looking tomorrow sees only a file they did not write.
            self.local(LocalRecord::new("mirror-replayed").with_rel(rel));
        }
        Ok(NoteContent { rel: rel.to_string(), text, hash: disk_hash.clone(), disk_hash, disk_mtime_ms: mtime_ms, mirrored })
    }

    /// Write the buffer, tell the queue its bytes changed, and drop the mirror that was standing in for it.
    pub fn save_note(&self, rel: &str, text: &str) -> Result<WriteResult, IpcError> {
        if let Err(e) = files::write_note(&self.vault, rel, text.as_bytes()) {
            self.local(LocalRecord::new("write-failed").with_rel(rel).with_detail(e.to_string()));
            return Err(e.into());
        }
        // The byte count, never the bytes: "was anything written at all" is the question a log answers, and it
        // does not need somebody's sentence to answer it.
        self.local(LocalRecord::new("save").with_rel(rel).with_bytes(text.len() as u64));
        let _ = buffers::clear_mirror_in(&self.data, &self.vault, rel);
        self.queue.note_saved(rel);
        self.write_result(rel)
    }

    /// An empty note, and the folders it needs. Refuses a name that is taken (§9's new-note dialog).
    pub fn create_note(&self, rel: &str) -> Result<WriteResult, IpcError> {
        files::create_note(&self.vault, rel).map_err(IpcError::from)?;
        self.local(LocalRecord::new("created").with_rel(rel));
        self.queue.note_saved(rel);
        self.write_result(rel)
    }

    pub fn create_folder(&self, rel: &str) -> Result<(), IpcError> {
        files::create_folder(&self.vault, rel).map_err(IpcError::from)
    }

    /// Mirror text the editor holds but has not written (§18.13). Autosave calls this on its own tick,
    /// which is the difference between "we lose nothing typed" and a promise.
    pub fn mirror_buffer(&self, rel: &str, text: &str) -> Result<(), IpcError> {
        // Guarded like everything else: this one writes into the app data dir rather than the vault, but
        // an escaping `rel` would still let one window overwrite another file's mirror.
        files::PathGuard::new(&self.vault).resolve(rel).map_err(IpcError::from)?;
        // What the buffer was typed against: the file's own mtime right now. That is what makes §18.13's
        // "newer" decidable later, rather than a race between two writes in the same clock tick.
        let base = std::fs::metadata(self.vault.join(rel)).and_then(|m| m.modified()).ok();
        buffers::write_mirror_in(&self.data, &self.vault, rel, text.as_bytes(), base).map_err(|e| IpcError::io(rel, e))
    }

    /// Throw away a mirrored buffer the person chose not to use (§18.13's "Discard it"). Without this the
    /// draft would come back on the next open, which is the opposite of what was just agreed.
    pub fn discard_mirror(&self, rel: &str) -> Result<(), IpcError> {
        files::PathGuard::new(&self.vault).resolve(rel).map_err(IpcError::from)?;
        buffers::clear_mirror_in(&self.data, &self.vault, rel).map_err(|e| IpcError::io(rel, e))?;
        self.local(LocalRecord::new("mirror-discarded").with_rel(rel));
        Ok(())
    }

    pub fn rename_entry(&self, from: &str, to: &str) -> Result<RenameOutcome, IpcError> {
        let report = files::rename(&self.vault, from, to).map_err(IpcError::from)?;
        // Both names matter to the tree: what moved, and whatever it left behind in the index.
        self.local(LocalRecord::new("renamed").with_rel(to).with_detail(format!("from {from}")));
        self.queue.note_saved(to);
        Ok(RenameOutcome { incoming_links: report.incoming_links.into_iter().map(LinkRefDto::from).collect() })
    }

    /// Move a file or folder without renaming it into a different name — §9's context menu offers both,
    /// and they are the same operation with different words in the dialog.
    pub fn move_entry(&self, from: &str, to: &str) -> Result<(), IpcError> {
        files::move_entry(&self.vault, from, to).map_err(IpcError::from)?;
        self.local(LocalRecord::new("moved").with_rel(to).with_detail(format!("from {from}")));
        self.queue.note_saved(to);
        Ok(())
    }

    /// Out of the vault and into a trash somebody can open. The sentence names which one, because "I
    /// deleted it" and "Jott put it where you can get it back" are different promises.
    pub fn delete_entry(&self, rel: &str) -> Result<String, IpcError> {
        // The sentence names where the note went — and when the operating system refused to take it, why. It goes
        // in the log too: a deletion with no record of a deletion is the "my note vanished" report by definition.
        let where_it_went = files::delete_to_trash(&self.vault, rel).map_err(IpcError::from)?.sentence();
        self.local(LocalRecord::new("deleted").with_rel(rel).with_detail(&where_it_went));
        Ok(where_it_went)
    }

    /// Content search. Reader connection, so it answers while an embed is running — §18.1 measured
    /// 102.1 s for a search queued behind an embed on one connection, and a frozen overlay is the whole
    /// reason there are two connections.
    pub async fn search(&self, query: &str, limit: u8) -> Result<SearchResults, IpcError> {
        let started = std::time::Instant::now();
        let answer = self.pool.search(&self.vault, query, limit).await.map_err(|fault| {
            self.record_fault("search", started.elapsed(), fault.code, &fault.message);
            IpcError::np_fault(&fault)
        })?;
        // `offline` is its own outcome here: §4 wants a degraded answer visible, and a log that calls a
        // keyword-only search "applied" hides the one thing worth knowing about it.
        self.record("search", started.elapsed(), if answer.offline { "offline" } else { "applied" });
        Ok(SearchResults {
            hits: answer.hits.into_iter().map(HitDto::from).collect(),
            // np's own word for which half answered (§18.11), not a translation of it.
            mode: answer.mode.label().to_string(),
            vectors: answer.vectors,
            offline: answer.offline,
            reason: answer.reason,
            indexed: answer.indexed,
            keyword_used: answer.keyword_used,
            semantic_used: answer.semantic_used,
            notice: answer.note,
        })
    }

    /// The `[embed]` escape hatch and "ingest everything waiting" (§11): the same queue, run now.
    /// "Index now" and `[embed]`: every note in the vault, not only the ones this run of the window saved.
    ///
    /// It used to flush whatever was pending, which meant that on a folder nobody had typed into it did
    /// nothing at all — np scans the whole workspace whenever *any* note triggers it, but Jott's stamps are
    /// per note, so every other row went on reading "not indexed yet" under a status line saying the pass was
    /// done. A button with a label that promises the vault has to mean the vault.
    pub fn ingest_all(&self) {
        let mut queued = 0usize;
        if let Ok(notes) = walk::note_rels(&self.vault) {
            for rel in notes {
                self.queue.note_saved(&rel);
                queued += 1;
            }
        }
        self.local(LocalRecord::new("index-now").with_detail(format!("{queued} notes queued")));
        self.queue.flush_now();
    }

    /// Whether embeddings are still something to *offer* — see [`IngestQueue::offer`]. The window asks this
    /// when it wants to put a "Set up embeddings" control somewhere findable after an earlier no.
    pub async fn embeddings_offer(&self) -> Option<crate::ingest::queue::ConsentOffer> {
        self.queue.offer().await
    }

    /// The consent prompt's two buttons. Writing the decision into np's config is the queue's job (§11);
    /// remembering it across launches is the registry's, and the shell does that beside this call.
    pub fn set_consent(&self, granted: bool) {
        self.local(LocalRecord::new("consent").with_detail(if granted { "granted" } else { "declined" }));
        self.queue.set_consent(if granted { Consent::Granted } else { Consent::Declined });
    }

    /// Closing: finish what is in flight up to the queue's cap, then stop watching. Never waits for a
    /// model (§11).
    pub async fn close(&self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(task) = guarded(&self.watch_task).take() {
            task.abort();
        }
        // The queue drains its own in-flight pass and closes the pool, at its cap (§11). Bounded here too,
        // because a window that cannot close is worse than one that closes while np finishes thinking.
        let queue = self.queue.clone();
        let _ = tokio::time::timeout(IngestQueue::DRAIN_CAP + Duration::from_millis(500), async move {
            queue.drain_now().await;
        })
        .await;
        // The queue only shuts the pool when it had something to drain, so a vault that was opened, looked at,
        // and closed — with one search, say — used to leave its np children alive holding the index open. On
        // Windows that is a locked workspace for the next np, and switching folders did it again each time.
        // Idempotent: if the drain already closed it, this asks nothing of anything.
        let _ = self.pool.close_within(Duration::from_secs(2)).await;
    }

    /// One line in the hand-off log. Same shape as the queue's: never questioned, never fatal.
    fn record(&self, method: &str, elapsed: std::time::Duration, outcome: &'static str) {
        let _ = guarded(&self.journal).clone().record(&RequestRecord::new(method, &self.vault, elapsed, outcome));
        // The same facts, for the window that is open now rather than the file a person sends later. One place
        // fills both, so they cannot drift into disagreeing about the same request (§11).
        self.trace.push(TraceEvent {
            at_ms: crate::diagnostics::now_ms(),
            kind: TraceKind::Request,
            method: Some(method.to_string()),
            outcome: Some(outcome),
            elapsed_ms: Some(elapsed.as_millis() as u64),
            detail: None,
        });
    }

    /// One line about what Jott did to a file, in the log and in the drawer.
    ///
    /// Both from one call so the two can never disagree about whether a save happened, and neither can fail the
    /// operation it describes: the log's own rule is that a diagnostic must not stop a save (§19).
    fn local(&self, record: LocalRecord) {
        let _ = guarded(&self.journal).clone().record_local(&record);
        let mut detail = record.event.to_string();
        if let Some(rel) = &record.rel {
            detail.push(' ');
            detail.push_str(rel);
        }
        if let Some(more) = &record.detail {
            detail.push_str(" — ");
            detail.push_str(more);
        }
        self.trace.push(TraceEvent {
            at_ms: now_ms(),
            kind: TraceKind::File,
            method: None,
            outcome: None,
            elapsed_ms: None,
            detail: Some(detail),
        });
    }

    fn record_fault(&self, method: &str, elapsed: std::time::Duration, code: i32, message: &str) {
        let _ = guarded(&self.journal).clone().record(&RequestRecord::fault(
            method,
            &self.vault,
            elapsed,
            code as i64,
            message,
        ));
        self.trace.push(TraceEvent {
            at_ms: now_ms(),
            kind: TraceKind::Request,
            method: Some(method.to_string()),
            outcome: Some("fault"),
            elapsed_ms: Some(elapsed.as_millis() as u64),
            // np's sentence, unchanged. The row may be shortened for display; the value never is (§4).
            detail: Some(message.to_string()),
        });
    }

    fn write_result(&self, rel: &str) -> Result<WriteResult, IpcError> {
        let path = self.vault.join(rel);
        let bytes = std::fs::read(&path).map_err(|e| IpcError::io(rel, e))?;
        let mtime_ms = path.metadata().and_then(|m| m.modified()).ok().map(|w| to_millis(&w)).unwrap_or(0);
        Ok(WriteResult { hash: crate::hashes::sha256_hex(&bytes), mtime_ms })
    }
}

impl Drop for VaultSession {
    fn drop(&mut self) {
        // The last handle went, so the vault is closed whether or not `close` was called. A dropped
        // session that kept a thread alive would make every test — and every quit — hang in exactly the
        // way this one did the first time.
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(task) = guarded(&self.watch_task).take() {
            task.abort();
        }
    }
}

impl From<TreeEntry> for NodeEntry {
    fn from(entry: TreeEntry) -> NodeEntry {
        NodeEntry {
            rel: entry.rel,
            kind: entry.kind,
            size: entry.size,
            mtime_ms: (entry.mtime != SystemTime::UNIX_EPOCH).then(|| to_millis(&entry.mtime)),
            state: entry.state,
        }
    }
}

impl From<crate::vault::files::LinkRef> for LinkRefDto {
    fn from(link: crate::vault::files::LinkRef) -> LinkRefDto {
        LinkRefDto { src: link.src, line: link.line, target: link.target }
    }
}

impl From<crate::np::outcomes::Hit> for HitDto {
    fn from(hit: crate::np::outcomes::Hit) -> HitDto {
        HitDto { path: hit.path, snippet: hit.snippet, kind: hit.kind, score: hit.score, bm25: hit.bm25, cosine: hit.cosine }
    }
}

impl From<MirroredBuffer> for MirroredText {
    fn from(mirror: MirroredBuffer) -> MirroredText {
        MirroredText {
            text: String::from_utf8_lossy(&mirror.text).into_owned(),
            mtime_ms: mirror.mtime_ms(),
            hash: mirror.hash,
        }
    }
}

/// What the shell asks once at startup: is np there, and what does it say it can do.
///
/// The answer is a value, not a panic or a fatal error. A machine without np gets the same window —
/// tree, editor, save, everything except search and embedding (§1) — with one line saying what is
/// missing and how to install it (§2).
pub async fn read_np_info(source: &NpSource) -> Result<NpInfo, NpGone> {
    let (client, info) = read_np_info_warm(source).await?;
    // This child was only ever a telephone; nothing is waiting for it to stay.
    let _ = client.shutdown_within(Duration::from_secs(5)).await;
    Ok(info)
}

/// The same question, keeping the child: the startup handshake and the first session's writer are the same
/// conversation, and Jott used to have it twice — start np, ask `initialize`, kill it, then start it again when
/// a vault opened. Measured against the real binary, a spawn and handshake is ~2 ms and an idle child is ~8 MB,
/// so this is not a speed-up; it is one less process that exists for no reason, and — since Windows scans each
/// new executable on first run — one less thing for antivirus to notice.
///
/// Whoever takes the client owns shutting it down. An np left running at quit holds its index open on Windows,
/// which is precisely the lock friction §3 is about.
pub async fn read_np_info_warm(source: &NpSource) -> Result<(Arc<crate::np::client::NpClient>, NpInfo), NpGone> {
    let client = crate::np::client::NpClient::spawn(source.path())
        .await
        .map_err(|e| NpGone::Fault(format!("could not start {}: {e}", source.path().display())))?;
    // No workspace is named: the version floor is a property of the binary, and every method that works
    // names its own vault (§18.6). Asking here with one would make the startup check depend on which
    // folder was open last time.
    //
    // Progress is asked for because this child does not go away: it becomes the pool's writer, and np 0.4.0 only
    // streams `np/progress` to a connection whose `initialize` sent the flag. Asking with `false` here would leave
    // the one connection that indexes everything silent for the whole run.
    let answer = client
        .request("initialize", serde_json::json!({ "notifications": true }))
        .await
        .map_err(|fault| NpGone::Fault(fault.message))?;
    let info = crate::np::handshake::parse_initialize(&answer).map_err(NpGone::Missing)?;
    Ok((Arc::new(client), info))
}

/// Why the startup handshake failed, in a form the window can show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NpGone {
    /// np was not found, is not executable, or is too old — Task 2's own verdict.
    Missing(NpMissing),
    /// np started and answered with something unusable, or the pipe broke. Its sentence, kept.
    Fault(String),
}

impl NpGone {
    pub fn payload(&self, os: Os) -> NpMissingPayload {
        match self {
            NpGone::Missing(missing) => np_missing_payload(missing, os),
            // A fault from the handshake is not a version problem, and saying so cost a colleague an
            // afternoon: every failure to reach np used to end in "np is too old … replace np.exe", including
            // this one, which was Jott starting np without telling it to serve. So the hint says where to look
            // (the About panel names the exact file) and where to report it, and accuses nobody.
            NpGone::Fault(message) => NpMissingPayload {
                message: message.clone(),
                install_hint: format!(
                    "Jott could not get an answer from the np it runs; Jott needs np {} or newer. \
                     About (in the sidebar) names that file — if it is not the copy inside the installer, \
                     unset NP_PATH and restart Jott. If it is, please report it at {}/issues with a line \
                     from the log.",
                    version_text(),
                    crate::about::REPO_URL
                ),
            },
        }
    }
}

// (The `Path` variant is the honest one: this says "try to run `np`", and it will fail if there is none.)
/// The inputs for a session on a machine where np could not be asked anything.
///
/// A vault still opens: the tree, the editor, saves, renames and trash are all local work (§1). What this
/// stands for is "no usable np" — every method on the resulting pool answers *could not talk to np*, which
/// is the truth rather than a guess. The empty `methods` list and `progress: false` claim nothing about
/// any real build, and no consent prompt appears because asking np about models fails first.
pub fn np_absent() -> (NpSource, NpInfo) {
    let info = NpInfo {
        version: "0.0.0".into(),
        protocol: 1,
        methods: Vec::new(),
        features: crate::np::NpFeatures { local_model: false, fetch: false, vision: false },
        data_dir: String::new(),
        progress: false,
    };
    (NpSource::Env(std::ffi::OsString::from("np").into()), info)
}

/// What the registry remembers, beside the tree: the recents menu and the consent decision (§12).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RegistryInfo {
    pub recent: Vec<String>,
    pub consent: Consent,
}


/// The payload for `np://missing`: what is wrong, and how to fix it on this platform (§2).
/// The floor as a person reads it — the same function an install hint quotes, so a refusal and its fix cannot name
/// different versions.
fn version_text() -> String {
    crate::np::resolve::min_version_text()
}

pub fn np_missing_payload(missing: &NpMissing, os: Os) -> NpMissingPayload {
    NpMissingPayload { message: missing.to_string(), install_hint: install_hint(missing, os) }
}

fn to_millis(when: &SystemTime) -> u64 {
    when.duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Until this vault has an answer, np gets no free hand: `[models] auto_fetch` defaults to yes and a plain
/// `search` will fetch its built-in model — measured here at 309,855,456 bytes, on the first query, with
/// nobody asked. §12's promise is that the person hears about the download *first*, so an undecided vault
/// writes "no" and the prompt that follows offers one click to reverse it. A recorded answer is left exactly
/// as it is: reopening a vault is not a new question, and undoing someone's "yes" on relaunch would be worse
/// than the hole being closed.
///
/// Best effort in both directions. A config that cannot be written stays np's own problem to report — it is
/// not a reason to fail opening a folder — and a config that cannot be parsed is left alone rather than
/// rewritten over, since Jott did not write it.
fn hold_np_fetching_until_asked(consent: Consent, np_config: Option<&std::path::Path>) {
    use crate::ingest::consent::{auto_fetch_allowed, consent_file_is_unreadable, set_auto_fetch};

    if consent != Consent::Undecided {
        return;
    }
    let Some(path) = np_config else { return };
    if consent_file_is_unreadable(path) || !auto_fetch_allowed(path) {
        // Already "no", or a file whose meaning nobody can read: in neither case does Jott have a change to
        // make that is worth touching someone's configuration for.
        return;
    }
    let _ = set_auto_fetch(path, false);
}
