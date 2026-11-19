//! Symlinks inside a vault: Jott has a rule about them in one place and none in another (issue #10).
//!
//! `PathGuard::resolve` walks a `rel` component by component, canonicalises every symlink it meets and requires the
//! target to stay inside the root — so reading and writing *through* a link that stays in the vault is deliberate.
//! The tree walk does the opposite: `WalkDir::new(root).follow_links(false)` (walk.rs:88), and `entry_for` then
//! decides a row's kind from `file_type().is_dir()`, which is false for a link to a folder — so a link fell through
//! to the extension match and became a Note, an Image or an Attachment **by name alone**, with a marker computed
//! from the target's bytes next to a size read from the link's own `lstat`. Two rulers, one row.
//!
//! The rule this repo picks is the issue's second branch — **keep not following them, and say so** — because the
//! first branch (`follow_links(true)`) follows *every* link, including `notes -> /home/ana/private`, which would put
//! files outside the vault in the tree, in search results, and in np's index, while `PathGuard` still refuses to
//! write through them. A tree that shows what the app will not touch is a worse disagreement than one that says
//! *I do not follow this*. Cycles are the other cost the issue names (`notes/parent -> ..`).
//!
//! So: a symlink is never presented as a note, an image or an attachment chosen by its name; it carries the reason
//! `symlink`, and when the target cannot be read at all, `unreadable`. Its size and mtime come from the target when
//! it resolves, so the number on the row and the reason beside it are about the same bytes. Symlinked notes join
//! `note_rels`, because rename's link scan should read whatever the guard will let it read. A dangling link answers
//! `NotFound` with a vault-relative `rel`, matching what the tree already said, instead of an absolute path inside
//! an OS error.
//!
//! `#[cfg(unix)]`: making a symlink on Windows needs privileges, so this would not run on every runner — which is
//! exactly how the disagreement got shipped in the first place.

#![cfg(unix)]

use jott_core::vault::files::{self, FileError};
use jott_core::vault::stamps::Stamps;
use jott_core::vault::stamps::StampState;
use jott_core::vault::walk::{self, EntryKind, TreeEntry};
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

fn vault() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().expect("temp dir");
    let root = tmp.path().join("vault");
    std::fs::create_dir_all(&root).unwrap();
    (tmp, root)
}

fn write(root: &Path, rel: &str, bytes: &[u8]) {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

fn row(root: &Path, rel: &str) -> TreeEntry {
    let stamps = Stamps::with_root(root.join(".jott-state"), root);
    let entries = walk::walk(root, &stamps).expect("the tree lists");
    entries.into_iter().find(|e| e.rel == rel).unwrap_or_else(|| panic!("no row for {rel}: {:?}", names(root)))
}

/// What the tree actually drew, for a failure message a person can read.
fn names(root: &Path) -> Vec<String> {
    let stamps = Stamps::with_root(root.join(".jott-state"), root);
    walk::walk(root, &stamps).expect("the tree lists").into_iter().map(|e| e.rel).collect()
}

/// A folder that is a symlink used to be one grey `Attachment` row, because no extension matched and
/// `file_type().is_dir()` is false for a link. Now it says what it is, and the rule it is following.
#[test]
fn a_symlinked_folder_says_it_is_a_link_rather_than_an_attachment() {
    let (_tmp, root) = vault();
    let outside = _tmp.path().join("elsewhere.md");
    std::fs::write(&outside, b"# elsewhere\n").unwrap();
    symlink(&outside, root.join("notes")).expect("make the link");

    let entry = row(&root, "notes");
    assert_eq!(
        entry.state,
        Some(StampState::Refused("symlink".into())),
        "the row explains itself instead of guessing a kind from the name: {entry:?}"
    );
    // The chosen rule, pinned rather than implied: its contents are not walked, so they get no rows and no markers.
    let rels: Vec<String> = walk::note_rels(&root).expect("rels") ;
    assert!(rels.is_empty(), "nothing below a link is indexed while links are not followed: {rels:?}");
}

/// The two-rulers row: a 14-byte link to a 4 KB note used to read "14 B" beside a dot that was a verdict about the
/// target's bytes. Size now comes from the same thing the reason is about.
#[test]
fn a_symlinked_note_measures_the_bytes_it_is_talking_about() {
    let (_tmp, root) = vault();
    write(&root, "real.md", &vec![b'x'; 4096]);
    symlink(root.join("real.md"), root.join("link.md")).expect("make the link");

    let entry = row(&root, "link.md");
    assert_eq!(entry.size, 4096, "the target's length, not the link's 8-byte name: {entry:?}");
    assert_eq!(entry.kind, EntryKind::Note, "a `.md` link is still shown as a note-shaped thing");
    assert_eq!(
        entry.state,
        Some(StampState::Refused("symlink".into())),
        "and it never claims `indexed` for bytes the walk did not read"
    );

    let real = row(&root, "real.md");
    assert_ne!(real.state, entry.state, "the file itself keeps its own verdict: {real:?}");
}

/// Consequence three: the tree said *unreadable* and opening it said
/// "could not use /home/me/notes/old.md: No such file or directory", with an absolute path in the field every other
/// error carries a vault-relative one. The sentence now matches the row, because the answer is the same answer.
#[test]
fn a_dangling_link_is_not_there_in_the_tree_and_when_opened() {
    let (_tmp, root) = vault();
    symlink(root.join("gone.md"), root.join("old.md")).expect("make the dangling link");

    let entry = row(&root, "old.md");
    assert_eq!(entry.state, Some(StampState::Refused("unreadable".into())), "{entry:?}");

    let error = files::read_note(&root, "old.md").expect_err("there is nothing to read");
    match &error {
        FileError::NotFound { rel } => assert_eq!(rel, "old.md", "vault-relative, like every other refusal"),
        other => panic!("a dangling link should say it is not there, not quote the OS: {other:?}"),
    }
    assert!(!error.to_string().contains(&root.display().to_string()), "no absolute path in the sentence: {error}");
}

/// A link written in a symlinked note still points at the note being renamed. The scan reads what the guard will let
/// it read, so `note_rels` has to include the link — otherwise rename reports fewer incoming links than exist.
#[test]
fn a_rename_sees_links_written_inside_a_symlinked_note() {
    let (_tmp, root) = vault();
    write(&root, "target.md", b"# Target\n");
    write(&root, "somewhere/real.md", b"see [[target]]\n");
    symlink(root.join("somewhere/real.md"), root.join("alias.md")).expect("make the link");

    let report = files::rename(&root, "target.md", "moved.md").expect("renamed");
    let sources: Vec<String> = report.incoming_links.iter().map(|l| l.src.clone()).collect();
    assert!(sources.contains(&"alias.md".to_string()), "the link is a note the scan must open: {sources:?}");
    assert!(sources.contains(&"somewhere/real.md".to_string()), "and so is the file beside it: {sources:?}");
}

/// The guard's rule is unchanged and stays pinned here, because it is the reason the tree's rule was chosen the way
/// it was: a link that leaves the vault may be *listed* as a link, but nothing is read or written through it.
#[test]
fn a_link_out_of_the_vault_still_cannot_be_written_through() {
    let (_tmp, root) = vault();
    let outside = _tmp.path().join("private.md");
    std::fs::write(&outside, b"somebody else's file\n").unwrap();
    symlink(&outside, root.join("escape.md")).expect("make the link");

    let error = files::read_note(&root, "escape.md").expect_err("the guard refuses");
    assert!(matches!(error, FileError::EscapesRoot { .. }), "{error:?}");
    assert_eq!(std::fs::read_to_string(&outside).unwrap(), "somebody else's file\n", "untouched");
}
