//! np's result documents as types — and the rule they exist to keep: **a state stays a state**.
//!
//! `locked`, `offline` and a configuration refusal arrive as *data in a result* (spec §5, and
//! F44 in np: a JSON-RPC fault would fire an app's "the backend refused" handler on something
//! it is meant to retry). So they get variants of their own here, and the only things that come
//! back as [`NpFault`] are np's own faults.
//!
//! Every struct mirrors one np document, key for key, with two rules: a field np sends as
//! `null` stays `None` rather than becoming a plausible zero (a semantic-only hit and a hit with
//! bm25 0.0 are different facts — spec §10 renders them differently), and keys this build has no
//! use for are ignored rather than guessed at. What is left out by design is named where it sits.

use super::NpFault;
use serde::de::Deserializer;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// How a search was answered: what we ask for, and what np says it did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Hybrid,
    Semantic,
    Keyword,
}

impl Mode {
    pub fn label(&self) -> &'static str {
        match self {
            Mode::Hybrid => "hybrid",
            Mode::Semantic => "semantic",
            Mode::Keyword => "keyword",
        }
    }
}

/// A result document that is not shaped like the method that sent it.
///
/// `-32603` is np's own internal-error code, borrowed here because the honest reading is the
/// same: the protocol broke somewhere past our control. The message names the missing key, so
/// a log line says what to go and look at rather than "unexpected response".
fn shape_fault(method: &str, what: impl std::fmt::Display) -> NpFault {
    NpFault {
        code: -32603,
        message: format!("np's {method} result is not shaped like one: {what}"),
    }
}

/// `locked` as np writes it: its own three-key document (`command`, `locked`, `waited_ms`) and
/// no counts at all. `Some(true)` means the request never started.
fn is_locked(value: &Value) -> bool {
    value.get("locked").and_then(Value::as_bool).unwrap_or(false)
}

/// The wait np measured, or a fault: "locked" without a duration is np contradicting its own
/// document, and 0 ms would read as "nobody was holding it".
fn waited_ms(value: &Value, method: &str) -> Result<u64, NpFault> {
    value
        .get("waited_ms")
        .and_then(Value::as_u64)
        .ok_or_else(|| shape_fault(method, "said `locked` without saying how long it waited"))
}

// ---------------------------------------------------------------- scan

/// What one scan did. Field-for-field np's `--json` report; the counts are np's, and our own
/// per-file stamps (Task 7) are display-only because np reports aggregates (§18.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanReport {
    pub added: u64,
    pub modified: u64,
    pub moved: u64,
    pub removed: u64,
    pub unchanged: u64,
    /// Paths np could not index. **Paths only, no reason** (§18.5): the tree marks those files
    /// refused and stays silent about why rather than inventing one.
    pub skipped: Vec<String>,
    pub notes: i64,
    pub unresolved_links: i64,
    pub unresolved_targets: Vec<String>,
    /// `INDEX.md` and its shards — the files np wrote, which the tree greys out (spec §9).
    pub index_files: Vec<String>,
    pub elapsed_ms: u64,
    /// Present-and-null is np's distinction between "looked, lost nothing" and "said nothing".
    pub upgrade_orphans: Option<String>,
    pub merged_notes: Option<String>,
    pub recorded_upgrade_orphans: Option<String>,
    pub recorded_merged_notes: Option<String>,
    pub unindexable: Option<Unindexable>,
}

/// The loss record beside a scan: what the walk could not read, and up to eight names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Unindexable {
    pub unreadable: u64,
    pub unkeyable: u64,
    pub duplicates: u64,
    pub ambiguous: u64,
    pub names: Vec<String>,
}

// A scan report is a few hundred bytes and one exists per ingest cycle, so it travels by value
// rather than making every reader follow a box.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum ScanOutcome {
    Applied(ScanReport),
    /// Another writer holds the workspace. Brief and rare (§18.9) — retry with backoff, and say
    /// `index behind · retrying`, never "failed" (spec §11).
    Locked { waited_ms: u64 },
}

impl ScanOutcome {
    /// The word the log writes for this outcome (`applied`, `locked`). Same words as the status line's
    /// states, so a line in `jott.jsonl` and a sentence in the window cannot disagree (§11).
    pub fn label(&self) -> &'static str {
        match self {
            ScanOutcome::Applied(_) => "applied",
            ScanOutcome::Locked { .. } => "locked",
        }
    }
}

