//! Per-file stamps: what np last proved about each file, so the tree can answer "waiting" without
//! asking np (§9, §18.2 — np reports aggregates, so this store is the only honest source).
//!
//! A stamp is about **bytes**, not paths: it records the hash np saw, and a different hash means
//! whatever np said no longer applies.

use jott_core::hashes::sha256_file;
use jott_core::vault::stamps::{StampState, Stamps};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// A vault folder, plus the guard that cleans it up.
fn vault() -> (TempDir, PathBuf) {
    let tmp = tempfile::tempdir().expect("temp dir");
    let root = tmp.path().join("vault");
    std::fs::create_dir_all(&root).unwrap();
    (tmp, root)
}

fn stamps_in(root: &Path, vault: &Path) -> Stamps {
    Stamps::with_root(root.to_path_buf(), vault)
}

fn write_note(vault: &Path, rel: &str, body: &str) -> String {
    let path = vault.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&path, body).unwrap();
    sha256_file(&path).expect("hash the note just written")
}

#[test]
fn a_file_that_was_never_seen_is_waiting() {
    let (_dir, vault) = vault();
    let stamps = stamps_in(&vault.join("appdata"), &vault);
    assert_eq!(stamps.state_of("notes/a.md", "whatever-hash"), StampState::Waiting);
}

#[test]
fn the_hash_np_embedded_reads_as_embedded() {
    let (_dir, vault) = vault();
    let hash = write_note(&vault, "a.md", "# A\n");
    let mut stamps = stamps_in(&vault.join("appdata"), &vault);
    stamps.mark_indexed("a.md", &hash);
    stamps.mark_embedded("a.md", &hash);
    assert_eq!(stamps.state_of("a.md", &hash), StampState::Embedded);
}

#[test]
fn indexed_without_embedding_reads_as_indexed_not_embedded() {
    // The offline case, and the one §11 says must not be dressed up as finished: np indexed the
    // bytes and could not fetch a model, so the file is *indexed*.
    let (_dir, vault) = vault();
    let hash = write_note(&vault, "a.md", "# A\n");
    let mut stamps = stamps_in(&vault.join("appdata"), &vault);
    stamps.mark_indexed("a.md", &hash);
    assert_eq!(stamps.state_of("a.md", &hash), StampState::Indexed);
}

#[test]
fn a_hash_that_matches_neither_is_waiting_again() {
    let (_dir, vault) = vault();
    let old = write_note(&vault, "a.md", "# A\n");
    let mut stamps = stamps_in(&vault.join("appdata"), &vault);
    stamps.mark_indexed("a.md", &old);
    stamps.mark_embedded("a.md", &old);

    let fresh = write_note(&vault, "a.md", "# A, edited\n");
    assert_ne!(old, fresh, "the test needs the edit to change the hash");
    assert_eq!(stamps.state_of("a.md", &fresh), StampState::Waiting);
    // The old bytes are still what np holds — but nobody should ask about them any more, and if
    // they do, the answer is "embedded", not "waiting".
    assert_eq!(stamps.state_of("a.md", &old), StampState::Embedded);
}

#[test]
fn a_refusal_survives_a_reload_and_names_its_reason() {
    let (_dir, vault) = vault();
    let root = vault.join("appdata");
    let hash = write_note(&vault, "a.md", "# A\n");
    let mut stamps = stamps_in(&root, &vault);
    stamps.mark_refused("a.md", "over-budget");

    assert_eq!(stamps.state_of("a.md", &hash), StampState::Refused("over-budget".into()));
    stamps.save().unwrap();

    let reloaded = stamps_in(&root, &vault);
    assert_eq!(
        reloaded.state_of("a.md", &hash),
        StampState::Refused("over-budget".into()),
        "a refusal that vanished on restart would quietly become \"waiting\" and then nothing"
    );
}

