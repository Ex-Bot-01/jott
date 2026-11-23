//! Versions and sync for the open vault (design: `docs/superpowers/specs/2026-11-23-git-versions-and-sync-design.md`).
//!
//! Every git call is `jott-history`'s. This module decides *when* — after the vault has been quiet for a while, on
//! close, before a restore or a sync — remembers what the Settings panel shows, and keeps the per-vault choices in
//! `state/<vault key>/history.json` beside the stamps. Nothing here runs git until history is on or the panel asks.
//!
//! All of it is blocking (git is a child process); the session runs it on blocking threads so a slow push never
//! holds up the window.

use crate::sync::guarded;
use crate::vault::stamps::workspace_key;
use jott_history::cloud::CloudProbe;
use jott_history::locate::Probe;
use jott_history::{
    cloud_service, install_hint, locate, CloudService, CommitOutcome, Discovery, Git, GitMissing, GitSource,
    HistoryError, InstallHint, LargeFile, Layout, Platform, Repo, SyncOutcome, SystemCloudProbe, SystemProbe,
    Version,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const DEFAULT_QUIET_SECS: u64 = 60;
pub const MIN_QUIET_SECS: u64 = 10;
pub const MAX_QUIET_SECS: u64 = 3600;
/// However busy the typing, a version is made at least this often while changes keep arriving.
const LONGEST_WAIT: Duration = Duration::from_secs(600);
/// A version that failed (no name yet, a merge open in an adopted repository) is not retried by the timer sooner.
const RETRY_AFTER_FAILURE: Duration = Duration::from_secs(300);

/// What this vault's owner chose. Stored per vault: a decision about `~/Notes` says nothing about `~/Work`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HistorySettings {
    pub enabled: bool,
    pub quiet_secs: u64,
    pub sync_on_open_close: bool,
}

impl Default for HistorySettings {
    fn default() -> Self {
        HistorySettings { enabled: false, quiet_secs: DEFAULT_QUIET_SECS, sync_on_open_close: false }
    }
}

/// The git this app uses, as the panel shows it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitReport {
    pub found: bool,
    pub path: Option<String>,
    pub version: Option<String>,
    pub source: Option<GitSource>,
    /// The path chosen in Settings, if any — shown even when it does not work, so it can be cleared.
    pub configured: Option<String>,
    /// Why no git is in use, as a sentence.
    pub problem: Option<String>,
    pub install: InstallHint,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Existing {
    /// Not looked at: there is no git to look with.
    Unknown,
    Nothing,
    AtRoot,
    Inside,
    AppData,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Identity {
    pub name: String,
    pub email: String,
}

/// The last sync: when, and either what happened or git's sentence about why it did not.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SyncReport {
    pub at_ms: u64,
    pub outcome: Option<SyncOutcome>,
    pub error: Option<String>,
}

/// Everything the "Versions & sync" section draws, in one answer.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryStatus {
    pub git: GitReport,
    pub enabled: bool,
    pub existing: Existing,
    /// The repository this vault sits inside, when that is why history cannot be turned on.
    pub inside_top: Option<String>,
    pub layout: Option<Layout>,
    /// The service syncing this folder, by name, when there is one.
    pub cloud: Option<String>,
    pub identity: Option<Identity>,
    pub remote: Option<String>,
    /// False for a cloud-synced folder: the service already syncs it.
    pub sync_offered: bool,
    pub quiet_secs: u64,
    pub sync_on_open_close: bool,
    pub too_large: Vec<LargeFile>,
    pub last_version_at_ms: Option<u64>,
    /// Changes have arrived since the last version.
    pub pending: bool,
    pub last_sync: Option<SyncReport>,
    /// The last thing that stopped a version, as a sentence; cleared by the next version.
    pub problem: Option<String>,
}

/// Why a history call did not happen, before it reaches the window as an `IpcError`.
#[derive(Debug)]
pub enum HistoryFault {
    GitMissing(GitMissing),
    History(HistoryError),
}

