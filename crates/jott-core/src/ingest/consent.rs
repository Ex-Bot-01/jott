//! Consent, written where np will read it.
//!
//! §11 is specific about the mechanism: "Not now" sets `[models] auto_fetch = false` **in np's config
//! file**, not `NP_NO_AUTO_FETCH`, because env is frozen at spawn and the child is spawned before
//! consent exists — while np re-reads its config on every request (§18.6), so a changed key takes
//! effect with no respawn.
//!
//! That file is not ours otherwise. It may hold someone's mirror, their log level, comments explaining
//! why, and keys this app has never heard of; all of it has to come back unchanged, which is why the
//! edit goes through `toml_edit` (which keeps formatting) rather than a re-serialised structure
//! (which does not).

use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConsentError {
    #[error("could not read {}: {source}", .path.display())]
    Read { path: PathBuf, source: std::io::Error },
    #[error("could not write {}: {source}", .path.display())]
    Write { path: PathBuf, source: std::io::Error },
    #[error("{} is not TOML I can edit: {message}", .path.display())]
    Parse { path: PathBuf, message: String },
    #[error("{} holds something where `[models]` should be a table", .path.display())]
    NotATable { path: PathBuf },
}

/// Where np looks for its config — np's own rule (`np-core/src/config.rs:1124`), not Jott's:
/// `$XDG_CONFIG_HOME/note-processor/config.toml`, else `~/.config/note-processor/config.toml`,
/// Windows `%APPDATA%\note-processor\`. `None` only when the OS has told us neither, which is a
/// broken environment rather than a case to write into.
pub fn np_config_path() -> Option<PathBuf> {
    #[cfg(windows)]
    let root = std::env::var_os("APPDATA").map(PathBuf::from);
    #[cfg(not(windows))]
    let root = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")));
    root.map(|dir| dir.join("note-processor").join("config.toml"))
}

/// Whether np's config exists and cannot be read as TOML. Absent is readable (nothing to misread); a file
/// that fails to parse is not, and the difference decides whether the consent gate can trust what it saw.
pub fn consent_file_is_unreadable(path: &Path) -> bool {
    // `is_err`, not a match: unreadable is exactly "could not be loaded as TOML".
    load(path).is_err()
}

/// What the file says about fetching, with np's default when it says nothing.
///
/// A file that cannot be parsed answers `false`, and that is the survivable direction: `true` here
/// would let the app promise a download that the child — which reports its own config error in its own
/// words — then refuses. Absent key is np's `auto_fetch_default()`, measured as `true`.
pub fn auto_fetch_allowed(path: &Path) -> bool {
    match load(path) {
        // No file at all is still "np has never been told otherwise", so np's default answers.
        Ok(None) => true,
        Ok(Some(document)) => match document.get("models").and_then(|models| models.get("auto_fetch")) {
            None => true,
            Some(toml_edit::Item::Value(value)) => value.as_bool().unwrap_or(false),
            // `auto_fetch = "yes"` is not a boolean and np would say so itself; Jott does not decide
            // someone's download permission by guessing what they meant.
            Some(_) => false,
        },
        Err(_) => false,
    }
}

/// The model np is configured to embed with: `[embeddings] model`, and only when a provider is named —
/// np treats an absent `provider` as none (`EmbeddingProvider::None`), in which case there is nothing
/// to fetch whatever the model key says.
///
/// This is how the consent prompt gets a model id without asking np to try a download first (§11:
/// decide from what is configured and on disk, not by attempting).
pub fn embedding_model_named(path: &Path) -> Option<String> {
    let document = load(path).ok()??;
    let embeddings = document.get("embeddings")?;
    embeddings.get("provider")?.as_str()?;
    embeddings.get("model")?.as_str().map(str::to_string)
}

/// Write `[models] auto_fetch`, leaving the rest of the file as it was — comments, other sections,
/// unknown keys, blank lines and the spacing around this very key.
pub fn set_auto_fetch(path: &Path, allow: bool) -> Result<(), ConsentError> {
    let existed = path.exists();
    let mut document = match load(path)? {
        Some(document) => document,
        // A file that is not there yet gets the minimum that says what we mean, plus one line saying
        // who wrote it: §12's rule about not overwriting a config we did not write starts by making
        // that legible.
        None => {
            let mut fresh = toml_edit::DocumentMut::new();
            if !existed {
                // One line saying who wrote this file, because §12's rule about not overwriting a
                // config Jott did not write starts by making that fact legible to whoever reads it.
                fresh
                    .as_table_mut()
                    .decor_mut()
                    .set_prefix("# Added by Jott: whether note-processor may download the embedding model it needs.\n");
            }
            fresh
        }
    };

    if document.get("models").is_none() {
        document["models"] = toml_edit::Item::Table(toml_edit::Table::new());
    }
    let Some(models) = document["models"].as_table_like_mut() else {
        return Err(ConsentError::NotATable { path: path.to_path_buf() });
    };
    // The old value's decorations are copied onto the new one, because a trailing `# my own mirror`
    // is somebody's note to themselves and a key replacement would silently take it away.
    let decor = models.get("auto_fetch").and_then(|item| item.as_value()).map(|value| value.decor().clone());
    models.insert("auto_fetch", toml_edit::value(allow));
    if let Some(decor) = decor {
        if let Some(value) = models.get_mut("auto_fetch").and_then(|item| item.as_value_mut()) {
            *value.decor_mut() = decor;
        }
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| ConsentError::Write { path: path.to_path_buf(), source })?;
    }
    std::fs::write(path, document.to_string()).map_err(|source| ConsentError::Write { path: path.to_path_buf(), source })
}

/// `None` when the file does not exist yet — which is a state, not a failure. A parse error is one:
/// §4 says show np's problem rather than inventing an answer to it.
fn load(path: &Path) -> Result<Option<toml_edit::DocumentMut>, ConsentError> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => return Err(ConsentError::Read { path: path.to_path_buf(), source }),
    };
    text.parse::<toml_edit::DocumentMut>()
        .map(Some)
        .map_err(|e| ConsentError::Parse { path: path.to_path_buf(), message: e.to_string() })
}
