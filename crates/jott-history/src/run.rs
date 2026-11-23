//! Running git so that the person's own configuration cannot break an unattended commit.
//!
//! Configuration files are *not* discarded: Git for Windows keeps `credential.helper=manager` in the system config,
//! and that is exactly what a push needs. Instead the few keys that would break Jott are overridden with `-c`, which
//! outranks every file, on every call (spec: *Isolation from the person's git configuration*).

use crate::error::HistoryError;
use crate::locate::Git;
use std::ffi::OsStr;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Win32 `CREATE_NO_WINDOW`: no console flashes up for a child of a windowed app. Mirrors `jott-core`'s np client.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub(crate) fn no_window(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = command;
}

/// Where a repository's metadata and files live. Always passed explicitly, so git never goes looking upwards and
/// finds a repository the vault merely sits inside.
#[derive(Debug, Clone)]
pub(crate) struct Place {
    pub git_dir: PathBuf,
    pub work_tree: PathBuf,
}

pub(crate) struct Output {
    pub status: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

impl Output {
    pub fn ok(&self) -> bool {
        self.status == Some(0)
    }

    pub fn stdout_text(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }

    /// What git said, for a person: stderr, or stdout when git put its complaint there (it does for `commit`).
    pub fn said(&self) -> String {
        let err = String::from_utf8_lossy(&self.stderr).trim().to_string();
        if err.is_empty() {
            String::from_utf8_lossy(&self.stdout).trim().to_string()
        } else {
            err
        }
    }
}

impl Git {
    /// A git command with the isolation applied. `place` adds `--git-dir`/`--work-tree` and runs inside the work tree.
    pub(crate) fn command(&self, place: Option<&Place>) -> Command {
        let mut command = Command::new(&self.path);
        command.arg("--no-pager");
        let overrides: [(&str, &OsStr); 9] = [
            // Bytes in, bytes out: Jott's external-change detection compares hashes of what is on disk.
            ("core.autocrlf", OsStr::new("false")),
            ("core.safecrlf", OsStr::new("false")),
            // A key prompt or a missing key must not fail a save nobody is watching.
            ("commit.gpgsign", OsStr::new("false")),
            ("tag.gpgsign", OsStr::new("false")),
            // Non-ASCII note names come back as names, not octal escapes.
            ("core.quotepath", OsStr::new("false")),
            // No daemon started on somebody's behalf.
            ("core.fsmonitor", OsStr::new("false")),
            ("color.ui", OsStr::new("false")),
            ("core.longpaths", OsStr::new("true")),
            ("core.precomposeunicode", OsStr::new("true")),
        ];
        for (key, value) in overrides {
            command.arg("-c").arg(config_pair(key, value));
        }
        if let Some(place) = place {
            // A global hook must not run on every autosave. The folder is never created: no folder, no hooks.
            command.arg("-c").arg(config_pair("core.hooksPath", place.git_dir.join("jott-no-hooks").as_os_str()));
            // For this call only — a vault on another drive or a share passes the ownership check without anything
            // being written into anyone's global config. Older gits ignore it from the command line.
            command.arg("-c").arg(config_pair("safe.directory", place.work_tree.as_os_str()));
            command.arg("-c").arg(config_pair("safe.directory", place.git_dir.as_os_str()));
            command.arg("--git-dir").arg(&place.git_dir).arg("--work-tree").arg(&place.work_tree);
            command.current_dir(&place.work_tree);
        }
        // A missing credential helper fails instead of waiting on a terminal nobody can see.
        command.env("GIT_TERMINAL_PROMPT", "0");
        // A note called `*.md` or `[draft].md` is that file, not a pattern.
        command.env("GIT_LITERAL_PATHSPECS", "1");
        command.env("GIT_OPTIONAL_LOCKS", "0");
        command.env_remove("GIT_DIR");
        command.env_remove("GIT_WORK_TREE");
        command.env_remove("GIT_INDEX_FILE");
        for (key, value) in &self.env {
            command.env(key, value);
        }
        command.stdin(Stdio::null());
        no_window(&mut command);
        command
    }

    pub(crate) fn output(&self, mut command: Command, input: Option<&[u8]>) -> Result<Output, HistoryError> {
        let spawn_error = |source| HistoryError::Spawn { path: self.path.clone(), source };
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        if input.is_some() {
            command.stdin(Stdio::piped());
        }
        let mut child = command.spawn().map_err(spawn_error)?;
        if let Some(bytes) = input {
            // Written from a thread so a large pathspec list cannot deadlock against git filling its stdout pipe.
            let mut stdin = child.stdin.take().expect("stdin was piped");
            let bytes = bytes.to_vec();
            let writer = std::thread::spawn(move || {
                let _ = stdin.write_all(&bytes);
            });
            let output = child.wait_with_output().map_err(spawn_error)?;
            let _ = writer.join();
            return Ok(Output { status: output.status.code(), stdout: output.stdout, stderr: output.stderr });
        }
        let output = child.wait_with_output().map_err(spawn_error)?;
        Ok(Output { status: output.status.code(), stdout: output.stdout, stderr: output.stderr })
    }

    /// Run in a repository; a non-zero exit is an error carrying git's sentence.
    pub(crate) fn run(&self, place: &Place, args: &[&OsStr]) -> Result<Output, HistoryError> {
        let output = self.try_run(place, args)?;
        if output.ok() {
            Ok(output)
        } else {
            Err(failed(args, &output))
        }
    }

    /// Run in a repository and hand back whatever happened; for commands whose exit code is an answer.
    pub(crate) fn try_run(&self, place: &Place, args: &[&OsStr]) -> Result<Output, HistoryError> {
        let mut command = self.command(Some(place));
        command.args(args);
        self.output(command, None)
    }

    pub(crate) fn run_with_input(&self, place: &Place, args: &[&OsStr], input: &[u8]) -> Result<Output, HistoryError> {
        let mut command = self.command(Some(place));
        command.args(args);
        let output = self.output(command, Some(input))?;
        if output.ok() {
            Ok(output)
        } else {
            Err(failed(args, &output))
        }
    }

    /// Run outside any repository (`ls-remote` before a remote is added, `rev-parse` while discovering).
    pub(crate) fn run_bare(&self, cwd: Option<&Path>, args: &[&OsStr]) -> Result<Output, HistoryError> {
        let mut command = self.command(None);
        if let Some(cwd) = cwd {
            command.current_dir(cwd);
        }
        command.args(args);
        self.output(command, None)
    }
}

pub(crate) fn failed(args: &[&OsStr], output: &Output) -> HistoryError {
    let command = args.first().map(|a| a.to_string_lossy().into_owned()).unwrap_or_default();
    HistoryError::Git { command, said: output.said() }
}

fn config_pair(key: &str, value: &OsStr) -> std::ffi::OsString {
    let mut pair = std::ffi::OsString::from(key);
    pair.push("=");
    pair.push(value);
    pair
}

/// `["status", "--porcelain=v2"]` as the `&[&OsStr]` the runner takes.
macro_rules! args {
    ($($arg:expr),* $(,)?) => {
        &[$(::std::convert::AsRef::<::std::ffi::OsStr>::as_ref(&$arg)),*][..]
    };
}
pub(crate) use args;