pub fn scan_outcome(value: &Value) -> Result<ScanOutcome, NpFault> {
    if is_locked(value) {
        return Ok(ScanOutcome::Locked { waited_ms: waited_ms(value, "scan")? });
    }
    serde_json::from_value::<ScanReport>(value.clone())
        .map(ScanOutcome::Applied)
        .map_err(|e| shape_fault("scan", e))
}

// ---------------------------------------------------------------- embed

/// One image link np looked at and would not embed, as `(locator, class)`. The locator is the
/// figure's path, or the linking note's when there was no figure to name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImageRefusal {
    pub path: String,
    pub class: String,
}

fn image_refusals<'de, D>(d: D) -> Result<Vec<ImageRefusal>, D::Error>
where
    D: Deserializer<'de>,
{
    let pairs = Vec::<(String, String)>::deserialize(d)?;
    Ok(pairs
        .into_iter()
        .map(|(path, class)| ImageRefusal { path, class })
        .collect())
}

/// What one embed run did. `artifact_check`, `workers` and `fetched` are np's too and are left
/// out on purpose: nothing in M1 reads them, and a field nobody uses is a field nobody checks.
/// Serialize is for the window's payload; the two directions are not mirrors. np sends
/// `image_refusals` as pairs and this writes objects, because a webview wants named fields while §18.5
/// keeps np's spelling on np's wire.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmbedReport {
    /// The model this run answered questions about — `None` when configuration named none.
    pub model: Option<String>,
    pub chunks_seen: u64,
    pub current: u64,
    pub embedded: u64,
    pub refused_budget: u64,
    pub busy_gap: u64,
    pub errors: u64,
    pub images_seen: u64,
    pub images_embedded: u64,
    pub images_refused: u64,
    pub images_pruned: u64,
    pub images_unchecked: u64,
    pub images_sweep_ran: bool,
    #[serde(deserialize_with = "image_refusals")]
    pub image_refusals: Vec<ImageRefusal>,
    /// np's own caveat string, kept as it wrote it — never promoted to "verified" here.
    pub verification: String,
    pub offline: bool,
}

/// Why an embed run stopped where it did. Six arms because np has six, and the seventh is for
/// the arm a later np adds: an unrecognised label must never fall through to [`EmbedOutcome::Applied`],
/// which would tell the tree those notes have vectors when the run said it stopped short.
#[allow(clippy::large_enum_variant)] // same reason as `ScanOutcome`
#[derive(Debug, Clone, PartialEq)]
pub enum EmbedOutcome {
    /// Nothing owed after this run — np's `current`.
    Applied(EmbedReport),
    Locked { waited_ms: u64 },
    /// A fetch was allowed and needed and nothing was reached. Names the model still owed; np's
    /// offline arm carries the id and no sentence, so none is written for it (spec §18.5).
    Offline { model_id: Option<String> },
    /// Configuration made embedding impossible before anything was read: `[embeddings]` switched
    /// off, or `model` naming no row. Keyword search keeps working (spec §11).
    Disabled {
        key: Option<String>,
        model_id: Option<String>,
        /// np's own command, present only when the configuration is right and the artifact has
        /// simply never been installed here. Absent means there is no command to offer.
        hint: Option<String>,
    },
    /// A state that stopped the work and names itself with np's label — `artifact-absent`,
    /// `runtime-not-built`, `owed and not produced`, or one from a future build. `detail` is
    /// np's own text where it sent any.
    Refused {
        state: String,
        detail: Option<String>,
        model_id: Option<String>,
    },
}

fn field(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

impl EmbedOutcome {
    /// The log's word for this outcome. `refused` covers every state that names itself (`artifact-absent`
    /// and friends): the *which* is in the event and, on a fault, in np's sentence — the log's job is the
    /// shape of the exchange, not a copy of np's documents.
    pub fn label(&self) -> &'static str {
        match self {
            EmbedOutcome::Applied(_) => "applied",
            EmbedOutcome::Locked { .. } => "locked",
            EmbedOutcome::Offline { .. } => "offline",
            EmbedOutcome::Disabled { .. } => "disabled",
            EmbedOutcome::Refused { .. } => "refused",
        }
    }
}

