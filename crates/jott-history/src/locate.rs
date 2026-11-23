//! Where the `git` binary comes from, and what to say when there isn't one.
//!
//! Order, the same shape as np's (`jott-core/src/np/resolve.rs`): a path chosen in Settings → the program search
//! path → the platform's usual install locations. A chosen path that does not work is an error of its own and never
//! a fallthrough: somebody picked a binary, and quietly committing with a different one is not what they picked.
//!
//! The usual locations are what let "Check again" work without a restart. A Windows install changes the PATH that
//! new processes get, not the one the running Jott already has.

use serde::Serialize;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// 2.28 is the oldest git with `init -b`; `--pathspec-from-file` (2.26) and `%as` (2.25) are older still. Ubuntu
/// 22.04 ships 2.34.1, current Git for Windows is 2.56.
pub const MIN_GIT_VERSION: (u32, u32, u32) = (2, 28, 0);

pub const DOWNLOAD_URL: &str = "https://git-scm.com/downloads";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Platform {
    Windows,
    Macos,
    Linux,
}

impl Platform {
    pub fn current() -> Platform {
        if cfg!(windows) {
            Platform::Windows
        } else if cfg!(target_os = "macos") {
            Platform::Macos
        } else {
            Platform::Linux
        }
    }

    fn program(self) -> &'static str {
        match self {
            Platform::Windows => "git.exe",
            _ => "git",
        }
    }
}

/// How a git was found, in the words the Settings panel and the diagnostics drawer use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum GitSource {
    /// The path chosen in Settings.
    Configured,
    /// The program search path (`PATH`).
    Path,
    /// One of the platform's usual install locations.
    Usual,
}

/// A git that answered `--version` at or above the floor.
#[derive(Debug, Clone)]
pub struct Git {
    pub path: PathBuf,
    pub version: (u32, u32, u32),
    /// What git said, e.g. `2.56.0.windows.2`.
    pub version_text: String,
    pub source: GitSource,
    /// Extra environment for every call. Empty in the app; tests use it to stand in a hostile global config.
    pub(crate) env: Vec<(OsString, OsString)>,
}

impl Git {
    /// A git at a known path, without searching. The version is asked for as usual.
    pub fn at(path: &Path) -> Result<Git, GitMissing> {
        check(&SystemProbe, path, GitSource::Configured)
    }

    /// Add environment to every call this git makes.
    pub fn with_env(mut self, key: impl Into<OsString>, value: impl Into<OsString>) -> Git {
        self.env.push((key.into(), value.into()));
        self
    }
}

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum GitMissing {
    #[error("the git chosen in Settings ({0}) is not a program Jott can run")]
    ConfiguredNotRunnable(PathBuf),
    #[error("git is not installed on this computer, or Jott cannot find it")]
    NotFound,
    #[error("git {found} at {path} is too old — Jott needs {needed} or newer")]
    TooOld { found: String, needed: String, path: PathBuf },
    #[error("{path} did not answer the way git does: {said}")]
    NotGit { path: PathBuf, said: String },
}

/// What the machine looks like, so the search can be tested without installing anything.
pub trait Probe {
    fn env(&self, key: &str) -> Option<OsString>;
    fn is_file(&self, path: &Path) -> bool;
    /// What `<path> --version` printed, or why it could not run.
    fn version_of(&self, path: &Path) -> Result<String, String>;
    /// macOS only: whether the command line developer tools are installed, which is what makes `/usr/bin/git` a
    /// git rather than a stub that opens an installer dialog when run.
    fn apple_tools_present(&self) -> bool;
}

pub struct SystemProbe;

impl Probe for SystemProbe {
    fn env(&self, key: &str) -> Option<OsString> {
        std::env::var_os(key)
    }

    fn is_file(&self, path: &Path) -> bool {
        path.is_file()
    }

    fn version_of(&self, path: &Path) -> Result<String, String> {
        let mut command = std::process::Command::new(path);
        command.arg("--version").stdin(std::process::Stdio::null());
        crate::run::no_window(&mut command);
        let output = command.output().map_err(|e| e.to_string())?;
        let said = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if output.status.success() {
            Ok(said)
        } else {
            Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
        }
    }

    fn apple_tools_present(&self) -> bool {
        std::process::Command::new("/usr/bin/xcode-select")
            .arg("-p")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    }
}

