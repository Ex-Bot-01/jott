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

use std::path::PathBuf;
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
