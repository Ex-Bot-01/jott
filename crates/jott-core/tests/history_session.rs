//! Versions through the session the window talks to: turning history on, a restore that is itself undoable, the
//! last version on close, and the path guard in front of all of it. Real git, temp folders, np absent (§1: the
//! editor works without it, and so do versions).

use jott_core::ipc::{self, ErrorKind, VaultSession};
use jott_core::vault::store::Consent;
use std::path::Path;
use std::sync::{Arc, OnceLock};

/// One global git config for this whole test binary, with a name in it, so the machine's own config cannot leak in.
fn isolate_git() {
    static CONFIG: OnceLock<tempfile::TempDir> = OnceLock::new();
    let dir = CONFIG.get_or_init(|| {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("global.gitconfig"), "[user]\n\tname = Test Person\n\temail = test@example.com\n").unwrap();
        dir
    });
    std::env::set_var("GIT_CONFIG_GLOBAL", dir.path().join("global.gitconfig"));
    std::env::set_var("GIT_CONFIG_NOSYSTEM", "1");
}

struct Harness {
    data: tempfile::TempDir,
    vault: tempfile::TempDir,
}

impl Harness {
    fn new() -> Harness {
        isolate_git();
        Harness { data: tempfile::tempdir().unwrap(), vault: tempfile::tempdir().unwrap() }
    }

    fn session(&self) -> Arc<VaultSession> {
        let (np, info) = ipc::np_absent();
        VaultSession::open_in(np, info, self.vault.path(), Consent::Undecided, None, self.data.path().to_path_buf())
    }

    fn write(&self, rel: &str, text: &str) {
        std::fs::write(self.vault.path().join(rel), text).unwrap();
    }

    fn read(&self, rel: &str) -> String {
        std::fs::read_to_string(self.vault.path().join(rel)).unwrap()
    }
}

fn has_git_dir(vault: &Path) -> bool {
    vault.join(".git").is_dir()
}

#[tokio::test]
async fn opening_a_folder_runs_no_versions_until_somebody_turns_them_on() {
    let harness = Harness::new();
    harness.write("a.md", "one\n");
    let session = harness.session();
    let status = session.history_status().await.unwrap();
    assert!(status.git.found, "{:?}", status.git.problem);
    assert!(!status.enabled);
    assert!(!has_git_dir(harness.vault.path()), "looking is not turning on");
    session.close().await;
}

#[tokio::test]
async fn a_restore_puts_old_text_back_as_a_new_version_so_it_can_be_undone_too() {
    let harness = Harness::new();
    harness.write("a.md", "one\n");
    let session = harness.session();
    let status = session.history_enable().await.unwrap();
    assert!(status.enabled);
    assert!(status.last_version_at_ms.is_some(), "the folder as it was is the first version");
    assert!(has_git_dir(harness.vault.path()));

    session.save_note("a.md", "two\n").unwrap();
    session.history_version_now().await.unwrap();
    let versions = session.history_versions("a.md".into()).await.unwrap();
    assert_eq!(versions.len(), 2);
    let oldest = versions.last().unwrap().clone();
    assert_eq!(session.history_version_text(oldest.id.clone(), oldest.rel.clone()).await.unwrap(), "one\n");

    session.history_restore("a.md".into(), oldest.id, oldest.rel).await.unwrap();
    assert_eq!(harness.read("a.md"), "one\n");
    let after = session.history_versions("a.md".into()).await.unwrap();
    assert_eq!(after.len(), 3, "the restore is a version of its own");
    assert!(after[0].message.starts_with("Jott: restored a.md"), "{}", after[0].message);
    session.close().await;
}

#[tokio::test]
async fn closing_the_folder_makes_the_version_that_was_still_waiting() {
    let harness = Harness::new();
    harness.write("a.md", "one\n");
    let session = harness.session();
    session.history_enable().await.unwrap();
    session.save_note("a.md", "typed just before closing\n").unwrap();
    session.close().await;

    let reopened = harness.session();
    let versions = reopened.history_versions("a.md".into()).await.unwrap();
    assert_eq!(versions.len(), 2, "history stayed on across the reopen, and the last edit was kept");
    reopened.close().await;
}

#[tokio::test]
async fn paths_from_the_window_are_guarded_like_every_other_path() {
    let harness = Harness::new();
    harness.write("a.md", "one\n");
    let session = harness.session();
    session.history_enable().await.unwrap();
    let error = session.history_restore("../outside.md".into(), "abcdef1".into(), "a.md".into()).await.unwrap_err();
    assert_eq!(error.kind, ErrorKind::EscapesRoot);
    let error = session.history_versions("../outside.md".into()).await.unwrap_err();
    assert_eq!(error.kind, ErrorKind::EscapesRoot);
    session.close().await;
}

#[tokio::test]
async fn a_login_in_a_remote_address_is_refused_with_its_own_kind() {
    let harness = Harness::new();
    let session = harness.session();
    session.history_enable().await.unwrap();
    let error = session.history_connect("https://ghp_secret@github.com/ana/notes.git".into()).await.unwrap_err();
    assert_eq!(error.kind, ErrorKind::History);
    assert!(!error.message.contains("ghp_secret"), "the secret is not repeated back: {}", error.message);
    session.close().await;
}
