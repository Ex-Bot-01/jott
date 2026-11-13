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
use std::time::Duration;
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
    /// Set once np's stdout ended: the child is gone, and every request still owed an answer
    /// never will get one. The pool reads this to decide when a connection needs replacing.
    closed: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

/// What we ask np to do: answer JSON-RPC 2.0 on stdin and stdout.
///
/// This argument is the whole difference between a working install and a window that says "np ended before
/// answering". Run with no subcommand, np prints its usage on stderr, exits 2, and never reads a byte — which
/// is what reached the first machine that ran the shipped installer. Nothing noticed for eleven tasks because
/// every test here talks to `tests/fake-np/np.py`, which speaks NDJSON with any arguments at all; the stand-in
/// now has an option to have real np's manners (`require_args`), and `tests/np_serve_args.rs` uses it.
const SERVE_ARGS: [&str; 1] = ["serve"];

fn command_for(np: &Path) -> Command {
    // The test stand-in is a script; the real thing is a binary. Decided by name so the
    // tests never need an executable bit that Windows has no notion of.
    let mut c = if np.extension().map(|e| e == "py").unwrap_or(false) {
        let mut c = Command::new("python3");
        c.arg(np);
        c
    } else {
        Command::new(np)
    };
    c.args(SERVE_ARGS);
    // np is a console program, and on Windows starting one from a GUI process gives it a console of its own:
    // a black window titled `...\np\np.exe`, one per connection, open for as long as Jott runs. Nothing about
    // that is an error — it is our background helper making the user wonder whether they just opened a terminal.
    // `CREATE_NO_WINDOW` keeps the pipes and drops the window.
    #[cfg(windows)]
    c.creation_flags(CREATE_NO_WINDOW);
    c
}

/// Win32 `CREATE_NO_WINDOW`: no console for the child, stdio still attached to whatever we handed it.
#[cfg(windows)]
pub(crate) const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// The flag is worth pinning: `CREATE_NEW_CONSOLE` (0x10) is one digit away and does the exact opposite — and
/// on Linux, where nothing can catch this test failing, the value is the only evidence there is.
#[test]
fn the_windows_flag_is_create_no_window() {
    // https://learn.microsoft.com/en-us/windows/win32/procthread/process-creation-flags
    assert_eq!(0x0800_0000u32, 1 << 27, "CREATE_NO_WINDOW is the 28th flag, not a neighbour of 0x10");
}

/// How long a call may take before Jott stops waiting on it.
///
/// The split is by what the call is for, not by how big the vault might be. `initialize`, `models.list` and
/// `status.get` answer from memory; §18.1 measured a healthy `search` in milliseconds, so a search still pending
/// after a minute is a wedged child rather than a slow one — and a spinning search overlay is what gets reported
/// as "the whole app froze".
///
/// `scan` and `embed` get **no** deadline on purpose. Indexing or embedding a large vault legitimately runs for
/// minutes, and killing it mid-flight would throw away real work to guard against a stall that is already bounded
/// where it matters: the drain cap at close (§11). A stalled long call stays visible instead — the diagnostics
/// drawer shows it as an unfinished row with its elapsed time.
pub fn deadline_for(method: &str) -> Option<Duration> {
    match method {
        "initialize" | "models.list" | "status.get" => Some(Duration::from_secs(30)),
        "search" => Some(Duration::from_secs(60)),
        _ => None,
    }
}

