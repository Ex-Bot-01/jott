//! The log Jott writes for someone else to read (§19's hand-off, Task 11 Step 1): one JSON line per np
//! request, in a file whose path can be stated in one sentence.
//!
//! What it holds is the exchange, not the notes: method, workspace, how long np took, which outcome came
//! back, and — on a fault — np's own code and sentence, unchanged. Note text never enters it, which is why
//! `Outcome` is an enum of verdicts rather than a bag of serde values from np's answers.

use serde::Serialize;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::sync::guarded;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Rotate once the file passes this, and keep one older copy beside it. Two megabytes is roughly ten
/// thousand requests: enough for a day of ordinary work, small enough to attach to a message (§19).
pub const ROTATE_AT_BYTES: u64 = 2 * 1024 * 1024;

/// One np request, as the log records it. `workspace` is a path — the person reading needs to know *which*
/// vault was slow — but no field here can hold note contents.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RequestRecord {
    /// The JSON-RPC method np was asked for: `initialize`, `scan`, `embed`, `search`, `models.list`.
    pub method: String,
    /// Written with `/` separators on every platform, so a log read on Linux after a Windows run needs no
    /// translation and a path cannot be mistaken for a field boundary.
    pub workspace: String,
    pub elapsed_ms: u64,
    /// One of `applied`, `locked`, `offline`, `disabled`, `refused`, `fault` — Jott's own verdict names from
    /// Task 4's outcomes, so a log line and the status line never disagree (§11).
    pub outcome: &'static str,
    /// np's JSON-RPC code, on a fault only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<i64>,
    /// np's own sentence, byte for byte, on a fault only (§4: its words, not a paraphrase).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Milliseconds since the epoch, so two runs can be told apart.
    pub at_ms: u64,
}

impl RequestRecord {
    pub fn new(method: impl Into<String>, workspace: &Path, elapsed: Duration, outcome: &'static str) -> RequestRecord {
        RequestRecord {
            method: method.into(),
            workspace: workspace.display().to_string().replace('\\', "/"),
            elapsed_ms: elapsed.as_millis() as u64,
            outcome,
            code: None,
            message: None,
            at_ms: now_ms(),
        }
    }

    /// The same, with np's fault attached. Takes the sentence by value because it is np's and must not be
    /// edited on the way in.
    pub fn fault(method: impl Into<String>, workspace: &Path, elapsed: Duration, code: i64, message: impl Into<String>) -> RequestRecord {
        let mut record = RequestRecord::new(method, workspace, elapsed, "fault");
        record.code = Some(code);
        record.message = Some(message.into());
        record
    }

    /// The keys a line may have. Part of the log's contract: a future field holding "the note involved"
    /// would turn a diagnostic into a copy of somebody's writing, and this test is where that gets refused.
    pub const KEYS: [&'static str; 7] = ["method", "workspace", "elapsed_ms", "outcome", "code", "message", "at_ms"];
}

/// Where one run of Jott began, written once at startup.
///
/// Without it a log that spans an upgrade reads as one long run: the tester says "it worked yesterday" and there
/// is no line saying yesterday's build talked to a different `np.exe`, from a different place, with a different
/// state folder. The state folder is here for the same reason — [`crate::ipc`] falls back to a temp folder when
/// the app data dir is unwritable, and unsaved-buffer mirrors then live somewhere nobody told anyone about.
#[derive(Debug, Clone, Serialize)]
pub struct RunRecord {
    /// Jott's version, as the shell was built with it.
    pub jott: String,
    /// What np answered at the handshake, or `None` when nothing answered yet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub np: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol: Option<i64>,
    /// The binary this run talks to, with `/` separators like every path in this file. `None` when nothing was
    /// found — a run with no np still has a version and a state folder worth writing down.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub np_path: Option<String>,
    /// `bundled`, `$NP_PATH` or `$PATH` — how that binary was found (§14's precedence).
    pub np_source: String,
    /// `windows-x86_64`, `linux-x86_64`: the platform the bytes were built for.
    pub platform: String,
    /// Where Jott keeps its own files: stamps, mirrored buffers, this log.
    pub state_root: String,
    /// True when that is a temporary fallback rather than the real app data folder.
    pub state_root_is_fallback: bool,
    pub at_ms: u64,
}

impl RunRecord {
    /// Paths are written with `/` on every platform, as everywhere in this file.
    pub fn new(
        jott: impl Into<String>,
        np: Option<String>,
        protocol: Option<i64>,
        np_path: Option<&Path>,
        np_source: impl Into<String>,
        state_root: &Path,
        state_root_is_fallback: bool,
    ) -> RunRecord {
        RunRecord {
            jott: jott.into(),
            np,
            protocol,
            np_path: np_path.map(portable),
            np_source: np_source.into(),
            platform: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
            state_root: portable(state_root),
            state_root_is_fallback,
            at_ms: now_ms(),
        }
    }

