//! Where the `np` binary comes from, and what to say when there isn't one.
//!
//! Order (spec §14): a binary bundled beside the app → `$NP_PATH` → `$PATH`. A set-but-wrong
//! `NP_PATH` is an error of its own rather than a shrug and a fallthrough: someone named a
//! binary, and quietly using a different one would index their vault with something they did
//! not choose.

use std::path::{Path, PathBuf};
use thiserror::Error;

/// The lowest np this app speaks to. Protocol 1 arrived in 0.3.0; `progress` notifications
/// and `status.get` — both of which the ingest queue uses — arrived in 0.3.1.
pub const MIN_NP_VERSION: (u64, u64, u64) = (0, 3, 1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Windows,
    Linux,
    Macos,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NpSource {
    /// Shipped inside the app bundle.
    Bundled(PathBuf),
    /// Named by `$NP_PATH`.
    Env(PathBuf),
    /// Found on `$PATH`.
    Path(PathBuf),
}

impl NpSource {
    /// The binary to run, whichever way it was found. Where it came from stays on the enum
    /// because the message about a missing np has to name the place it looked.
    pub fn path(&self) -> &Path {
        match self {
            NpSource::Bundled(p) | NpSource::Env(p) | NpSource::Path(p) => p,
        }
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum NpMissing {
    #[error("np was not found (nothing bundled, NP_PATH unset, nothing on PATH)")]
    NotFound,
    #[error("NP_PATH points at {0}, which is not an executable file")]
    OverrideMissing(PathBuf),
    #[error("np {found} is too old; Jott needs 0.3.1 or newer")]
    TooOld { found: String },
    #[error("np did not report a usable version")]
    NoProtocol,
}

pub fn current_os() -> Os {
    if cfg!(windows) {
        Os::Windows
    } else if cfg!(target_os = "macos") {
        Os::Macos
    } else {
        Os::Linux
    }
}

fn exe_name(base: &str) -> String {
    if cfg!(windows) { format!("{base}.exe") } else { base.to_string() }
}

fn is_executable(p: &Path) -> bool {
    p.is_file()
        && p.file_name().map(|n| n == std::ffi::OsString::from(exe_name("np"))).unwrap_or(false)
        && p.metadata().map(|m| is_runnable(&m)).unwrap_or(false)
}

/// Whether the operating system would let this file run. Unix keeps that fact in the mode bits;
/// elsewhere there is no such bit to read, so a file that exists under the right name is what
/// "runnable" means — which is why `NP_PATH` gets checked by hand rather than trusted.
fn is_runnable(metadata: &std::fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        true
    }
}

pub fn resolve_np(app_dir: &Path) -> Result<NpSource, NpMissing> {
    let bundled = app_dir.join(exe_name("np"));
    if is_executable(&bundled) {
        return Ok(NpSource::Bundled(bundled));
    }

    if let Some(override_) = std::env::var_os("NP_PATH") {
        let p = PathBuf::from(override_);
        // An override that is a directory is treated as the folder holding np, which is
        // what people who build from source reach for (`target/debug`).
        let candidate = if p.is_dir() { p.join(exe_name("np")) } else { p };
        return if is_executable(&candidate) {
            Ok(NpSource::Env(candidate))
        } else {
            Err(NpMissing::OverrideMissing(candidate))
        };
    }

    for dir in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
        let candidate = dir.join(exe_name("np"));
        if is_executable(&candidate) {
            return Ok(NpSource::Path(candidate));
        }
    }
    Err(NpMissing::NotFound)
}

fn version_of(missing: &NpMissing) -> &str {
    match missing {
        NpMissing::TooOld { found } => found,
        _ => "unknown",
    }
}

/// What to tell a user whose np is absent or old: the asset and the check for *their*
/// platform, never a generic "install np".
pub fn install_hint(missing: &NpMissing, os: Os) -> String {
    let release = "https://github.com/Ex-Bot-01/note-processor/releases/latest";
    let too_old = matches!(missing, NpMissing::TooOld { .. });
    match (os, too_old) {
        (Os::Windows, true) => format!(
            "np is too old for Jott. Download np-0.3.1-x86_64-pc-windows-gnu-cross.zip from {release}, \
             compare its hash against SHA256SUMS.txt, and replace np.exe where it is installed."
        ),
        (Os::Windows, false) => format!(
            "Jott needs the np command (0.3.1 or newer). Download np-0.3.1-x86_64-pc-windows-gnu-cross.zip \
             from {release}, check it with `(Get-FileHash .\\np-0.3.1-x86_64-pc-windows-gnu-cross.zip).Hash.ToLower()` \
             against SHA256SUMS.txt, put np.exe somewhere on PATH — or set NP_PATH to it."
        ),
        (Os::Linux, true) => format!(
            "np {} is too old for Jott, which needs 0.3.1 or newer. Get the current asset from \
             {release}, verify it with `sha256sum -c SHA256SUMS.txt`, and install it with \
             `install -m 0755 np ~/.local/bin/np`.", version_of(missing)
        ),
        (Os::Linux, false) => format!(
            "Jott needs the np command (0.3.1 or newer). Download np-0.3.1-x86_64-unknown-linux-gnu.tar.gz \
             from {release}, verify it with `sha256sum -c SHA256SUMS.txt`, then \
             `install -m 0755 np ~/.local/bin/np` — or set NP_PATH to a build you already have."
        ),
        (Os::Macos, _) => format!(
            "There is no macOS asset for np: it has to be built from source with \
             `cargo install --git https://github.com/Ex-Bot-01/note-processor -p np-cli --locked`, then set \
             NP_PATH to the binary if it is not on PATH."
        ),
    }
}
