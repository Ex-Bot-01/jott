//! Everything Jott keeps about somebody's notes lives in one folder whose name gives nothing away, and until
//! issue #9 every file in it went out through `std::fs::write` or `create_dir_all` — which means the process umask
//! chose the mode, and a typical umask says 0644 in a 0755 directory. On a shared Linux box, where `~/.local/share`
//! is world-readable by default, another account could read:
//!
//! * `buffers/<hash>.buffer` — **the text of a note as it is being typed**, before the person decided anyone should
//!   see it (§18.13's whole point);
//! * `vaults.json` — every vault path they have opened, even for a vault whose folder they could not enter;
//! * `<hash>/state.json` — the marker map, which gives up note names through its hashed parent;
//! * `logs/jott.jsonl` — whatever paths and sentences Jott wrote into the log.
//!
//! No credentials are involved; this is privacy of drafts. Windows does not have the problem (`%APPDATA%` is
//! per-user), and neither does a single-user laptop, which is why it stayed unnoticed. The fix tightens what Jott
//! *creates* rather than rewriting what exists — and on unix a temp-then-rename carries the **temp file's** mode, so
//! the chmod has to land before the rename, not after.
//!
//! These tests are `#[cfg(unix)]`: on Windows there is no mode to assert, and pretending otherwise would pin a
//! number that means nothing there.

#![cfg(unix)]

use jott_core::diagnostics::{Journal, LocalRecord};
use jott_core::vault::buffers;
use jott_core::vault::stamps::Stamps;
use jott_core::vault::store::VaultStore;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

/// The group and other bits, which is the whole question: `0` means nobody but the owner can read or write it.
fn group_and_other(path: &Path) -> u32 {
    let mode = std::fs::metadata(path).unwrap_or_else(|e| panic!("{:?}: {e}", path)).permissions().mode();
    mode & 0o077
}

fn vault_in(root: &Path, name: &str) -> std::path::PathBuf {
    let dir = root.join(name);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.md"), "# A\n").unwrap();
    dir
}

/// The file with something to hide: an unsaved draft, mirrored so a crash does not cost the keystrokes (§18.13).
#[test]
fn an_unsaved_draft_is_not_readable_by_the_rest_of_the_machine() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("appdata");
    let vault = vault_in(tmp.path(), "vault");

    buffers::write_mirror_in(&root, &vault, "a.md", b"a sentence nobody has read yet\n", None)
        .expect("the mirror is written");

    let dir = root.join("buffers");
    let buffer = std::fs::read_dir(&dir).expect("buffers/ exists").next().expect("one buffer").expect("it reads").path();
    assert_eq!(group_and_other(&buffer), 0, "the draft of a note in progress: {:?}", std::fs::metadata(&buffer).unwrap().permissions());
    assert_eq!(group_and_other(&dir), 0, "and the folder holding them is not listable either");
}

/// `vaults.json` names every folder the person has opened. That is a map of where their private things are, and it
/// is written on every open, so this is also the case where "tighten new writes" reaches an existing install.
#[test]
fn the_registry_of_vault_paths_is_the_owners_only() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("appdata");
    let vault = vault_in(tmp.path(), "vault");

    let store = VaultStore::new(root.clone());
    store.register(&vault).expect("registered");

    assert_eq!(group_and_other(&root.join("vaults.json")), 0, "recents are nobody else's list");
    // The temp file is what a rename moves its mode from, so this is the assertion that makes the next one true.
    assert_eq!(group_and_other(&root), 0, "the app data folder itself, not only the file in it");
}

/// The marker map, under its hashed per-vault directory. Both halves matter: the file, and the directory whose name
/// is a digest but whose mode is a permission.
#[test]
fn the_marker_map_and_its_folder_are_the_owners_only() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("appdata");
    let vault = vault_in(tmp.path(), "vault");

    let mut stamps = Stamps::with_root(root.clone(), &vault);
    stamps.mark_indexed("a.md", &jott_core::hashes::sha256_hex(b"# A\n"));
    stamps.save().expect("saved");

    // `<app data>/state/<16 hex of the vault path>/state.json` — the same digest np uses to name a workspace's
    // own files, which is why the folder needs a mode: a digest is a name, not a lock.
    let state = std::fs::read_dir(root.join("state")).expect("`state/` holds the per-vault folders")
        .flatten().map(|e| e.path()).find(|p| p.join("state.json").is_file())
        .expect("a per-vault folder holding state.json");
    assert_eq!(group_and_other(&state), 0, "the hashed folder is a name, not a lock: it still needs a mode");
    assert_eq!(group_and_other(&state.join("state.json")), 0);
    // `state/` is one level up. A digest tells a stranger nothing about which note, but the *list* of digests says
    // how many vaults this account has opened, and that is still theirs to know.
    assert_eq!(group_and_other(&root.join("state")), 0, "and the folder holding those names");
}

/// The log is the file a person is asked to send me. It should not be one they have to think about before sending.
#[test]
fn the_journal_a_person_is_asked_to_send_is_theirs_alone() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("appdata");
    let journal = Journal::open(&root);
    journal.record_local(&LocalRecord::new("save").with_rel("a.md")).expect("recorded");

    assert_eq!(group_and_other(&journal.path()), 0, "logs/jott.jsonl");
    assert_eq!(group_and_other(&root.join("logs")), 0);
}

/// A file that already exists with loose bits is rewritten, and the rewrite lands the tight mode — which matters,
/// because the registry and the stamps are rewritten on every open, so an install from before this fix tightens
/// itself within a session rather than waiting for a fresh machine.
#[test]
fn rewriting_a_loose_file_tightens_it() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("appdata");
    let store = VaultStore::new(root.clone());
    let vault = vault_in(tmp.path(), "vault");
    store.register(&vault).expect("registered");
    // Pretend this install predates the fix, which is exactly the case that matters: an existing file with the
    // umask's mode, about to be rewritten by a version that chooses its own.
    std::fs::set_permissions(root.join("vaults.json"), std::fs::Permissions::from_mode(0o666)).unwrap();
    assert_eq!(group_and_other(&root.join("vaults.json")), 0o066, "the starting point this test is about");

    store.set_consent(&vault, jott_core::vault::store::Consent::Granted).expect("saved");

    assert_eq!(group_and_other(&root.join("vaults.json")), 0, "a rewrite is also a chance to fix the mode");
}

/// The bargain the issue states: a filesystem that cannot hold permission bits answers `EPERM`/`ENOTSUP`, and that
/// is not a reason to fail someone's save. What can be pinned here is the shape of the promise — tightening is
/// best effort and never reports a problem, whatever the path turns out to be. (A real no-permission-bits mount,
/// like a FAT USB stick used as app data, cannot be made in a unit test; the code simply ignores that error, which
/// is why these helpers return `()` for the file case.)
#[test]
fn tightening_is_best_effort_and_never_reports_a_problem() {
    jott_core::app_paths::make_private(Path::new("/nonexistent/jott-thing"));
    jott_core::app_paths::make_private(Path::new(""));

    // A directory that cannot be created still reports the *io* cause, and not a mode failure: the caller's error
    // message is about the folder it could not make, which is what §4 asks a refusal to name.
    let impossible = std::path::PathBuf::from("/proc/jott-cannot-make-this");
    let error = jott_core::app_paths::create_private_dir_all(&impossible).expect_err("/proc is not ours to write");
    assert!(matches!(error.kind(), std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::NotFound), "{error}");
}
