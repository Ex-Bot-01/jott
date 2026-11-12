//! Per-file stamps: what np last proved about each file, so the tree can say "waiting" without
//! asking np again.
//!
//! np reports aggregates — `added 3, unchanged 12` — and names paths only for what it skipped
//! (§18.2, §18.5). The marker a note carries in §9 is therefore *our* record, and it is kept honest
//! by one rule: a stamp is about **bytes**. It stores the hash np saw, so edited bytes are never
//! shown as embedded, and a file np refused stays refused until np says otherwise.
//!
//! State lives in the app data dir, one directory per vault keyed by the absolute path the way np
//! keys a workspace (§18.6). Losing this file costs markers, not notes — nothing here is the only
//! copy of anything — so an unreadable state file is forgotten rather than quarantined.

use crate::app_paths::app_data_dir;
use crate::hashes::sha256_prefix;
use crate::vault::store::{to_abs, StoreError};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// What the tree marks a note with (§9). `Refused` carries a reason because §9 promises one — and
/// only ever a reason somebody actually reported: ours from the walk, or np's token from a scan.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum StampState {
    /// These bytes have vectors.
    Embedded,
    /// These bytes are in the index and owe embedding (§11's offline case).
    Indexed,
    /// Never seen, or changed since whatever np said.
    Waiting,
    Refused(String),
}

/// One file's record: the hash each verdict was about, and np's refusal token if that is where it
/// stopped. Every field is optional because "np has not told us" is a real state, and writing a
/// plausible value in its place is how a tree starts lying.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileStamp {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    indexed: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    embedded: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    refused: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct StateFile {
    version: u32,
    /// Keyed by the tree's `rel` — forward slashes on every platform, which is np's spelling too.
    files: BTreeMap<String, FileStamp>,
}

impl Default for StateFile {
    fn default() -> Self {
        StateFile { version: 1, files: BTreeMap::new() }
    }
}

#[derive(Clone)]
pub struct Stamps {
    vault: PathBuf,
    file: PathBuf,
    files: BTreeMap<String, FileStamp>,
}

impl Stamps {
    /// Load the stamps for this vault from the app data dir.
    pub fn load(vault: &Path) -> Result<Stamps, StoreError> {
        Ok(Self::with_root(app_data_dir()?, vault))
    }

    /// The same, with the state kept under a root the caller chose. This is the seam for tests, and
    /// the one place a portable build would point state beside itself instead of the home folder.
    pub fn with_root(root: PathBuf, vault: &Path) -> Stamps {
        let key = workspace_key(vault);
        let file = root.join("state").join(key).join("state.json");
        let files = std::fs::read_to_string(&file)
            .ok()
            .and_then(|text| serde_json::from_str::<StateFile>(&text).ok())
            .map(|state| state.files)
            .unwrap_or_default();
        Stamps { vault: to_abs(vault), file, files }
    }

    pub fn vault(&self) -> &Path {
        &self.vault
    }

    /// What to mark this file with, given the bytes it holds now.
    pub fn state_of(&self, rel: &str, current_hash: &str) -> StampState {
        let Some(stamp) = self.files.get(rel) else { return StampState::Waiting };
        if stamp.embedded.as_deref() == Some(current_hash) {
            StampState::Embedded
        } else if stamp.indexed.as_deref() == Some(current_hash) {
            StampState::Indexed
        } else if let Some(reason) = &stamp.refused {
            StampState::Refused(reason.clone())
        } else {
            StampState::Waiting
        }
    }

    /// np indexed these bytes. Whatever it said before about them is history, including a refusal:
    /// a scan that read the file and did not skip it is the evidence that ends one.
    pub fn mark_indexed(&mut self, rel: &str, hash: &str) {
        let stamp = self.files.entry(rel.to_string()).or_default();
        stamp.indexed = Some(hash.to_string());
        stamp.refused = None;
    }

    /// np embedded these bytes. The index verdict comes with it, because embedding reads the index.
    pub fn mark_embedded(&mut self, rel: &str, hash: &str) {
        let stamp = self.files.entry(rel.to_string()).or_default();
        stamp.indexed = Some(hash.to_string());
        stamp.embedded = Some(hash.to_string());
        stamp.refused = None;
    }

    /// np refused these bytes, in its words (`over-budget`, `identity-taken`). The reason is kept
    /// verbatim and never improved on: §18.5 says the marker reads *refused* with no reason rather
    /// than inventing one when np did not give one.
    pub fn mark_refused(&mut self, rel: &str, reason: &str) {
        self.files.entry(rel.to_string()).or_default().refused = Some(reason.to_string());
    }

    /// Write the whole file: temp in the same directory, then one rename, so a crash leaves either
    /// the old record or the new one.
    pub fn save(&self) -> Result<(), StoreError> {
        let dir = self.file.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
        std::fs::create_dir_all(&dir).map_err(|source| StoreError::Io { path: dir.clone(), source })?;
        let text = serde_json::to_string_pretty(&StateFile { version: 1, files: self.files.clone() })
            .expect("a map of strings serializes");
        let temp = dir.join(format!("state.json.tmp-{}", std::process::id()));
        std::fs::write(&temp, text).map_err(|source| StoreError::Io { path: temp.clone(), source })?;
        std::fs::rename(&temp, &self.file)
            .map_err(|source| StoreError::Io { path: self.file.clone(), source })
    }
}

/// The directory name for one vault: the first 16 hex of its absolute path's digest, which is how
/// np names a workspace's own files (§18.6). Same derivation, so "which folder is this" has one
/// answer in two programs.
fn workspace_key(vault: &Path) -> String {
    let abs = to_abs(vault);
    sha256_prefix(abs.to_string_lossy().as_bytes(), 16)
}