impl std::fmt::Display for HistoryFault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HistoryFault::GitMissing(missing) => missing.fmt(f),
            HistoryFault::History(error) => error.fmt(f),
        }
    }
}

impl From<GitMissing> for HistoryFault {
    fn from(missing: GitMissing) -> Self {
        HistoryFault::GitMissing(missing)
    }
}

impl From<HistoryError> for HistoryFault {
    fn from(error: HistoryError) -> Self {
        HistoryFault::History(error)
    }
}

struct State {
    git: Option<Result<Git, GitMissing>>,
    settings: HistorySettings,
    repo: Option<Repo>,
    too_large: Vec<LargeFile>,
    last_version_at_ms: Option<u64>,
    last_sync: Option<SyncReport>,
    problem: Option<String>,
    /// After a version failed, the timer leaves it alone until then — one journal line per problem, not one per tick.
    retry_after: Option<Instant>,
}

/// When changes arrived since the last version: the first one, and the latest.
#[derive(Clone, Copy)]
struct Pending {
    first: Instant,
    last: Instant,
}

pub struct History {
    vault: PathBuf,
    /// The app data dir, where `git.json` (the chosen git) lives.
    data: PathBuf,
    /// `state/<vault key>`: `history.json`, and the repository of a cloud-synced vault.
    dir: PathBuf,
    cloud: Option<CloudService>,
    /// Held for the length of every git operation, so two never interleave in one repository.
    op: Mutex<()>,
    state: Mutex<State>,
    pending: Mutex<Option<Pending>>,
}

impl History {
    /// Reads two small files and looks at the folder's path. Runs nothing.
    pub fn new(vault: &Path, data: &Path) -> History {
        History::with_cloud(vault, data, &SystemCloudProbe)
    }

    pub fn with_cloud(vault: &Path, data: &Path, probe: &dyn CloudProbe) -> History {
        let dir = data.join("state").join(workspace_key(vault));
        let settings = std::fs::read_to_string(dir.join("history.json"))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        History {
            vault: vault.to_path_buf(),
            data: data.to_path_buf(),
            dir,
            cloud: cloud_service(vault, probe),
            op: Mutex::new(()),
            state: Mutex::new(State {
                git: None,
                settings,
                repo: None,
                too_large: Vec::new(),
                last_version_at_ms: None,
                last_sync: None,
                problem: None,
                retry_after: None,
            }),
            pending: Mutex::new(None),
        }
    }

    pub fn settings(&self) -> HistorySettings {
        guarded(&self.state).settings.clone()
    }

    fn app_dir(&self) -> PathBuf {
        self.dir.join("history.git")
    }

    // ---- git ----

    fn configured_git(&self) -> Option<PathBuf> {
        let text = std::fs::read_to_string(self.data.join("git.json")).ok()?;
        let value: serde_json::Value = serde_json::from_str(&text).ok()?;
        value.get("path").and_then(|p| p.as_str()).filter(|p| !p.is_empty()).map(PathBuf::from)
    }

    fn git(&self) -> Result<Git, GitMissing> {
        let mut state = guarded(&self.state);
        if state.git.is_none() {
            let configured = self.configured_git();
            state.git = Some(locate(&SystemProbe as &dyn Probe, Platform::current(), configured.as_deref()));
        }
        state.git.clone().expect("set above")
    }

    /// Look for git again — after somebody installed it, or chose a different one.
    pub fn check_git(&self) {
        let _op = guarded(&self.op);
        let mut state = guarded(&self.state);
        state.git = None;
        state.repo = None;
    }

    /// Choose the git to use, or `None` to go back to searching. App-wide: one machine, one git.
    pub fn set_git_path(&self, path: Option<&str>) -> std::io::Result<()> {
        let path = path.map(str::trim).filter(|p| !p.is_empty());
        let file = self.data.join("git.json");
        match path {
            Some(path) => write_private(&file, &serde_json::json!({ "path": path }).to_string())?,
            None => {
                let _ = std::fs::remove_file(&file);
            }
        }
        self.check_git();
        Ok(())
    }

