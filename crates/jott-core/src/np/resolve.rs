//! Where the `np` binary comes from, and what to say when there isn't one.
//!
//! Order (amended when installers started carrying np inside them): `$NP_PATH` → the bundled
//! binary → `$PATH`. An explicit override comes first because it is a decision someone made, while the
//! bundled binary is a file we shipped — and a variable that loses to a shipped file is a variable that
//! does nothing. A set-but-wrong `NP_PATH` is still an error of its own rather than a shrug and a
//! fallthrough: someone named a binary, and quietly using a different one would index their vault with
//! something they did not choose.

use std::path::{Path, PathBuf};
use thiserror::Error;

/// The lowest np this app speaks to. Protocol 1 arrived in 0.3.0; `progress` notifications
/// and `status.get` — both of which the ingest queue uses — arrived in 0.3.1.
///
/// 0.5.1 is the floor, because two things Jott used to work around became np's own job: `embeddings.enable` and
/// `embeddings.disable` record consent over the pipe instead of a person pasting settings (note-processor#7), and
/// `{"prefix": true}` lets a search reach *grandmother* from `grand` while saying which words it widened (#6).
/// 0.5.1 rather than 0.5.0 for one reason of its own: the provenance comment written into someone's `config.toml`
/// names the method over `np serve` instead of a terminal command nobody ran (#10), and Jott quotes that line as an
/// audit trail — a record that invents a command is worse than no record. Jott still *reads* an older answer, every
/// new key being optional on the wire, but a backend that cannot take the answer itself is not one to ship.
pub const MIN_NP_VERSION: (u64, u64, u64) = (0, 5, 1);

/// The np release Jott puts inside its installer, and the one an older install is pointed at. CI downloads the one
/// named in `packaging/np-pin.json`; `the_hint_names_the_np_the_installer_ships` fails if the two disagree, because
/// a hint that sends someone to fetch a build Jott then refuses is worse than no hint.
///
/// 0.6.0 (note-processor#12, #13, #14, #15), checked against the published Linux binary rather than against its
/// release page: protocol still `1`, the same ten methods in the handshake, and an existing `.np/index.db` opened by
/// the new build with every note `unchanged` — no re-scan, no re-embed. Two keys appear on a `scan` answer
/// (`attachments`, `attachment_examples`) and Jott ignores both: `ScanReport` has no `deny_unknown_fields`, so an
/// added key is silence rather than a fault, which is the direction of tolerance issue #11 wanted kept. The floor
/// stays 0.5.1 on purpose — nothing here needs 0.6.0, and raising it would refuse a person whose np is fine.
pub const NP_RELEASE_VERSION: &str = "0.6.0";

/// The floor as a sentence fragment: `0.5.1`. Spelled once so a refusal and an install hint cannot quote different
/// numbers at the same person.
pub fn min_version_text() -> String {
    let (major, minor, patch) = MIN_NP_VERSION;
    format!("{major}.{minor}.{patch}")
}

/// The exact filename of this release's asset for a platform — what appears on the release page and in
/// `SHA256SUMS.txt`. macOS has none: np is built from source there.
pub fn asset_for(os: Os) -> Option<String> {
    match os {
        Os::Windows => Some(format!("np-{NP_RELEASE_VERSION}-x86_64-pc-windows-gnu-cross.zip")),
        Os::Linux => Some(format!("np-{NP_RELEASE_VERSION}-x86_64-unknown-linux-gnu.tar.gz")),
        Os::Macos => None,
    }
}

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
    #[error("np was not found (NP_PATH unset, nothing bundled, nothing on PATH)")]
    NotFound,
    #[error("NP_PATH points at {0}, which is not an executable file")]
    OverrideMissing(PathBuf),
    #[error("np {found} is too old; Jott needs {} or newer", min_version_text())]
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

    // Then what the installer put beside the app: a version whose protocol pair is known to work with
    // this build, which beats whatever else happens to be on PATH.
    let bundled = app_dir.join(exe_name("np"));
    if is_executable(&bundled) {
        return Ok(NpSource::Bundled(bundled));
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
    let min = min_version_text();
    match (os, too_old) {
        (Os::Windows, true) => format!(
            "np is too old for Jott. Download {} from {release}, \
             compare its hash against SHA256SUMS.txt, and replace np.exe where it is installed.",
            asset_for(os).expect("windows has an asset")
        ),
        (Os::Windows, false) => format!(
            "Jott needs the np command ({min} or newer). Download {} from {release}, \
             check it with `(Get-FileHash .\\{}).Hash.ToLower()` against SHA256SUMS.txt, \
             put np.exe somewhere on PATH — or set NP_PATH to it.",
            asset_for(os).expect("windows has an asset"),
            asset_for(os).expect("windows has an asset")
        ),
        (Os::Linux, true) => format!(
            "np {} is too old for Jott, which needs {min} or newer. Download {} from \
             {release}, verify it with `sha256sum -c SHA256SUMS.txt`, and install it with \
             `install -m 0755 np ~/.local/bin/np`.",
            version_of(missing),
            asset_for(os).expect("linux has an asset")
        ),
        (Os::Linux, false) => format!(
            "Jott needs the np command ({min} or newer). Download {} from {release}, \
             verify it with `sha256sum -c SHA256SUMS.txt`, then \
             `install -m 0755 np ~/.local/bin/np` — or set NP_PATH to a build you already have.",
            asset_for(os).expect("linux has an asset")
        ),
        // No `format!`: nothing is interpolated, and a literal states that in one character.
        (Os::Macos, _) => "There is no macOS asset for np: it has to be built from source with \
             `cargo install --git https://github.com/Ex-Bot-01/note-processor -p np-cli --locked`, then set \
             NP_PATH to the binary if it is not on PATH."
            .to_string(),
    }
}