/// Keep the last thing np said, in one line, so it can travel inside a fault message. `String` is bytes, and
/// slicing at an arbitrary index can cut a character in half — hence chars.
fn last_words(raw: &str, budget: usize) -> String {
    let folded = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if folded.chars().count() <= budget {
        return folded
    }
    let kept: String = folded.chars().take(budget - 1).collect();
    format!("{kept}…")
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
        // Drain stderr — np drops diagnostics rather than waiting on us, and on Windows an undrained pipe
        // really does slow the loop down — and keep the tail of it. When a child dies without answering, its
        // own sentence is the most useful thing in the report, and discarding it is what made the first
        // Windows report take three days and a release to understand.
        let said: std::sync::Arc<std::sync::Mutex<String>> = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        if let Some(mut stderr) = child.stderr.take() {
            let said = std::sync::Arc::clone(&said);
            tokio::spawn(async move {
                let mut buf = [0u8; 4096];
                use tokio::io::AsyncReadExt;
                loop {
                    match stderr.read(&mut buf).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            let mut tail = said.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                            tail.push_str(&String::from_utf8_lossy(&buf[..n]));
                            // The last kilobyte is what it was saying when it went.
                            if tail.len() > 1024 {
                                let cut = tail.len() - 1024;
                                tail.drain(..cut);
                            }
                        }
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
        let closed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (reader_pending, reader_notes, reader_closed) =
            (pending.clone(), notes_tx.clone(), closed.clone());
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
            // EOF on stdout, or a read error: nothing more will be answered. Say so to everyone
            // still waiting *before* publishing that the connection is gone, so a request that
            // races this either gets drained here or sees the flag and faults at once. A caller
            // left holding an answer that will never come is a frozen window (spec §11).
            reader_closed.store(true, std::sync::atomic::Ordering::SeqCst);
            let said = last_words(&said.lock().unwrap_or_else(|poisoned| poisoned.into_inner()), 400);
            let message = if said.is_empty() {
                "np ended before answering".to_string()
            } else {
                format!("np ended before answering — np said: {said}")
            };
            let owed: Vec<(i64, oneshot::Sender<Result<Value, NpFault>>)> =
                reader_pending.lock().await.drain().collect();
            for (_, tx) in owed {
                let _ = tx.send(Err(NpFault { code: -32603, message: message.clone() }));
            }
        });

        Ok(NpClient {
            writer: Mutex::new(Some(writer_tx)),
            pending,
            next_id: std::sync::atomic::AtomicI64::new(1),
            notes: notes_tx,
            child: Mutex::new(Some(child)),
            closed,
        })
    }

    /// Whether this child's stdout has ended. A connection that reports true is a connection to
    /// something that is no longer there.
    pub fn is_closed(&self) -> bool {
        self.closed.load(std::sync::atomic::Ordering::SeqCst)
    }

    pub fn notifications(&self) -> broadcast::Receiver<ProgressNote> {
        self.notes.subscribe()
    }

    /// Ask np something and wait as long as this kind of call is allowed to take. See [`deadline_for`].
    pub async fn request(&self, method: &str, params: Value) -> Result<Value, NpFault> {
        self.request_within(method, params, deadline_for(method)).await
    }

    /// [`Self::request`] with the patience named. `None` waits for as long as the child takes, which is right for
    /// a first index of a big vault and wrong for a search box. Tests use the short end of it.
    pub async fn request_within(&self, method: &str, params: Value, limit: Option<Duration>) -> Result<Value, NpFault> {
        let id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        {
            // Registered under the same lock the reader loop drains under, and after checking
            // that np is still there: one of the two always covers a child that dies right now.
            let mut pending = self.pending.lock().await;
            if self.is_closed() {
                return Err(NpFault {
                    code: -32603,
                    message: format!("np is gone: nothing is left to answer {method}"),
                });
            }
            pending.insert(id, tx);
        }
        let writer = self.writer.lock().await;
        let Some(writer) = writer.as_ref() else {
            return Err(NpFault { code: -32603, message: "np's stdin is closed".into() });
        };
        writer
            .send(encode_request(id, method, Some(&params)))
            .map_err(|_| NpFault { code: -32603, message: "np's stdin is closed".into() })?;
        // The guard is held until the answer arrives, on purpose: it is what makes `shutdown`
        // wait for in-flight work rather than closing np's stdin mid-request (the queue's drain
        // step depends on that), and np answers one request per connection regardless.
        // Two layers of result here: whether the channel delivered anything at all, and what np said inside it.
        // The `?` below takes the first layer; the second is the caller's answer.
        let answer = match limit {
            None => rx.await.map_err(|_| NpFault {
                code: -32603,
                message: format!("np ended while answering {method}"),
            }),
            Some(wait) => match tokio::time::timeout(wait, rx).await {
                Ok(inside) => inside.map_err(|_| NpFault {
                    code: -32603,
                    message: format!("np ended while answering {method}"),
                }),
                Err(_elapsed) => {
                    // Nobody is owed this answer any more, so stop the reader from holding it (and the id map
                    // from holding a sender that will never be read). The child keeps its request — np may well
                    // still be working on it — and later calls go through on the same connection.
                    self.pending.lock().await.remove(&id);
                    Err(NpFault {
                        code: -32603,
                        message: format!(
                            "np did not answer {method} within {} s and Jott stopped waiting",
                            wait.as_secs().max(1)
                        ),
                    })
                }
            },
        }?;
        answer
    }

    /// Ask np to end, then close stdin. Exit 0 at EOF is normal, not a crash.
    pub async fn shutdown(&self) -> Result<(), NpError> {
        self.shutdown_within(std::time::Duration::from_secs(5)).await
    }

    /// As [`Self::shutdown`], with the grace period named. A child that is mid-request cannot even
    /// read the ask — np serves one request at a time (§18.1) — so past the grace it is killed, and a
    /// killed `np serve` loses nothing: the vault is files and its database commits per run (§11).
    pub async fn shutdown_within(&self, grace: Duration) -> Result<(), NpError> {
        let id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        // The stdin lock is held by an in-flight request until its answer comes back, so asking politely
        // has to be *attempted* within the grace rather than waited for: a child that is mid-request
        // could not read the ask anyway (np answers one request per connection, §18.1), and waiting for
        // the lock would make "shut down anyway" wait for the very work we gave up on.
        let asked = match tokio::time::timeout(grace, self.writer.lock()).await {
            Ok(mut guard) => {
                if let Some(writer) = guard.take() {
                    let _ = writer.send(encode_request(id, "shutdown", None));
                    drop(writer); // the writer task ends, and np's stdin closes with it
                    true
                } else {
                    false
                }
            }
            Err(_) => false,
        };
        let Some(mut child) = self.child.lock().await.take() else { return Ok(()) };
        match tokio::time::timeout(grace, child.wait()).await {
            Ok(Ok(status)) if status.success() => Ok(()),
            Ok(Ok(_)) => Err(NpError::Transport("np exited non-zero".into())),
            Ok(Err(e)) => Err(NpError::Transport(format!("could not wait for np: {e}"))),
            Err(_) => {
                let _ = child.kill().await;
                Err(NpError::Transport(format!(
                    "{}: np did not exit within {} ms and was killed",
                    if asked { "asked to shut down" } else { "still busy, could not be asked" },
                    grace.as_millis()
                )))
            }
        }
    }
}
