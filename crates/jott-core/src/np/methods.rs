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
    embed_outcome, model_listing, scan_outcome, search_answer, EmbedOutcome, Mode, ModelListing,
    ScanOutcome, SearchAnswer,
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
) -> Result<SearchAnswer, NpFault> {
    let mut params = workspace_params(workspace);
    params["query"] = json!(query);
    params["limit"] = json!(limit);
    params["mode"] = json!(mode.label());
    let value = client.request("search", params).await?;
    search_answer(&value)
}

/// What models this workspace has rows for, and whether their bytes are on disk — the consent
/// gate's only source before it offers a download (§18.6: check the artifact, do not try to embed).
pub async fn models_list(client: &NpClient, workspace: &Path) -> Result<ModelListing, NpFault> {
    let value = client.request("models.list", workspace_params(workspace)).await?;
    model_listing(&value)
}