pub fn embed_outcome(value: &Value) -> Result<EmbedOutcome, NpFault> {
    if is_locked(value) {
        return Ok(EmbedOutcome::Locked { waited_ms: waited_ms(value, "embed")? });
    }
    let report = || -> Result<EmbedReport, NpFault> {
        serde_json::from_value::<EmbedReport>(value.clone()).map_err(|e| shape_fault("embed", e))
    };
    let model = field(value, "model");
    match value.get("outcome") {
        // np's `current` is a bare string; every other arm is an object with one key.
        Some(Value::String(label)) if label.as_str() == "current" => Ok(EmbedOutcome::Applied(report()?)),
        Some(Value::String(other)) => Ok(EmbedOutcome::Refused {
            state: other.to_string(),
            detail: None,
            model_id: model,
        }),
        Some(Value::Object(one)) if one.len() == 1 => {
            let (label, detail) = one.iter().next().expect("one key");
            match label.as_str() {
                "configuration-disabled" => Ok(EmbedOutcome::Disabled {
                    key: field(detail, "key"),
                    model_id: field(detail, "id"),
                    hint: field(detail, "hint"),
                }),
                "offline" => Ok(EmbedOutcome::Offline { model_id: field(detail, "id") }),
                // `artifact-absent` sends `model` + `hint`, `not-produced` sends `reason`,
                // `runtime-not-built` sends only `id`; anything else gets what it sent.
                other => Ok(EmbedOutcome::Refused {
                    state: other.to_string(),
                    detail: field(detail, "hint").or_else(|| field(detail, "reason")),
                    model_id: field(detail, "model").or_else(|| field(detail, "id")),
                }),
            }
        }
        _ => Err(shape_fault(
            "embed",
            "no `outcome` to say how the run ended",
        )),
    }
}

// ---------------------------------------------------------------- search

/// One hit. Deliberately no title and no line number: np sends neither (§5), which is why
/// scroll-to-match is ours to compute and "by meaning" is what a vector-only hit can be labelled.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Hit {
    pub path: String,
    pub snippet: Option<String>,
    pub kind: String,
    pub score: f64,
    /// `None` means the keyword half did not answer this hit — not that it scored zero.
    pub bm25: Option<f64>,
    pub cosine: Option<f64>,
}

/// A search answer plus its provenance: the four keys that let an empty or half list explain
/// itself (spec §10). np's `note` prose and `fetched` record are not modelled — the status line
/// branches on `mode`, `vectors`, `offline` and `reason`, which are tokens, not sentences.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchAnswer {
    pub hits: Vec<Hit>,
    pub mode: Mode,
    pub vectors: bool,
    pub offline: bool,
    /// np's token for why a half was missing (`provider-disabled`, `no-vectors`, …).
    pub reason: Option<String>,
    /// 0.4.0 (note-processor#5): has this folder ever been scanned? `None` on 0.3.1, which cannot say — and an
    /// unknown answer must not become `false`, because "not indexed" is a diagnosis someone will act on.
    pub indexed: Option<bool>,
    /// Did the keyword half actually run? np's `mode` word alone was the bug: 0.3.1 answered `hybrid` for searches
    /// where only keywords ran, and Jott printed it.
    pub keyword_used: Option<bool>,
    /// …and the vector half. Together with `keyword_used` these are what the window may claim about a result.
    pub semantic_used: Option<bool>,
    /// np's own sentence about what happened to this answer — e.g. "this workspace has never been scanned", or that
    /// it fetched a model while searching. Kept verbatim (§4); shown as Jott's words around it, never rewritten.
    pub note: Option<String>,
}

#[derive(Deserialize)]
struct RawSearchAnswer {
    hits: Vec<Hit>,
    mode: Mode,
    vectors: bool,
    offline: bool,
    reason: Option<String>,
    #[serde(default)]
    indexed: Option<bool>,
    #[serde(default)]
    keyword_used: Option<bool>,
    #[serde(default)]
    semantic_used: Option<bool>,
    #[serde(default)]
    note: Option<String>,
}

pub fn search_answer(value: &Value) -> Result<SearchAnswer, NpFault> {
    serde_json::from_value::<RawSearchAnswer>(value.clone())
        .map(|r| SearchAnswer {
            hits: r.hits,
            mode: r.mode,
            vectors: r.vectors,
            offline: r.offline,
            reason: r.reason,
            indexed: r.indexed,
            keyword_used: r.keyword_used,
            semantic_used: r.semantic_used,
            note: r.note,
        })
        .map_err(|e| shape_fault("search", e))
}

