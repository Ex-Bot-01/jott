//! Reading and writing notes the way §18.8 demands: **the bytes, all the bytes, nothing but the
//! bytes**. Hashes are SHA-256 over what is on disk, so a save that "tidies" line endings or adds
//! the newline an editor thinks a file should end with tells np that the whole vault changed and
//! re-embeds it forever. Windows is the platform where this bites first (§20).

use jott_core::hashes::sha256_file;
use jott_core::vault::files::{self, FileError, RenameReport, Trashed};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn vault() -> (TempDir, PathBuf) {
    let tmp = tempfile::tempdir().expect("temp dir");
    let root = tmp.path().join("vault");
    std::fs::create_dir_all(&root).unwrap();
    (tmp, root)
}

fn write_raw(root: &Path, rel: &str, bytes: impl AsRef<[u8]>) {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, bytes.as_ref()).unwrap();
}

// ---- byte fidelity (spec §18.8)

#[test]
fn crlf_survives_a_read_and_write_byte_for_byte() {
    let (_dir, root) = vault();
    let bytes = b"line one\r\nline two\r\n";
    write_raw(&root, "crlf.md", bytes);
    let before = sha256_file(&root.join("crlf.md")).unwrap();

    let read = files::read_note(&root, "crlf.md").unwrap();
    assert_eq!(read, bytes, "what came back is what is on disk, CRLF and all");
    files::write_note(&root, "crlf.md", &read).unwrap();

    assert_eq!(sha256_file(&root.join("crlf.md")).unwrap(), before, "np sees no change, so it re-embeds nothing");
}

#[test]
fn a_file_without_a_trailing_newline_keeps_none() {
    let (_dir, root) = vault();
    write_raw(&root, "short.md", b"# No newline at the end");
    let read = files::read_note(&root, "short.md").unwrap();
    files::write_note(&root, "short.md", &read).unwrap();
    assert_eq!(std::fs::read(root.join("short.md")).unwrap(), b"# No newline at the end");
}

#[test]
fn a_utf8_bom_survives() {
    // np strips a BOM when *parsing* a note but hashes the raw bytes, so stripping here would make
    // our stamp disagree with np's row.
    let (_dir, root) = vault();
    write_raw(&root, "bom.md", "\u{feff}# Exported from Word\n".as_bytes());
    let before = sha256_file(&root.join("bom.md")).unwrap();

    let read = files::read_note(&root, "bom.md").unwrap();
    files::write_note(&root, "bom.md", &read).unwrap();

    assert_eq!(std::fs::read(root.join("bom.md")).unwrap()[..3], [0xEF, 0xBB, 0xBF]);
    assert_eq!(sha256_file(&root.join("bom.md")).unwrap(), before);
}

#[test]
fn a_write_lands_exactly_the_bytes_it_was_given() {
    let (_dir, root) = vault();
    let bytes: Vec<u8> = vec![0xF0, 0x9F, 0x92, 0xA9, 0x0A, b'\r', 0x00, 0x41]; // emoji, LF, CR, NUL, 'A'
    files::write_note(&root, "odd.md", &bytes).unwrap();
    assert_eq!(std::fs::read(root.join("odd.md")).unwrap(), bytes, "no encoding pass at all");
}

// ---- the guard (every entry point)

#[test]
fn a_rel_that_climbs_out_of_the_vault_is_refused() {
    let (_dir, root) = vault();
    for rel in ["../escape.md", "notes/../../escape.md", "a/b/../../../out.md"] {
        assert!(
            matches!(files::read_note(&root, rel), Err(FileError::EscapesRoot { .. })),
            "{rel} must not be readable"
        );
    }
}

#[test]
fn an_absolute_rel_is_refused() {
    let (_dir, root) = vault();
    for rel in ["/etc/passwd", "C:\\Windows\\notepad.exe"] {
        let err = files::read_note(&root, rel).unwrap_err();
        assert!(matches!(err, FileError::EscapesRoot { .. }), "{rel}: {err}");
    }
}

#[test]
fn a_symlink_pointing_out_of_the_vault_is_refused() {
    let (_dir, root) = vault();
    let outside = tempfile::tempdir().unwrap();
    let secret = outside.path().join("secret.md");
    std::fs::write(&secret, "not in the vault\n").unwrap();
    symlink(&root.join("link.md"), &secret);

    assert!(matches!(files::read_note(&root, "link.md"), Err(FileError::EscapesRoot { .. })));
    // The guard is on every entry point, not just the one that reads.
    assert!(matches!(files::write_note(&root, "link.md", b"overwritten\n"), Err(FileError::EscapesRoot { .. })));
    assert!(matches!(files::delete_to_trash(&root, "link.md"), Err(FileError::EscapesRoot { .. })));
}