    /// The keys a run line may have — see [`RequestRecord::KEYS`] for why that list is a contract.
    pub const KEYS: [&'static str; 9] = [
        "jott",
        "np",
        "protocol",
        "np_path",
        "np_source",
        "platform",
        "state_root",
        "state_root_is_fallback",
        "at_ms",
    ];

    #[doc(hidden)]
    pub fn for_test(jott: &str) -> RunRecord {
        RunRecord::new(jott, Some("0.3.1".into()), Some(1), Some(Path::new("/np/np")), "bundled", Path::new("/state"), false)
    }
}

/// One piece of file work around np: what Jott did to a note, or refused to do, and why.
///
/// `rel` is in this record on purpose — "my note vanished" is a question about *which file*, and no np request line
/// can answer it. Contents are out on the same rule as everywhere else: the byte count says whether a write was
/// empty without copying the note into a log someone will attach to a message.
#[derive(Debug, Clone, Serialize)]
pub struct LocalRecord {
    /// `save`, `write-failed`, `conflict`, `mirror-replayed`, `mirror-discarded`, `deleted`, `consent`,
    /// `index-now`, `watcher-unavailable`, `state-root-fallback`.
    pub event: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rel: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes: Option<u64>,
    /// A sentence in Jott's own words — for a deletion, where the note went and what the system trash said; for a
    /// failure, the OS's reason. Never np's answer to a search, and never note text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub at_ms: u64,
}

impl LocalRecord {
    pub fn new(event: &'static str) -> LocalRecord {
        LocalRecord { event, rel: None, bytes: None, detail: None, at_ms: now_ms() }
    }

    /// Vault-relative, with `/`, like every `rel` Jott passes anywhere.
    pub fn with_rel(mut self, rel: impl Into<String>) -> LocalRecord {
        self.rel = Some(rel.into());
        self
    }

