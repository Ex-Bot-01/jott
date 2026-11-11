//! One `np serve` child: exactly one writer to its stdin, one reader loop, responses
//! matched by id and everything else forwarded as it arrives.
//!
//! Two rules from np's own interface drive the shape: writing two requests into one line
//! corrupts the stream beyond recovery, so all writes go through one task; and a
//! notification can land between any two responses, so "the next line" is never assumed
//! to be the answer.

use super::frame::{encode_request, Frame};
use super::{NpError, NpFault, ProgressNote};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, Command};
use tokio::sync::{broadcast, mpsc, oneshot, Mutex};

type Pending = Mutex<HashMap<i64, oneshot::Sender<Result<Value, NpFault>>>>;

pub struct NpClient {
    /// Taken on shutdown, which ends the writer task and closes np's stdin.
    writer: Mutex<Option<mpsc::UnboundedSender<String>>>,
    pending: std::sync::Arc<Pending>,
    next_id: std::sync::atomic::AtomicI64,
    notes: broadcast::Sender<ProgressNote>,
    child: Mutex<Option<tokio::process::Child>>,
}

fn command_for(np: &Path) -> Command {
    // The test stand-in is a script; the real thing is a binary. Decided by name so the
    // tests never need an executable bit that Windows has no notion of.
    if np.extension().map(|e| e == "py").unwrap_or(false) {
        let mut c = Command::new("python3");
        c.arg(np);
        c
    } else {
        Command::new(np)
    }
}

impl NpClient {
    pub async fn spawn(np: &Path) -> Result<NpClient, NpError> {
        let mut cmd = command_for(np);
        let mut child = cmd
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| NpError::Transport(format!("could not start {}: {e}", np.display())))?;

        let stdin = child.stdin.take().expect("stdin piped");
        let stdout = child.stdout.take().expect("stdout piped");
        // Drain stderr and throw it away: np drops diagnostics rather than waiting on us,
        // but on Windows an undrained pipe really does slow the loop down.
        if let Some(mut stderr) = child.stderr.take() {
            tokio::spawn(async move {
                let mut buf = [0u8; 4096];
                use tokio::io::AsyncReadExt;
                loop {
                    match stderr.read(&mut buf).await {
                        Ok(0) | Err(_) => break,
                        Ok(_) => {}
                    }
                }
            });
        }

        let (writer_tx, mut writer_rx) = mpsc::unbounded_channel::<String>();
        tokio::spawn(async move {
            let mut stdin: ChildStdin = stdin;
            while let Some(line) = writer_rx.recv().await {
                if stdin.write_all(line.as_bytes()).await.is_err() || stdin.flush().await.is_err() {
                    break;
                }
            }
        });

        let pending: std::sync::Arc<Pending> =
            std::sync::Arc::new(Mutex::new(HashMap::new()));
        let (notes_tx, _) = broadcast::channel(256);
        let (reader_pending, reader_notes) = (pending.clone(), notes_tx.clone());
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                match Frame::decode(&line) {
                    Frame::Response { id, result } => {
                        if let Some(tx) = reader_pending.lock().await.remove(&id) {
                            let _ = tx.send(Ok(result));
                        }
                    }
                    Frame::Fault { id: Some(id), fault } => {
                        if let Some(tx) = reader_pending.lock().await.remove(&id) {
                            let _ = tx.send(Err(fault));
                        }
                    }
                    // A fault with no id answers nothing that was waiting.
                    Frame::Fault { .. } => {}
                    Frame::Note(note) => {
                        let _ = reader_notes.send(note);
                    }
                    Frame::Ignored => {}
                }
            }
        });

        Ok(NpClient {
            writer: Mutex::new(Some(writer_tx)),
            pending,
            next_id: std::sync::atomic::AtomicI64::new(1),
            notes: notes_tx,
            child: Mutex::new(Some(child)),
        })
    }

    pub fn notifications(&self) -> broadcast::Receiver<ProgressNote> {
        self.notes.subscribe()
    }

    pub async fn request(&self, method: &str, params: Value) -> Result<Value, NpFault> {
        let id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);
        let writer = self.writer.lock().await;
        let Some(writer) = writer.as_ref() else {
            return Err(NpFault { code: -32603, message: "np's stdin is closed".into() });
        };
        writer
            .send(encode_request(id, method, Some(&params)))
            .map_err(|_| NpFault { code: -32603, message: "np's stdin is closed".into() })?;
        drop(writer);
        rx.await.map_err(|_| NpFault {
            code: -32603,
            message: format!("np ended while answering {method}"),
        })?
    }

    /// Ask np to end, then close stdin. Exit 0 at EOF is normal, not a crash.
    pub async fn shutdown(&self) -> Result<(), NpError> {
        let id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if let Some(writer) = self.writer.lock().await.take() {
            let _ = writer.send(encode_request(id, "shutdown", None));
            drop(writer); // the writer task ends, and np's stdin closes with it
        }
        let Some(mut child) = self.child.lock().await.take() else { return Ok(()) };
        match tokio::time::timeout(std::time::Duration::from_secs(5), child.wait()).await {
            Ok(Ok(status)) if status.success() => Ok(()),
            Ok(Ok(_)) => Err(NpError::Transport("np exited non-zero".into())),
            Ok(Err(e)) => Err(NpError::Transport(format!("could not wait for np: {e}"))),
            Err(_) => {
                let _ = child.kill().await;
                Err(NpError::Transport("np did not exit within 5 s of shutdown".into()))
            }
        }
    }
}