#[test]
fn a_symlink_to_a_folder_outside_the_vault_is_refused() {
    let (_dir, root) = vault();
    let outside = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(outside.path().join("elsewhere")).unwrap();
    std::fs::write(outside.path().join("elsewhere/x.md"), "outside\n").unwrap();
    symlink(&root.join("through"), &outside.path().join("elsewhere"));

    assert!(matches!(
        files::read_note(&root, "through/x.md"),
        Err(FileError::EscapesRoot { .. })
    ));
}

#[test]
fn a_symlink_that_stays_inside_the_vault_is_readable() {
    let (_dir, root) = vault();
    write_raw(&root, "notes/real.md", "# real\n");
    symlink(&root.join("alias.md"), &root.join("notes/real.md"));
    assert_eq!(files::read_note(&root, "alias.md").unwrap(), b"# real\n");
}

/// `symlink(link, target)` — the link is created at the first path and points at the second.
#[cfg(unix)]
fn symlink(link: &Path, target: &Path) {
    std::os::unix::fs::symlink(target, link).unwrap();
}

#[cfg(windows)]
fn symlink(link: &Path, target: &Path) {
    // Windows needs a privilege for symlinks; junctions cover the directory case and are what a
    // sync client would leave behind anyway.
    if target.is_dir() {
        let ok = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        assert!(ok, "could not make a junction for the test");
    } else {
        std::os::windows::fs::symlink_file(target, link).expect("symlink_file (needs dev mode)");
    }
}

// ---- long paths on Windows (spec §18.8: np does this, so must we)

#[test]
fn a_windows_path_at_260_chars_gets_one_extended_length_prefix() {
    let path = r"C:\Users\someone\Notes".to_string() + &"\\a-very-long-note-name-part".repeat(9);
    assert!(path.len() >= 260, "the fixture has to be long enough to matter: {}", path.len());
    let prefixed = files::extended_length_prefix(&path);
    assert_eq!(prefixed, format!(r"\\?\{path}"), "np's rule, applied once");
    // Applying it twice is the bug that makes a path nobody can open.
    assert_eq!(files::extended_length_prefix(&prefixed), prefixed);
}

#[test]
fn a_short_windows_path_is_left_alone() {
    let path = r"C:\Users\someone\Notes\note.md".to_string();
    assert!(path.len() < 260);
    assert_eq!(files::extended_length_prefix(&path), path, "a prefix nobody needs is a path nobody recognises");
}

#[test]
fn the_windows_containment_check_folds_case_and_the_long_path_prefix() {
    // Runs on every platform: this comparison is what decides whether a symlink may reach outside the
    // vault, and it has to be tested before Windows is the first machine to run it.
    assert!(files::windows_path_is_under(r"C:\Users\ana\Notes", r"C:\users\ana\notes\a.md"));
    assert!(files::windows_path_is_under(r"C:\Notes", r"C:\Notes"), "the root is inside itself");
    assert!(
        files::windows_path_is_under(r"C:\Notes", &format!(r"\\?\C:\Notes\{}", "d".repeat(300))),
        "a long prefixed path and its unprefixed root are the same location"
    );
    assert!(files::windows_path_is_under(r"\\?\UNC\srv\vault", r"\\?\UNC\srv\vault\a.md"));

    assert!(!files::windows_path_is_under(r"C:\Notes", r"C:\NotesEvil\a.md"), "a shared prefix is not a parent");
    assert!(!files::windows_path_is_under(r"C:\Notes", r"C:\Other\a.md"));
    assert!(files::windows_path_is_under(r"C:\Notes", "C:/Notes/a.md"), "Windows accepts either separator, and so does this");
    // Dot-segments belong to the caller: `PathGuard` refuses `..` before any of this is asked.
    assert!(!files::windows_path_is_under(r"C:\Notes", r"C:"), "a root is not inside its own child");
}