    pub fn with_bytes(mut self, bytes: u64) -> LocalRecord {
        self.bytes = Some(bytes);
        self
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> LocalRecord {
        self.detail = Some(detail.into());
        self
    }

    /// The keys a local line may have. `rel` is deliberately the only path-shaped thing here and there is no
    /// field that could hold what the file says — this list is where that stays true.
    pub const KEYS: [&'static str; 5] = ["event", "rel", "bytes", "detail", "at_ms"];
}

/// The app data log. Writing never fails upward: a diagnostic that can stop a save is worse than no
/// diagnostic, so the errors come back for callers to ignore rather than to propagate.
#[derive(Debug)]
pub struct Journal {
    dir: PathBuf,
    /// `None` once writing has failed for good (a read-only disk), so one broken filesystem does not make
    /// every later request pay for another attempt.
    state: Mutex<Option<Writer>>,
}

#[derive(Debug)]
struct Writer {
    path: PathBuf,
    written: u64,
}

impl Journal {
    /// `<app_data>/logs/jott.jsonl`. Creating the folder is part of opening; if that fails the journal
    /// stays usable as a no-op and says so through [`Journal::broken`].
    pub fn open(app_data: &Path) -> Journal {
        let dir = app_data.join("logs");
        let path = dir.join("jott.jsonl");
        let written = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        // The file is created at open, so the path About and the drawer name exists the moment it is named.
        let state = match std::fs::create_dir_all(&dir)
            .and_then(|()| std::fs::OpenOptions::new().create(true).append(true).open(&path).map(drop))
        {
            Ok(()) => Some(Writer { path, written }),
            Err(_) => None,
        };
        Journal { dir, state: Mutex::new(state) }
    }

    pub fn path(&self) -> PathBuf {
        self.dir.join("jott.jsonl")
    }

    /// The file to name in a window, or `None` when writing gave up. A drawer that shows a path nobody opened
    /// sends someone looking for a file that will never appear (§4).
    pub fn file(&self) -> Option<PathBuf> {
        (!self.broken()).then(|| self.path())
    }

    /// Bytes in the log right now, read from the filesystem rather than a running counter: a rotated or partly
    /// copied file should still report its real size. `None` when there is no readable file.
    pub fn size(&self) -> Option<u64> {
        self.file().and_then(|p| std::fs::metadata(p).ok().map(|m| m.len()))
    }

    /// Whether writing has given up. The window does not show this (§4: nobody can act on "the log folder
    /// is unwritable"); it exists so a test and a future support screen can say it truthfully.
    pub fn broken(&self) -> bool {
        guarded(&self.state).is_none()
    }

    /// Append one np request. Rotates first if the file has passed [`ROTATE_AT_BYTES`].
    pub fn record(&self, record: &RequestRecord) -> std::io::Result<()> {
        self.append(record)
    }

    /// Append the line that says which run this is. See [`RunRecord`].
    pub fn record_run(&self, record: &RunRecord) -> std::io::Result<()> {
        self.append(record)
    }

    /// Append one piece of file work. See [`LocalRecord`].
    pub fn record_local(&self, record: &LocalRecord) -> std::io::Result<()> {
        self.append(record)
    }

    fn append<T: Serialize + ?Sized>(&self, record: &T) -> std::io::Result<()> {
        let line = serde_json::to_string(record).map_err(std::io::Error::other)? + "\n";
        let mut guard = guarded(&self.state);
        let Some(writer) = guard.as_mut() else {
            return Err(std::io::Error::new(std::io::ErrorKind::BrokenPipe, "the log is not writable"));
        };
        if writer.written >= ROTATE_AT_BYTES {
            Self::rotate(writer)?;
        }
        // Appended and flushed per line: the log's whole purpose is the run that did not end well, and a
        // buffer of records lost in a crash is a log with the interesting part missing.
        let mut file = std::fs::OpenOptions::new().create(true).append(true).open(&writer.path)?;
        file.write_all(line.as_bytes())?;
        file.flush()?;
        writer.written += line.len() as u64;
        Ok(())
    }

    /// `jott.jsonl` → `jott.jsonl.1`, the old `.1` → `.2`, anything older dropped: two files, and the newest
    /// one always starts empty rather than half-written.
    fn rotate(writer: &mut Writer) -> std::io::Result<()> {
        let second = format!("{}.2", writer.path.display());
        let first = format!("{}.1", writer.path.display());
        match std::fs::remove_file(&second) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        match std::fs::rename(&first, &second) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        match std::fs::rename(&writer.path, &first) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        writer.written = 0;
        Ok(())
    }

    /// Read the log back, newest file first. For tests and for whoever is asked to send this file.
    pub fn read_lines(&self) -> std::io::Result<Vec<serde_json::Value>> {
        let mut lines = Vec::new();
        for path in [format!("{}.1", self.path().display()), self.path().display().to_string()] {
            let Ok(file) = std::fs::File::open(&path) else { continue };
            let reader = BufReader::new(file);
            for line in reader.lines() {
                let line = line?;
                if line.trim().is_empty() {
                    continue;
                }
                lines.push(serde_json::from_str(&line).unwrap_or_else(|_| serde_json::json!({ "unparsed": line })));
            }
        }
        Ok(lines)
    }

    /// How many bytes the newest file holds — a file size, not a collection's length, which is why it is
    /// not called `len`. The rotation test watches this; nothing else depends on it.
    pub fn bytes_on_disk(&self) -> u64 {
        std::fs::metadata(self.path()).map(|m| m.len()).unwrap_or(0)
    }

    /// An empty journal that writes nowhere, for a session with no app data dir.
    pub fn void() -> Journal {
        Journal { dir: std::env::temp_dir(), state: Mutex::new(None) }
    }
}

/// A path as this file writes every path: `/` separators, so one log read after a Windows run needs no
/// translation and a path can never be mistaken for a field boundary.
fn portable(path: &Path) -> String {
    path.display().to_string().replace('\\', "/")
}

/// Milliseconds since the epoch, for anything that wants to say *when* without a clock of its own.
pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Read the last line of a file, for tests that want the newest record without pulling in a JSON-lines
/// dependency.
pub fn last_line(path: &Path) -> std::io::Result<Option<String>> {
    let mut file = std::fs::File::open(path)?;
    let size = file.metadata()?.len();
    if size == 0 {
        return Ok(None);
    }
    // Read the tail rather than the whole file: this runs against a log that is allowed to be 2 MiB.
    let from = size.saturating_sub(64 * 1024);
    file.seek(SeekFrom::Start(from))?;
    let mut tail = Vec::new();
    file.read_to_end(&mut tail)?;
    let text = String::from_utf8_lossy(&tail).into_owned();
    // From the back: the tail is already in hand, and this stops at the first non-empty line instead of
    // splitting all 64 KiB of it.
    Ok(text.lines().rev().find(|line| !line.trim().is_empty()).map(str::to_string))
}

// ---------------------------------------------------------------- the trace, for the window

/// One thing the backend did, as the diagnostics drawer shows it.
///
/// The same rule as [`RequestRecord`], for the same reason: this screen exists partly so a person can screenshot
/// it at whoever maintains Jott, and a field that could hold note text would be sent before anyone thought about
/// it. So the key set is closed and pinned by a test — `TraceEvent::KEYS`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceEvent {
    /// Milliseconds since the epoch, so two runs and two rows can be told apart.
    pub at_ms: u64,
    pub kind: TraceKind,
    /// The np method for a request or a progress line; nothing else has one.
    pub method: Option<String>,
    /// Jott's own state word — `applied`, `locked`, `offline`, `disabled`, `refused`, `fault` — the same one the
    /// status line and the journal use, so no two surfaces disagree about what happened (§11).
    pub outcome: Option<&'static str>,
    pub elapsed_ms: Option<u64>,
    /// np's sentence on a fault, byte for byte (§4); a counters summary on progress (`chunks 7/7`); a short fact
    /// on the rest. Never anything read out of a note.
    pub detail: Option<String>,
}

impl TraceEvent {
    /// A row about a child process: started, refused to start, or shut down. Factories exist because most rows
    /// are one of the five shapes, and building them by hand at every call site is how a field silently stops
    /// being filled in.
    pub fn connection(detail: impl Into<String>) -> TraceEvent {
        TraceEvent {
            at_ms: now_ms(),
            kind: TraceKind::Connection,
            method: None,
            outcome: None,
            elapsed_ms: None,
            detail: Some(detail.into()),
        }
    }

