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
use super::outcomes::{EmbedOutcome, Mode, ScanOutcome, SearchAnswer};
use super::resolve::NpSource;
use super::{NpError, NpFault, ProgressNote};
use std::path::{Path, PathBuf};
use std::sync::Arc;
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

pub struct NpPool {
    np: PathBuf,
    info: NpInfo,
    writer: Slot,
    reader: Slot,
    notes: broadcast::Sender<PoolNote>,
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
            closed: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// What the handshake said this np can do — feature detection lives with the caller that has
    /// a use for it (no `models.list`, no `progress`, no vision).
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

    /// Every progress line from every child this pool has opened, tagged by connection. A
    /// receiver that misses lines gets the lag signal rather than a stalled status line.
    pub fn progress(&self) -> broadcast::Receiver<PoolNote> {
        self.notes.subscribe()
    }

    /// Ask both children to end, and wait for them. A child that already died is not a failed
    /// shutdown — it has nothing left to close, and reporting it as a fault would make a crash
    /// recovery look like the app breaking on exit.
    pub async fn close(&self) -> Result<(), NpError> {
        self.closed.store(true, std::sync::atomic::Ordering::SeqCst);
        let writer = self.writer.0.lock().await.take();
        let reader = self.reader.0.lock().await.take();
        let mut first_error = None;
        for client in writer.into_iter().chain(reader) {
            if client.is_closed() {
                continue;
            }
            if let Err(e) = client.shutdown().await {
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
        let client = Arc::new(NpClient::spawn(&self.np).await?);

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
