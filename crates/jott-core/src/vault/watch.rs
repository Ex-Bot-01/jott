//! Watching the vault for changes nobody made here (§14). The policy lives in
//! [`crate::vault::files::decide`]; this module's whole job is to turn OS events into `rel` paths
//! and to keep Jott from announcing its own saves back to the user.

use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::path::Path;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How long one of our writes counts as "we did that". The plan's number: 200 ms is long enough for
/// the OS to deliver the event and short enough that the next real edit — a person typing in another
/// editor — is not swallowed.
const DEFAULT_WINDOW: Duration = Duration::from_millis(200);

#[derive(Debug, thiserror::Error)]
pub enum WatchError {
    #[error("cannot watch {} for changes: {message}", .path.display())]
    Failed { path: std::path::PathBuf, message: String },
}

/// A short-lived memory of what Jott wrote itself, so a save does not come back as an external
/// change. Paths are `rel`s with `/`, because that is the spelling everything else here uses.
pub struct SelfWrites {
    window: Duration,
    recent: Mutex<HashMap<String, Instant>>,
}

impl Default for SelfWrites {
    fn default() -> Self {
        Self::new()
    }
}

impl SelfWrites {
    pub fn new() -> Self {
        Self::with_window(DEFAULT_WINDOW)
    }

    /// Tests widen this so a slow callback cannot arrive after the window and turn our own save into
    /// a reload prompt; production uses [`Self::new`].
    pub fn with_window(window: Duration) -> Self {
        Self { window, recent: Mutex::new(HashMap::new()) }
    }

    pub fn shared(self) -> Arc<Self> {
        Arc::new(self)
    }

    /// Call this *before* the write, so an event that lands while the bytes are still going out is
    /// already covered.
    pub fn record(&self, rel: &str) {
        let Ok(mut recent) = self.recent.lock() else { return };
        // Whatever nobody will ask about again goes, so a long session does not remember every file
        // it ever saved.
        if recent.len() > 512 {
            let window = self.window;
            recent.retain(|_, when| when.elapsed() <= window);
        }
        recent.insert(rel.to_string(), Instant::now());
    }

    pub fn is_recent(&self, rel: &str) -> bool {
        let Ok(recent) = self.recent.lock() else { return false };
        recent.get(rel).is_some_and(|when| *when + self.window > Instant::now())
    }
}

/// Whether an event on this path is worth telling the UI about.
///
/// Dot-names are excluded because the tree does not show them (Task 7, matching np's walk), and a
/// change to `.git/HEAD` has nothing on screen to update — without this rule, every branch switch in
/// a git-synced vault wakes the editor.
pub fn should_report(rel: &str, writes: &SelfWrites) -> bool {
    !rel.split('/').any(|part| part.starts_with('.')) && !writes.is_recent(rel)
}

fn interesting(kind: &EventKind) -> bool {
    matches!(kind, EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_))
}

/// A recursive watcher over one vault. Events are raw paths; [`VaultWatch::next_change`] is the only
/// reader, and it hands out `rel`s that passed [`should_report`].
pub struct VaultWatch {
    root: std::path::PathBuf,
    rx: Receiver<std::path::PathBuf>,
    // Held so the watcher thread lives as long as this struct; nothing reads it.
    _watcher: RecommendedWatcher,
    writes: Arc<SelfWrites>,
}

impl VaultWatch {
    pub fn start(root: &Path, writes: Arc<SelfWrites>) -> Result<Self, WatchError> {
        let (tx, rx) = mpsc::channel();
        let failed = |message: String| WatchError::Failed { path: root.to_path_buf(), message };
        let mut watcher = RecommendedWatcher::new(
            move |event: notify::Result<notify::Event>| {
                if let Ok(event) = event {
                    if interesting(&event.kind) {
                        for path in event.paths {
                            // The receiver may be gone during shutdown; a dropped event then is a
                            // dropped window, not a lost note.
                            let _ = tx.send(path);
                        }
                    }
                }
            },
            Config::default(),
        )
        .map_err(|e| failed(e.to_string()))?;
        watcher.watch(root, RecursiveMode::Recursive).map_err(|e| failed(e.to_string()))?;
        Ok(Self { root: root.to_path_buf(), rx, _watcher: watcher, writes })
    }

    /// The next path somebody outside Jott changed, or `None` when nothing arrived within `wait`.
    /// Our own writes and dot-names are skipped here rather than queued up for later: an event that
    /// is not worth showing is not worth remembering either.
    pub fn next_change(&mut self, wait: Duration) -> Option<String> {
        let deadline = Instant::now() + wait;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return None;
            }
            match self.rx.recv_timeout(left) {
                Ok(path) => {
                    let Some(rel) = rel_of(&self.root, &path) else { continue };
                    if should_report(&rel, &self.writes) {
                        return Some(rel);
                    }
                }
                // Timeout means nobody touched anything; disconnected means the watcher thread is
                // gone, which the UI treats as "no live watch" rather than as an error (§4: no
                // invented state — Task 10 says so in the status line if it ever matters).
                Err(RecvTimeoutError::Timeout) | Err(RecvTimeoutError::Disconnected) => return None,
            }
        }
    }
}

fn rel_of(root: &Path, path: &Path) -> Option<String> {
    let rest = path.strip_prefix(root).ok()?;
    let rel = rest.to_string_lossy().replace('\\', "/");
    (!rel.is_empty()).then_some(rel)
}
