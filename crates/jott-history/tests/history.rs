//! jott-history against the real git on this machine, in temp folders. A bare repository in another temp folder
//! plays the remote, so nothing here touches a network. No git on the machine is a failure with that sentence, not
//! a skip: CI runners have git, and a suite that passes by testing nothing is worse than one that says why.

use jott_history::locate::Probe;
use jott_history::{locate, Busy, Git, HistoryError, Layout, Platform, Repo, SyncOutcome, SystemProbe, MAX_FILE_BYTES};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

struct World {
    root: TempDir,
    global: PathBuf,
}

impl World {
    /// A git whose global config is a file in this world, empty unless a test writes to it.
    fn new() -> World {
        let root = tempfile::tempdir().unwrap();
        let global = root.path().join("global.gitconfig");
        std::fs::write(&global, "").unwrap();
        World { root, global }
    }

    fn git(&self) -> Git {
        let probe = SystemProbe;
        assert!(probe.env("PATH").is_some(), "no PATH to search for git");
        locate(&probe, Platform::current(), None)
            .expect("these tests need git installed")
            .with_env("GIT_CONFIG_GLOBAL", &self.global)
            .with_env("GIT_CONFIG_NOSYSTEM", "1")
    }

    fn dir(&self, name: &str) -> PathBuf {
        let path = self.root.path().join(name);
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    fn app_dir(&self, name: &str) -> PathBuf {
        self.root.path().join("appdata").join("history").join(format!("{name}.git"))
    }

    /// A vault with history on and an identity, ready to commit.
    fn vault(&self, name: &str) -> (PathBuf, Repo) {
        let vault = self.dir(name);
        let repo = Repo::enable(&self.git(), &vault, &self.app_dir(name), false).unwrap();
        repo.set_identity("Test Person", "test@example.com").unwrap();
        (vault, repo)
    }

    fn bare_remote(&self) -> PathBuf {
        let remote = self.dir("remote.git");
        let status = std::process::Command::new(&self.git().path)
            .args(["init", "-q", "--bare", "-b", "main"])
            .arg(&remote)
            .env("GIT_CONFIG_GLOBAL", &self.global)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .status()
            .unwrap();
        assert!(status.success());
        remote
    }
}

fn write(vault: &Path, rel: &str, text: &str) {
    let path = vault.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn read(vault: &Path, rel: &str) -> String {
    std::fs::read_to_string(vault.join(rel)).unwrap()
}

fn url(path: &Path) -> String {
    path.display().to_string()
}

#[test]
fn turning_history_on_makes_a_repository_in_the_vault_and_a_version_of_what_is_there() {
    let world = World::new();
    let (vault, repo) = world.vault("notes");
    write(&vault, "a.md", "first\n");
    write(&vault, "deep/b c.md", "second\n");
    assert_eq!(repo.layout(), Layout::InVault);
    assert!(vault.join(".git").is_dir());

    let outcome = repo.commit_if_dirty(None).unwrap();
    assert!(outcome.version.is_some());
    assert_eq!(outcome.paths, vec!["a.md", "deep/b c.md"]);

    let again = repo.commit_if_dirty(None).unwrap();
    assert_eq!(again.version, None, "nothing changed, so no version");
}

#[test]
fn np_s_catalogue_is_never_versioned_and_nothing_is_written_into_the_notes_to_achieve_it() {
    let world = World::new();
    let (vault, repo) = world.vault("notes");
    write(&vault, "INDEX.md", "generated\n");
    write(&vault, "INDEX-2.md", "generated\n");
    write(&vault, "note.md", "mine\n");
    assert_eq!(repo.commit_if_dirty(None).unwrap().paths, vec!["note.md"]);
    assert!(!vault.join(".gitignore").exists());
}

#[test]
fn a_hostile_global_config_cannot_break_or_change_an_unattended_commit() {
    let world = World::new();
    let hooks = world.dir("hooks");
    let hook = hooks.join("pre-commit");
    std::fs::write(&hook, "#!/bin/sh\nexit 1\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    std::fs::write(
        &world.global,
        format!(
            "[core]\n\tautocrlf = true\n\thooksPath = {}\n[commit]\n\tgpgsign = true\n[gpg]\n\tprogram = /nowhere/gpg\n",
            hooks.display().to_string().replace('\\', "/")
        ),
    )
    .unwrap();
    let (vault, repo) = world.vault("notes");
    let bytes = "windows line\r\nunix line\n";
    write(&vault, "mixed.md", bytes);

    let version = repo.commit_if_dirty(None).unwrap().version.expect("the commit went through");
    assert_eq!(repo.bytes_at(&version, "mixed.md").unwrap(), bytes.as_bytes(), "bytes in, bytes out");
}

#[test]
fn without_a_name_and_email_jott_asks_instead_of_inventing_one() {
    let world = World::new();
    let vault = world.dir("notes");
    let repo = Repo::enable(&world.git(), &vault, &world.app_dir("notes"), false).unwrap();
    write(&vault, "a.md", "x\n");
    assert!(matches!(repo.commit_if_dirty(None), Err(HistoryError::NeedsIdentity)));
    repo.set_identity("Ana", "ana@example.com").unwrap();
    assert!(repo.commit_if_dirty(None).unwrap().version.is_some());
}

#[test]
fn every_version_of_a_note_is_listed_across_a_rename_and_its_old_bytes_come_back() {
    let world = World::new();
    let (vault, repo) = world.vault("notes");
    write(&vault, "a.md", "one\n");
    repo.commit_if_dirty(None).unwrap();
    write(&vault, "a.md", "one\ntwo\n");
    repo.commit_if_dirty(None).unwrap();
    std::fs::rename(vault.join("a.md"), vault.join("b.md")).unwrap();
    repo.commit_if_dirty(None).unwrap();

    let versions = repo.versions("b.md").unwrap();
    assert_eq!(versions.len(), 3);
    assert_eq!(versions[0].rel, "b.md");
    assert_eq!(versions[2].rel, "a.md", "the oldest version knew the note by its old name");
    assert_eq!(repo.bytes_at(&versions[2].id, &versions[2].rel).unwrap(), b"one\n");
    assert!(matches!(repo.bytes_at("--output=x", "b.md"), Err(HistoryError::BadVersion(_))));
}

#[test]
fn a_vault_inside_somebody_else_s_repository_is_refused_by_name() {
    let world = World::new();
    let project = world.dir("project");
    let git = world.git();
    Repo::enable(&git, &project, &world.app_dir("project"), false).unwrap();
    let vault = world.dir("project/docs/notes");
    match Repo::enable(&git, &vault, &world.app_dir("notes"), false) {
        Err(HistoryError::InsideRepository { top }) => {
            assert_eq!(std::fs::canonicalize(top).unwrap(), std::fs::canonicalize(&project).unwrap());
        }
        other => panic!("expected a refusal, got {other:?}"),
    }
    assert!(!vault.join(".git").exists());
}

#[test]
fn a_cloud_synced_vault_keeps_its_repository_in_app_data_and_nothing_in_the_folder() {
    let world = World::new();
    let vault = world.dir("OneDrive/notes");
    let app_dir = world.app_dir("cloud");
    let git = world.git();
    let repo = Repo::enable(&git, &vault, &app_dir, true).unwrap();
    repo.set_identity("Ana", "ana@example.com").unwrap();
    write(&vault, "a.md", "x\n");
    assert!(repo.commit_if_dirty(None).unwrap().version.is_some());

    assert_eq!(repo.layout(), Layout::AppData);
    let names: Vec<_> = std::fs::read_dir(&vault).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(names, vec![std::ffi::OsString::from("a.md")], "git left nothing in the synced folder");
    assert_eq!(Repo::open(&git, &vault, &app_dir).unwrap().layout(), Layout::AppData, "and finds it again");
}

#[test]
fn a_file_over_the_limit_is_left_out_and_reported_every_time() {
    let world = World::new();
    let (vault, repo) = world.vault("notes");
    write(&vault, "note.md", "small\n");
    let big = std::fs::File::create(vault.join("video.mp4")).unwrap();
    big.set_len(MAX_FILE_BYTES + 1).unwrap();
    drop(big);

    let outcome = repo.commit_if_dirty(None).unwrap();
    assert_eq!(outcome.paths, vec!["note.md"]);
    assert_eq!(outcome.too_large.len(), 1);
    assert_eq!((outcome.too_large[0].rel.as_str(), outcome.too_large[0].bytes), ("video.mp4", MAX_FILE_BYTES + 1));

    let again = repo.commit_if_dirty(None).unwrap();
    assert_eq!(again.version, None);
    assert_eq!(again.too_large.len(), 1, "still listed, so the panel stays current");
}

#[test]
fn an_adopted_repository_with_work_staged_by_somebody_else_is_left_alone() {
    let world = World::new();
    let (vault, repo) = world.vault("notes");
    write(&vault, "a.md", "x\n");
    repo.commit_if_dirty(None).unwrap();
    write(&vault, "b.md", "theirs\n");
    let status = std::process::Command::new(&world.git().path)
        .args(["add", "b.md"])
        .current_dir(&vault)
        .env("GIT_CONFIG_GLOBAL", &world.global)
        .status()
        .unwrap();
    assert!(status.success());
    // Opened again the way Jott finds an existing repository.
    let adopted = Repo::open(&world.git(), &vault, &world.app_dir("notes")).unwrap();
    assert!(matches!(adopted.commit_if_dirty(None), Err(HistoryError::Busy(Busy::StagedElsewhere))));
}

#[test]
fn two_machines_sync_through_a_remote_and_a_note_both_changed_keeps_both_texts() {
    let world = World::new();
    let remote = world.bare_remote();
    let (a, repo_a) = world.vault("machine-a");
    write(&a, "shared.md", "base\n");
    repo_a.commit_if_dirty(None).unwrap();
    assert!(repo_a.connect(&url(&remote)).unwrap().remote_was_empty);
    assert_eq!(repo_a.sync().unwrap(), SyncOutcome::Pushed { commits: 1 });

    // A second machine with an empty folder takes everything.
    let (b, repo_b) = world.vault("machine-b");
    assert!(!repo_b.connect(&url(&remote)).unwrap().remote_was_empty);
    assert!(matches!(repo_b.sync().unwrap(), SyncOutcome::Pulled { commits: 1, .. }));
    assert_eq!(read(&b, "shared.md"), "base\n");

    // Both change the same note.
    write(&a, "shared.md", "from a\n");
    repo_a.commit_if_dirty(None).unwrap();
    repo_a.sync().unwrap();
    write(&b, "shared.md", "from b\n");
    repo_b.commit_if_dirty(None).unwrap();

    let SyncOutcome::Merged { kept_both, pulled: 1, pushed: 1, .. } = repo_b.sync().unwrap() else {
        panic!("expected a merge")
    };
    assert_eq!(kept_both.len(), 1);
    assert_eq!(kept_both[0].rel, "shared.md");
    assert!(kept_both[0].remote_copy.starts_with("shared (from remote, "), "{}", kept_both[0].remote_copy);
    assert_eq!(read(&b, "shared.md"), "from b\n", "this machine's text stays where it was");
    assert_eq!(read(&b, &kept_both[0].remote_copy), "from a\n", "and the remote's is beside it");

    // The first machine gets the resolution, and nothing is open on either side.
    assert!(matches!(repo_a.sync().unwrap(), SyncOutcome::Pulled { .. }));
    assert_eq!(read(&a, "shared.md"), "from b\n");
    assert_eq!(read(&a, &kept_both[0].remote_copy), "from a\n");
    assert_eq!(repo_a.sync().unwrap(), SyncOutcome::UpToDate);
    assert_eq!(repo_b.sync().unwrap(), SyncOutcome::UpToDate);
}

#[test]
fn changes_to_different_notes_on_two_machines_merge_without_copies() {
    let world = World::new();
    let remote = world.bare_remote();
    let (a, repo_a) = world.vault("machine-a");
    write(&a, "x.md", "x\n");
    write(&a, "y.md", "y\n");
    repo_a.commit_if_dirty(None).unwrap();
    repo_a.connect(&url(&remote)).unwrap();
    repo_a.sync().unwrap();
    let (b, repo_b) = world.vault("machine-b");
    repo_b.connect(&url(&remote)).unwrap();
    repo_b.sync().unwrap();

    write(&a, "x.md", "x from a\n");
    repo_a.commit_if_dirty(None).unwrap();
    repo_a.sync().unwrap();
    write(&b, "y.md", "y from b\n");
    repo_b.commit_if_dirty(None).unwrap();

    let SyncOutcome::Merged { kept_both, paths, .. } = repo_b.sync().unwrap() else { panic!("expected a merge") };
    assert!(kept_both.is_empty());
    assert!(paths.contains(&"x.md".to_string()), "{paths:?}");
    assert_eq!(read(&b, "x.md"), "x from a\n");
    assert_eq!(read(&b, "y.md"), "y from b\n");
}

#[test]
fn a_remote_with_an_unrelated_history_is_refused_and_not_kept() {
    let world = World::new();
    let remote = world.bare_remote();
    let (a, repo_a) = world.vault("machine-a");
    write(&a, "a.md", "a\n");
    repo_a.commit_if_dirty(None).unwrap();
    repo_a.connect(&url(&remote)).unwrap();
    repo_a.sync().unwrap();

    let (c, repo_c) = world.vault("machine-c");
    write(&c, "c.md", "c\n");
    repo_c.commit_if_dirty(None).unwrap();
    assert!(matches!(repo_c.connect(&url(&remote)), Err(HistoryError::UnrelatedHistory)));
    assert_eq!(repo_c.remote_url().unwrap(), None);
}

#[test]
fn an_address_with_a_login_in_it_never_reaches_the_repository() {
    let world = World::new();
    let (_vault, repo) = world.vault("notes");
    assert!(matches!(
        repo.connect("https://ghp_secret@github.com/ana/notes.git"),
        Err(HistoryError::CredentialsInUrl)
    ));
    assert_eq!(repo.remote_url().unwrap(), None);
    assert!(matches!(repo.sync(), Err(HistoryError::NoRemote)));
}
