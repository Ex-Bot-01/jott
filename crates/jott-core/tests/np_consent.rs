//! Consent, written where np will read it. §11 is specific: "Not now" changes `[models]
//! auto_fetch` **in the config file**, because env is frozen at spawn and the child exists before
//! consent does — and np re-reads config per request (§18.6), so flipping the key needs no respawn.
//! Everything else in that file is somebody's own configuration, so it has to come back unchanged.

use jott_core::ingest::consent::{auto_fetch_allowed, embedding_model_named, np_config_path, set_auto_fetch};

/// `NP_PATH`-style env games again: one variable, several threads.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn rich_config() -> String {
    "# kept on purpose: this file predates Jott\n\
     [log]\n\
     level = \"info\"  # verbose makes np slow on big vaults\n\
     \n\
     [models]\n\
     base_url = \"https://mirror.example/models\"  # my own mirror\n\
     auto_fetch = true\n\
     \n\
     [embeddings]\n\
     provider = \"local\"\n\
     model = \"bge-m3\"\n"
        .to_string()
}

#[test]
fn auto_fetch_is_yes_until_the_file_says_otherwise() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    assert!(auto_fetch_allowed(&path), "np's own default is yes (measured: `auto_fetch_default`)");

    std::fs::write(&path, "").unwrap();
    assert!(auto_fetch_allowed(&path), "an empty file says nothing, so the default stands");

    std::fs::write(&path, "[models]\nauto_fetch = false\n").unwrap();
    assert!(!auto_fetch_allowed(&path));

    std::fs::write(&path, "[models]\nauto_fetch = true\n").unwrap();
    assert!(auto_fetch_allowed(&path));
}

#[test]
fn declining_changes_one_line_and_nothing_else() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let before = rich_config();
    std::fs::write(&path, &before).unwrap();

    set_auto_fetch(&path, false).unwrap();

    let after = std::fs::read_to_string(&path).unwrap();
    let changed: Vec<(&str, &str)> = before
        .lines()
        .zip(after.lines())
        .filter(|(before, after)| before != after)
        .collect();
    assert_eq!(changed, vec![("auto_fetch = true", "auto_fetch = false")], "the diff is one key: {after}");
    assert_eq!(before.lines().count(), after.lines().count(), "no line added or dropped");
    assert!(after.contains("# kept on purpose"), "comments are the user's, not ours to lose");
    assert!(after.contains("base_url = \"https://mirror.example/models\"  # my own mirror"), "even a trailing one: {after}");
    assert!(after.contains("[log]\nlevel = \"info\"  # verbose makes np slow on big vaults\n"));
}

#[test]
fn granting_writes_true_and_keeps_every_line_that_was_there() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    // No `[models]` at all: the key has to arrive without disturbing what is.
    let before = "[embeddings]\nprovider = \"local\"\nmodel = \"bge-m3\"\n".to_string();
    std::fs::write(&path, &before).unwrap();

    set_auto_fetch(&path, true).unwrap();

    let after = std::fs::read_to_string(&path).unwrap();
    assert!(auto_fetch_allowed(&path), "{after}");
    for line in before.lines() {
        assert!(after.lines().any(|a| a == line), "lost a line: {line}\n{after}");
    }
}

#[test]
fn a_file_that_is_not_there_becomes_the_smallest_one_that_says_no() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested").join("config.toml");

    set_auto_fetch(&path, false).unwrap();

    assert!(!auto_fetch_allowed(&path));
    let text = std::fs::read_to_string(&path).unwrap();
    assert_eq!(text.lines().filter(|l| !l.trim().is_empty() && !l.starts_with('#')).count(), 2, "no invented keys: {text}");
    assert!(text.contains("[models]"));
    assert!(text.contains("auto_fetch = false"));
}

#[test]
fn a_config_jott_cannot_read_answers_no() {
    // np itself would refuse to load this file and say so on its own terms. Jott's reader must not
    // guess permission it cannot see: `true` here means promising a download the child will refuse.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "[models\n  auto_fetch = ").unwrap();
    assert!(!auto_fetch_allowed(&path));
}

#[test]
fn the_model_named_for_embedding_comes_from_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");

    std::fs::write(&path, rich_config()).unwrap();
    assert_eq!(embedding_model_named(&path).as_deref(), Some("bge-m3"));

    std::fs::write(&path, "[embeddings]\nprovider = \"local\"\n").unwrap();
    assert_eq!(embedding_model_named(&path), None, "a provider with no model is not a model");

    std::fs::write(&path, "# model = \"bge-m3\"\n").unwrap();
    assert_eq!(embedding_model_named(&path), None, "a commented-out line is a comment");

    assert_eq!(embedding_model_named(&dir.path().join("absent.toml")), None);
}

#[test]
fn the_config_path_is_nps_rule_not_jotts() {
    let _turn = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();

    std::env::set_var("XDG_CONFIG_HOME", dir.path());
    assert_eq!(np_config_path().unwrap(), dir.path().join("note-processor").join("config.toml"));

    std::env::remove_var("XDG_CONFIG_HOME");
    let previous_home = std::env::var_os("HOME");
    std::env::set_var("HOME", dir.path());
    assert_eq!(np_config_path().unwrap(), dir.path().join(".config").join("note-processor").join("config.toml"));

    match previous_home {
        Some(home) => std::env::set_var("HOME", home),
        None => std::env::remove_var("HOME"),
    }
}
