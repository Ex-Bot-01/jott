//! One typed call per np method.
//!
//! These are thin on purpose: build the params, ask [`NpClient`], hand the document to
//! [`outcomes`](super::outcomes) for mapping. Everything interesting about *what np meant* lives
//! there, so a caller cannot accidentally read a raw field and miss that `locked` was in it.
//!
//! The workspace travels as a request parameter on every method that names one — never config
//! (§18.6), because one `np serve` answers for whichever vault a request asks about.

use super::client::NpClient;
use super::handshake::{parse_initialize, NpInfo};
use super::outcomes::{
    embed_outcome, model_listing, scan_outcome, search_answer, EmbedOutcome, EnableAnswer, Mode,
    ModelListing, RawEnableAnswer, ScanOutcome, SearchAnswer,
};
use super::{NpError, NpFault};
use serde_json::{json, Value};
use std::path::Path;

fn workspace_params(workspace: &Path) -> Value {
    json!({ "workspace": workspace.display().to_string() })
}

/// Shake hands and record what this np can do.
///
/// `notifications` is per connection (spec §5): the writer asks for progress so the status line
/// has something to show, and a reader that never will does not pay for it. Support is advertised
/// by np answering with a `progress` key, so asking honestly is what makes `NpInfo::progress`
/// mean anything.
pub async fn initialize(
    client: &NpClient,
    workspace: &Path,
    notifications: bool,
) -> Result<NpInfo, NpError> {
    let mut params = workspace_params(workspace);
    params["notifications"] = json!(notifications);
    let value = client.request("initialize", params).await?;
    Ok(parse_initialize(&value)?)
}

/// Re-index the vault. Takes a workspace and nothing else — there is no per-file scan (§18.2).
pub async fn scan(client: &NpClient, workspace: &Path) -> Result<ScanOutcome, NpFault> {
    let value = client.request("scan", workspace_params(workspace)).await?;
    scan_outcome(&value)
}

/// Embed whatever the index says is owed. Always after a [`scan`], because embed reads the index.
pub async fn embed(client: &NpClient, workspace: &Path) -> Result<EmbedOutcome, NpFault> {
    let value = client.request("embed", workspace_params(workspace)).await?;
    embed_outcome(&value)
}

/// Content search. `limit` caps the list; np has no pagination, so there is nothing after it.
pub async fn search(
    client: &NpClient,
    workspace: &Path,
    query: &str,
    limit: u8,
    mode: Mode,
    // Widen terms to word beginnings (0.5.x, note-processor#6). Always sent, always a boolean: np's own default is
    // off and Jott's toggle is off until asked, so `false` is the honest value rather than an omitted key that lets
    // somebody else's default decide what a person's search meant.
    prefix: bool,
    // 0.7.0 (F51): ask for the rerank stage, or say explicitly that it is not wanted — never omitted, so np's own
    // `[search] rerank` cannot decide for the window. An older np ignores the key and answers without a stage.
    rerank: bool,
) -> Result<SearchAnswer, NpFault> {
    let mut params = workspace_params(workspace);
    params["query"] = json!(query);
    params["limit"] = json!(limit);
    params["mode"] = json!(mode.label());
    params["prefix"] = json!(prefix);
    params["rerank"] = json!(rerank);
    let value = client.request("search", params).await?;
    search_answer(&value)
}

/// What models this workspace has rows for, and whether their bytes are on disk — the consent
/// gate's only source before it offers a download (§18.6: check the artifact, do not try to embed).
pub async fn models_list(client: &NpClient, workspace: &Path) -> Result<ModelListing, NpFault> {
    let value = client.request("models.list", workspace_params(workspace)).await?;
    model_listing(&value)
}

/// Record consent where it belongs — in np. `embeddings.enable` writes np's own `embeddings.provider` and
/// `embeddings.model`, refuses an id this build cannot use, and refuses to rewrite a file it cannot read; the answer
/// says which keys moved, into which file, and what the eventual fetch costs.
///
/// Jott sends the model id and nothing else. It does not send a provider: choosing one was never ours to make, which
/// is exactly what this method replaced.
pub async fn embeddings_enable(client: &NpClient, model: &str) -> Result<EnableAnswer, NpFault> {
    let value = client.request("embeddings.enable", json!({ "model": model })).await?;
    Ok(serde_json::from_value::<RawEnableAnswer>(value).map_err(|e| NpFault {
        code: -32603,
        message: format!("np answered embeddings.enable in a shape Jott cannot read: {e}"),
    })?
    .into_answer())
}

/// Take the answer back. np switches the provider off and **keeps** `embeddings.model`, because revoking permission
/// to download is not a licence to erase which model someone chose; Jott reports whatever np says it changed rather
/// than describing the file from memory.
pub async fn embeddings_disable(client: &NpClient) -> Result<EnableAnswer, NpFault> {
    let value = client.request("embeddings.disable", json!({})).await?;
    Ok(serde_json::from_value::<RawEnableAnswer>(value).map_err(|e| NpFault {
        code: -32603,
        message: format!("np answered embeddings.disable in a shape Jott cannot read: {e}"),
    })?
    .into_answer())
}
