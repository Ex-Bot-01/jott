//! `vaults.json` — which folders are Jott's, which one was last, and what each was told about
//! downloading a model.
//!
//! Two rules shape this file. It holds the only absolute paths the app keeps (§12), so it is the
//! one place that has to be right about a folder; and losing it must cost someone *nothing but
//! their recents list* — an unreadable registry is quarantined and rebuilt, because a note app
//! that refuses to open is worse than one that forgot which folder was last (§4).
//!
//! Consent is stored per vault, not globally: the model artifact is per workspace (§5), so the
//! answer someone gave about `~/Notes` says nothing about `~/Work`.

use super::looks_like_np_vault;
use crate::app_paths::{app_data_dir, AppPathsError};
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

/// What this vault's owner said about fetching the embedding model. `Undecided` is a state, not a
/// default that quietly becomes yes: nothing downloads until it says otherwise (§11).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Consent {
    Undecided,
    Granted,
    Declined,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultEntry {
    pub path: PathBuf,
    /// When this vault was last opened or picked. The registry's order is the recents list, so a
    /// re-open moves the entry rather than leaving it where it first appeared.
    pub last_opened: SystemTime,
    pub consent: Consent,
}

/// Something worth saying about a registration that still happened. Never a refusal: picking a
/// folder inside another vault is legal, and §8.2 asks for a warning the user may accept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegisterWarning {
    /// The parent at `parent` already holds np's `INDEX.md`, so indexing this folder as its own
    /// workspace means a second index — and, when embedding is set up, a second model download.
    NestedVault { parent: PathBuf },
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("{path} is not a folder I can open")]
    NotADirectory { path: PathBuf },
    #[error("{path} is not a registered vault, so there is nothing here to record its consent in")]
    NotRegistered { path: PathBuf },
    #[error("could not write {}: {source}", .path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error(transparent)]
    Paths(#[from] AppPathsError),
}

/// The file as it sits on disk. `version` is here so a later format can tell this one apart
/// instead of guessing from which keys happen to be present.
#[derive(Debug, Serialize, Deserialize)]
struct Registry {
    version: u32,
    vaults: Vec<StoredEntry>,
}

/// Paths are stored as text, which is what a JSON file can hold. A folder whose name is not valid
/// UTF-8 comes back lossily — §16 leaves this format to the builder, and a native folder picker
/// is not what produces such names.
#[derive(Debug, Serialize, Deserialize)]
struct StoredEntry {
    path: String,
    last_opened_ms: u64,
    consent: Consent,
}

pub struct VaultStore(PathBuf);

impl VaultStore {
    /// Rooted at a directory chosen by the caller — the app data dir in the app, a temp dir in a
    /// test. Nothing is created until something has to be written.
    pub fn new(root: PathBuf) -> VaultStore {
        VaultStore(root)
    }

    /// Rooted where the platform says app data lives. Startup uses this and shows what it says on
    /// an error: a machine with neither `XDG_DATA_HOME` nor `HOME` is a machine worth telling.
    pub fn in_app_data_dir() -> Result<VaultStore, StoreError> {
        Ok(VaultStore::new(app_data_dir()?))
    }

    pub fn root(&self) -> &Path {
        &self.0
    }

    fn file(&self) -> PathBuf {
        self.0.join("vaults.json")
    }

    /// The registry, or an empty list. A file that will not parse is renamed out of the way and
    /// kept, so the cause is still there tomorrow and startup is not blocked by it today.
    pub fn load(&self) -> Vec<VaultEntry> {
        let text = match std::fs::read_to_string(self.file()) {
            Ok(text) => text,
            // No file yet is the ordinary state of a first launch.
            Err(_) => return Vec::new(),
        };
        match serde_json::from_str::<Registry>(&text) {
            Ok(registry) => registry.vaults.into_iter().map(VaultStore::to_entry).collect(),
            Err(_) => {
                self.quarantine();
                Vec::new()
            }
        }
    }

    /// Add this folder, or move it to the front of the recents list if it is already there.
    ///
    /// A re-registration keeps the consent decision: someone opening a vault again has not
    /// un-answered a question about their bandwidth.
    pub fn register(&self, path: &Path) -> Result<Option<RegisterWarning>, StoreError> {
        let path = to_abs(path);
        if !path.is_dir() {
            return Err(StoreError::NotADirectory { path });
        }
        let warning = nested_np_vault(&path).map(|parent| RegisterWarning::NestedVault { parent });

        let mut entries: Vec<VaultEntry> = self
            .load()
            .into_iter()
            .filter(|e| to_abs(&e.path) != path)
            .collect();
        let consent = self.consent(&path);
        entries.push(VaultEntry { path, last_opened: SystemTime::now(), consent });
        self.save(&entries)?;
        Ok(warning)
    }