#[test]
fn jott_does_not_write_over_nps_own_catalogs() {
    let (_dir, root) = vault();
    write_raw(&root, "INDEX.md", "# INDEX\n_generated by note-processor; do not edit_\n");
    write_raw(&root, "a-note.md", "# mine\n");

    // §9 greys these in the tree; refusing here is what makes the grey more than decoration.
    assert!(matches!(files::write_note(&root, "INDEX.md", b"oops\n"), Err(FileError::NpGenerated { .. })));
    assert!(matches!(files::create_note(&root, "INDEX-people.md"), Err(FileError::NpGenerated { .. })));
    assert!(matches!(files::delete_to_trash(&root, "INDEX.md"), Err(FileError::NpGenerated { .. })));
    let err = files::rename(&root, "a-note.md", "INDEX.md").unwrap_err();
    assert!(matches!(err, FileError::NpGenerated { .. }), "{err}");
    assert_eq!(
        std::fs::read(root.join("INDEX.md")).unwrap(),
        b"# INDEX\n_generated by note-processor; do not edit_\n",
        "the catalog is untouched"
    );

    // A nested `INDEX.md` is somebody's note, and np agrees (§9): the rule is about np's own names at
    // the root, not about every file wearing that name.
    files::create_note(&root, "notes/INDEX.md").unwrap();
}