    fn repo(&self) -> Result<Repo, HistoryFault> {
        if let Some(repo) = guarded(&self.state).repo.clone() {
            return Ok(repo);
        }
        if !guarded(&self.state).settings.enabled {
            return Err(HistoryError::NotEnabled.into());
        }
        let repo = Repo::open(&self.git()?, &self.vault, &self.app_dir())?;
        guarded(&self.state).repo = Some(repo.clone());
        Ok(repo)
    }

    // ---- what the panel shows ----

    pub fn status(&self) -> HistoryStatus {
        let git = self.git();
        let configured = self.configured_git().map(|p| p.display().to_string());
        let report = match &git {
            Ok(git) => GitReport {
                found: true,
                path: Some(git.path.display().to_string()),
                version: Some(git.version_text.clone()),
                source: Some(git.source),
                configured,
                problem: None,
                install: install_hint(Platform::current()),
            },
            Err(missing) => GitReport {
                found: false,
                path: None,
                version: None,
                source: None,
                configured,
                problem: Some(missing.to_string()),
                install: install_hint(Platform::current()),
            },
        };
        let (existing, inside_top) = match &git {
            Ok(git) => match Repo::discover(git, &self.vault, &self.app_dir()) {
                Ok(Discovery::Nothing) => (Existing::Nothing, None),
                Ok(Discovery::AtRoot) => (Existing::AtRoot, None),
                Ok(Discovery::AppData) => (Existing::AppData, None),
                Ok(Discovery::Inside { top }) => (Existing::Inside, Some(top.display().to_string())),
                Err(_) => (Existing::Unknown, None),
            },
            Err(_) => (Existing::Unknown, None),
        };
        let enabled = guarded(&self.state).settings.enabled;
        let repo = if enabled && git.is_ok() { self.repo().ok() } else { None };
        let identity = repo
            .as_ref()
            .and_then(|r| r.identity().ok().flatten())
            .map(|(name, email)| Identity { name, email });
        let remote = repo.as_ref().and_then(|r| r.remote_url().ok().flatten());
        let pending = guarded(&self.pending).is_some();
        let state = guarded(&self.state);
        HistoryStatus {
            git: report,
            enabled: state.settings.enabled,
            existing,
            inside_top,
            layout: repo.as_ref().map(Repo::layout),
            cloud: self.cloud.map(|c| c.name().to_string()),
            identity,
            remote,
            sync_offered: self.cloud.is_none(),
            quiet_secs: state.settings.quiet_secs,
            sync_on_open_close: state.settings.sync_on_open_close,
            too_large: state.too_large.clone(),
            last_version_at_ms: state.last_version_at_ms,
            pending,
            last_sync: state.last_sync.clone(),
            problem: state.problem.clone(),
        }
    }

    // ---- turning it on and off ----

    /// Create or adopt the repository, remember the choice, and make the first version if git knows who to name.
    pub fn enable(&self) -> Result<Option<CommitOutcome>, HistoryFault> {
        let git = self.git()?;
        {
            let _op = guarded(&self.op);
            let repo = Repo::enable(&git, &self.vault, &self.app_dir(), self.cloud.is_some())?;
            let mut state = guarded(&self.state);
            state.repo = Some(repo);
            state.settings.enabled = true;
            self.save_settings(&state.settings);
        }
        self.touch();
        match self.version_now(None) {
            Ok(outcome) => Ok(Some(outcome)),
            // Turning it on worked; the panel asks for a name and the first version follows.
            Err(HistoryFault::History(HistoryError::NeedsIdentity)) => Ok(None),
            Err(other) => Err(other),
        }
    }

    /// Stop making versions. The repository and its history stay exactly where they are.
    pub fn disable(&self) {
        let _op = guarded(&self.op);
        let mut state = guarded(&self.state);
        state.settings.enabled = false;
        state.repo = None;
        self.save_settings(&state.settings);
        *guarded(&self.pending) = None;
    }

