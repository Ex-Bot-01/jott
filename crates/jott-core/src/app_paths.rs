//! Where Jott keeps its own files — and only its own.
//!
//! Everything the app remembers (vault registry, ingest stamps, unsaved-buffer mirrors, trash
//! fallback, logs) lives under one directory the platform names, and nothing is ever written
//! inside a vault (§7). One root for all of it, because two homes means two places to look when
//! something goes wrong and two places to clean up.
//!
//! Linux: `$XDG_DATA_HOME/jott`, falling back to `~/.local/share/jott`. Windows: `%APPDATA%\jott`.
//! The XDG *data* directory rather than the config one because what lives here is state — a
//! registry, stamps, buffers — which is also where the Tauri shell puts its own files.

use std::path::{Path, PathBuf};
use thiserror::Error;

/// The folder's name inside the platform's data root: named after the app, not after the project
/// directory or the crate (§12's rename).
pub const APP_DIR_NAME: &str = "jott";

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AppPathsError {
    #[error("neither XDG_DATA_HOME nor HOME is set, so there is no data directory to use; set one of them")]
    NoUnixRoot,
    #[error("%APPDATA% is not set, so there is no application data directory to use")]
    NoWindowsRoot,
}

/// The app's data directory. Not created here: asking where it is should not write anything.
pub fn app_data_dir() -> Result<PathBuf, AppPathsError> {
    #[cfg(windows)]
    {
        let root = std::env::var_os("APPDATA").ok_or(AppPathsError::NoWindowsRoot)?;
        return Ok(PathBuf::from(root).join(APP_DIR_NAME));
    }
    #[cfg(not(windows))]
    {
        unix_data_dir()
    }
}

#[cfg(not(windows))]
fn unix_data_dir() -> Result<PathBuf, AppPathsError> {
    if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
        let root = PathBuf::from(&xdg);
        // The base directory spec requires an absolute path here. A relative value is not a
        // location — it would resolve against whatever folder the app was started from — so it is
        // ignored rather than obeyed.
        if root.is_absolute() {
            return Ok(root.join(APP_DIR_NAME));
        }
    }
    let home = std::env::var_os("HOME").ok_or(AppPathsError::NoUnixRoot)?;
    Ok(PathBuf::from(home).join(".local/share").join(APP_DIR_NAME))
}

/// Create a folder of Jott's own, readable only by its owner (issue #9).
///
/// `create_dir_all` applies the process umask, and the usual umask says 0755 — which on a shared Linux box means
/// another account can walk into `~/.local/share/jott/buffers` and list the mirrored drafts of notes in progress.
/// The mode is applied *after* creation because there is no way to ask `create_dir_all` for one: that leaves a short
/// window on a folder that did not exist a moment ago, stated here rather than hidden, and it has one useful
/// consequence — the mode is set on every call, so an install that predates this fix gets tightened as it is used
/// instead of waiting for a fresh machine.
pub fn create_private_dir_all(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    set_owner_only_mode(dir, 0o700);
    Ok(())
}

/// Best effort: this file belongs to one person.
///
/// For a temp file that is about to be renamed onto its destination — a rename moves with the **temp's** mode, so
/// setting it on `state.json.tmp-<pid>` is what actually lands, and setting it afterwards would chase the file it
/// already replaced. Used directly for files written in place (the buffer mirrors).
pub fn make_private(path: &Path) {
    set_owner_only_mode(path, 0o600);
}

#[cfg(unix)]
fn set_owner_only_mode(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    // Errors are ignored deliberately, and not lazily: a filesystem with no permission bits (a FAT stick used as an
    // app-data folder, some SMB mounts) answers `EPERM`/`ENOTSUP`, and failing somebody's save over a mode Jott
    // cannot set is the worse outcome — the note is the thing that must not be lost. The same reasoning is why this
    // helper takes no `Result`: every caller would discard it anyway.
    let Ok(meta) = std::fs::metadata(path) else { return };
    // Anything above the permission bits (setuid, setgid, sticky) belongs to whoever mounted the folder.
    let keep = meta.permissions().mode() & !0o777;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(keep | mode));
}

#[cfg(not(unix))]
fn set_owner_only_mode(_path: &Path, _mode: u32) {
    // Windows has no owner-only bit to flip: `%APPDATA%` is per-user already and its ACL comes from the profile,
    // which is why issue #9 says the problem is the multi-user Linux box rather than the machine Jott ships for.
    // macOS is unix and takes the real path above.
}
