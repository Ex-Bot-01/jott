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
use crate::np::client::NpClient;
use crate::np::methods::{embeddings_disable, embeddings_enable};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConsentError {
    #[error("could not read {}: {source}", .path.display())]
    Read { path: PathBuf, source: std::io::Error },
    #[error("could not write {}: {source}", .path.display())]
    Write { path: PathBuf, source: std::io::Error },
    #[error("{} is not TOML I can edit: {message}", .path.display())]
    Parse { path: PathBuf, message: String },
    #[error("{} holds something where `[{table}]` should be a table", .path.display())]
    NotATable { path: PathBuf, table: &'static str },
    #[error("`{key}` must be between {min} and {max}, not {value}")]
    OutOfRange { key: &'static str, value: i64, min: i64, max: i64 },
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

/// What happened to Jott's own key, `models.auto_fetch`. Four states because there are four honest things to say:
/// it was written, it was deliberately left alone, there is no file to write to, and the write failed. Collapsing
/// "failed" into "written" would tell somebody their guardrail is down when it is not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutoFetch {
    Written,
    /// Consent was refused, so Jott touched nothing: a guardrail off behind a door that will not open is the worst
    /// combination available, and it is invisible unless somebody says so.
    Untouched,
    NoConfigPath,
    Failed { sentence: String },
}

/// The receipt for one consent answer — np's words where np had any, and the truth about Jott's own key.
///
/// Nothing here is inferred from what Jott intended: `changed`, `provider`, `model` and `persisted_to` are exactly
/// what np answered, and `np_refused` carries its fault sentence unchanged so a card can quote it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsentReceipt {
    pub granted: bool,
    /// np's own sentence when it refused to open or close its door. `None` means np did what was asked.
    pub np_refused: Option<String>,
    /// Which settings np moved itself, in the order it named them. Empty after an accepted **yes** means it had
    /// already been done — enabling what is enabled writes no bytes.
    pub changed: Vec<String>,
    pub provider: Option<String>,
    /// The model np kept in mind. A decline keeps naming it: revoking permission to download is not a licence to
    /// erase which model somebody chose.
    pub model: Option<String>,
    /// The file np says it wrote. Absent means np did not say, and the card must speak without naming a path.
    pub persisted_to: Option<PathBuf>,
    /// What the fetch would cost, in bytes, from np — not from Jott's compiled-in copy of an old answer.
    pub fetch_bytes: Option<u64>,
    pub auto_fetch: AutoFetch,
}

/// The receipt when there was no np to ask at all — the child would not start, or died mid-answer. np's own words do
/// not exist, so this says that plainly rather than inventing a refusal it never heard.
///
/// Our key is still ours to write, and which way depends on the answer: a **no** needs no door, so the guardrail goes
/// down regardless; a **yes** has no door to open, so raising it would promise a download nothing can serve.
pub fn answer_without_np(config: Option<&Path>, granted: bool, sentence: String) -> ConsentReceipt {
    let auto_fetch = if granted {
        AutoFetch::Untouched
    } else {
        match config {
            None => AutoFetch::NoConfigPath,
            Some(path) => match set_auto_fetch(path, false) {
                Ok(()) => AutoFetch::Written,
                Err(why) => AutoFetch::Failed { sentence: why.to_string() },
            },
        }
    };
    ConsentReceipt {
        granted,
        np_refused: Some(sentence),
        changed: Vec::new(),
        provider: None,
        model: None,
        persisted_to: None,
        fetch_bytes: None,
        auto_fetch,
    }
}

/// Record a consent answer where it belongs: np opens its own door (`embeddings.enable`), np closes it
/// (`embeddings.disable`), and Jott writes the one key that is actually its business — `[models] auto_fetch`, the
/// guardrail against an unasked-for download (§11).
///
/// The order matters on a **yes**: np first, then our key. `auto_fetch = true` next to a provider that cannot fetch
/// is a promise nobody can keep, and if np refuses we leave the guardrail exactly where it was. On a **no** the order
/// flips in effect — our `false` is written even when np refuses to revoke, because the guardrail is the part that
/// protects the person's bandwidth and it is the one thing Jott may guarantee.
///
/// Jott never writes `embeddings.provider` or `embeddings.model`. That was the whole point of note-processor#7.
pub async fn answer_consent(
    client: &NpClient,
    config: Option<&Path>,
    granted: bool,
    model_id: &str,
) -> ConsentReceipt {
    let door = if granted { embeddings_enable(client, model_id).await } else { embeddings_disable(client).await };

    let (np_refused, changed, provider, model, persisted_to, fetch_bytes) = match door {
        Ok(answer) => (None, answer.changed, answer.provider, answer.model, answer.persisted_to, answer.fetch_bytes),
        Err(fault) => (Some(fault.message), Vec::new(), None, None, None, None),
    };

    // A refused *yes* writes nothing of ours. A *no* writes its guardrail whatever np said.
    let auto_fetch = if granted && np_refused.is_some() {
        AutoFetch::Untouched
    } else {
        match config {
            None => AutoFetch::NoConfigPath,
            Some(path) => match set_auto_fetch(path, granted) {
                Ok(()) => AutoFetch::Written,
                Err(why) => AutoFetch::Failed { sentence: why.to_string() },
            },
        }
    };

    ConsentReceipt { granted, np_refused, changed, provider, model, persisted_to, fetch_bytes, auto_fetch }
}

pub fn set_auto_fetch(path: &Path, allow: bool) -> Result<(), ConsentError> {
    set_key(
        path,
        "models",
        "auto_fetch",
        toml_edit::Value::from(allow),
        "# Added by Jott: whether note-processor may download the embedding model it needs.\n",
    )
}

/// The rerank stage's depth as np's config states it (`[search] rerank_top_n`, np 0.7.0+). `None` when the file or
/// the key is absent — np's own default (5) then applies — and also when the file cannot be read or holds something
/// that is not an integer: np refuses that value itself, naming the key, and Jott must not print a number over it.
pub fn rerank_top_n(path: &Path) -> Option<i64> {
    let document = load(path).ok()??;
    document.get("search")?.get("rerank_top_n")?.as_integer()
}

/// np's accepted range for `[search] rerank_top_n`. np refuses anything outside it rather than clamping, so Jott
/// refuses first: a value np would reject turns every reranked search into a refusal.
pub const RERANK_TOP_N_RANGE: std::ops::RangeInclusive<i64> = 1..=64;

/// Write `[search] rerank_top_n`. Out-of-range values are refused here, before the file is touched.
pub fn set_rerank_top_n(path: &Path, n: i64) -> Result<(), ConsentError> {
    if !RERANK_TOP_N_RANGE.contains(&n) {
        return Err(ConsentError::OutOfRange {
            key: "search.rerank_top_n",
            value: n,
            min: *RERANK_TOP_N_RANGE.start(),
            max: *RERANK_TOP_N_RANGE.end(),
        });
    }
    set_key(
        path,
        "search",
        "rerank_top_n",
        toml_edit::Value::from(n),
        "# Added by Jott: how many top search results note-processor's ranking model reorders.\n",
    )
}

/// Set one key in one table of np's config, keeping everything else in the file — formatting, comments, keys Jott
/// has never heard of — exactly as it was.
fn set_key(path: &Path, table: &'static str, key: &str, value: toml_edit::Value, header: &str) -> Result<(), ConsentError> {
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
                fresh.as_table_mut().decor_mut().set_prefix(header);
            }
            fresh
        }
    };

    if document.get(table).is_none() {
        document[table] = toml_edit::Item::Table(toml_edit::Table::new());
    }
    let Some(section) = document[table].as_table_like_mut() else {
        return Err(ConsentError::NotATable { path: path.to_path_buf(), table });
    };
    // The old value's decorations are copied onto the new one, because a trailing `# my own mirror`
    // is somebody's note to themselves and a key replacement would silently take it away.
    let decor = section.get(key).and_then(|item| item.as_value()).map(|value| value.decor().clone());
    section.insert(key, toml_edit::Item::Value(value));
    if let Some(decor) = decor {
        if let Some(value) = section.get_mut(key).and_then(|item| item.as_value_mut()) {
            *value.decor_mut() = decor;
        }
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| ConsentError::Write { path: path.to_path_buf(), source })?;
    }

    // Written beside the destination and renamed over it, as `VaultStore::save` and `Stamps::save` do it: a crash
    // leaves either the old file or the new one, never half of either (issue #4). This was the last whole-file
    // rewrite Jott did in place, and it is the one file here that nobody owns — somebody's mirror URL, their log
    // level, comments, keys Jott has never heard of, and since note-processor#7 the `embeddings.provider` and
    // `embeddings.model` that say what the person answered. A truncated `config.toml` makes np answer every
    // request with its own parse error, Jott reports `configUnreadable`, and the consent answer is gone with it.
    //
    // The cost of a rename is honest and stated: writing now needs permission to create a file in the folder, not
    // only to open the one that is there. A read-only folder therefore refuses the write instead of half-doing it,
    // which is the same bargain the registry and the stamp file already make.
    let name = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_else(|| "config.toml".into());
    let temp = path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(format!("{name}.tmp-{}-{}", std::process::id(), WRITE_TICK.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
    let written = std::fs::write(&temp, document.to_string()).map_err(|source| ConsentError::Write {
        path: path.to_path_buf(),
        source,
    });
    if let Err(error) = written {
        let _ = std::fs::remove_file(&temp);
        return Err(error);
    }
    // A rename moves the temporary into place with *its* permission bits, so the destination's own are copied
    // across first. `config.toml` can name a mirror somebody did not want read; Jott must not be the reason that
    // changes. Best effort: a filesystem without permission bits answers an error here, and that is no reason to
    // fail someone's consent answer.
    if let Ok(existing) = std::fs::metadata(path) {
        let _ = std::fs::set_permissions(&temp, existing.permissions());
    }
    std::fs::rename(&temp, path).map_err(|source| {
        // np indexes this folder, so a leftover temporary is the one thing this rewrite must not leave behind.
        let _ = std::fs::remove_file(&temp);
        ConsentError::Write { path: path.to_path_buf(), source }
    })
}

/// So two writes from one process — opening an undecided folder while a consent answer is in flight — never share
/// a temporary name. The pid separates processes; this separates the calls inside one.
static WRITE_TICK: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

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
