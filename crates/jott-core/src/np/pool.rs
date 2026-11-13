//! Two connections to np, because one is measurably wrong.
//!
//! §18.1: a search queued behind a full-vault embed on the same connection waited **102.1 s**,
//! while the same search from a second process answered in 0.00 s. So `scan` and `embed` go to
//! the writer, `search` goes to the reader, and the reader does not exist until a search asks
//! for it — an idle `np serve` is ~10 MB and the ~115 MB model map is paid only by whichever
//! connection actually loads vectors (spec §7).
//!
//! A connection whose child dies is replaced on next use. That is not a retry loop over a fault:
//! the request that was in flight when the child died still fails, and only then does the pool
//! open another. What it must never do is keep answering searches from a corpse.

use super::client::NpClient;
use super::handshake::NpInfo;
use super::methods;
use super::outcomes::{EmbedOutcome, Mode, ScanOutcome, SearchAnswer, ModelListing};
use super::resolve::NpSource;
use super::{NpError, NpFault, ProgressNote};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, Mutex};

/// Which child a progress line came from. §7's rule: an embed's `embedding 3/7` must never be
/// drawn against a search's counters, and one merged stream cannot tell them apart without this.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Connection {
    Writer,
    Reader,
}

#[derive(Debug, Clone)]
pub struct PoolNote {
    pub connection: Connection,
    pub note: ProgressNote,
}

/// One lazily opened child. Its own lock, so opening the reader never keeps a writer's caller
/// waiting at the door.
struct Slot(Mutex<Option<Arc<NpClient>>>);

/// Whether a slot holds a live child. Three answers, because "never needed one" and "had one, it is gone" are
/// different things for a window to say — the first is normal at startup, the second usually means np died.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotState {
    Unopened,
    Open,
    Gone,
}

/// What happened to one connection. Named events rather than a `Result`, because "shut down politely" and
/// "failed to start" are both `Ok`-shaped facts and the drawer has to tell them apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionEvent {
    Started,
    /// The child could not be spawned or never answered its handshake. Carries the sentence to show.
    StartFailed(String),
    Closed,
}

/// Told what happened to a connection. The session forwards these into the diagnostics drawer; the np module
/// itself has no opinion about what a window shows, which is why this is a callback and not an import.
pub type ConnectionObserver = Box<dyn Fn(Connection, ConnectionEvent) + Send + Sync>;

pub struct NpPool {
    np: PathBuf,
    info: NpInfo,
    writer: Slot,
    reader: Slot,
    notes: broadcast::Sender<PoolNote>,
    observer: std::sync::Mutex<Option<ConnectionObserver>>,
    /// Set by `close`. The pool is shared between tasks, so a save that lands after shutdown
    /// must be refused rather than spawning one last child the app is no longer waiting on.
    closed: std::sync::atomic::AtomicBool,
}

