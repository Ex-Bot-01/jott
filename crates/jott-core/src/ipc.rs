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

use crate::diagnostics::{Journal, RequestRecord};
use crate::ingest::queue::{IngestEvent, IngestQueue};
use crate::np::handshake::NpInfo;
use crate::np::pool::NpPool;
use crate::np::resolve::{install_hint, NpMissing, NpSource, Os};
use crate::vault::buffers::{self, MirroredBuffer};
use crate::vault::files::{self, FileError, Trashed};
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
    NpGone,
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

/// The session for one open vault: its np pool, its ingest queue, its external-change watcher.
pub struct VaultSession {
    vault: PathBuf,
    /// One line per np request, for the hand-off (§19). `void()` until the shell says where the log lives;
    /// a diagnostic never fails a command, so this is an attachment and not a dependency.
    journal: Arc<Mutex<Arc<Journal>>>,
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
            Err(_) => VaultSession::open_in(np, info, vault, consent, np_config, std::env::temp_dir().join("jott-unwritable")),
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
        let pool = Arc::new(NpPool::new(np, info));
        let stamps = Arc::new(Mutex::new(Stamps::with_root(data.clone(), vault)));
        let queue = Arc::new(
            IngestQueue::spawn(pool.clone(), stamps.clone(), vault.to_path_buf()).with_np_config(np_config),
        );
        // The registry's remembered answer, applied without touching np's config file (§12): opening a
        // vault is not the same event as deciding about a download.
        queue.seed_consent(consent);
        let session = Arc::new(VaultSession {
            vault: vault.to_path_buf(),
            journal: Arc::new(Mutex::new(Arc::new(Journal::void()))),
            data: data.clone(),
            pool,
            stamps,
            queue,
            // The same registry the write paths record into. A fresh one here would make every save by
            // Jott arrive back as `vault://changed`, and the window would reload the note under the
            // person typing in it.
            self_writes: crate::vault::files::default_self_writes().clone(),
            changed: tokio::sync::broadcast::channel(64).0,
            watch_task: Mutex::new(None),
            stop: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        });
        session.start_watching();
        session
    }

    /// Attach the hand-off log. Also reaches the ingest queue, so a scan or an embed is recorded where it
    /// happened rather than being reconstructed later from what the window was told.
    pub fn with_journal(self: &Arc<Self>, journal: Arc<Journal>) -> Arc<VaultSession> {
        *self.journal.lock().unwrap() = journal.clone();
        self.queue.set_journal(journal);
        Arc::clone(self)
    }

    /// Where the log goes, for whoever opens a second window on the same vault.
    pub fn journal(&self) -> Arc<Journal> {
        self.journal.lock().unwrap().clone()
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
    pub fn subscribe_events(&self) -> tokio::sync::broadcast::Receiver<IngestEvent> {
        self.queue.events()
    }

    fn start_watching(&self) {
        let Ok(watch) = VaultWatch::start(&self.vault, self.self_writes.clone()) else {
            // No watcher backend is a degraded session, not a failed one: the tree stays as walked and
            // the next save still ingests. The status line says nothing about it, because "no live watch"
            // is not something a user can act on (§4).
            return;
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
        *self.watch_task.lock().unwrap() = Some(handle);
    }

    /// The tree, from our own walk (Task 7) with the stamps that make §9's markers answerable.
    pub fn listing(&self) -> Result<VaultListing, IpcError> {
        // A read-only copy: the walk asks questions of the stamps and must not hold the shared store.
        let stamps = (*self.stamps.lock().unwrap()).clone();
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
        Ok(NoteContent { rel: rel.to_string(), text, hash: disk_hash.clone(), disk_hash, disk_mtime_ms: mtime_ms, mirrored })
    }

    /// Write the buffer, tell the queue its bytes changed, and drop the mirror that was standing in for it.
    pub fn save_note(&self, rel: &str, text: &str) -> Result<WriteResult, IpcError> {
        files::write_note(&self.vault, rel, text.as_bytes()).map_err(IpcError::from)?;
        let _ = buffers::clear_mirror_in(&self.data, &self.vault, rel);
        self.queue.note_saved(rel);
        self.write_result(rel)
    }

    /// An empty note, and the folders it needs. Refuses a name that is taken (§9's new-note dialog).
    pub fn create_note(&self, rel: &str) -> Result<WriteResult, IpcError> {
        files::create_note(&self.vault, rel).map_err(IpcError::from)?;
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
        buffers::clear_mirror_in(&self.data, &self.vault, rel).map_err(|e| IpcError::io(rel, e))
    }

    pub fn rename_entry(&self, from: &str, to: &str) -> Result<RenameOutcome, IpcError> {
        let report = files::rename(&self.vault, from, to).map_err(IpcError::from)?;
        // Both names matter to the tree: what moved, and whatever it left behind in the index.
        self.queue.note_saved(to);
        Ok(RenameOutcome { incoming_links: report.incoming_links.into_iter().map(LinkRefDto::from).collect() })
    }

    /// Move a file or folder without renaming it into a different name — §9's context menu offers both,
    /// and they are the same operation with different words in the dialog.
    pub fn move_entry(&self, from: &str, to: &str) -> Result<(), IpcError> {
        files::move_entry(&self.vault, from, to).map_err(IpcError::from)?;
        self.queue.note_saved(to);
        Ok(())
    }

    /// Out of the vault and into a trash somebody can open. The sentence names which one, because "I
    /// deleted it" and "Jott put it where you can get it back" are different promises.
    pub fn delete_entry(&self, rel: &str) -> Result<String, IpcError> {
        match files::delete_to_trash(&self.vault, rel).map_err(IpcError::from)? {
            Trashed::Os => Ok("moved to the system trash".into()),
            Trashed::AppDir(path) => Ok(format!("moved to Jott's own folder: {}", path.display())),
        }
    }

    /// Content search. Reader connection, so it answers while an embed is running — §18.1 measured
    /// 102.1 s for a search queued behind an embed on one connection, and a frozen overlay is the whole
    /// reason there are two connections.
    pub async fn search(&self, query: &str, limit: u8) -> Result<SearchResults, IpcError> {
        let started = std::time::Instant::now();
        let answer = self.pool.search(&self.vault, query, limit).await.map_err(|fault| {
            self.record_fault("search", started.elapsed(), fault.code, &fault.message);
            IpcError { kind: ErrorKind::NpGone, message: fault.message }
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
        })
    }

    /// The `[embed]` escape hatch and "ingest everything waiting" (§11): the same queue, run now.
    pub fn ingest_all(&self) {
        self.queue.flush_now();
    }

    /// The consent prompt's two buttons. Writing the decision into np's config is the queue's job (§11);
    /// remembering it across launches is the registry's, and the shell does that beside this call.
    pub fn set_consent(&self, granted: bool) {
        self.queue.set_consent(if granted { Consent::Granted } else { Consent::Declined });
    }

    /// Closing: finish what is in flight up to the queue's cap, then stop watching. Never waits for a
    /// model (§11).
    pub async fn close(&self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(task) = self.watch_task.lock().unwrap().take() {
            task.abort();
        }
        // The queue drains its own in-flight pass and closes the pool, at its cap (§11). Bounded here too,
        // because a window that cannot close is worse than one that closes while np finishes thinking.
        let queue = self.queue.clone();
        let _ = tokio::time::timeout(IngestQueue::DRAIN_CAP + Duration::from_millis(500), async move {
            queue.drain_now().await;
        })
        .await;
    }

    /// One line in the hand-off log. Same shape as the queue's: never questioned, never fatal.
    fn record(&self, method: &str, elapsed: std::time::Duration, outcome: &'static str) {
        let _ = self.journal.lock().unwrap().clone().record(&RequestRecord::new(method, &self.vault, elapsed, outcome));
    }

    fn record_fault(&self, method: &str, elapsed: std::time::Duration, code: i32, message: &str) {
        let _ = self
            .journal
            .lock()
            .unwrap()
            .clone()
            .record(&RequestRecord::fault(method, &self.vault, elapsed, code as i64, message));
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
        if let Some(task) = self.watch_task.lock().unwrap().take() {
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
    let client = crate::np::client::NpClient::spawn(source.path())
        .await
        .map_err(|e| NpGone::Fault(format!("could not start {}: {e}", source.path().display())))?;
    // No workspace is named: the version floor is a property of the binary, and every method that works
    // names its own vault (§18.6). Asking here with one would make the startup check depend on which
    // folder was open last time.
    let answer = client
        .request("initialize", serde_json::json!({ "notifications": false }))
        .await
        .map_err(|fault| NpGone::Fault(fault.message))?;
    let info = crate::np::handshake::parse_initialize(&answer).map_err(NpGone::Missing)?;
    // This child was only ever a telephone: the pool spawns its own when there is work.
    let _ = client.shutdown_within(Duration::from_secs(5)).await;
    Ok(info)
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
            // Nothing to invent here either: the fault is what it said, and the hint points at the one
            // thing that fixes most of them (a newer np).
            NpGone::Fault(message) => NpMissingPayload {
                message: message.clone(),
                install_hint: format!("Jott needs np 0.3.1 or newer. {}", install_hint(&NpMissing::TooOld { found: "unknown".into() }, os)),
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
pub fn np_missing_payload(missing: &NpMissing, os: Os) -> NpMissingPayload {
    NpMissingPayload { message: missing.to_string(), install_hint: install_hint(missing, os) }
}

fn to_millis(when: &SystemTime) -> u64 {
    when.duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}
