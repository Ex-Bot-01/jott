//! What Jott does when the file under someone's cursor changes (§14), and what it refuses to
//! notice. `decide` is pure, so the whole policy is testable without a disk; the watcher is thin
//! plumbing around it that turns OS events into `rel` paths and stays quiet about our own saves.

use jott_core::vault::files::{self, ReloadPolicy};
use jott_core::vault::watch::{SelfWrites, VaultWatch};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use tempfile::TempDir;

fn vault() -> (TempDir, PathBuf) {
    let tmp = tempfile::tempdir().expect("temp dir");
    let root = tmp.path().join("vault");
    std::fs::create_dir_all(root.join("notes")).unwrap();
    (tmp, root)
}

fn write_raw(root: &Path, rel: &str, bytes: impl AsRef<[u8]>) {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes.as_ref()).unwrap();
}

fn seconds_ago(secs: u64) -> SystemTime {
    SystemTime::now() - Duration::from_secs(secs)
}

// ---- the policy (spec §14)

#[test]
fn identical_bytes_are_no_change() {
    let read_at = seconds_ago(60);
    assert_eq!(
        decide(false, "same", "same", read_at, read_at),
        ReloadPolicy::NoChange,
        "the same bytes are not a conflict, whoever wrote them"
    );
}

#[test]
fn our_own_save_reads_as_no_change_even_though_the_clock_moved() {
    // The watcher will still see this write; the policy is what stops Jott reloading a buffer the
    // user is looking at because it just wrote the same bytes to disk.
    let read_at = seconds_ago(60);
    assert_eq!(decide(false, "abc", "abc", SystemTime::now(), read_at), ReloadPolicy::NoChange);
}

#[test]
fn a_clean_buffer_follows_the_disk() {
    let read_at = seconds_ago(60);
    assert_eq!(
        decide(false, "old", "new", SystemTime::now(), read_at),
        ReloadPolicy::ReloadSilently,
        "nothing of ours would be lost"
    );
}

#[test]
fn a_dirty_buffer_keeps_its_text_and_says_why() {
    let read_at = seconds_ago(60);
    assert_eq!(
        decide(true, "old", "new", SystemTime::now(), read_at),
        ReloadPolicy::KeepAndWarn,
        "§14: never overwrite unsaved work without asking"
    );
}

#[test]
fn a_touched_file_with_the_same_bytes_is_still_no_change() {
    // Sync clients and `git checkout` rewrite files with identical contents; the mtime moves and
    // the bytes do not, and re-rendering an editor for nothing loses a cursor at best.
    let read_at = seconds_ago(60);
    assert_eq!(decide(false, "same", "same", SystemTime::now(), read_at), ReloadPolicy::NoChange);
}

#[test]
fn a_different_hash_on_a_disk_nobody_touched_is_our_own_editing() {
    // Nothing external happened since we read the file, so the difference is the user typing. There
    // is nothing to reload — and if the clock granularity hid a real second writer, keeping the
    // buffer is the direction §14 protects.
    let read_at = SystemTime::now();
    assert_eq!(decide(false, "buffer", "disk", seconds_ago(60), read_at), ReloadPolicy::NoChange);
    assert_eq!(decide(true, "buffer", "disk", seconds_ago(60), read_at), ReloadPolicy::NoChange);
}

fn decide(dirty: bool, buffer_hash: &str, disk_hash: &str, disk_mtime: SystemTime, last_read: SystemTime) -> ReloadPolicy {
    jott_core::vault::files::decide(dirty, disk_mtime, last_read, buffer_hash, disk_hash)
}

// ---- what the watcher is allowed to report

#[test]
fn a_change_to_a_dot_file_is_not_reported() {
    // The tree does not show dot-names (Task 7), so there is nothing on screen to update — and
    // `.git` churn would otherwise wake the editor on every branch switch.
    let writes = SelfWrites::new();
    assert!(!jott_core::vault::watch::should_report(".git/HEAD", &writes));
    assert!(!jott_core::vault::watch::should_report("notes/.draft.md", &writes));
    assert!(jott_core::vault::watch::should_report("notes/a.md", &writes));
}

#[test]
fn a_write_of_ours_is_not_an_external_change() {
    let writes = SelfWrites::with_window(Duration::from_secs(30));
    assert!(jott_core::vault::watch::should_report("notes/a.md", &writes), "nothing was written yet, so a change there is somebody else's");
    writes.record("notes/a.md");
    assert!(!jott_core::vault::watch::should_report("notes/a.md", &writes), "we wrote it");
    assert!(jott_core::vault::watch::should_report("notes/b.md", &writes), "somebody else wrote that one");
}

#[test]
fn the_suppression_expires_and_a_real_change_gets_through() {
    let writes = SelfWrites::with_window(Duration::from_millis(50));
    writes.record("notes/a.md");
    std::thread::sleep(Duration::from_millis(120));
    assert!(jott_core::vault::watch::should_report("notes/a.md", &writes), "the window is 200 ms in production precisely so it does not swallow the next real edit");
}

// ---- the watcher itself

#[test]
fn a_change_from_outside_arrives_as_a_rel_path() {
    let (_dir, root) = vault();
    write_raw(&root, "notes/edited.md", b"# first\n");
    let mut watch = VaultWatch::start(&root, SelfWrites::new().shared()).expect("a watcher backend here");

    // Somebody else's editor, three seconds later.
    std::fs::write(root.join("notes/edited.md"), b"# changed by someone else\n").unwrap();

    assert_eq!(watch.next_change(Duration::from_secs(10)).as_deref(), Some("notes/edited.md"));
}

#[test]
fn our_own_save_is_quiet() {
    let (_dir, root) = vault();
    write_raw(&root, "notes/saved.md", b"# first\n");
    // A wide window here, so a slow callback cannot arrive after the 200 ms default and turn our
    // own save into a reload prompt. Production uses 200 ms (the plan's number).
    let writes = SelfWrites::with_window(Duration::from_secs(30)).shared();
    let mut watch = VaultWatch::start(&root, writes.clone()).expect("a watcher backend here");

    files::write_note_with(&root, "notes/saved.md", b"# saved by jott\n", &writes).unwrap();

    assert_eq!(watch.next_change(Duration::from_secs(3)), None, "Jott does not announce its own write back to the user");
}

#[test]
fn a_new_file_somewhere_in_the_vault_arrives() {
    let (_dir, root) = vault();
    let mut watch = VaultWatch::start(&root, SelfWrites::new().shared()).expect("a watcher backend here");
    write_raw(&root, "notes/new.md", b"# new\n");
    assert_eq!(watch.next_change(Duration::from_secs(10)).as_deref(), Some("notes/new.md"));
}