/// Find git. `configured` is the path from Settings, if any.
pub fn locate(probe: &dyn Probe, platform: Platform, configured: Option<&Path>) -> Result<Git, GitMissing> {
    if let Some(path) = configured {
        if !probe.is_file(path) {
            return Err(GitMissing::ConfiguredNotRunnable(path.to_path_buf()));
        }
        return check(probe, path, GitSource::Configured);
    }
    let mut too_old = None;
    for (path, source) in candidates(probe, platform) {
        if !probe.is_file(&path) || !runnable_here(probe, platform, &path) {
            continue;
        }
        match check(probe, &path, source) {
            Ok(git) => return Ok(git),
            // Keep looking: a newer git may sit further down the list. Say the old one if nothing better turns up.
            Err(old @ GitMissing::TooOld { .. }) => {
                too_old.get_or_insert(old);
            }
            Err(_) => {}
        }
    }
    Err(too_old.unwrap_or(GitMissing::NotFound))
}

fn candidates(probe: &dyn Probe, platform: Platform) -> Vec<(PathBuf, GitSource)> {
    let mut out = Vec::new();
    if let Some(path) = probe.env("PATH") {
        for dir in std::env::split_paths(&path) {
            out.push((dir.join(platform.program()), GitSource::Path));
        }
    }
    for path in usual_locations(probe, platform) {
        out.push((path, GitSource::Usual));
    }
    out
}

fn usual_locations(probe: &dyn Probe, platform: Platform) -> Vec<PathBuf> {
    match platform {
        Platform::Windows => {
            let mut out = Vec::new();
            for (key, tail) in [
                ("ProgramFiles", r"Git\cmd\git.exe"),
                ("ProgramW6432", r"Git\cmd\git.exe"),
                ("LOCALAPPDATA", r"Programs\Git\cmd\git.exe"),
                ("USERPROFILE", r"scoop\apps\git\current\cmd\git.exe"),
            ] {
                if let Some(root) = probe.env(key) {
                    out.push(PathBuf::from(root).join(tail));
                }
            }
            out
        }
        Platform::Macos => ["/opt/homebrew/bin/git", "/usr/local/bin/git", "/usr/bin/git"].map(PathBuf::from).to_vec(),
        Platform::Linux => ["/usr/bin/git", "/usr/local/bin/git"].map(PathBuf::from).to_vec(),
    }
}

/// On macOS `/usr/bin/git` is a stub until Apple's developer tools are installed, and running it opens their
/// installer. Jott must not pop a dialog just by looking.
fn runnable_here(probe: &dyn Probe, platform: Platform, path: &Path) -> bool {
    platform != Platform::Macos || path != Path::new("/usr/bin/git") || probe.apple_tools_present()
}

fn check(probe: &dyn Probe, path: &Path, source: GitSource) -> Result<Git, GitMissing> {
    let said = probe.version_of(path).map_err(|said| GitMissing::NotGit { path: path.to_path_buf(), said })?;
    let Some((version, text)) = parse_version(&said) else {
        return Err(GitMissing::NotGit { path: path.to_path_buf(), said });
    };
    if version < MIN_GIT_VERSION {
        return Err(GitMissing::TooOld { found: text, needed: min_version_text(), path: path.to_path_buf() });
    }
    Ok(Git { path: path.to_path_buf(), version, version_text: text, source, env: Vec::new() })
}

pub fn min_version_text() -> String {
    let (major, minor, patch) = MIN_GIT_VERSION;
    format!("{major}.{minor}.{patch}")
}

/// `git version 2.56.0.windows.2` → `((2, 56, 0), "2.56.0.windows.2")`. Apple's `git version 2.39.5 (Apple Git-154)`
/// parses too.
pub fn parse_version(said: &str) -> Option<((u32, u32, u32), String)> {
    let text = said.lines().next()?.trim().strip_prefix("git version ")?.split_whitespace().next()?.to_string();
    let mut parts = text.split('.').map(|part| part.parse::<u32>().ok());
    let major = parts.next()??;
    let minor = parts.next()??;
    let patch = parts.next().flatten().unwrap_or(0);
    Some(((major, minor, patch), text))
}

/// Where to get git, for the sentence shown when there is none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallHint {
    pub url: String,
    /// One-line routes, the platform's own first.
    pub commands: Vec<String>,
}