    /// Record what this vault's owner decided. Refuses a folder that is not registered: an answer
    /// written where nobody will read it is how a download happens without consent (§11).
    pub fn set_consent(&self, path: &Path, consent: Consent) -> Result<(), StoreError> {
        let path = to_abs(path);
        let mut entries = self.load();
        let mut found = false;
        for entry in entries.iter_mut() {
            if to_abs(&entry.path) == path {
                entry.consent = consent;
                found = true;
            }
        }
        if !found {
            return Err(StoreError::NotRegistered { path });
        }
        self.save(&entries)
    }

    /// The vault to open when the app starts. Ties go to whichever was written last, which is the
    /// one that was opened last: a millisecond clock is not always finer than two clicks.
    pub fn last_opened(&self) -> Option<PathBuf> {
        self.load()
            .into_iter()
            .enumerate()
            .max_by_key(|(i, e)| (e.last_opened, *i))
            .map(|(_, e)| e.path)
    }

    /// What was decided about embedding for this vault (§8). `Undecided` for a folder never opened here.
    pub fn consent(&self, path: &Path) -> Consent {
        self.load()
            .into_iter()
            .find(|e| to_abs(&e.path) == path)
            .map(|e| e.consent)
            .unwrap_or(Consent::Undecided)
    }

    /// Write the whole registry: a temp file in the same directory, then one rename. A crash
    /// leaves either the old file or the new one, never half of either.
    fn save(&self, entries: &[VaultEntry]) -> Result<(), StoreError> {
        let dir = self.file().parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
        std::fs::create_dir_all(&dir).map_err(|source| StoreError::Io { path: dir.clone(), source })?;
        let registry = Registry {
            version: 1,
            vaults: entries.iter().map(VaultStore::to_stored).collect(),
        };
        let text = serde_json::to_string_pretty(&registry)
            .expect("a registry of strings and numbers serializes");
        let temp = dir.join(format!("vaults.json.tmp-{}", std::process::id()));
        std::fs::write(&temp, text).map_err(|source| StoreError::Io { path: temp.clone(), source })?;
        std::fs::rename(&temp, self.file()).map_err(|source| StoreError::Io { path: self.file(), source })
    }

    fn to_stored(entry: &VaultEntry) -> StoredEntry {
        StoredEntry {
            path: entry.path.to_string_lossy().into_owned(),
            last_opened_ms: entry
                .last_opened
                .duration_since(UNIX_EPOCH)
                .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
                .unwrap_or(0),
            consent: entry.consent,
        }
    }

    fn to_entry(stored: StoredEntry) -> VaultEntry {
        VaultEntry {
            path: PathBuf::from(stored.path),
            last_opened: UNIX_EPOCH + std::time::Duration::from_millis(stored.last_opened_ms),
            consent: stored.consent,
        }
    }

    /// `vaults.json.corrupt-<unix>` — kept rather than deleted, because "when did this stop
    /// working" is the first question and an overwritten file cannot answer it.
    fn quarantine(&self) {
        let file = self.file();
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let name = format!("vaults.json.corrupt-{stamp}");
        let _ = std::fs::rename(&file, file.with_file_name(name));
    }
}

/// The nearest ancestor that is already an indexed vault, if any. The folder itself is not asked:
/// registering a vault that has been indexed is the normal case, not a warning.
fn nested_np_vault(path: &Path) -> Option<PathBuf> {
    path.ancestors()
        .skip(1)
        .find(|dir| looks_like_np_vault(dir))
        .map(Path::to_path_buf)
}

/// Absolute, without `.` segments or trailing separators, resolved lexically and with no syscall.
///
/// Deliberately not `std::fs::canonicalize`: on Windows that returns a `\\?\` verbatim path, and
/// np keys a workspace by the absolute path it was given (§18.6) — two spellings of one folder
/// would mean two indexes and two model downloads for one vault. Following symlinks here would
/// have the same effect through a different door.
/// The path as the registry understands it: absolute, lexically cleaned, with any trailing separator gone.
/// Public so the shell can ask "is this the vault that is already open?" with the same ruler the recents
/// list uses, rather than inventing a second notion of sameness.
pub fn to_abs(path: &Path) -> PathBuf {
    let mut out = if path.is_absolute() {
        PathBuf::new()
    } else {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    };
    for component in path.components() {
        match component {
            Component::CurDir => {}
            // Resolved lexically: `..` means "the folder above as written", which is what the
            // user typed, and it keeps symlinks from silently moving a vault somewhere else.
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}