// ---------------------------------------------------------------- models.list

/// Where one model's artifact stands on this disk, in np's four words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelState {
    Present,
    Missing,
    NoArtifactExpected,
    Mismatch,
    /// A label from a build newer than this code — kept as np wrote it, never guessed at.
    Other(String),
}

impl From<&str> for ModelState {
    fn from(label: &str) -> Self {
        match label {
            "present" => ModelState::Present,
            "missing" => ModelState::Missing,
            "no artifact expected" => ModelState::NoArtifactExpected,
            "mismatch" => ModelState::Mismatch,
            other => ModelState::Other(other.to_string()),
        }
    }
}

fn model_state<'de, D>(d: D) -> Result<ModelState, D::Error>
where
    D: Deserializer<'de>,
{
    let label = String::deserialize(d)?;
    Ok(ModelState::from(label.as_str()))
}

/// One row of `np models list`. The row's prefixes and member files are np's too and are not
/// modelled: nothing in M1 embeds or installs anything, so it has no use for them.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ModelRow {
    pub id: String,
    pub dim: i64,
    /// `None` for a provider with no local weights — np's fact, not a missing value.
    pub sha256: Option<String>,
    pub installed_at: i64,
    #[serde(deserialize_with = "model_state")]
    pub state: ModelState,
}

/// What `models.list` answered — rows, and the reason there are none when the directory itself
/// could not be read.
///
/// np keeps a directory it cannot list a **result** with `models_dir_error` filled rather than a
/// fault. Answering that as "no models" would send the consent gate to ask for a 295 MiB download
/// of an artifact the user already has, so the distinction has to survive this far (§18.6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelListing {
    pub rows: Vec<ModelRow>,
    pub models_dir_error: Option<String>,
    /// What this np build would fetch, and what it costs (note-processor#3): `pinned` is priced from the sha256
    /// table compiled into the binary, so a GUI can name a download's size before any transport is asked. Empty on
    /// 0.3.1, which quoted nothing — that is *unknown*, and the prompt says "a model" instead of a number.
    pub pinned: Vec<PinnedModel>,
}

/// One entry of np's `pinned` table: an id, the members it needs, and their total bytes when np knew them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinnedModel {
    pub id: String,
    pub bytes_total: Option<u64>,
}

impl ModelListing {
    /// Whether the model's bytes are here — the one question the consent gate asks before it
    /// offers a download. An unreadable models directory answers "no", and its caller reads
    /// `models_dir_error` before saying anything about a download.
    pub fn artifact_present(&self, id: &str) -> bool {
        self.rows
            .iter()
            .any(|row| row.id == id && row.state == ModelState::Present)
    }

    /// Bytes a download of `id` would take, from np's own table. `None` means no price is known — never 0.
    pub fn price_of(&self, id: &str) -> Option<u64> {
        self.pinned.iter().find(|p| p.id == id).and_then(|p| p.bytes_total)
    }

    /// The price of what a fresh np would fetch on its own: the built-in default, quoted as the table's one entry.
    /// Deliberately refuses to guess when np quoted several and none was named — picking one at random is how a
    /// consent prompt ends up promising 310 MB for an 865 MB download.
    pub fn only_price(&self) -> Option<u64> {
        match self.pinned.as_slice() {
            [one] => one.bytes_total,
            _ => None,
        }
    }
}

#[derive(Deserialize)]
struct RawListing {
    models: Vec<ModelRow>,
    models_dir_error: Option<String>,
    #[serde(default)]
    pinned: Option<Vec<RawPinned>>,
}

#[derive(Deserialize)]
struct RawPinned {
    id: String,
    /// np spells it `bytes_total`; a build that quotes a model without knowing its size omits it.
    #[serde(default)]
    bytes_total: Option<u64>,
}

pub fn model_listing(value: &Value) -> Result<ModelListing, NpFault> {
    serde_json::from_value::<RawListing>(value.clone())
        .map(|r| ModelListing {
            rows: r.models,
            models_dir_error: r.models_dir_error,
            pinned: r
                .pinned
                .unwrap_or_default()
                .into_iter()
                .map(|p| PinnedModel { id: p.id, bytes_total: p.bytes_total })
                .collect(),
        })
        .map_err(|e| shape_fault("models.list", e))
}