    pub fn set_options(&self, quiet_secs: u64, sync_on_open_close: bool) {
        let mut state = guarded(&self.state);
        state.settings.quiet_secs = quiet_secs.clamp(MIN_QUIET_SECS, MAX_QUIET_SECS);
        state.settings.sync_on_open_close = sync_on_open_close;
        self.save_settings(&state.settings);
    }

    /// The name and email for versions, written into this repository only. Then the version that was waiting.
    pub fn set_identity(&self, name: &str, email: &str) -> Result<CommitOutcome, HistoryFault> {
        let (name, email) = (name.trim(), email.trim());
        let bad = |s: &str| s.is_empty() || s.chars().any(char::is_control) || s.contains(['<', '>']);
        if bad(name) || bad(email) || !email.contains('@') {
            return Err(HistoryError::BadVersion("a name and an email address are both needed".into()).into());
        }
        let repo = self.repo()?;
        {
            let _op = guarded(&self.op);
            repo.set_identity(name, email)?;
        }
        self.touch();
        self.version_now(None)
    }

    fn save_settings(&self, settings: &HistorySettings) {
        let text = serde_json::to_string_pretty(settings).expect("settings serialize");
        // A setting that could not be written still applies for this run; the drawer shows the state root.
        let _ = write_private(&self.dir.join("history.json"), &text);
    }

    // ---- versions ----

    /// Something in the vault changed. The version follows once things are quiet.
    pub fn touch(&self) {
        let now = Instant::now();
        let mut pending = guarded(&self.pending);
        *pending = Some(match *pending {
            Some(p) => Pending { first: p.first, last: now },
            None => Pending { first: now, last: now },
        });
    }

    /// Whether a version is owed now: quiet for long enough, or changes have been arriving for too long.
    pub fn due(&self) -> bool {
        let (settings, retry_after) = {
            let state = guarded(&self.state);
            (state.settings.clone(), state.retry_after)
        };
        if !settings.enabled || retry_after.is_some_and(|at| Instant::now() < at) {
            return false;
        }
        guarded(&self.pending).is_some_and(|p| {
            p.last.elapsed() >= Duration::from_secs(settings.quiet_secs) || p.first.elapsed() >= LONGEST_WAIT
        })
    }

    /// Make a version of whatever changed, now.
    pub fn version_now(&self, message: Option<&str>) -> Result<CommitOutcome, HistoryFault> {
        let repo = self.repo()?;
        let _op = guarded(&self.op);
        let started = Instant::now();
        let result = repo.commit_if_dirty(message);
        let mut state = guarded(&self.state);
        match &result {
            Ok(outcome) => {
                state.too_large = outcome.too_large.clone();
                state.problem = None;
                state.retry_after = None;
                if outcome.version.is_some() {
                    state.last_version_at_ms = Some(crate::diagnostics::now_ms());
                }
                let mut pending = guarded(&self.pending);
                // A change that arrived while git was working is still owed a version.
                if pending.is_some_and(|p| p.last <= started) {
                    *pending = None;
                }
            }
            Err(error) => {
                state.problem = Some(error.to_string());
                state.retry_after = Some(Instant::now() + RETRY_AFTER_FAILURE);
            }
        }
        Ok(result?)
    }

    pub fn versions(&self, rel: &str) -> Result<Vec<Version>, HistoryFault> {
        let repo = self.repo()?;
        let _op = guarded(&self.op);
        Ok(repo.versions(rel)?)
    }

    pub fn bytes_at(&self, version: &str, rel: &str) -> Result<Vec<u8>, HistoryFault> {
        let repo = self.repo()?;
        let _op = guarded(&self.op);
        Ok(repo.bytes_at(version, rel)?)
    }

    // ---- remote ----

    pub fn connect(&self, url: &str) -> Result<(), HistoryFault> {
        if let Some(service) = self.cloud {
            return Err(HistoryError::SyncedByCloud { service: service.name().to_string() }.into());
        }
        let repo = self.repo()?;
        let _op = guarded(&self.op);
        repo.connect(url)?;
        Ok(())
    }