impl NpPool {
    /// Take the resolved np and the handshake the app already read. Nothing spawns here: a pool
    /// that opens children in its constructor would pay 10 MB (or 115) for a window that only
    /// wants to read a note.
    pub fn new(source: NpSource, info: NpInfo) -> NpPool {
        let (notes, _) = broadcast::channel(512);
        NpPool {
            np: source.path().to_path_buf(),
            info,
            writer: Slot(Mutex::new(None)),
            reader: Slot(Mutex::new(None)),
            notes,
            observer: std::sync::Mutex::new(None),
            closed: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// A pool whose writer is already running: the child the app started to read the handshake, handed over
    /// instead of shut down and started again two milliseconds later. The reader stays lazy either way — a
    /// second idle np for someone who never types in the search box is 8 MB they did not ask for.
    pub fn with_writer(source: NpSource, info: NpInfo, writer: Arc<NpClient>) -> NpPool {
        let mut pool = NpPool::new(source, info);
        pool.writer = Slot(Mutex::new(Some(writer)));
        pool
    }

    /// What the handshake said this np can do — feature detection lives with the caller that has
    /// a use for it (no `models.list`, no `progress`, no vision).
    /// Which np binary this pool would spawn. The drawer shows it because a machine with two installs makes the
    /// path the fastest sentence on screen.
    pub fn binary(&self) -> &Path {
        &self.np
    }

    pub fn info(&self) -> &NpInfo {
        &self.info
    }

    /// The connection `scan` and `embed` belong to.
    pub async fn writer(&self) -> Result<Arc<NpClient>, NpError> {
        self.connection(Connection::Writer).await
    }

    /// The connection `search` belongs to, spawned on first use.
    pub async fn reader(&self) -> Result<Arc<NpClient>, NpError> {
        self.connection(Connection::Reader).await
    }

    pub async fn scan(&self, workspace: &Path) -> Result<ScanOutcome, NpFault> {
        let writer = self.writer().await?;
        methods::scan(&writer, workspace).await
    }

    pub async fn embed(&self, workspace: &Path) -> Result<EmbedOutcome, NpFault> {
        let writer = self.writer().await?;
        methods::embed(&writer, workspace).await
    }

    /// Content search, on the reader, in hybrid mode — which is what the overlay asks for (§10):
    /// both halves answer if both are there, and the answer says which ones did.
    pub async fn search(&self, workspace: &Path, query: &str, limit: u8) -> Result<SearchAnswer, NpFault> {
        let reader = self.reader().await?;
        methods::search(&reader, workspace, query, limit, Mode::Hybrid).await
    }

    /// What models this np knows about and where their artifacts stand, on the reader: it answers in
    /// milliseconds, and the consent gate must be able to ask it while an embed is running (§11).
    pub async fn models_list(&self, workspace: &Path) -> Result<ModelListing, NpFault> {
        let reader = self.reader().await?;
        methods::models_list(&reader, workspace).await
    }

    /// Every progress line from every child this pool has opened, tagged by connection. A
    /// receiver that misses lines gets the lag signal rather than a stalled status line.
    pub fn progress(&self) -> broadcast::Receiver<PoolNote> {
        self.notes.subscribe()
    }

    /// Ask both children to end, and wait for them. A child that already died is not a failed
    /// shutdown — it has nothing left to close, and reporting it as a fault would make a crash
    /// recovery look like the app breaking on exit.
    pub async fn close(&self) -> Result<(), NpError> {
        self.close_within(Duration::from_secs(5)).await
    }

    /// The same ask with a named grace, for the path that has already decided it is leaving: closing
    /// the app must not wait for a model that is mid-download (§11), so a child still busy when the
    /// grace ends is killed rather than followed.
    pub async fn close_within(&self, grace: Duration) -> Result<(), NpError> {
        self.closed.store(true, std::sync::atomic::Ordering::SeqCst);
        let writer = self.writer.0.lock().await.take();
        let reader = self.reader.0.lock().await.take();
        // Said before the goodbye, so a drawer still open at quit shows the child going rather than waiting for
        // a row that arrives after nobody is listening.
        if writer.is_some() {
            self.note_connection(Connection::Writer, ConnectionEvent::Closed);
        }
        if reader.is_some() {
            self.note_connection(Connection::Reader, ConnectionEvent::Closed);
        }
        let mut first_error = None;
        for client in writer.into_iter().chain(reader) {
            if client.is_closed() {
                continue;
            }
            if let Err(e) = client.shutdown_within(grace).await {
                first_error.get_or_insert(Err(e));
            }
        }
        first_error.unwrap_or(Ok(()))
    }

    async fn connection(&self, which: Connection) -> Result<Arc<NpClient>, NpError> {
        if self.closed.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(NpError::Transport("this np pool is closed".into()));
        }
        let slot = match which {
            Connection::Writer => &self.writer,
            Connection::Reader => &self.reader,
        };
        let mut guard = slot.0.lock().await;
        if guard.as_ref().map(|c| c.is_closed()).unwrap_or(false) {
            // Drop it here so the replacement below is the only child this connection has.
            *guard = None;
        }
        if let Some(client) = guard.clone() {
            return Ok(client);
        }
        let client = self.open(which).await?;
        *guard = Some(client.clone());
        Ok(client)
    }

    /// Spawn one child and ask it to send progress. The version floor was checked once, by the
    /// handshake that produced [`NpInfo`](super::handshake::NpInfo), against this same binary;
    /// what a new connection needs for itself is the per-connection `notifications` flag (§5).
    /// No workspace is named: every method that works names its own (§18.6).
    async fn open(&self, which: Connection) -> Result<Arc<NpClient>, NpError> {
        let client = match NpClient::spawn(&self.np).await {
            Ok(client) => Arc::new(client),
            Err(fault) => {
                // Only "we could not even ask" is a connection event: an np that answers with a JSON-RPC fault
                // is an np that is running, and calling that a connection failure would be a lie in the drawer.
                if !matches!(fault, NpError::Fault(_)) {
                    self.note_connection(which, ConnectionEvent::StartFailed(fault.to_string()));
                }
                return Err(fault);
            }
        };
        self.note_connection(which, ConnectionEvent::Started);

        let mut notes = client.notifications();
        let sink = self.notes.clone();
        tokio::spawn(async move {
            loop {
                match notes.recv().await {
                    Ok(note) => {
                        let _ = sink.send(PoolNote { connection: which, note });
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });

        client
            .request("initialize", serde_json::json!({ "notifications": true }))
            .await?;
        Ok(client)
    }
}

impl NpPool {
    /// Say who to tell about connections. Set once, at session open; setting it twice is a bug in the caller and
    /// the second observer simply wins rather than panicking in a UI path.
    pub fn set_connection_observer(&self, observer: ConnectionObserver) {
        if let Ok(mut slot) = self.observer.lock() {
            *slot = Some(observer);
        }
    }

    /// The state of each slot right now. Never spawns anything: a drawer that asks a question must not be the
    /// thing that starts an 8 MB child.
    pub fn slot_states(&self) -> (SlotState, SlotState) {
        let state = |slot: &Slot| match slot.0.try_lock().ok().and_then(|guard| guard.clone()) {
            None => SlotState::Unopened,
            Some(client) if client.is_closed() => SlotState::Gone,
            Some(_) => SlotState::Open,
        };
        (state(&self.writer), state(&self.reader))
    }

    fn note_connection(&self, which: Connection, event: ConnectionEvent) {
        if let Ok(guard) = self.observer.lock() {
            if let Some(observer) = guard.as_ref() {
                observer(which, event);
            }
        }
    }
}
