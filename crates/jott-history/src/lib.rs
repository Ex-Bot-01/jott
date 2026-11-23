//! Versions and sync for a vault, through the `git` someone already has (design:
//! `docs/superpowers/specs/2026-11-23-git-versions-and-sync-design.md`).
//!
//! Jott ships no git. This crate finds the one on the machine, runs it with the few settings that would break an
//! unattended commit overridden, and returns data. It never writes a note on its own behalf except where git itself
//! must — a merge's working tree — and it knows nothing about windows, np, or Jott's other state.

pub mod cloud;
pub mod error;
pub mod locate;
pub mod remote;
pub mod repo;
mod run;

pub use cloud::{cloud_service, CloudService, SystemCloudProbe};
pub use error::{Busy, HistoryError};
pub use locate::{install_hint, locate, Git, GitMissing, GitSource, InstallHint, Platform, SystemProbe, MIN_GIT_VERSION};
pub use remote::{ConnectOutcome, KeptBoth, SyncOutcome};
pub use repo::{CommitOutcome, Discovery, LargeFile, Layout, Repo, Version, MAX_FILE_BYTES};