    pub fn disconnect(&self) -> Result<(), HistoryFault> {
        let repo = self.repo()?;
        let _op = guarded(&self.op);
        Ok(repo.disconnect()?)
    }

    pub fn has_remote(&self) -> bool {
        self.repo().ok().and_then(|repo| repo.remote_url().ok().flatten()).is_some()
    }

    /// A version of what is unsaved in history, then the remote's versions in and ours out.
    pub fn sync(&self) -> Result<SyncOutcome, HistoryFault> {
        if let Some(service) = self.cloud {
            return Err(HistoryError::SyncedByCloud { service: service.name().to_string() }.into());
        }
        self.version_now(None)?;
        let repo = self.repo()?;
        let result = {
            let _op = guarded(&self.op);
            repo.sync()
        };
        let report = match &result {
            Ok(outcome) => SyncReport { at_ms: crate::diagnostics::now_ms(), outcome: Some(outcome.clone()), error: None },
            Err(error) => SyncReport { at_ms: crate::diagnostics::now_ms(), outcome: None, error: Some(error.to_string()) },
        };
        guarded(&self.state).last_sync = Some(report);
        Ok(result?)
    }
}

/// Owner-only, through a temp file and one rename, like the registry: these name where somebody's notes live.
fn write_private(path: &Path, text: &str) -> std::io::Result<()> {
    let dir = path.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
    crate::app_paths::create_private_dir_all(&dir)?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let temp = dir.join(format!("{name}.tmp-{}", std::process::id()));
    std::fs::write(&temp, text)?;
    crate::app_paths::make_private(&temp);
    std::fs::rename(&temp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NoCloud;
    impl CloudProbe for NoCloud {
        fn env(&self, _: &str) -> Option<PathBuf> {
            None
        }
        fn home(&self) -> Option<PathBuf> {
            None
        }
        fn read(&self, _: &Path) -> Option<String> {
            None
        }
        fn names_in(&self, _: &Path) -> Vec<String> {
            Vec::new()
        }
        fn case_insensitive(&self) -> bool {
            false
        }
    }

    #[test]
    fn nothing_is_owed_while_history_is_off_however_much_changes() {
        let dir = tempfile::tempdir().unwrap();
        let history = History::with_cloud(&dir.path().join("vault"), dir.path(), &NoCloud);
        history.touch();
        assert!(!history.due());
    }

    #[test]
    fn a_version_waits_for_quiet_and_the_choice_survives_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().join("vault");
        let history = History::with_cloud(&vault, dir.path(), &NoCloud);
        {
            let mut state = guarded(&history.state);
            state.settings.enabled = true;
            history.save_settings(&state.settings);
        }
        history.set_options(1, true);
        assert_eq!(history.settings().quiet_secs, MIN_QUIET_SECS, "clamped to the shortest quiet period");
        history.touch();
        assert!(!history.due(), "changes just arrived");
        {
            let mut pending = guarded(&history.pending);
            let long_ago = Instant::now() - Duration::from_secs(MIN_QUIET_SECS + 1);
            *pending = Some(Pending { first: long_ago, last: long_ago });
        }
        assert!(history.due());

        let again = History::with_cloud(&vault, dir.path(), &NoCloud);
        assert_eq!(again.settings(), HistorySettings { enabled: true, quiet_secs: MIN_QUIET_SECS, sync_on_open_close: true });
    }

    #[test]
    fn steady_typing_still_gets_a_version_eventually() {
        let dir = tempfile::tempdir().unwrap();
        let history = History::with_cloud(&dir.path().join("vault"), dir.path(), &NoCloud);
        guarded(&history.state).settings.enabled = true;
        *guarded(&history.pending) =
            Some(Pending { first: Instant::now() - LONGEST_WAIT - Duration::from_secs(1), last: Instant::now() });
        assert!(history.due());
    }
}
