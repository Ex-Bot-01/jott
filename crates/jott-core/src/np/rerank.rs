//! The ranking model's bytes (np 0.7.0, F51): getting them onto disk for one workspace.
//!
//! `np serve` has no method that fetches or installs anything, so both doors are np's command line, run as a
//! one-shot child: `np models fetch` (np downloads and verifies), and `np models install` for a file the person
//! downloaded themselves — np copies it and checks it against the digest before it accepts it. Jott never writes
//! into np's models directory itself.
//!
//! A search never fetches the reranker (np's own rule), which is why this exists at all: the window asks for it once
//! when a folder opens with reranking switched on.

use super::client::np_command;
use super::handshake::NpInfo;
use std::path::Path;

/// The id np 0.7.0 pins for its reranker. Also the name np's `models.list` answers it under.
pub const RERANKER_ID: &str = "bge-reranker-v2-m3";

/// What a person needs to download the reranker by hand when np cannot reach its origin — behind a TLS-inspecting
/// proxy, for instance, whose certificate np's downloader does not trust. These are np 0.7.0's own pin values
/// (`np-core/src/pins.rs` at `v0.7.0`, re-hashed here against the published file); bump them with
/// `packaging/np-pin.json` if a later np pins different bytes. np checks the digest again on install, so a stale
/// value here can only make an install refuse — never make it accept the wrong file.
pub struct ManualDownload {
    pub url: &'static str,
    pub file_name: &'static str,
    pub sha256: &'static str,
    pub bytes: u64,
}

pub const MANUAL_DOWNLOAD: ManualDownload = ManualDownload {
    url: "https://huggingface.co/smarttasks/bge-reranker-v2-m3-GGUF/resolve/main/bge-reranker-v2-m3-Q8_0.gguf",
    file_name: "bge-reranker-v2-m3-Q8_0.gguf",
    sha256: "d32a877b7266f5275172458616824ce6728502416672052c2da795da3389201e",
    bytes: 635_674_432,
};

/// The first np with a rerank stage.
pub const MIN_RERANK_NP: (u64, u64, u64) = (0, 7, 0);

/// Can this np rerank at all? It needs the stage (0.7.0+) and the in-process runtime (`local_model`): a build without
/// it accepts the flag and refuses every reranked search.
pub fn supports_rerank(info: &NpInfo) -> bool {
    info.features.local_model && version_at_least(&info.version, MIN_RERANK_NP)
}

fn version_at_least(version: &str, min: (u64, u64, u64)) -> bool {
    let mut nums = [0u64; 3];
    for (slot, raw) in nums.iter_mut().zip(version.split('.')) {
        let digits: String = raw.trim().chars().take_while(|c| c.is_ascii_digit()).collect();
        match digits.parse() {
            Ok(n) => *slot = n,
            Err(_) => return false,
        }
    }
    nums >= [min.0, min.1, min.2]
}

/// Is this np fault the rerank stage refusing an explicit request (artifact missing, unverified, runtime absent)?
/// np words all of them with one opening, measured on 0.7.0; the search itself was never run.
pub fn is_rerank_refusal(code: i32, message: &str) -> bool {
    code == -32000 && message.starts_with("rerank was requested and could not run")
}

/// `np models fetch <workspace> --id bge-reranker-v2-m3`. `discard_marker` clears a marker a previous fetch left
/// behind — only ever passed when Jott knows that fetch was its own and is no longer running.
///
/// The child is killed if this future is dropped, which is how closing a folder stops a download.
pub async fn fetch(np: &Path, workspace: &Path, discard_marker: bool) -> Result<(), String> {
    let mut cmd = np_command(np);
    cmd.args(["models", "fetch"]).arg(workspace).args(["--id", RERANKER_ID, "--json"]);
    if discard_marker {
        cmd.arg("--discard-marker");
    }
    run(cmd, np).await
}

/// `np models install <workspace> --id bge-reranker-v2-m3 --path <file> --sha256 <pin>`: np copies the file and
/// refuses it unless it hashes to the pin.
pub async fn install(np: &Path, workspace: &Path, file: &Path) -> Result<(), String> {
    let mut cmd = np_command(np);
    cmd.args(["models", "install"])
        .arg(workspace)
        .args(["--id", RERANKER_ID, "--path"])
        .arg(file)
        .args(["--sha256", MANUAL_DOWNLOAD.sha256, "--json"]);
    run(cmd, np).await
}

async fn run(mut cmd: tokio::process::Command, np: &Path) -> Result<(), String> {
    let output = cmd
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|e| format!("could not start {}: {e}", np.display()))?;
    if output.status.success() {
        return Ok(());
    }
    // np's own sentence, which is the useful part (§4). Its stdout is the JSON outcome when there is one.
    let said = String::from_utf8_lossy(&output.stderr);
    let said = said.trim();
    let said = if said.is_empty() { String::from_utf8_lossy(&output.stdout).trim().to_string() } else { said.to_string() };
    Err(if said.is_empty() {
        format!("note-processor exited with {} and said nothing", output.status)
    } else {
        said.strip_prefix("np: ").unwrap_or(&said).to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stage_needs_0_7_0_numerically() {
        assert!(version_at_least("0.7.0", MIN_RERANK_NP));
        assert!(version_at_least("0.10.0", MIN_RERANK_NP));
        assert!(version_at_least("1.0.0-rc.1", MIN_RERANK_NP));
        assert!(!version_at_least("0.6.1", MIN_RERANK_NP));
        assert!(!version_at_least("garbage", MIN_RERANK_NP));
    }

    #[test]
    fn only_the_rerank_refusal_is_read_as_one() {
        let said = "rerank was requested and could not run: the reranker `bge-reranker-v2-m3` is not installed: run …";
        assert!(is_rerank_refusal(-32000, said));
        assert!(!is_rerank_refusal(-32602, said));
        assert!(!is_rerank_refusal(-32000, "workspace /x is not a directory that exists"));
    }
}
