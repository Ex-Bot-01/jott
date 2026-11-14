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
/// The provider np's config names, when it names one. Kept separate from `settings_gap` because the consent card
/// quotes this back verbatim — a person should see their own word for it, not Jott's category.
pub fn embedding_provider_named(path: &Path) -> Option<String> {
    let document = load(path).ok()??;
    document.get("embeddings")?.get("provider")?.as_str().map(str::to_string)
}

pub fn embedding_model_named(path: &Path) -> Option<String> {
    let document = load(path).ok()??;
    let embeddings = document.get("embeddings")?;
    embeddings.get("provider")?.as_str()?;
    embeddings.get("model")?.as_str().map(str::to_string)
}

/// A model np ships a pin for and will fetch without being told to: `np serve` with no config at all answered
/// a plain `search` by downloading this one, unprompted, which is how Jott learned the numbers.
///
/// This is **wording, not authority**. §12's prompt has to name a model and a size before anything is fetched,
/// and a fresh np can name neither until it has fetched something — `models.list` answers empty when nothing is
/// configured and nothing is on disk. Whenever np itself reports a model, np's answer is used instead. If a
/// future np changes its built-in default, the worst this stale copy can do is put an old sentence in a dialog
/// that also says which folder the bytes would land in.
pub struct DefaultModel {
    pub id: &'static str,
    pub bytes: Option<u64>,
    pub sha256: Option<&'static str>,
}

/// Measured on 2026-11-12 against the shipped np 0.3.1 binary: the fetch reported 309,855,456 bytes for its
/// `main` member, and the `models.list` after it reported this id, this digest, and dim 768.
pub const fn builtin_default_model() -> DefaultModel {
    DefaultModel {
        id: "embeddinggemma-2",
        bytes: Some(309_855_456),
        sha256: Some("2188ac1deca4b77dffefd603c2776a9d76d9d74ec01841392982ebb840b09135"),
    }
}

/// The size to show for a model, preferring anything np actually reported.
pub fn default_bytes_for(model_id: &str) -> Option<u64> {
    let default = builtin_default_model();
    (model_id == default.id).then_some(default.bytes).flatten()
}

/// Write `[models] auto_fetch`, leaving the rest of the file as it was — comments, other sections,
/// unknown keys, blank lines and the spacing around this very key.
/// Which setting stands between "consent given" and vectors existing, read out of np's own config.
///
/// This exists because a tester clicked **Allow download**, and the window's only report was np's true and
/// unactionable sentence `keyword-only: embeddings.provider is not set`. The four states below are the four config
/// shapes measured against np 0.4.0 — including the nastiest, where an absent `[embeddings]` makes `embed` *succeed*
/// while embedding nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsGap {
    /// No `[embeddings] provider` at all — np reads that as `provider = "none"` and embeds nothing, happily.
    ProviderMissing,
    /// Provider is `local-model`, but no model is named, so np faults rather than guesses.
    ModelMissing,
    /// Someone chose a provider deliberately. Jott has no advice about it: the separation ruling says we ask np for
    /// a method (note-processor#7) instead of editing a decision that is not ours.
    ProviderOther { provider: String },
    /// The file exists and will not parse — the state a person walks into by editing it themselves, which np answers
    /// with its own `config parse error in <path>: TOML parse error at line …`. Nothing about this file can be quoted
    /// back as a setting, so nothing is: no lines to paste, no claim about what `auto_fetch` says.
    ConfigUnreadable,
    /// Nothing missing: consent and configuration agree.
    Ready,
}

/// What np's config is short of.
///
/// An absent file is `ProviderMissing` — there is nothing to read, and np's defaults are a readable state. A file that
/// exists and will not parse is `ConfigUnreadable`, which used to answer `ProviderMissing` as well; that conflation
/// told someone with a half-edited `config.toml` that three lines were missing from it, when the one true sentence was
/// that nobody could read the file. np says so itself, in those words, and Jott must not contradict it.
///
/// `quoted_model` is unused here on purpose: whether the *artifact* is present is `models.list`'s question, and
/// asking a TOML file it cannot answer is how a consent prompt ends up promising a download somebody already has.
pub fn settings_gap(np_config: Option<&Path>, _quoted_model: Option<&str>) -> SettingsGap {
    let Some(path) = np_config else { return SettingsGap::ProviderMissing };
    if consent_file_is_unreadable(path) {
        return SettingsGap::ConfigUnreadable;
    }
    let Ok(Some(document)) = load(path) else { return SettingsGap::ProviderMissing };
    let provider = document.get("embeddings").and_then(|e| e.get("provider")).and_then(|v| v.as_str());
    match provider {
        None => SettingsGap::ProviderMissing,
        Some("local-model") if embedding_model_named(path).is_some() => SettingsGap::Ready,
        Some("local-model") => SettingsGap::ModelMissing,
        Some(other) => SettingsGap::ProviderOther { provider: other.to_string() },
    }
}

/// The lines that would close a gap, in the order they belong in the file.
///
/// Empty for `ProviderOther` and `Ready`: Jott does not tell someone to overwrite a provider they chose, and it has
/// no business writing np's config itself while np#7 is open — these lines are for a person to paste deliberately.
pub fn gap_lines(gap: &SettingsGap, quoted_model: Option<&str>) -> Vec<String> {
    // A fresh np names no model in its config and still has a built-in pin, so "no id quoted" gets that id rather
    // than a placeholder nobody can paste into a file.
    let model = quoted_model.unwrap_or_else(|| builtin_default_model().id);
    match gap {
        SettingsGap::ProviderMissing => vec![
            "[embeddings]".to_string(),
            "provider = \"local-model\"".to_string(),
            format!("model = \"{model}\""),
        ],
        // The section and the provider already exist; repeating them would either break the file or "fix" something
        // that is not wrong.
        SettingsGap::ModelMissing => vec![format!("model = \"{model}\"")],
        // Nothing to paste: not over a deliberate choice, not once the settings agree, and not into a file that does
        // not parse — those lines would land somewhere np has already refused to read.
        SettingsGap::ProviderOther { .. } | SettingsGap::Ready | SettingsGap::ConfigUnreadable => Vec::new(),
    }
}

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