#[test]
fn to_open_path_leaves_a_unix_path_exactly_as_it_is() {
    // The probe switch exists so this can be asserted on every platform the suite runs on.
    std::env::set_var("JOTT_LONG_PATH_PROBE", "1");
    let (_dir, root) = vault();
    // Length from the folders: 255 bytes is a *name* limit, so one long name is not a long path.
    let deep = format!("{}/note.md", "a-long-folder-name/".repeat(12));
    write_raw(&root, &deep, "# deep\n");

    let opened = files::to_open_path(&root.join(&deep));
    if cfg!(windows) {
        assert!(opened.starts_with(r"\\?\"), "on Windows a long path is prefixed: {}", opened.display());
    } else {
        assert_eq!(opened, root.join(&deep), "Linux has no MAX_PATH to route around: {}", opened.display());
    }
    assert_eq!(files::read_note(&root, &deep).unwrap(), b"# deep\n", "and it still opens");
    std::env::remove_var("JOTT_LONG_PATH_PROBE");
}

// ---- create, rename, move, delete

#[test]
fn a_new_note_is_created_and_an_existing_one_is_never_touched() {
    let (_dir, root) = vault();
    write_raw(&root, "exists.md", "# mine\n");

    files::create_note(&root, "fresh.md").unwrap();
    assert!(root.join("fresh.md").is_file());

    let err = files::create_note(&root, "exists.md").unwrap_err();
    assert!(matches!(err, FileError::AlreadyExists { .. }), "{err}");
    assert_eq!(std::fs::read(root.join("exists.md")).unwrap(), b"# mine\n", "the refusal wrote nothing");
}

#[test]
fn creating_a_note_creates_the_folder_it_lives_in() {
    let (_dir, root) = vault();
    files::create_note(&root, "projects/2026/new.md").unwrap();
    assert!(root.join("projects/2026/new.md").is_file());
}

#[test]
fn a_rename_moves_the_bytes_and_reports_the_links_that_point_at_it() {
    let (_dir, root) = vault();
    write_raw(&root, "notes/q3-review.md", "# Q3\n");
    write_raw(
        &root,
        "notes/other.md",
        b"see [Q3](notes/q3-review.md) and [[q3-review]] plus a mention of q3-review.md in prose\n",
    );

    let RenameReport { incoming_links } =
        files::rename(&root, "notes/q3-review.md", "archive/2026/q2-review.md").unwrap();

    assert!(!root.join("notes/q3-review.md").exists());
    assert_eq!(std::fs::read(root.join("archive/2026/q2-review.md")).unwrap(), b"# Q3\n");
    // Detection only in M1 (§13): the links are reported, never rewritten.
    let mut sources: Vec<&str> = incoming_links.iter().map(|l| l.src.as_str()).collect();
    sources.dedup();
    assert_eq!(sources, ["notes/other.md"], "one note holds two links to that path");
    assert!(incoming_links.iter().all(|l| l.line >= 1), "{incoming_links:?}");
    assert_eq!(incoming_links.len(), 2, "the markdown link and the wikilink: {incoming_links:?}");
}

#[test]
fn a_rename_never_overwrites_something_that_is_already_there() {
    let (_dir, root) = vault();
    write_raw(&root, "a.md", "# A\n");
    write_raw(&root, "b.md", "# B\n");

    let err = files::rename(&root, "a.md", "b.md").unwrap_err();
    assert!(matches!(err, FileError::AlreadyExists { .. }), "{err}");
    assert_eq!(std::fs::read(root.join("b.md")).unwrap(), b"# B\n", "the target survived");
    assert_eq!(std::fs::read(root.join("a.md")).unwrap(), b"# A\n", "and so did the source");
}

#[test]
fn renaming_something_that_is_not_there_says_so() {
    let (_dir, root) = vault();
    let err = files::rename(&root, "ghost.md", "here.md").unwrap_err();
    assert!(matches!(err, FileError::NotFound { .. }), "{err}");
}

#[test]
fn a_move_takes_a_folder_and_everything_inside_it() {
    let (_dir, root) = vault();
    write_raw(&root, "inbox/deep/a.md", "# A\n");
    write_raw(&root, "inbox/deep/b.md", "# B\n");

    files::move_entry(&root, "inbox/deep", "projects/2026/deep").unwrap();

    assert!(!root.join("inbox/deep").exists());
    assert_eq!(std::fs::read(root.join("projects/2026/deep/a.md")).unwrap(), b"# A\n");
    assert_eq!(std::fs::read(root.join("projects/2026/deep/b.md")).unwrap(), b"# B\n");
}

#[test]
fn a_move_that_would_put_a_folder_inside_itself_is_refused() {
    let (_dir, root) = vault();
    write_raw(&root, "inbox/a.md", "# A\n");
    let err = files::move_entry(&root, "inbox", "inbox/2026/inbox").unwrap_err();
    assert!(matches!(err, FileError::IntoItself { .. }), "{err}");
    assert!(root.join("inbox/a.md").is_file(), "nothing moved: {err}");
}

#[test]
fn a_deletion_says_where_the_file_went() {
    let (_dir, root) = vault();
    write_raw(&root, "notes/gone.md", "# gone\n");

    let trashed = files::delete_to_trash(&root, "notes/gone.md").unwrap();
    assert!(!root.join("notes/gone.md").exists(), "it is out of the vault either way");

    match trashed {
        // The OS trash: Jott cannot read it back to prove anything, and does not claim to.
        Trashed::Os => {}
        Trashed::AppDir(path) => {
            assert!(path.is_file(), "{} is where the bytes are now", path.display());
            assert_eq!(std::fs::read(&path).unwrap(), b"# gone\n", "the fallback keeps the bytes, not a copy of a name");
            assert!(path.starts_with(jott_core::app_paths::app_data_dir().unwrap()), "inside the app data dir: {}", path.display());
        }
    }
}

#[test]
fn two_deletions_of_the_same_name_do_not_overwrite_each_other() {
    let (_dir, root) = vault();
    write_raw(&root, "gone.md", "# first\n");
    let first = files::delete_to_trash(&root, "gone.md").unwrap();
    write_raw(&root, "gone.md", "# second\n");
    let second = files::delete_to_trash(&root, "gone.md").unwrap();

    if let (Trashed::AppDir(a), Trashed::AppDir(b)) = (&first, &second) {
        assert_ne!(a, b, "one trash file cannot hold both notes: {} / {}", a.display(), b.display());
        assert_eq!(std::fs::read(a).unwrap(), b"# first\n");
        assert_eq!(std::fs::read(b).unwrap(), b"# second\n");
    }
    // On a desktop with a working OS trash both are `Os` and the OS names them; nothing to assert.
}

#[test]
fn deleting_a_folder_removes_its_contents_and_says_where() {
    let (_dir, root) = vault();
    write_raw(&root, "inbox/a.md", "# A\n");
    write_raw(&root, "inbox/b.md", "# B\n");

    files::delete_to_trash(&root, "inbox").unwrap();
    assert!(!root.join("inbox").exists());
}

#[test]
fn reading_a_note_that_is_not_there_says_so() {
    let (_dir, root) = vault();
    let err = files::read_note(&root, "ghost.md").unwrap_err();
    assert!(matches!(err, FileError::NotFound { .. }), "{err}");
}

#[test]
fn reading_a_note_bigger_than_the_editor_refuses_by_name() {
    let (_dir, root) = vault();
    // Smaller than the real limit only in the sense that it *is* the real limit: one stray video
    // export must not be swallowed into a buffer (Task 7 marks it `too-large` in the tree).
    write_raw(&root, "huge.md", vec![b'x'; jott_core::vault::walk::MAX_NOTE_BYTES as usize + 1]);
    let err = files::read_note(&root, "huge.md").unwrap_err();
    assert!(matches!(err, FileError::TooLarge { .. }), "{err}");
}

#[test]
fn a_save_records_itself_so_the_watcher_can_stay_quiet() {
    // Task 8's own suppression registry, seen from the outside: our write is not an external change.
    let (_dir, root) = vault();
    files::write_note(&root, "saved.md", b"# saved\n").unwrap();
    assert!(files::is_our_write("saved.md"), "the watcher would otherwise reload what we just wrote");
    assert!(!files::is_our_write("someone-else.md"));
}