pub fn install_hint(platform: Platform) -> InstallHint {
    let commands = match platform {
        Platform::Windows => vec!["winget install --id Git.Git -e --source winget".to_string()],
        Platform::Macos => vec!["xcode-select --install".to_string(), "brew install git".to_string()],
        Platform::Linux => vec!["sudo apt install git".to_string(), "sudo dnf install git".to_string()],
    };
    InstallHint { url: DOWNLOAD_URL.to_string(), commands }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Default)]
    struct Fake {
        env: HashMap<String, OsString>,
        files: HashMap<PathBuf, Result<String, String>>,
        apple_tools: bool,
    }

    impl Fake {
        fn file(mut self, path: &str, said: &str) -> Fake {
            self.files.insert(PathBuf::from(path), Ok(said.to_string()));
            self
        }
        fn env(mut self, key: &str, value: &str) -> Fake {
            self.env.insert(key.to_string(), value.into());
            self
        }
    }

    impl Probe for Fake {
        fn env(&self, key: &str) -> Option<OsString> {
            self.env.get(key).cloned()
        }
        fn is_file(&self, path: &Path) -> bool {
            self.files.contains_key(path)
        }
        fn version_of(&self, path: &Path) -> Result<String, String> {
            self.files.get(path).cloned().unwrap_or_else(|| Err("no such file".into()))
        }
        fn apple_tools_present(&self) -> bool {
            self.apple_tools
        }
    }

    #[test]
    fn versions_parse_from_every_platform_s_wording() {
        assert_eq!(parse_version("git version 2.56.0.windows.2").unwrap().0, (2, 56, 0));
        assert_eq!(parse_version("git version 2.39.5 (Apple Git-154)").unwrap().0, (2, 39, 5));
        assert_eq!(parse_version("git version 2.34.1\n").unwrap().0, (2, 34, 1));
        assert!(parse_version("hello").is_none());
    }

    #[test]
    fn the_path_comes_before_the_usual_locations() {
        let probe = Fake::default()
            .env("PATH", "/opt/bin")
            .file("/opt/bin/git", "git version 2.40.0")
            .file("/usr/bin/git", "git version 2.45.0");
        let git = locate(&probe, Platform::Linux, None).unwrap();
        assert_eq!(git.path, PathBuf::from("/opt/bin/git"));
        assert_eq!(git.source, GitSource::Path);
    }

    #[test]
    fn a_usual_location_is_found_when_the_path_has_nothing() {
        let probe = Fake::default().file("/usr/bin/git", "git version 2.45.0");
        let git = locate(&probe, Platform::Linux, None).unwrap();
        assert_eq!(git.source, GitSource::Usual);
    }

    #[test]
    fn a_chosen_path_that_does_not_work_is_an_error_and_not_a_fallthrough() {
        let probe = Fake::default().file("/usr/bin/git", "git version 2.45.0");
        let chosen = PathBuf::from("/nowhere/git");
        assert_eq!(locate(&probe, Platform::Linux, Some(&chosen)).unwrap_err(), GitMissing::ConfiguredNotRunnable(chosen));
    }

    #[test]
    fn an_old_git_is_named_when_nothing_newer_exists() {
        let probe = Fake::default().file("/usr/bin/git", "git version 2.25.1");
        match locate(&probe, Platform::Linux, None).unwrap_err() {
            GitMissing::TooOld { found, needed, .. } => {
                assert_eq!(found, "2.25.1");
                assert_eq!(needed, "2.28.0");
            }
            other => panic!("expected TooOld, got {other:?}"),
        }
    }

    #[test]
    fn an_old_git_on_the_path_does_not_hide_a_newer_one_installed_elsewhere() {
        let probe = Fake::default()
            .env("PATH", "/opt/bin")
            .file("/opt/bin/git", "git version 2.20.0")
            .file("/usr/bin/git", "git version 2.45.0");
        assert_eq!(locate(&probe, Platform::Linux, None).unwrap().version, (2, 45, 0));
    }

    #[test]
    fn the_apple_stub_is_never_run_without_the_developer_tools() {
        let probe = Fake::default().file("/usr/bin/git", "git version 2.39.5 (Apple Git-154)");
        assert_eq!(locate(&probe, Platform::Macos, None).unwrap_err(), GitMissing::NotFound);
        let probe = Fake { apple_tools: true, ..probe };
        assert!(locate(&probe, Platform::Macos, None).is_ok());
    }

    #[test]
    fn a_windows_install_is_found_without_a_restart() {
        let program_files = r"C:\Program Files";
        let exe = PathBuf::from(program_files).join(r"Git\cmd\git.exe");
        let mut probe = Fake::default().env("ProgramFiles", program_files).env("PATH", r"C:\Windows");
        probe.files.insert(exe.clone(), Ok("git version 2.56.0.windows.2".into()));
        let git = locate(&probe, Platform::Windows, None).unwrap();
        assert_eq!(git.path, exe);
        assert_eq!(git.source, GitSource::Usual);
    }
}
