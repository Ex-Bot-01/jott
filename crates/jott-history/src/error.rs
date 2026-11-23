use serde::Serialize;
use std::path::PathBuf;
use thiserror::Error;

/// Everything that can stop a history or sync step. Each arm reads as a sentence for the person, because the
/// window shows it as it came; git's own words are carried verbatim rather than parsed.
#[derive(Debug, Error)]
pub enum HistoryError {
    #[error("could not run git at {path}: {source}")]
    Spawn { path: PathBuf, source: std::io::Error },
    /// git ran and said no. `said` is its stderr (or stdout when stderr was empty), trimmed.
    #[error("git {command} failed: {said}")]
    Git { command: String, said: String },
    #[error(
        "this folder is inside the git repository at {top}. Jott keeps versions only for a folder that is its own \
         repository, so its saves never land in a project somebody else keeps"
    )]
    InsideRepository { top: PathBuf },
    #[error("version history is not turned on for this folder")]
    NotEnabled,
    #[error("{0}")]
    Busy(Busy),
    #[error("git does not know who you are yet — enter a name and an email for your versions")]
    NeedsIdentity,
    #[error(
        "this address has a login written into it. Jott does not keep logins: use the address without it, and git \
         will ask for the login itself"
    )]
    CredentialsInUrl,
    #[error("that address is not a repository git can use: {0}")]
    BadUrl(String),
    #[error(
        "the repository at that address already has a history that has nothing in common with this folder's. \
         Combining two histories is a job for git itself, not for a button"
    )]
    UnrelatedHistory,
    #[error("no remote is connected")]
    NoRemote,
    #[error("{service} already syncs this folder, so Jott does not offer a second sync for it")]
    SyncedByCloud { service: String },
    #[error("the remote changed again while Jott was syncing. Nothing was lost — press Sync now once more")]
    PushRaced,
    #[error("{0}")]
    BadVersion(String),
    #[error("could not use {path}: {source}")]
    Io { path: PathBuf, source: std::io::Error },
}

/// Why Jott is leaving an adopted repository alone right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Busy {
    Merging,
    Rebasing,
    CherryPicking,
    Reverting,
    /// Changes staged by somebody else. Committing them under Jott's message would take credit for their work.
    StagedElsewhere,
}

impl std::fmt::Display for Busy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let what = match self {
            Busy::Merging => "is in the middle of a merge",
            Busy::Rebasing => "is in the middle of a rebase",
            Busy::CherryPicking => "is in the middle of a cherry-pick",
            Busy::Reverting => "is in the middle of a revert",
            Busy::StagedElsewhere => "has changes staged outside Jott",
        };
        write!(f, "the repository {what}; Jott will save versions again once that is finished")
    }
}