    /// The closed contract with `src/lib/diagnostics.ts`. A field added here without a decision added there is
    /// what this list is for: the test that compares them fails first.
    pub const KEYS: &'static [&'static str] = &["atMs", "kind", "method", "outcome", "elapsedMs", "detail"];
}

/// What kind of thing happened, in the words the window groups rows by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TraceKind {
    /// A JSON-RPC request and how it ended.
    Request,
    /// np's own progress, coalesced: a fetch sends one line per chunk of bytes and the drawer is not a console.
    Progress,
    /// A child was started, or stopped, or could not be started.
    Connection,
    /// Someone answered the download question.
    Consent,
    /// Work went back for another try, and why.
    Queue,
    /// File work around np: a note written, moved, trashed, a mirrored draft replayed or thrown away. The rows
    /// that answer "my note vanished", which no request line can.
    File,
}

/// What this run of Jott has done, kept in memory so the window can show it without reading a file.
///
/// Fixed size on purpose: the drawer is for *this* run, the journal is for the runs before it, and an unbounded
/// trace would turn a 300 MB fetch — one np progress line per chunk — into a memory leak in a debugging screen.
/// A subscriber that falls behind (drawer closed, webview hung) loses events and says nothing; the run must not
/// feel the window.
pub struct Trace {
    capacity: usize,
    events: std::sync::Mutex<std::collections::VecDeque<TraceEvent>>,
    tx: tokio::sync::broadcast::Sender<TraceEvent>,
}

impl Default for Trace {
    fn default() -> Self {
        Self::new(200)
    }
}

impl Trace {
    pub fn new(capacity: usize) -> Trace {
        let (tx, _) = tokio::sync::broadcast::channel(capacity.max(16) * 2);
        Trace { capacity, events: std::sync::Mutex::new(std::collections::VecDeque::new()), tx }
    }

    /// Record one event and tell whoever is watching. Losing a send because nobody is listening is the normal
    /// case, not an error worth returning.
    pub fn push(&self, event: TraceEvent) {
        // Never drop a row because somebody else's panic poisoned the ring: this is the screen a person looks at
        // precisely when something has gone wrong.
        let mut events = guarded(&self.events);
        events.push_back(event.clone());
        while events.len() > self.capacity {
            events.pop_front();
        }
        drop(events);
        let _ = self.tx.send(event);
    }

    /// Everything kept, oldest first — the window puts the newest at the top of its own list.
    pub fn recent(&self) -> Vec<TraceEvent> {
        guarded(&self.events).iter().cloned().collect()
    }

    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<TraceEvent> {
        self.tx.subscribe()
    }
}