#[test]
fn a_later_successful_index_clears_a_refusal() {
    let (_dir, vault) = vault();
    let root = vault.join("appdata");
    let hash = write_note(&vault, "a.md", "# A\n");
    let mut stamps = stamps_in(&root, &vault);
    stamps.mark_refused("a.md", "identity-taken");
    assert!(matches!(stamps.state_of("a.md", &hash), StampState::Refused(_)));

    stamps.mark_indexed("a.md", &hash);
    assert_eq!(stamps.state_of("a.md", &hash), StampState::Indexed, "np indexed it, so the refusal is history");
    stamps.save().unwrap();

    let reloaded = stamps_in(&root, &vault);
    assert_eq!(reloaded.state_of("a.md", &hash), StampState::Indexed);
}

#[test]
fn embedding_a_file_that_was_only_refused_clears_the_refusal_too() {
    let (_dir, vault) = vault();
    let hash = write_note(&vault, "a.md", "# A\n");
    let mut stamps = stamps_in(&vault.join("appdata"), &vault);
    stamps.mark_refused("a.md", "image-refused");
    stamps.mark_embedded("a.md", &hash);
    assert_eq!(stamps.state_of("a.md", &hash), StampState::Embedded);
}

#[test]
fn two_vaults_do_not_share_stamps() {
    let (_one_dir, one) = vault();
    let (_two_dir, two) = vault();
    let root = one.join("shared-appdata");

    let hash_one = write_note(&one, "a.md", "# One\n");
    let mut stamps = stamps_in(&root, &one);
    stamps.mark_embedded("a.md", &hash_one);

    let other = stamps_in(&root, &two);
    assert_eq!(other.state_of("a.md", &hash_one), StampState::Waiting, "one vault's index says nothing about another's");
}

#[test]
fn saving_leaves_the_state_file_and_no_temp_files_behind() {
    let (_dir, vault) = vault();
    let root = vault.join("appdata");
    let hash = write_note(&vault, "a.md", "# A\n");
    let mut stamps = stamps_in(&root, &vault);
    stamps.mark_indexed("a.md", &hash);
    stamps.save().expect("save");

    let dir = state_dir(&root);
    let names: Vec<String> = std::fs::read_dir(&dir)
        .expect("the vault's state directory")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert!(names.iter().any(|n| n == "state.json"), "{names:?}");
    assert!(
        !names.iter().any(|n| n.contains("tmp")),
        "the rename happened, it did not leave debris: {names:?}"
    );
}

#[test]
fn a_corrupt_state_file_forgets_the_stamps_and_says_nothing_else() {
    // Losing these costs markers in a tree, not notes: unlike `vaults.json`, nothing here is the
    // only copy of anything, so it is rebuilt rather than quarantined.
    let (_dir, vault) = vault();
    let root = vault.join("appdata");
    let hash = write_note(&vault, "a.md", "# A\n");
    let mut stamps = stamps_in(&root, &vault);
    stamps.mark_embedded("a.md", &hash);
    stamps.save().unwrap();

    let file = state_dir(&root).join("state.json");
    std::fs::write(&file, b"{\"files\": {\"a.md\": {\"inde").unwrap();

    let reloaded = stamps_in(&root, &vault);
    assert_eq!(reloaded.state_of("a.md", &hash), StampState::Waiting, "unparsable means we know nothing, not that it is embedded");
}

#[test]
fn a_stamped_note_survives_a_restart_without_being_rescanned() {
    let (_dir, vault) = vault();
    let root = vault.join("appdata");
    let hash = write_note(&vault, "notes/deep/a.md", "# A\n");
    let mut stamps = stamps_in(&root, &vault);
    stamps.mark_embedded("notes/deep/a.md", &hash);
    stamps.save().unwrap();

    let reloaded = stamps_in(&root, &vault);
    assert_eq!(reloaded.state_of("notes/deep/a.md", &hash), StampState::Embedded);
}

/// Where `Stamps` keeps its file for this vault: the app data dir, keyed by the vault's absolute
/// path the way np keys a workspace (§18.6) — reached here through a save, not by rebuilding the
/// derivation in a test.
fn state_dir(app_data: &Path) -> PathBuf {
    let mut candidates = Vec::new();
    let mut stack = vec![app_data.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.file_name().map(|n| n == "state.json").unwrap_or(false) {
                candidates.push(path);
            }
        }
    }
    assert_eq!(candidates.len(), 1, "exactly one state file for one vault: {candidates:?}");
    candidates[0].parent().expect("a file has a parent").to_path_buf()
}
