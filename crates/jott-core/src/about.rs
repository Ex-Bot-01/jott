//! What the About panel shows, worked out where it can be tested.
//!
//! This module exists because of one support round trip: the report said "np is too old", and Jott — which
//! knew both the version it had found and the exact file it had run — told nobody either. So every fact the
//! panel needs is assembled here, in one pure function, with the path spelled out in whichever separator the
//! person's own file manager understands.

use crate::ipc::NpMissingPayload;
use crate::np::{NpInfo, NpSource};
use serde::Serialize;
use std::path::Path;

/// Where the code lives. Named in the panel so a person can file something against the right repository —
/// and so np problems can be pointed at np's own tracker instead of being worked around here.
pub const REPO_URL: &str = "https://github.com/Ex-Bot-01/jott";

/// The walk-through for whoever is testing a build.
pub const DOCS_URL: &str = "https://github.com/Ex-Bot-01/jott/blob/main/docs/testing-on-windows.md";

pub fn releases_url() -> String {
    format!("{REPO_URL}/releases")
}

/// Which of the three places Jott looks a binary came from, in the words someone would use for it. `$NP_PATH`
/// and `$PATH` are spelled as variables because that is how they were set; "bundled" is how the installer
/// describes it.
pub fn source_label(source: &NpSource) -> &'static str {
    match source {
        NpSource::Bundled(_) => "bundled",
        NpSource::Env(_) => "$NP_PATH",
        NpSource::Path(_) => "$PATH",
    }
}

/// np as the panel shows it: found, with a version and a path — or not found, with the sentence explaining
/// why. Never both, and never the shell's stand-in pair pretending to be an installed binary.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NpReport {
    pub found: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Why there is no np, in the sentence we would have shown in the sidebar anyway.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AboutInfo {
    pub jott_version: String,
    pub np: NpReport,
    /// Where the registry, the mirrored buffers and the trash live.
    pub data_dir: String,
    pub log_path: String,
    pub repo_url: String,
    pub releases_url: String,
    pub docs_url: String,
    /// The same facts as one block of text, for the "Copy details" button. Sent ready-made so the words a
    /// colleague pastes into a message are the ones this module's tests pinned, not something the front end
    /// assembles differently on a Tuesday.
    pub diagnostics: String,
}

/// Assemble the panel. `problem` wins over `source`/`info` when both are handed in: a stand-in pair is what
/// the shell keeps so other commands still answer, not a fact about this machine.
pub fn about(
    jott_version: &str,
    source: Option<&NpSource>,
    info: Option<&NpInfo>,
    problem: Option<&NpMissingPayload>,
    data_dir: &Path,
    log_path: &Path,
) -> AboutInfo {
    let np = match (problem, source, info) {
        (Some(problem), _, _) => NpReport {
            found: false,
            version: None,
            source: None,
            path: None,
            problem: Some(problem.message.clone()),
            hint: Some(problem.install_hint.clone()),
        },
        (None, Some(source), Some(info)) => NpReport {
            found: true,
            version: Some(info.version.clone()),
            source: Some(source_label(source).to_string()),
            path: Some(source.path().display().to_string()),
            problem: None,
            hint: None,
        },
        // Nobody has asked yet. Say so plainly rather than showing an empty row a person reads as "broken".
        _ => NpReport {
            found: false,
            version: None,
            source: None,
            path: None,
            problem: Some("np has not been asked yet in this run".into()),
            hint: None,
        },
    };

    let draft = AboutInfo {
        jott_version: jott_version.to_string(),
        np,
        data_dir: data_dir.display().to_string(),
        log_path: log_path.display().to_string(),
        repo_url: REPO_URL.to_string(),
        releases_url: releases_url(),
        docs_url: DOCS_URL.to_string(),
        diagnostics: String::new(),
    };
    AboutInfo { diagnostics: draft.diagnostics_text(), ..draft }
}

impl AboutInfo {
    /// The text behind "Copy details": short enough to paste into a message, complete enough that the person
    /// reading it does not need to ask which np was used. Paths are shown as this machine spells them, so one
    /// can be pasted into Explorer or a shell and work.
    pub fn diagnostics_text(&self) -> String {
        let mut lines = vec![format!("Jott {} · {}", self.jott_version, self.repo_url)];
        lines.push(match (&self.np.found, &self.np.version, &self.np.path, &self.np.source) {
            (true, Some(version), Some(path), Some(source)) => format!("np {version} from {source} ({path})"),
            _ => format!(
                "np: {}{}",
                self.np.problem.clone().unwrap_or_else(|| "not found".into()),
                self.np.hint.as_deref().map(|hint| format!(" — {hint}")).unwrap_or_default()
            ),
        });
        lines.push(format!("data {}", self.data_dir));
        lines.push(format!("log  {}", self.log_path));
        lines.join("\n")
    }
}
