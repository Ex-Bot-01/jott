//! The command surface as the webview will call it. This is where "is the front end trusted?" and "what
//! does the window actually receive?" get answered, in a test binary, on any machine — because the shell
//! that hosts these commands only builds on a desktop, and a promise you can only check on one operating
//! system is not checked (§19).

mod common;

use common::repo_root;
use jott_core::diagnostics::{TraceEvent, TraceKind};
use jott_core::ingest::consent::auto_fetch_allowed;
use jott_core::ingest::queue::{IngestEvent, StateReason};
use jott_core::ipc::{
    self, ConnectionState, ConsentAnswerDto, ConsentHelp, ErrorKind, HitDto, NoteContent, RenameOutcome, SearchResults,
    VaultListing, WriteResult,
};
use jott_core::np::outcomes::{EmbedReport, ScanReport};
use jott_core::np::resolve::{NpMissing, NpSource, Os};
use jott_core::np::{NpFeatures, NpInfo};
use jott_core::vault::walk::EntryKind;
use jott_core::ingest::consent::AutoFetch;
use jott_core::ipc::VaultSession;
use jott_core::vault::stamps::StampState;
use jott_core::vault::store::{Consent, StoreError};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// These tests own `$XDG_DATA_HOME` (the app data dir holds their stamps, mirrors and trash) and the
/// stand-in's scenario file, both process-wide, so they take turns.
static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn info() -> NpInfo {
    NpInfo {
        version: "0.3.1".into(),
        protocol: 1,
        methods: vec!["initialize", "search", "scan", "embed", "models.list", "status.get", "shutdown"]
            .into_iter()
            .map(str::to_string)
            .collect(),
        features: NpFeatures { local_model: true, fetch: true, vision: false },
        data_dir: "/fake/data".into(),
        progress: true,
    }
}

/// A vault with its own app data dir, so nothing here reads a real `~/.local/share/jott`.
struct Harness {
    /// The state root this session is told to use — stamps, mirrors, trash fallback (§12). Named rather
    /// than read from the environment so two tests in one process cannot see each other's files.
    data: tempfile::TempDir,
    vault: tempfile::TempDir,
}

impl Harness {
    fn new() -> Harness {
        let data = tempfile::tempdir().expect("data dir");
        let vault = tempfile::tempdir().expect("vault dir");
        Harness { data, vault }
    }

    fn write(&self, rel: &str, bytes: &[u8]) {
        let path = self.vault.path().join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("parent");
        }
        std::fs::write(path, bytes).expect("write");
    }

    /// A session whose np is a script that does not exist: the tree and the editor must work anyway (§1).
    fn session(&self) -> Arc<VaultSession> {
        self.session_with(NpSource::Env(PathBuf::from("/nonexistent/np")))
    }

    fn session_with(&self, np: NpSource) -> Arc<VaultSession> {
        self.session_with_consent(np, Consent::Undecided)
    }

    fn session_with_consent(&self, np: NpSource, consent: Consent) -> Arc<VaultSession> {
        VaultSession::open_in(np, info(), self.vault.path(), consent, None, self.data.path().to_path_buf())
    }
}

/// Point the stand-in at a scenario; the caller holds `SERIAL`.
fn scenario(value: Value) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("scenario dir");
    // Both the scenario and a record file, so a test can prove what np was actually asked rather than
    // inferring it from timing.
    std::fs::write(dir.path().join("calls.ndjson"), "").expect("write record");
    let mut value = value;
    value["record"] = json!(dir.path().join("calls.ndjson").display().to_string());
    std::fs::write(dir.path().join("scenario.json"), value.to_string()).expect("write scenario");
    std::env::set_var("FAKE_NP_SCENARIO", dir.path().join("scenario.json"));
    dir
}

/// The methods the stand-in was asked for, in order, handshake excluded.
fn recorded(dir: &Path) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(dir.join("calls.ndjson")) else { return Vec::new() };
    text.lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter_map(|line| line["method"].as_str().map(str::to_string))
        .filter(|m| m != "initialize" && m != "shutdown")
        .collect()
}

fn fake_np() -> NpSource {
    NpSource::Env(repo_root().join("tests/fake-np/np.py"))
}

const SCAN_REPORT: &str = r#"{"added": 1, "modified": 0, "moved": 0, "removed": 0, "unchanged": 0,
    "skipped": [], "notes": 1, "unresolved_links": 0, "unresolved_targets": [],
    "index_files": ["INDEX.md"], "elapsed_ms": 7, "upgrade_orphans": null, "merged_notes": null,
    "recorded_upgrade_orphans": null, "recorded_merged_notes": null, "unindexable": null}"#;

const EMBED_REPORT: &str = r#"{"model": "embeddinggemma-2", "chunks_seen": 4, "current": 4, "embedded": 4,
    "refused_budget": 0, "busy_gap": 0, "errors": 0, "images_seen": 0, "images_embedded": 0,
    "images_refused": 0, "images_pruned": 0, "images_unchecked": 0, "images_sweep_ran": true,
    "image_refusals": [], "verification": "self-consistent, not verified", "offline": false,
    "outcome": "current"}"#;

const SEARCH_REPLY: &str = r#"{"hits": [{"path": "a.md", "snippet": "the [A] note", "kind": "note",
     "score": 1.4, "bm25": 1.4, "cosine": null}], "mode": "keyword", "vectors": false,
     "offline": false, "fetched": null, "note": null, "reason": null}"#;

#[tokio::test]
async fn a_rel_that_climbs_out_is_refused_by_kind_not_by_prose() {
    let _turn = SERIAL.lock().await;
    let harness = Harness::new();
    harness.write("notes/a.md", b"# A\n");
    let session = harness.session();

    for rel in ["../outside.md", "/etc/passwd", "notes/../../escape.md"] {
        let error = session.read_note(rel).expect_err("the webview is not a trusted caller");
        assert_eq!(error.kind, ErrorKind::EscapesRoot, "{rel}: {}", error.message);
        // The message still says what happened, because the window shows it (§4).
        assert!(!error.message.is_empty());
    }

    // ...and nothing outside the vault was created on the way.
    assert!(!harness.vault.path().parent().unwrap().join("escape.md").exists());
}

#[tokio::test]
async fn a_save_answers_with_the_hash_that_landed_and_reads_back_byte_exact() {
    let _turn = SERIAL.lock().await;
    let harness = Harness::new();
    let session = harness.session();

    // CRLF and no final newline: the two things a text layer tends to "helpfully" fix (§13).
    let crlf = "# Title\r\n\r\nkept\r\nno trailing newline";
    let written = session.save_note("notes/crlf.md", crlf).expect("save");
    let back = session.read_note("notes/crlf.md").expect("read");

    assert_eq!(back.text, crlf, "the file holds the bytes the editor held");
    assert_eq!(written.hash, back.disk_hash);
    assert_eq!(written.hash, jott_core::hashes::sha256_hex(crlf.as_bytes()));
    assert!(written.mtime_ms > 0);

    // A second save of the same bytes is a real save with the same result, not an error.
    let again = session.save_note("notes/crlf.md", crlf).expect("save again");
    assert_eq!(again.hash, written.hash);
}

#[tokio::test]
async fn the_tree_and_editor_work_with_no_np_anywhere_to_be_found() {
    let _turn = SERIAL.lock().await;
    let harness = Harness::new();
    harness.write("a.md", b"# A\n");
    let session = harness.session();

    let listing = session.listing().expect("a walk needs no np");
    assert_eq!(listing.nodes.len(), 1);
    assert_eq!(listing.nodes[0].rel, "a.md");
    // Nothing has been indexed, and that is what the marker is for (§9).
    assert_eq!(listing.nodes[0].state, Some(StampState::Waiting));

    session.save_note("b.md", "# B\n").expect("saving needs no np either");
    // Search does need it, and says so in one line the window can show.
    let error = session.search("anything", 8, false, "hybrid").await.expect_err("np is not installed here");
    assert_eq!(error.kind, ErrorKind::NpGone);
}

#[tokio::test]
async fn nps_own_catalog_is_visible_greyed_and_never_editable() {
    let _turn = SERIAL.lock().await;
    let harness = Harness::new();
    // np's exact root-catalog fingerprint, as measured in Task 7.
    harness.write("INDEX.md", b"# INDEX\n_generated by note-processor; do not edit_\n");
    harness.write("notes/real.md", b"# Real\n");
    let session = harness.session();

    let listing: VaultListing = session.listing().expect("walk");
    let index = listing.nodes.iter().find(|n| n.rel == "INDEX.md").expect("np's catalog is in the tree");
    assert_eq!(index.kind, EntryKind::Generated);
    assert_eq!(index.state, None, "a file np wrote has no index state of its own");

    // Editing it would be Jott writing into np's output (§9 greys it; the backend enforces that).
    let error = session.save_note("INDEX.md", "# Mine\n").expect_err("Jott does not write np's catalog");
    assert_eq!(error.kind, ErrorKind::NpGenerated);
}

/// A tester made a folder and the tree did not change until they made a file inside it. Either core does not list an
/// empty folder, or the window is not asking again — the two need different fixes, so this settles which.
#[tokio::test]
async fn a_folder_with_nothing_in_it_is_still_a_row() {
    let harness = Harness::new();
    harness.write("inbox.md", b"hi\n");
    let session = harness.session();

    session.create_folder("ideas").expect("the folder is made");
    let listing = session.listing().expect("the tree answers with no np at all");
    let kinds: Vec<(String, String)> = listing
        .nodes
        .iter()
        .map(|n| (n.rel.clone(), format!("{:?}", n.kind)))
        .collect();
    // Core spells a folder's `rel` without a trailing slash (`ideas`, not `ideas/`); the window tolerates both and
    // normalises when it builds destinations. What matters here is that the row exists at all: a folder you cannot see
    // is a folder you believe was never made.
    assert!(
        listing.nodes.iter().any(|n| n.rel == "ideas" && matches!(n.kind, jott_core::vault::walk::EntryKind::Folder)),
        "an empty folder is not a row, so the window cannot show what it just made: {kinds:?}"
    );
}

/// The settings panel's first control: *how search matches*. Which mode was chosen is only a fact if it reaches
/// note-processor, and a parameter that silently never sends is a whole feature missing while every other test still
/// passes — so this reads what crossed the wire rather than trusting the call site.
#[tokio::test]
async fn the_search_mode_that_was_chosen_is_the_mode_note_processor_is_asked_for() {
    let _turn = SERIAL.lock().await;
    let scenario_dir = scenario(json!({
        "replies": { "search": serde_json::from_str::<Value>(SEARCH_REPLY).unwrap() }
    }));

    let harness = Harness::new();
    let session = harness.session_with(fake_np());
    for label in ["keyword", "hybrid", "semantic"] {
        session.search("grandmother", 8, false, label).await.expect("the reader answers");
    }

    let lines = std::fs::read_to_string(scenario_dir.path().join("calls.ndjson")).expect("the stand-in recorded its calls");
    let asked: Vec<Value> = lines
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|call: &Value| call["method"] == json!("search"))
        .collect();
    assert_eq!(asked.len(), 3, "three searches, three calls: {lines}");
    for (call, label) in asked.iter().zip(["keyword", "hybrid", "semantic"]) {
        assert_eq!(call["params"]["mode"], json!(label), "a chosen mode must not be quietly rewritten: {call} in {lines}");
        assert_eq!(call["params"]["prefix"], json!(false), "{call}");
    }
}

/// A label Jott cannot map is its own verdict, said by name. Guessing `hybrid` here would answer a question nobody
/// asked — the same reason §18 refuses to invent a value that an absent key would have to stand in for.
#[tokio::test]
async fn an_unknown_search_mode_is_refused_by_name_and_never_guessed() {
    let _turn = SERIAL.lock().await;
    let scenario_dir = scenario(json!({}));

    let harness = Harness::new();
    let session = harness.session_with(fake_np());

    let error = session.search("grandmother", 8, false, "fuzzy").await.expect_err("nobody can search 'fuzzy'");
    assert_eq!(error.kind, ErrorKind::BadMode);
    for label in ["keyword", "hybrid", "semantic"] {
        assert!(error.message.contains(label), "the sentence should name what it accepts: {:?}", error.message);
    }

    // And note-processor was never asked: a refusal that still sent a request would be a guess with extra steps.
    let methods = recorded(scenario_dir.path());
    assert!(!methods.iter().any(|m| m == "search"), "a mode Jott refused still reached the pipe: {methods:?}");
}

/// Which kind a mute answer arrives as decides the sentence on screen, and `npGone` would be a contradiction: "could
/// not talk to note-processor", said about a program that just replied. It answered; it answered with nothing.
#[tokio::test]
async fn a_search_that_answered_with_nothing_arrives_as_a_refusal_not_an_absence() {
    let _turn = SERIAL.lock().await;
    let _scenario = scenario(json!({ "replies": { "search": null } }));

    let harness = Harness::new();
    let session = harness.session_with(fake_np());

    let error = session.search("grandmother", 8, false, "semantic").await.expect_err("nothing is not an answer");
    assert_eq!(error.kind, ErrorKind::NpRefused, "np answered this, so it is not absent: {error:?}");
    assert!(error.message.contains("note-processor#11"), "the window has to be able to say why: {:?}", error.message);
}

/// What a tester asked for in their own words: *"I can not open the Index.md file. Can we make it visible but
/// read only?"* The refusal was doing two jobs — "you may not change np's output" and "you may not look at it" —
/// and only the first one is ours to keep. `readonly` is how the window knows which editor to hand back.
#[tokio::test]
async fn np_s_catalogue_opens_for_reading_and_stays_unwritable() {
    let _turn = SERIAL.lock().await;
    let harness = Harness::new();
    let session = harness.session();
    harness.write("INDEX.md", b"# INDEX\n_generated by note-processor; do not edit_\n");
    harness.write("notes/real.md", b"# Real\n");
    // A folder's own index of its own notes is not np's catalogue: only the vault root's names are.
    harness.write("projects/2026/INDEX.md", b"# My own index of things\n");

    let catalogue = session.read_note("INDEX.md").expect("looking at np's output is allowed");
    assert!(catalogue.readonly, "the window has to know not to offer an edit");
    assert_eq!(catalogue.text, "# INDEX\n_generated by note-processor; do not edit_\n", "shown as it is on disk");

    let note = session.read_note("notes/real.md").expect("read");
    assert!(!note.readonly, "an ordinary note is ordinary");

    let theirs = session.read_note("projects/2026/INDEX.md").expect("read");
    assert!(!theirs.readonly, "a name inside a folder is somebody's own note, not np's file");

    // The refusal stays exactly where it was: writing np's output is still not something Jott does.
    let error = session.save_note("INDEX.md", "# Mine\n").expect_err("reading was never permission to overwrite");
    assert_eq!(error.kind, ErrorKind::NpGenerated);
}

#[cfg(unix)]
#[tokio::test]
async fn a_folder_that_cannot_be_read_becomes_a_warning_and_still_lists_the_rest() {
    use std::os::unix::fs::PermissionsExt;
    let _turn = SERIAL.lock().await;
    let harness = Harness::new();
    harness.write("locked/hidden.md", b"# hidden\n");
    harness.write("readable.md", b"# readable\n");
    std::fs::set_permissions(harness.vault.path().join("locked"), std::fs::Permissions::from_mode(0o000)).expect("chmod");

    let listing = harness.session().listing().expect("one unreadable folder is not a failed tree");
    assert!(listing.nodes.iter().any(|n| n.rel == "readable.md"));
    assert!(
        listing.warnings.iter().any(|w| w.contains("locked")),
        "the tree has to say what it could not read: {:?}",
        listing.warnings
    );

    std::fs::set_permissions(harness.vault.path().join("locked"), std::fs::Permissions::from_mode(0o755)).ok();
}

#[tokio::test]
async fn a_search_answers_while_an_embed_is_still_running() {
    let _turn = SERIAL.lock().await;
    let harness = Harness::new();
    harness.write("a.md", b"# A\n");
    let scenario_dir = scenario(json!({
        "sleep_ms": {"embed": 1500},
        "replies": {
            "search": serde_json::from_str::<Value>(SEARCH_REPLY).unwrap(),
            "scan": serde_json::from_str::<Value>(SCAN_REPORT).unwrap(),
            "embed": serde_json::from_str::<Value>(EMBED_REPORT).unwrap()
        }
    }));
    // Granted, not Declined: with a refusal there is no embed to be queued behind, and the test would be
    // timing a search against nothing.
    let session = harness.session_with_consent(fake_np(), Consent::Granted);

    // The `[embed]` button's own path, from a save that actually owes work: without a save there is no
    // batch, the flush is ignored, and the "search during an embed" below would race nothing at all.
    session.save_note("a.md", "# A changed
").expect("save");
    session.ingest_all();

    // Wait until the stand-in has been asked to embed — that is the thing the search must not queue behind.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        if recorded(scenario_dir.path()).iter().any(|m| m == "embed") {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(recorded(scenario_dir.path()).contains(&"embed".to_string()), "no embed was running: {:?}", recorded(scenario_dir.path()));

    let started = std::time::Instant::now();
    let answer = session.search("A", 8, false, "hybrid").await.expect("the reader answers");
    let waited = started.elapsed();
    assert!(waited < std::time::Duration::from_millis(400), "the search queued behind the embed: {waited:?}");
    assert_eq!(answer.mode, "keyword", "np's own word for which half answered (§18.11)");
    assert!(!answer.vectors);
    assert_eq!(answer.hits.len(), 1);
    assert_eq!(answer.hits[0].snippet.as_deref(), Some("the [A] note"), "np's marks, unescaped");

    session.close().await;
}

#[tokio::test]
async fn an_unsaved_buffer_newer_than_the_file_comes_back_with_it() {
    let _turn = SERIAL.lock().await;
    let harness = Harness::new();
    harness.write("draft.md", b"on disk\n");
    let session = harness.session();

    // Nothing mirrored yet: the note arrives alone.
    assert!(session.read_note("draft.md").expect("read").mirrored.is_none());

    session.mirror_buffer("draft.md", "typed, never saved").expect("mirror");
    let note = session.read_note("draft.md").expect("read");
    let mirror = note.mirrored.expect("§18.13 replays the buffer");
    assert_eq!(mirror.text, "typed, never saved");
    assert_eq!(note.text, "on disk\n", "a replay is an offer, not an overwrite: the window asks first");

    // Saving clears it. After that the file holds the text, and a leftover mirror would resurrect a draft
    // somebody deleted on purpose (§15's second risk).
    session.save_note("draft.md", "typed, never saved").expect("save");
    assert!(session.read_note("draft.md").expect("read").mirrored.is_none());

    // A `rel` that climbs out cannot be used to overwrite another file's mirror either.
    assert_eq!(session.mirror_buffer("../other.md", "someone else's").expect_err("guarded").kind, ErrorKind::EscapesRoot);
}

#[tokio::test]
async fn discarding_a_mirrored_draft_means_it_is_actually_gone() {
    let _turn = SERIAL.lock().await;
    let harness = Harness::new();
    harness.write("draft.md", b"the file\n");
    let session = harness.session();

    session.mirror_buffer("draft.md", "a draft nobody wants").expect("mirror");
    assert!(session.read_note("draft.md").expect("read").mirrored.is_some());

    session.discard_mirror("draft.md").expect("discard");
    assert!(session.read_note("draft.md").expect("read").mirrored.is_none(), "the file says it is gone");
    // The note itself is untouched: discarding a draft is not deleting a note.
    assert_eq!(std::fs::read_to_string(harness.vault.path().join("draft.md")).expect("read"), "the file\n");

    // Discarding twice, or discarding one that never existed, is not an error worth showing anyone.
    session.discard_mirror("draft.md").expect("idempotent");
    assert_eq!(session.discard_mirror("../elsewhere.md").expect_err("still guarded").kind, ErrorKind::EscapesRoot);
}

#[tokio::test]
async fn deleting_says_where_the_file_went() {
    let _turn = SERIAL.lock().await;
    let harness = Harness::new();
    harness.write("gone.md", b"# gone\n");
    let session = harness.session();

    let said = session.delete_entry("gone.md").expect("delete");
    assert!(!harness.vault.path().join("gone.md").exists(), "it left the vault");
    // Either the system trash or Jott's own folder, and the sentence names which (§13).
    assert!(said.contains("system trash") || said.contains("Jott's own folder"), "the window must be able to say: {said}");
    if let Some(path) = said.split(": ").nth(1) {
        assert!(Path::new(path).exists(), "and the file is really where it said: {path}");
    }

    let error = session.delete_entry("gone.md").expect_err("it is already gone");
    assert_eq!(error.kind, ErrorKind::NotFound);
    // The trash belongs to the app data dir, not to the vault.
    assert!(!harness.vault.path().join(".trash").exists());
}

#[tokio::test]
async fn deleting_asks_np_to_re_index_so_the_ghost_stops_answering() {
    // The tester's report: "when I delete it and search again the deleted file is in my list of found files."
    // np marks a gone path stale and never deletes it (`scan.rs:161`), so the index keeps answering until a
    // scan retires it. Measured on the bundled np 0.5.1: after `scan` answers `removed: 1`, the same search
    // returns `hits: []`. The window that moved the file to the trash is the one that has to say so.
    let _turn = SERIAL.lock().await;
    let harness = Harness::new();
    harness.write("ghost.md", b"# ghost\n");

    let record = harness.data.path().join("calls.ndjson");
    std::fs::write(&record, "").expect("empty call record");
    let scenario = json!({ "record": record.display().to_string(), "replies": { "scan": common::fixture_result(1) } });
    let scenario_file = harness.data.path().join("scenario.json");
    std::fs::write(&scenario_file, scenario.to_string()).expect("write scenario");
    std::env::set_var("FAKE_NP_SCENARIO", &scenario_file);

    let (_sentinel, info) = ipc::np_absent();
    let session = VaultSession::open_in_with(
        NpSource::Env(common::repo_root().join("tests/fake-np/np.py")),
        info,
        None,
        harness.vault.path(),
        Consent::Granted,
        Some(harness.data.path().join("np-config.toml")),
        harness.data.path().to_path_buf(),
    );

    // Whatever opening the folder asked for is not what this test is about.
    std::fs::write(&record, "").expect("clear the record");
    session.delete_entry("ghost.md").expect("deleted");

    // The rescan is background work: the delete's own reply must not wait on np.
    let mut asked = Vec::new();
    for _ in 0..40 {
        asked = std::fs::read_to_string(&record)
            .expect("the stand-in recorded")
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .filter_map(|line| line["method"].as_str().map(str::to_string))
            .collect();
        if asked.iter().any(|m| m == "scan") {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(asked.iter().any(|m| m == "scan"), "np was never told the file is gone: {asked:?}");

    session.close().await;
}

/// The same ghost arrives through the file manager as well as through Jott's own `delete`. The watcher only
/// reports that a path changed, so the question "is it still there?" is asked of the disk before anything is
/// concluded about it.
#[tokio::test]
async fn deleting_a_file_outside_jott_also_asks_np_to_re_index() {
    let _turn = SERIAL.lock().await;
    let harness = Harness::new();
    harness.write("outside.md", b"# outside\n");

    let record = harness.data.path().join("calls.ndjson");
    std::fs::write(&record, "").expect("empty call record");
    let scenario = json!({ "record": record.display().to_string(), "replies": { "scan": common::fixture_result(1) } });
    let scenario_file = harness.data.path().join("scenario.json");
    std::fs::write(&scenario_file, scenario.to_string()).expect("write scenario");
    std::env::set_var("FAKE_NP_SCENARIO", &scenario_file);

    let (_sentinel, info) = ipc::np_absent();
    let session = VaultSession::open_in_with(
        NpSource::Env(common::repo_root().join("tests/fake-np/np.py")),
        info,
        None,
        harness.vault.path(),
        Consent::Granted,
        Some(harness.data.path().join("np-config.toml")),
        harness.data.path().to_path_buf(),
    );
    std::fs::write(&record, "").expect("clear the record");

    std::fs::remove_file(harness.vault.path().join("outside.md")).expect("removed outside Jott");

    // Watcher backends are OS-level and this box may not have one; `watch_policy.rs` makes the same
    // allowance. Four seconds covers notify's own coalescing interval.
    let mut asked = Vec::new();
    for _ in 0..80 {
        asked = std::fs::read_to_string(&record)
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .filter_map(|line| line["method"].as_str().map(str::to_string))
            .collect();
        if asked.iter().any(|m| m == "scan") {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(asked.iter().any(|m| m == "scan"), "an outside delete went unnoticed by the index: {asked:?}");

    session.close().await;
}

/// The other half of the same story: a path that is absent is not automatically a note that was deleted.
/// np's catalogue rename leaves `INDEX.md.tmp` gone, an editor's swap file vanishes, and a folder removal
/// reports children that were already missing. Nothing here may turn one of those into a rescan request —
/// that is the loop that wrote 6,885 scan lines in under three minutes.
#[tokio::test]
async fn a_path_that_was_never_a_note_does_not_ask_for_a_rescan() {
    let _turn = SERIAL.lock().await;
    let harness = Harness::new();
    // np's own temporary, mid-rename: created and removed inside one scan.
    harness.write("INDEX.md.tmp", b"half-written catalogue\n");

    let record = harness.data.path().join("calls.ndjson");
    std::fs::write(&record, "").expect("empty call record");
    let scenario = json!({ "record": record.display().to_string(), "replies": { "scan": common::fixture_result(1) } });
    let scenario_file = harness.data.path().join("scenario.json");
    std::fs::write(&scenario_file, scenario.to_string()).expect("write scenario");
    std::env::set_var("FAKE_NP_SCENARIO", &scenario_file);

    let (_sentinel, info) = ipc::np_absent();
    let session = VaultSession::open_in_with(
        NpSource::Env(common::repo_root().join("tests/fake-np/np.py")),
        info,
        None,
        harness.vault.path(),
        Consent::Granted,
        Some(harness.data.path().join("np-config.toml")),
        harness.data.path().to_path_buf(),
    );
    std::fs::write(&record, "").expect("clear the record");

    std::fs::remove_file(harness.vault.path().join("INDEX.md.tmp")).expect("np renamed its temporary away");

    // Long enough that a loop would have filled the file many times over: at the rate in the tester's log
    // this window alone would hold ~150 scan lines.
    tokio::time::sleep(std::time::Duration::from_millis(1_200)).await;
    let asked: Vec<String> = std::fs::read_to_string(&record)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter_map(|line| line["method"].as_str().map(str::to_string))
        .collect();
    assert!(!asked.iter().any(|m| m == "scan"), "np's own rename was read as a deleted note: {asked:?}");

    session.close().await;
}

#[tokio::test]
async fn renaming_never_overwrites_and_says_who_links_to_it() {
    let _turn = SERIAL.lock().await;
    let harness = Harness::new();
    harness.write("target.md", b"# Target\n");
    harness.write("other.md", b"see [[target]] for the rest\n");
    harness.write("taken.md", b"# Taken\n");
    let session = harness.session();

    let outcome = session.rename_entry("target.md", "renamed.md").expect("rename");
    assert_eq!(outcome.incoming_links.len(), 1, "one note refers to what moved");
    assert_eq!(outcome.incoming_links[0].src, "other.md");
    assert_eq!(outcome.incoming_links[0].line, 1);

    let error = session.rename_entry("renamed.md", "taken.md").expect_err("no silent overwrite");
    assert_eq!(error.kind, ErrorKind::AlreadyExists);
    assert!(harness.vault.path().join("taken.md").exists(), "the file it refused to overwrite is intact");
}

#[tokio::test]
async fn creating_something_that_is_already_there_is_refused() {
    let _turn = SERIAL.lock().await;
    let harness = Harness::new();
    harness.write("taken.md", b"# keep me\n");
    let session = harness.session();

    assert_eq!(session.create_note("taken.md").expect_err("twice is a mistake").kind, ErrorKind::AlreadyExists);
    assert_eq!(std::fs::read_to_string(harness.vault.path().join("taken.md")).expect("read"), "# keep me\n");

    session.create_folder("inbox").expect("folder");
    assert_eq!(session.create_folder("inbox").expect_err("the same name twice").kind, ErrorKind::AlreadyExists);

    // A new note in a folder that does not exist yet gets the folder too (§9's `notes/2026/x.md`).
    let created = session.create_note("deep/nested/new.md").expect("create with parents");
    assert!(harness.vault.path().join("deep/nested/new.md").exists());
    assert_eq!(created.hash, jott_core::hashes::sha256_hex(b""));
}

#[tokio::test]
async fn missing_np_arrives_with_instructions_for_this_platform() {
    let payload = ipc::np_missing_payload(&NpMissing::NotFound, Os::Windows);
    // §2: the hint names np's Windows asset, not a generic "install it".
    assert!(
        payload.install_hint.contains("note-processor") && payload.install_hint.to_lowercase().contains("windows"),
        "windows instructions: {}",
        payload.install_hint
    );

    let macos = ipc::np_missing_payload(&NpMissing::TooOld { found: "0.2.0".into() }, Os::Macos);
    // The floor is quoted from the one place it is written down, so these tests prove a number is named without
    // becoming a second copy of it.
    let floor = jott_core::np::resolve::min_version_text();
    assert!(macos.message.contains("0.2.0") && macos.message.contains(&floor), "{}", macos.message);
    assert!(!macos.install_hint.is_empty());
}

#[tokio::test]
async fn what_the_window_receives_is_named_exactly_as_ipc_ts_says() {
    // This test is the contract for `src/lib/ipc.ts`. A field renamed on one side fails here rather than
    // arriving as `undefined` in the running app.
    let listing = VaultListing { vault: "/v".into(), nodes: vec![], warnings: vec![] };
    assert_eq!(serde_json::to_value(listing).expect("listing"), json!({ "vault": "/v", "nodes": [], "warnings": [] }));

    let write = serde_json::to_value(WriteResult { hash: "abc".into(), mtime_ms: 17 }).expect("write");
    assert_eq!(write, json!({ "hash": "abc", "mtimeMs": 17 }));

    let rename = serde_json::to_value(RenameOutcome { incoming_links: vec![] }).expect("rename");
    assert_eq!(rename, json!({ "incomingLinks": [] }));

    // First the shape an older np answers with: every key the window reads is present and **null**, because it has to
    // tell "np did not say" from "np said no". A missing key would be a third fact, arriving in TypeScript as
    // `undefined` where no test would ever notice.
    let search = serde_json::to_value(SearchResults {
        hits: vec![HitDto { path: "a.md".into(), snippet: None, kind: "note".into(), score: 1.0, bm25: None,
                            cosine: None, matched_exactly: None }],
        mode: "keyword".into(),
        vectors: false,
        offline: false,
        reason: None,
        indexed: None,
        keyword_used: None,
        semantic_used: None,
        notice: None,
        prefix_used: None,
        prefix_terms: vec![],
    })
    .expect("search");
    assert_eq!(search, json!({ "hits": [{"path": "a.md", "snippet": null, "kind": "note", "score": 1.0, "bm25": null,
                                         "cosine": null, "matchedExactly": null}],
                                 "mode": "keyword", "vectors": false, "offline": false, "reason": null,
                                 "indexed": null, "keywordUsed": null, "semanticUsed": null, "notice": null,
                                 "prefixUsed": null, "prefixTerms": [] }));

    // …then 0.5.1's widened answer: np says it widened, names the word, and marks the row as not an exact match.
    let widened = serde_json::to_value(SearchResults {
        hits: vec![HitDto { path: "a.md".into(), snippet: Some("my [grandmother] kept".into()), kind: "note".into(),
                            score: 1.1, bm25: None, cosine: Some(0.82), matched_exactly: Some(false) }],
        mode: "hybrid".into(),
        vectors: true,
        offline: false,
        reason: None,
        indexed: Some(true),
        keyword_used: Some(true),
        semantic_used: Some(true),
        notice: None,
        prefix_used: Some(true),
        prefix_terms: vec!["grand".into()],
    })
    .expect("widened search");
    assert_eq!(widened["hits"][0]["matchedExactly"], json!(false), "a word-beginning row is not an exact match");
    assert_eq!(widened["prefixUsed"], json!(true));
    assert_eq!(widened["prefixTerms"], json!(["grand"]), "the header may name these words and no others");

    let note = serde_json::to_value(NoteContent {
        rel: "a.md".into(),
        text: String::new(),
        hash: "h".into(),
        disk_hash: "h".into(),
        disk_mtime_ms: 3,
        mirrored: None,
        readonly: false,
    })
    .expect("note");
    // Sorted, because `serde_json`'s map is a BTreeMap: the names are the contract, their order is not.
    let mut keys: Vec<&str> = note.as_object().expect("object").keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, vec!["diskHash", "diskMtimeMs", "hash", "mirrored", "readonly", "rel", "text"]);

    // The status line's events, which `src/lib/statusLine.ts` pattern-matches on.
    let working = IngestEvent::Working { phase: "embedding".into(), done: 3, total: Some(7), member_index: None, member_count: None };
    assert_eq!(
        serde_json::to_value(working).expect("working"),
        json!({ "working": { "phase": "embedding", "done": 3, "total": 7, "memberIndex": null, "memberCount": null } })
    );

    let behind = IngestEvent::Behind { reason: StateReason::Locked { waited_ms: 5000 }, retry_in_ms: 5000 };
    assert_eq!(serde_json::to_value(behind).expect("behind"), json!({ "behind": { "reason": { "locked": { "waitedMs": 5000 } }, "retryInMs": 5000 } }));

    let consent = IngestEvent::NeedsConsent { model_id: Some("bge-m3".into()), bytes: None, data_dir: "/d".into() };
    assert_eq!(serde_json::to_value(consent).expect("consent"), json!({ "needsConsent": { "modelId": "bge-m3", "bytes": null, "dataDir": "/d" } }));

    // np's own report keeps np's spellings inside the event: it is np's document (§18.5).
    let report: ScanReport = serde_json::from_str(SCAN_REPORT).expect("np's report");
    let indexed = serde_json::to_value(IngestEvent::Indexed(report)).expect("indexed");
    assert_eq!(indexed["indexed"]["elapsed_ms"], 7);
    assert!(indexed["indexed"]["unresolved_links"].is_number(), "np's field name survives: {indexed}");

    // A refusal is one sentence, verbatim, with nothing added around it.
    let refused = serde_json::to_value(IngestEvent::Refused { message: "vault is locked by another process".into() }).expect("refused");
    assert_eq!(refused, json!({ "refused": { "message": "vault is locked by another process" } }));
}

#[tokio::test]
async fn a_note_that_is_not_text_says_so_instead_of_showing_replacement_characters() {
    let _turn = SERIAL.lock().await;
    let harness = Harness::new();
    harness.write("binary.md", &[0xff, 0xfe, 0x00, b'x']);
    let session = harness.session();

    // Lossy decoding here would write *different* bytes back on the next save (Task 7's ruling).
    let error = session.read_note("binary.md").expect_err("not editable text");
    assert_eq!(error.kind, ErrorKind::NotUtf8);
    assert!(error.message.contains("binary.md"), "{}", error.message);
}

#[tokio::test]
async fn closing_finishes_the_pass_it_started_without_waiting_on_a_model() {
    let _turn = SERIAL.lock().await;
    let harness = Harness::new();
    harness.write("a.md", b"# A\n");
    let _scenario = scenario(json!({ "replies": {"scan": serde_json::from_str::<Value>(SCAN_REPORT).unwrap()} }));
    let session = harness.session_with(fake_np());

    session.save_note("a.md", "# A\n").expect("save");
    // §11: closing waits for the pass in flight up to a cap, and never for a model download.
    tokio::time::timeout(std::time::Duration::from_secs(20), session.close())
        .await
        .expect("close returns on its own");
}

#[tokio::test]
async fn the_status_line_gets_its_events_from_the_session_not_from_np() {
    let _turn = SERIAL.lock().await;
    let harness = Harness::new();
    harness.write("a.md", b"# A\n");
    let _scenario = scenario(json!({ "replies": {"scan": serde_json::from_str::<Value>(SCAN_REPORT).unwrap()} }));
    let session = harness.session_with(fake_np());
    let mut events = session.subscribe_events();

    session.save_note("a.md", "# A\n").expect("save");
    session.ingest_all();

    // The window never sees an np frame: what arrives is one of the six things it knows how to draw.
    let mut seen = Vec::new();
    for _ in 0..8 {
        match tokio::time::timeout(std::time::Duration::from_secs(10), events.recv()).await {
            Ok(Ok(event)) => {
                let name = serde_json::to_value(&event).expect("event").as_object().and_then(|m| m.keys().next().cloned());
                let reached_the_end = name.as_deref() == Some("indexed");
                seen.push(name);
                if reached_the_end {
                    break;
                }
            }
            other => panic!("no event for the window: {other:?}"),
        }
    }
    assert!(seen.iter().any(|n| n.as_deref() == Some("indexed")), "the scan's report reached the window: {seen:?}");

    session.close().await;
}

#[tokio::test]
async fn a_startup_handshake_that_fails_is_a_value_the_window_can_show() {
    let _turn = SERIAL.lock().await;
    // The machine this runs on may have no np at all, and that must not be a panic or a dead window: it
    // is an answer with a sentence and an instruction in it (§1, §2).
    let gone = ipc::read_np_info(&NpSource::Env(PathBuf::from("/nonexistent/np"))).await.expect_err("there is no such binary");
    let payload = match &gone {
        ipc::NpGone::Fault(message) => {
            assert!(message.contains("/nonexistent/np"), "it names what it tried to run: {message}");
            gone.payload(Os::Windows)
        }
        ipc::NpGone::Missing(missing) => ipc::np_missing_payload(missing, Os::Windows),
    };
    assert!(!payload.message.is_empty());
    assert!(
        payload.install_hint.contains(&jott_core::np::resolve::min_version_text()),
        "the hint states the floor: {}",
        payload.install_hint
    );

    // A fault sentence from np is kept whole rather than paraphrased (§4).
    let fault = ipc::NpGone::Fault("np said: workspace is locked".into());
    assert_eq!(fault.payload(Os::Linux).message, "np said: workspace is locked");
}

/// The startup handshake used to end with the child being killed and another started when a folder opened. Now
/// the session takes that child as its writer, which means two things worth pinning: the child is *owned* by the
/// pool, so closing the session shuts it down; and nothing starts a second one behind our back.
///
/// It deliberately does not send a search through the session to prove the hand-off. Search goes to the *reader*,
/// which is a different connection, so that version of the test passed here by finding a real np on PATH and
/// failed on CI with "No such file or directory" — the second time this box's installed np has made a test lie.
#[tokio::test]
async fn a_session_adopts_the_child_the_app_already_started() {
    let _turn = SERIAL.lock().await;
    let harness = Harness::new();
    harness.write("a.md", b"# A\n");

    let (dir, client) = common::spawn(serde_json::json!({})).await;
    let warm = Arc::new(client);
    let (_sentinel, info) = ipc::np_absent();
    let source = common::unreachable_np();

    let session = VaultSession::open_in_with(
        source,
        info,
        Some(Arc::clone(&warm)),
        harness.vault.path(),
        Consent::Undecided,
        None,
        harness.data.path().to_path_buf(),
    );

    // Nothing was asked of it. The session only has to own it — and owning means letting go at the end.
    session.close().await;
    for _ in 0..40 {
        if warm.is_closed() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(warm.is_closed(), "the session took the child and never shut it down");
    drop(dir);
}

#[tokio::test]
async fn the_registry_answer_is_the_two_things_the_window_asks_for() {
    // `granted`, lower case: this is the registry's own spelling from Task 6, and one spelling for the
    // same decision in the file and on the wire is worth more than a prettier payload.
    let info = ipc::RegistryInfo { recent: vec!["/home/ana/notes".into()], consent: Consent::Granted };
    assert_eq!(
        serde_json::to_value(info).expect("registry"),
        json!({ "recent": ["/home/ana/notes"], "consent": "granted" })
    );
}

#[tokio::test]
async fn a_vault_still_opens_when_np_cannot_be_asked_anything() {
    let _turn = SERIAL.lock().await;
    let harness = Harness::new();
    harness.write("a.md", b"# A\n");
    // The shell's fallback for a machine with no usable np: same window, and every index question answers
    // honestly that there is nobody to ask.
    let (_sentinel, info) = ipc::np_absent();
    // `np_absent` names the program `np` and lets PATH answer, which is honest on a machine that has no np
    // and a false premise in a test suite on a box that does — where it starts the real one and this test
    // quietly stops testing anything. Name a binary that cannot exist instead. (Until 0.0.6 this passed by
    // accident: Jott forgot to tell np to `serve`, so even a real np on PATH died at once.)
    let source = common::unreachable_np();
    let session = VaultSession::open_in(source, info, harness.vault.path(), Consent::Undecided, None, harness.data.path().to_path_buf());

    assert_eq!(session.listing().expect("the tree needs no np").nodes.len(), 1);
    session.save_note("b.md", "# B\n").expect("neither does a save");
    assert_eq!(session.search("anything", 8, false, "hybrid").await.expect_err("np is not here").kind, ErrorKind::NpGone);

    // And the `[embed]` button's queue does not spin: it fails its batch and moves on.
    session.ingest_all();
    session.close().await;
}

// ---- findings from the whole-branch review, each pinned by the test that was missing ----

#[tokio::test]
async fn a_save_by_jott_is_not_reported_as_an_outside_change() {
    let _turn = SERIAL.lock().await;
    let harness = Harness::new();
    harness.write("a.md", b"# A\n");
    let session = harness.session();
    let mut changes = session.subscribe_changes();

    // Drain anything the watcher noticed while the vault was being set up.
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    while changes.try_recv().is_ok() {}

    session.save_note("a.md", "# A changed by Jott\n").expect("save");
    session.create_note("new.md").expect("create");

    // Our own writes are suppressed (§8's watcher rule, tested at the unit level in Task 8 and here at the
    // one place they are actually wired together: a session). An echo of our own save makes the window
    // reload the note the person is typing in.
    let quiet = tokio::time::timeout(std::time::Duration::from_millis(1_500), async {
        loop {
            match changes.recv().await {
                Ok(rel) => return Some(rel),
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => return None,
            }
        }
    })
    .await;
    assert!(quiet.is_err(), "Jott's own save came back as an outside change: {:?}", quiet.ok().flatten());
}

#[tokio::test]
async fn closing_an_idle_vault_does_not_wait_for_the_drain_cap() {
    let _turn = SERIAL.lock().await;
    let harness = Harness::new();
    let session = harness.session();

    // Nothing is owed: closing means stopping, not counting to five. §11's cap is for work in flight.
    let started = std::time::Instant::now();
    tokio::time::timeout(std::time::Duration::from_secs(2), session.close())
        .await
        .expect("an idle session closes promptly");
    assert!(started.elapsed() < std::time::Duration::from_secs(2), "idle close took {:?}", started.elapsed());
}

#[tokio::test]
async fn a_note_refused_by_kind_arrives_with_the_name_ipc_ts_uses() {
    // The webview branches on `kind`; the union in `src/lib/ipc.ts` is written by hand, so the casing is
    // part of the contract and nothing else in the suite touches it.
    let error = ipc::IpcError::no_vault();
    assert_eq!(serde_json::to_value(&error).expect("error"), json!({ "kind": "noVault", "message": "no vault is open" }));

    for (kind, word) in [
        (ErrorKind::EscapesRoot, "escapesRoot"),
        (ErrorKind::NotFound, "notFound"),
        (ErrorKind::AlreadyExists, "alreadyExists"),
        (ErrorKind::IntoItself, "intoItself"),
        (ErrorKind::TooLarge, "tooLarge"),
        (ErrorKind::NotUtf8, "notUtf8"),
        (ErrorKind::NpGenerated, "npGenerated"),
        (ErrorKind::NoVault, "noVault"),
        (ErrorKind::NpGone, "npGone"),
        (ErrorKind::Io, "io"),
    ] {
        let value = serde_json::to_value(ipc::IpcError { kind, message: "x".into() }).expect("kind");
        assert_eq!(value["kind"], word, "{word} must arrive as the webview spells it");
    }

    // The mirror's hash travels too: §18.13's offer is only checkable if the buffer can be compared to it.
    let mirrored = serde_json::to_value(jott_core::ipc::MirroredText { text: "t".into(), mtime_ms: 1, hash: "h".into() }).expect("mirror");
    assert_eq!(mirrored, json!({ "text": "t", "mtimeMs": 1, "hash": "h" }));
}

#[tokio::test]
async fn a_draft_survives_the_folder_being_opened_with_a_trailing_separator() {
    let _turn = SERIAL.lock().await;
    let harness = Harness::new();
    harness.write("draft.md", b"on disk\n");
    let plain = VaultSession::open_in(
        NpSource::Env(PathBuf::from("/nonexistent/np")),
        info(),
        harness.vault.path(),
        Consent::Undecided,
        None,
        harness.data.path().to_path_buf(),
    );
    plain.mirror_buffer("draft.md", "typed and unsaved").expect("mirror");

    // The same folder spelled differently — a picker that returns `\\?\C:\notes\`, a recent entry with a
    // trailing slash. The stamps already treat these as one vault (Task 6); a draft keyed on the raw string
    // would simply never come back (§18.13).
    let messy = format!("{}/", harness.vault.path().display());
    let again = VaultSession::open_in(
        NpSource::Env(PathBuf::from("/nonexistent/np")),
        info(),
        Path::new(&messy),
        Consent::Undecided,
        None,
        harness.data.path().to_path_buf(),
    );

    let note = again.read_note("draft.md").expect("read");
    assert_eq!(note.mirrored.map(|m| m.text).as_deref(), Some("typed and unsaved"), "one vault, one key, whatever the spelling");
}

// ---- np's own default, which fetches before anybody is asked ----

/// Measured here against the shipped binary: a bare `np serve` with no config answered its first `search` by
/// fetching 309,855,456 bytes. Nothing in Jott asked, because there was no decision to consult and np's
/// `[models] auto_fetch` defaults to yes. So opening a vault with no answer recorded says *no* on the person's
/// behalf — reversible by one click in the prompt that follows, where "No thanks" keeps it off.
#[tokio::test]
async fn opening_a_vault_with_no_answer_leaves_np_no_free_hand_to_fetch() {
    let _turn = SERIAL.lock().await;
    let harness = Harness::new();
    harness.write("a.md", b"# A\n");
    let config = harness.data.path().join("np-config.toml");
    std::fs::write(&config, "[log]\nlevel = \"info\"\n").expect("write np config");

    let (_sentinel, info) = ipc::np_absent();
    let session = VaultSession::open_in_with(
        common::unreachable_np(),
        info,
        None,
        harness.vault.path(),
        Consent::Undecided,
        Some(config.clone()),
        harness.data.path().to_path_buf(),
    );

    assert!(!auto_fetch_allowed(&config), "an undecided vault must not leave a 310 MB default switched on");
    let after = std::fs::read_to_string(&config).expect("the same file");
    assert!(after.contains("level = \"info\""), "np's own settings survive untouched: {after}");

    session.close().await;
}

/// A remembered answer is an answer. Opening the vault again is not a new question, and quietly turning
/// someone's "yes" into "no" because they relaunched the app would be worse than the hole being closed.
#[tokio::test]
async fn a_remembered_yes_is_not_overruled_by_opening_the_vault_again() {
    let _turn = SERIAL.lock().await;
    let harness = Harness::new();
    harness.write("a.md", b"# A\n");
    let config = harness.data.path().join("np-config.toml");
    std::fs::write(&config, "[models]\nauto_fetch = true\n").expect("write np config");

    let (_sentinel, info) = ipc::np_absent();
    let session = VaultSession::open_in_with(
        common::unreachable_np(),
        info,
        None,
        harness.vault.path(),
        Consent::Granted,
        Some(config.clone()),
        harness.data.path().to_path_buf(),
    );

    assert!(auto_fetch_allowed(&config), "a decision stands until somebody changes it");

    session.close().await;
}

/// "Index now" on a folder nobody has typed into. The button's label promises the vault; flushing what is
/// pending promises nothing, because on a freshly opened folder nothing is pending — and np's scan, which
/// covers the whole workspace whenever any single note triggers it, never got asked at all.
#[tokio::test]
async fn index_now_covers_every_note_and_not_only_the_saved_ones() {
    let _turn = SERIAL.lock().await;
    let harness = Harness::new();
    harness.write("a.md", b"# A\n");
    harness.write("b.md", b"# B\n");

    let record = harness.data.path().join("calls.ndjson");
    std::fs::write(&record, "").expect("empty call record");
    let scenario = json!({
        "record": record.display().to_string(),
        "replies": {
            "scan": common::fixture_result(2),
            "embed": embed_report(),
            "models.list": models_present()
        }
    });
    let scenario_file = harness.data.path().join("scenario.json");
    std::fs::write(&scenario_file, scenario.to_string()).expect("write scenario");
    // The stand-in reads its scenario from the environment, which is process-global: hence `SERIAL` above.
    std::env::set_var("FAKE_NP_SCENARIO", &scenario_file);

    let (_sentinel, info) = ipc::np_absent();
    let session = VaultSession::open_in_with(
        NpSource::Env(common::repo_root().join("tests/fake-np/np.py")),
        info,
        None,
        harness.vault.path(),
        Consent::Granted,
        Some(harness.data.path().join("np-config.toml")),
        harness.data.path().to_path_buf(),
    );

    session.ingest_all();

    let mut states = Vec::new();
    for _ in 0..80 {
        let listing = session.listing().expect("the vault is readable");
        states = listing
            .nodes
            .iter()
            .map(|node| (node.rel.clone(), node.state.clone()))
            .collect::<Vec<_>>();
        if states.iter().all(|(_, state)| state == &Some(StampState::Embedded)) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(
        states.iter().all(|(_, state)| state == &Some(StampState::Embedded)),
        "every note in the folder went through the pass: {states:?}"
    );

    let asked: Vec<String> = std::fs::read_to_string(&record)
        .expect("the stand-in recorded")
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter_map(|line| line["method"].as_str().map(str::to_string))
        .filter(|method| method != "initialize" && method != "shutdown")
        .collect();
    assert_eq!(asked, vec!["scan".to_string(), "embed".to_string()], "one pass, both halves");

    session.close().await;
}

/// np's embed report as the real one looked, trimmed to what `StampState::Embedded` depends on: `errors: 0`.
fn embed_report() -> Value {
    json!({
        "outcome": "current", "model": "embeddinggemma-2", "chunks_seen": 2, "embedded": 2, "errors": 0,
        "images_seen": 0, "images_embedded": 0, "images_refused": 0, "images_pruned": 0, "images_unchecked": 0,
        "images_sweep_ran": true, "image_refusals": [], "fetched": null, "offline": false, "busy_gap": 0,
        "refused_budget": 0, "current": 0, "verification": "self-consistent, not verified"
    })
}

fn models_present() -> Value {
    json!({
        "models": [{"id": "embeddinggemma-2", "dim": 768, "sha256": null, "prefix_doc": null,
                    "prefix_query": null, "installed_at": 1, "state": "present", "members": []}],
        "fetching": [], "unclaimed": [], "models_dir_error": null
    })
}

// ---- what the drawer is shown about this session ----

/// The drawer's promise is that it shows the same facts the log holds. A search through the session is a
/// request like any other, and a row with no elapsed time would leave "why is search slow?" unanswered.
#[tokio::test]
async fn the_trace_shows_what_the_session_asked_np() {
    let _turn = SERIAL.lock().await;
    let harness = Harness::new();
    harness.write("a.md", b"# A\n");
    let (dir, client) = common::spawn(json!({
        "replies": {"search": search_fixture()}
    }))
    .await;

    let (_sentinel, info) = ipc::np_absent();
    let session = VaultSession::open_in_with(
        NpSource::Env(common::repo_root().join("tests/fake-np/np.py")),
        info,
        Some(Arc::new(client)),
        harness.vault.path(),
        Consent::Granted,
        None,
        harness.data.path().to_path_buf(),
    );

    session.search("kumquat", 8, false, "hybrid").await.expect("the stand-in answers");

    let rows = session.trace().recent();
    let search = rows
        .iter()
        .find(|row| row.method.as_deref() == Some("search") && row.kind == TraceKind::Request)
        .unwrap_or_else(|| panic!("a search the window made should be on the screen: {rows:?}"));
    assert_eq!(search.outcome, Some("applied"));
    assert!(search.elapsed_ms.is_some(), "and how long it took");

    // A subscriber that attached afterwards still gets what comes next — that is how a drawer opened mid-run
    // keeps up without polling.
    let mut rx = session.trace_events();
    session.trace().push(TraceEvent::connection("test row"));
    let got = tokio::time::timeout(std::time::Duration::from_millis(500), rx.recv())
        .await
        .expect("delivered")
        .expect("open");
    assert_eq!(got.detail.as_deref(), Some("test row"));

    session.close().await;
    drop(dir);
}

/// A machine with no usable np is the state most worth showing, because it is the one where everything else on
/// screen still works. The drawer has to say the child could not be started — not stay silent and let the
/// runtime row claim a connection that never existed.
#[tokio::test]
async fn a_child_that_could_not_be_started_is_a_row_of_its_own() {
    let _turn = SERIAL.lock().await;
    let harness = Harness::new();
    harness.write("a.md", b"# A\n");

    let (_sentinel, info) = ipc::np_absent();
    let session = VaultSession::open_in_with(
        common::unreachable_np(),
        info,
        None,
        harness.vault.path(),
        Consent::Granted,
        None,
        harness.data.path().to_path_buf(),
    );

    let err = session.search("anything", 8, false, "hybrid").await.expect_err("np is not here");
    assert_eq!(err.kind, ErrorKind::NpGone);

    let rows = session.trace().recent();
    let connection = rows
        .iter()
        .find(|row| row.kind == TraceKind::Connection)
        .unwrap_or_else(|| panic!("the drawer has to show why nothing works: {rows:?}"));
    assert!(
        connection.detail.as_deref().is_some_and(|d| d.contains("np")),
        "and name what it could not start: {connection:?}"
    );

    session.close().await;
}

// np's search answer as the real one is shaped: hits with snippets, and the four provenance keys §10 asks for.
fn search_fixture() -> Value {
    json!({
        "hits": [{"path": "a.md", "snippet": "\u{2026}a [kumquat] tree\u{2026}", "kind": "note", "score": 1.2, "bm25": 1.2, "cosine": null}],
        "mode": "keyword", "vectors": false, "offline": false, "reason": null
    })
}

// ---- the snapshot behind the diagnostics drawer ----

/// Opening the drawer ten minutes into a run must not show an empty pane: the answer is what already happened,
/// not only what happens next.
#[tokio::test]
async fn the_snapshot_shows_the_run_so_far() {
    let _turn = SERIAL.lock().await;
    let harness = Harness::new();
    harness.write("a.md", b"# A\n");
    let (dir, client) = common::spawn(json!({"replies": {"search": search_fixture()}})).await;

    let (_sentinel, info) = ipc::np_absent();
    let protocol = info.protocol as u32;
    let version = info.version.clone();
    let session = VaultSession::open_in_with(
        NpSource::Env(common::repo_root().join("tests/fake-np/np.py")),
        info,
        Some(Arc::new(client)),
        harness.vault.path(),
        Consent::Granted,
        None,
        harness.data.path().to_path_buf(),
    );

    session.search("kumquat", 8, false, "hybrid").await.expect("the stand-in answers");
    let snapshot = session.diagnostics().await;

    assert!(snapshot.recent.iter().any(|row| row.method.as_deref() == Some("search")));
    assert_eq!(snapshot.runtime.protocol, Some(protocol));
    assert_eq!(snapshot.runtime.np_version.as_deref(), Some(version.as_str()));
    assert!(snapshot.runtime.np_path.as_deref().is_some_and(|p| p.ends_with("np.py")), "{:?}", snapshot.runtime.np_path);
    assert_eq!(snapshot.runtime.writer, ConnectionState::Up, "the warm child is a live writer");
    assert_eq!(snapshot.runtime.reader, ConnectionState::Up, "and search opened the reader");
    assert_eq!(snapshot.vault.notes, 1);
    assert_eq!(snapshot.queue.waiting, 0, "nothing was saved, so nothing is queued");

    session.close().await;
    drop(dir);
}

/// A folder that has only been looked at has no np children. Saying `idle` — rather than `down`, or a confident
/// `up` — is what lets the drawer be believed when it later reports a fault.
#[tokio::test]
async fn a_folder_that_was_only_opened_has_no_connections_to_show() {
    let _turn = SERIAL.lock().await;
    let harness = Harness::new();
    harness.write("a.md", b"# A\n");

    let (_sentinel, info) = ipc::np_absent();
    let session = VaultSession::open_in_with(
        common::unreachable_np(),
        info,
        None,
        harness.vault.path(),
        Consent::Undecided,
        None,
        harness.data.path().to_path_buf(),
    );

    session.listing().expect("listing needs no np");
    let snapshot = session.diagnostics().await;
    assert_eq!(snapshot.runtime.writer, ConnectionState::Idle);
    assert_eq!(snapshot.runtime.reader, ConnectionState::Idle);
    // No journal attached on this path: the drawer says there is no log rather than naming a file it never
    // opened. §4's rule about not inventing values, applied to a path string.
    assert_eq!(snapshot.runtime.log_path, None);
    assert_eq!(snapshot.runtime.log_bytes, None);

    session.close().await;
}

/// With np missing, every other half of the window still works and the drawer still answers. Nothing here may
/// panic or fill a blank with a guess.
#[tokio::test]
async fn the_snapshot_answers_even_with_no_backend_at_all() {
    let _turn = SERIAL.lock().await;
    let harness = Harness::new();
    harness.write("a.md", b"# A\n");
    harness.write("b.md", b"# B\n");

    let (_sentinel, info) = ipc::np_absent();
    let session = VaultSession::open_in_with(
        common::unreachable_np(),
        info,
        None,
        harness.vault.path(),
        Consent::Declined,
        None,
        harness.data.path().to_path_buf(),
    );

    let snapshot = session.diagnostics().await;
    assert_eq!(snapshot.vault.notes, 2);
    assert_eq!(snapshot.runtime.consent, Consent::Declined, "what the registry remembered is shown as it is");
    assert!(snapshot.recent.iter().all(|row| row.kind != TraceKind::Request), "nothing was asked, nothing to show");

    session.close().await;
}

/// The log line worth having: a drawer that says "log: 412 KB" when the file could not be opened is worse than
/// one that says nothing.
#[tokio::test]
async fn a_log_that_exists_is_named_and_measured() {
    let _turn = SERIAL.lock().await;
    let harness = Harness::new();
    harness.write("a.md", b"# A\n");

    let (_sentinel, info) = ipc::np_absent();
    let session = VaultSession::open_in_with(
        common::unreachable_np(),
        info,
        None,
        harness.vault.path(),
        Consent::Undecided,
        None,
        harness.data.path().to_path_buf(),
    )
    .with_journal(Arc::new(jott_core::diagnostics::Journal::open(
        &harness.data.path().join("logs"),
    )));

    let snapshot = session.diagnostics().await;
    let path = snapshot.runtime.log_path.clone().expect("a journal was opened");
    assert!(path.ends_with("jott.jsonl"), "{path}");
    assert_eq!(snapshot.runtime.log_bytes, Some(0), "opened, nothing written yet");

    session.close().await;
}

/// The four DTOs are a wire contract with `src/lib/diagnostics.ts`. Keys drift silently otherwise — one renamed
/// field is a drawer showing blanks while every Rust test still passes.
#[test]
fn the_snapshot_keys_are_the_ones_the_window_reads() {
    let runtime = jott_core::ipc::RuntimeInfo {
        np_version: Some("0.3.1".into()),
        np_path: Some("/np".into()),
        protocol: Some(1),
        writer: ConnectionState::Up,
        reader: ConnectionState::Idle,
        data_dir: "/data".into(),
        state_dir: "/state".into(),
        state_fallback: false,
        log_path: Some("/log".into()),
        log_bytes: Some(1),
        consent: Consent::Undecided,
        auto_fetch: Some(false),
    };
    let queue = jott_core::ipc::QueueState { waiting: 0, held_for_consent: 0, backoff: None, retry_in_ms: None };
    let vault = jott_core::ipc::VaultCounts { notes: 0, embedded: 0, indexed: 0, waiting: 0, refused: 0, warnings: 0 };
    let snapshot =
        jott_core::ipc::Diagnostics { runtime: runtime.clone(), recent: vec![], queue: queue.clone(), vault: vault.clone() };

    let cases: Vec<(serde_json::Value, &[&'static str])> = vec![
        (serde_json::to_value(&snapshot).unwrap(), &jott_core::ipc::Diagnostics::KEYS),
        (serde_json::to_value(&runtime).unwrap(), &jott_core::ipc::RuntimeInfo::KEYS),
        (serde_json::to_value(&queue).unwrap(), &jott_core::ipc::QueueState::KEYS),
        (serde_json::to_value(&vault).unwrap(), &jott_core::ipc::VaultCounts::KEYS),
    ];

    for (value, keys) in cases {
        let mut found: Vec<String> = value.as_object().expect("an object").keys().cloned().collect();
        found.sort();
        let mut expected: Vec<String> = keys.iter().map(|key| key.to_string()).collect();
        expected.sort();
        assert_eq!(found, expected);
    }

    // ConnectionState is read as a lowercase word by the window, not as a Rust variant name.
    assert_eq!(serde_json::to_value(ConnectionState::Idle).unwrap(), json!("idle"));
}

// ---- what the window is told when np answers and refuses ----

/// Measured against real np 0.3.1: an unknown folder answers `-32000` with "workspace … is not a directory that
/// exists". That is np talking, from a running process — reporting it as `npGone` tells the person to go install
/// something that is already installed. The v0.0.6 release was spent on exactly this class of mislabelling.
#[tokio::test]
async fn a_refusal_from_np_is_not_reported_as_np_being_gone() {
    let _turn = SERIAL.lock().await;
    let harness = Harness::new();
    harness.write("a.md", b"# A\n");
    let sentence = "workspace /tmp/notes is not a directory that exists (No such file or directory (os error 2)); \
                    refusing before anything is read or written";
    let (dir, client) = common::spawn(json!({
        "faults": { "search": { "code": -32000, "message": sentence } }
    }))
    .await;

    let (_sentinel, info) = ipc::np_absent();
    let session = VaultSession::open_in_with(
        NpSource::Env(common::repo_root().join("tests/fake-np/np.py")),
        info,
        Some(Arc::new(client)),
        harness.vault.path(),
        Consent::Undecided,
        None,
        harness.data.path().to_path_buf(),
    );

    let err = session.search("anything", 8, false, "hybrid").await.expect_err("np refused");
    assert_eq!(err.kind, ErrorKind::NpRefused, "np answered; nothing is missing");
    assert_eq!(err.message, sentence, "and np's own sentence is the message (§4)");

    session.close().await;
    drop(dir);
}

/// `-32601` is the one refusal that really does mean "this build is too old for what Jott asks", and np says so by
/// listing what it does answer. The floor belongs next to that list.
#[tokio::test]
async fn a_method_this_np_does_not_answer_says_which_floor_jott_needs() {
    let _turn = SERIAL.lock().await;
    let harness = Harness::new();
    harness.write("a.md", b"# A\n");
    let (dir, client) = common::spawn(json!({
        "faults": { "search": { "code": -32601, "message": "unknown method `search`: this build answers initialize, shutdown" } }
    }))
    .await;

    let (_sentinel, info) = ipc::np_absent();
    let session = VaultSession::open_in_with(
        NpSource::Env(common::repo_root().join("tests/fake-np/np.py")),
        info,
        Some(Arc::new(client)),
        harness.vault.path(),
        Consent::Undecided,
        None,
        harness.data.path().to_path_buf(),
    );

    let err = session.search("anything", 8, false, "hybrid").await.expect_err("too old");
    assert_eq!(err.kind, ErrorKind::NpTooOld);
    assert!(
        err.message.contains(&jott_core::np::resolve::min_version_text()),
        "the floor is named: {}",
        err.message
    );

    session.close().await;
    drop(dir);
}

/// The kinds are a wire contract with `src/lib/ipc.ts`, which branches on them. A kind the front end does not list
/// falls through silently, so the whole list is pinned here rather than the two new ones.
#[test]
fn every_error_kind_the_window_can_receive() {
    let kinds = [
        ErrorKind::EscapesRoot,
        ErrorKind::NotFound,
        ErrorKind::AlreadyExists,
        ErrorKind::IntoItself,
        ErrorKind::TooLarge,
        ErrorKind::NotUtf8,
        ErrorKind::NpGenerated,
        ErrorKind::ReservedName,
        ErrorKind::NoVault,
        ErrorKind::NpGone,
        ErrorKind::NpRefused,
        ErrorKind::NpTooOld,
        ErrorKind::Io,
    ];
    let spellings: Vec<String> = kinds
        .iter()
        .map(|kind| serde_json::to_value(kind).unwrap().as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        spellings,
        vec![
            "escapesRoot", "notFound", "alreadyExists", "intoItself", "tooLarge", "notUtf8", "npGenerated",
            "reservedName", "noVault", "npGone", "npRefused", "npTooOld", "io"
        ]
    );
}

// ---- one folder, one spelling, even when the picker disagrees with the registry ----

/// Filed against np as note-processor#4 and measured here: `scan` keys a workspace by the string it is handed, so
/// `/vault` and `/vault/` re-add a whole vault once when a client alternates between them. The folder picker on
/// Windows can hand back `D:\Notes\`; the registry stores `D:\Notes`. Jott normalises for its own keys, so it was
/// the one sending both spellings — and waiting for np to fix a thing we can simply stop doing.
#[tokio::test]
async fn np_is_asked_about_the_folder_and_not_one_spelling_of_it() {
    let _turn = SERIAL.lock().await;
    let harness = Harness::new();
    harness.write("a.md", b"# A\n");
    let recorded = harness.data.path().join("requests.jsonl");
    let (dir, client) = common::spawn(json!({
        "record": recorded.display().to_string(),
        "replies": {"search": search_fixture()}
    }))
    .await;

    // The spelling a picker produces: the same folder with a separator stuck on the end.
    let spelled = format!("{}/", harness.vault.path().display());
    let (_sentinel, info) = ipc::np_absent();
    let session = VaultSession::open_in_with(
        NpSource::Env(common::repo_root().join("tests/fake-np/np.py")),
        info,
        Some(Arc::new(client)),
        std::path::Path::new(&spelled),
        Consent::Undecided,
        None,
        harness.data.path().to_path_buf(),
    );

    session.search("kumquat", 8, false, "hybrid").await.expect("the stand-in answers");
    session.close().await;

    let lines = std::fs::read_to_string(&recorded).expect("the stand-in recorded what it was asked");
    let asked = lines
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .find(|line| line["method"] == "search")
        .expect("a search was recorded");
    assert_eq!(
        asked["params"]["workspace"].as_str(),
        Some(harness.vault.path().to_str().unwrap()),
        "np gets the folder, not the spelling"
    );
    drop(dir);
}

// ---- the log beside the window: what Jott did to a file ----

/// np request lines cannot answer the report that actually arrives ("I saved and my note vanished", "it deleted
/// the wrong thing"). The session writes one line per piece of file work, into the same file, under its own key
/// set — and the byte count is what gets recorded rather than the bytes.
#[tokio::test]
async fn the_log_records_the_file_work_around_np() {
    let _turn = SERIAL.lock().await;
    let harness = Harness::new();
    harness.write("b.md", b"# doomed\n");
    let journal = Arc::new(jott_core::diagnostics::Journal::open(harness.data.path()));

    // No np anywhere: this is about what Jott did to files, and it has to be recorded whether or not a child
    // is alive to be told about it.
    let (sentinel, info) = ipc::np_absent();
    let session = VaultSession::open_in_with(
        common::unreachable_np(),
        info,
        None,
        harness.vault.path(),
        Consent::Undecided,
        None,
        harness.data.path().to_path_buf(),
    )
    .with_journal(journal.clone());

    let typed = "# A, typed by somebody\n";
    session.save_note("a.md", typed).expect("saved");
    session.create_note("fresh.md").expect("created");
    let where_it_went = session.delete_entry("b.md").expect("deleted");
    session.ingest_all();
    session.close().await;

    let lines = journal.read_lines().expect("the log reads back");
    let events: Vec<&str> = lines.iter().filter_map(|line| line["event"].as_str()).collect();
    for expected in ["save", "created", "deleted", "index-now"] {
        assert!(events.contains(&expected), "{expected} is missing from {events:?}");
    }

    let save = lines.iter().find(|l| l["event"] == "save").expect("the save line");
    assert_eq!(save["rel"], "a.md");
    assert_eq!(save["bytes"], typed.len() as u64, "the count is the whole story the log needs");

    let deletion = lines.iter().find(|l| l["event"] == "deleted").expect("the delete line");
    assert_eq!(deletion["rel"], "b.md");
    assert_eq!(
        deletion["detail"].as_str(),
        Some(where_it_went.as_str()),
        "the log says the same place the window said the note went"
    );

    // The privacy rule, applied to a real run rather than to a fixture.
    let raw = std::fs::read_to_string(journal.path()).expect("the log is a file someone can send");
    assert!(!raw.contains("typed by somebody"), "note contents never enter the log: {raw}");
    drop(sentinel);
}

/// The drawer names np's folder already. It has to name Jott's own too, and say when that is a temp fallback —
/// the case where unsaved drafts land in `%TEMP%\jott-unwritable` and nobody told the person they were there.
#[tokio::test]
async fn the_snapshot_says_where_jott_keeps_its_own_files() {
    let _turn = SERIAL.lock().await;
    let harness = Harness::new();
    let (sentinel, info) = ipc::np_absent();
    let session = VaultSession::open_in_with(
        common::unreachable_np(),
        info,
        None,
        harness.vault.path(),
        Consent::Undecided,
        None,
        harness.data.path().to_path_buf(),
    );

    let snapshot = session.diagnostics().await;
    assert_eq!(snapshot.runtime.state_dir, harness.data.path().to_string_lossy(), "the folder named is the folder used");
    assert!(!snapshot.runtime.state_fallback, "somebody named this one, so it is not a fallback");
    drop(sentinel);
    session.close().await;
}

/// np 0.4.0 answers a search with what actually ran and whether there was an index to run against
/// (note-processor#5). Those four keys reach the window as-is — including `notice`, which is np's own sentence and
/// must not be paraphrased into "no results" or dressed up as Jott's words (§4).
#[tokio::test]
async fn the_window_is_told_what_ran_and_whether_there_was_an_index() {
    let _turn = SERIAL.lock().await;
    let harness = Harness::new();
    harness.write("a.md", b"# A\n");
    let (dir, client) = common::spawn(json!({"replies": {"search": {
        "hits": [],
        "indexed": false,
        "keyword_used": true,
        "semantic_used": false,
        "mode": "hybrid",
        "vectors": false,
        "offline": false,
        "reason": "no-index",
        "note": "this workspace has never been scanned — run `np scan …`"
    }}}))
    .await;

    let (_sentinel, info) = ipc::np_absent();
    let session = VaultSession::open_in_with(
        NpSource::Env(common::repo_root().join("tests/fake-np/np.py")),
        info,
        Some(Arc::new(client)),
        harness.vault.path(),
        Consent::Undecided,
        None,
        harness.data.path().to_path_buf(),
    );

    let answer = session.search("kumquat", 8, false, "hybrid").await.expect("the stand-in answered");
    assert_eq!(answer.indexed, Some(false));
    assert_eq!(answer.keyword_used, Some(true));
    assert_eq!(answer.semantic_used, Some(false), "whatever `mode` claims, the vector half did not run");
    assert_eq!(answer.notice.as_deref(), Some("this workspace has never been scanned — run `np scan …`"));

    session.close().await;
    drop(dir);
}

/// The card that replaces the tester's mystery sentence (`keyword-only: embeddings.provider is not set`) gets its
/// facts from core: which gap np's config shows, the lines that would close it, and what np's file says about
/// `auto_fetch` today. Note what is *not* here — a claim that Jott wrote that key. Presence in someone's config
/// proves only that it is there, and §4 does not let us take credit for a line we cannot trace.
#[tokio::test]
async fn the_card_names_the_provider_line_and_quotes_np_s_config_back() {
    let harness = Harness::new();
    let np_config = harness.data.path().join("config.toml");
    std::fs::write(&np_config, "[models]\nauto_fetch = true\n").unwrap();

    // The card reads a TOML file and never asks np, so PATH must not be allowed to answer for it: on this box that
    // is a real 0.4.0 binary, and any future assertion here would start it by accident.
    let (_sentinel, info) = ipc::np_absent();
    let session = VaultSession::open_in(
        common::unreachable_np(),
        info,
        harness.vault.path(),
        Consent::Granted,
        Some(np_config.clone()),
        harness.data.path().to_path_buf(),
    );

    let help = session.embeddings_help();
    assert_eq!(help.gap, "providerMissing");
    assert_eq!(help.provider, None, "np's config names no provider, so there is nothing to quote");
    assert_eq!(
        help.lines_to_add,
        vec!["[embeddings]", "provider = \"local-model\"", "model = \"embeddinggemma-2\""]
    );
    assert_eq!(help.auto_fetch, Some(true), "the card quotes the file rather than assuming");
    assert_eq!(help.config_path.as_deref(), Some(np_config.as_path()), "and says which file it read");

    session.close().await;
}

/// A provider somebody chose is not ours to change: no lines, and the provider quoted so the card can say what np
/// already says. (Separation ruling; note-processor#7 is the ask.)
#[tokio::test]
async fn a_provider_of_someones_own_gets_no_lines_from_jott() {
    let harness = Harness::new();
    let np_config = harness.data.path().join("config.toml");
    std::fs::write(&np_config, "[embeddings]\nprovider = \"none\"\nmodel = \"x\"\n").unwrap();

    let (_sentinel, info) = ipc::np_absent();
    let session = VaultSession::open_in(
        common::unreachable_np(),
        info,
        harness.vault.path(),
        Consent::Declined,
        Some(np_config),
        harness.data.path().to_path_buf(),
    );

    let help = session.embeddings_help();
    assert_eq!(help.gap, "providerOther");
    assert_eq!(help.provider.as_deref(), Some("none"));
    assert!(help.lines_to_add.is_empty(), "we do not advise overwriting a deliberate choice: {help:?}");

    session.close().await;
}

#[test]
fn consent_help_keys_are_pinned() {
    let help = ConsentHelp {
        gap: "ready".into(),
        provider: Some("local-model".into()),
        model_id: Some("embeddinggemma-2".into()),
        lines_to_add: vec![],
        config_path: Some("/home/you/.config/note-processor/config.toml".into()),
        auto_fetch: Some(true),
    };
    assert_eq!(
        serde_json::to_value(help).expect("serialises"),
        json!({ "gap": "ready", "provider": "local-model", "modelId": "embeddinggemma-2", "linesToAdd": [],
                "configPath": "/home/you/.config/note-processor/config.toml", "autoFetch": true })
    );
}

/// The state behind note-processor's own sentence *"config parse error in …"*: the file exists and cannot be read.
/// `auto_fetch` then has no answer at all — reporting `false` would be Jott stating what someone's file says when the
/// only true sentence is that nobody can read it (§4), and the guardrail's own doc says unreadable must not become "no"
/// on the wire.
#[tokio::test]
async fn an_unreadable_np_config_is_reported_as_unreadable() {
    let harness = Harness::new();
    let np_config = harness.data.path().join("config.toml");
    std::fs::write(&np_config, "[models\nauto_fetch = true\n").unwrap();

    let (_sentinel, info) = ipc::np_absent();
    let session = VaultSession::open_in(
        common::unreachable_np(),
        info,
        harness.vault.path(),
        Consent::Granted,
        Some(np_config),
        harness.data.path().to_path_buf(),
    );

    let help = session.embeddings_help();
    assert_eq!(help.gap, "configUnreadable");
    assert_eq!(help.auto_fetch, None, "a file that will not parse says nothing about auto_fetch");
    assert!(help.lines_to_add.is_empty(), "no paste advice for a file np cannot read: {:?}", help.lines_to_add);

    session.close().await;
}

/// The banner quotes `fetched`, so its wire keys are pinned like every other serialised shape: this is the payload
/// that turns "np downloaded a model" into bytes on screen, and a rename on one side reads as `NaN MiB`.
#[test]
fn an_embed_report_that_fetched_a_model_pins_its_wire_keys() {
    let report: EmbedReport = serde_json::from_str(
        r#"{"model":"embeddinggemma-2","chunks_seen":4,"current":4,"embedded":4,"refused_budget":0,"busy_gap":0,
            "errors":0,"images_seen":0,"images_embedded":0,"images_refused":0,"images_pruned":0,"images_unchecked":0,
            "images_sweep_ran":false,"image_refusals":[],"verification":"self-consistent, not verified","offline":false,
            "fetched":{"id":"embeddinggemma-2","members":[{"name":"main","bytes":309855456,"digest-verified":true}],
                       "origin":"compiled-in-pin"}}"#,
    )
    .expect("np's own embed document");

    let value = serde_json::to_value(IngestEvent::Embedded(report)).expect("serialises");
    assert_eq!(
        value["embedded"]["fetched"],
        json!({ "id": "embeddinggemma-2", "bytesTotal": 309_855_456 })
    );
}

/// The consent receipt is the card’s whole claim about what just happened, so its keys are a contract with
/// `src/lib/ipc.ts` like every other payload: renamed here, the card would say "np said nothing" where np said plenty.
#[test]
fn the_consent_receipt_is_named_exactly_as_ipc_ts_says() {
    use jott_core::ingest::consent::{AutoFetch, ConsentReceipt};
    let accepted = ConsentAnswerDto::from(ConsentReceipt {
        granted: true,
        np_refused: None,
        changed: vec!["embeddings.provider".into(), "embeddings.model".into()],
        provider: Some("local-model".into()),
        model: Some("embeddinggemma-2".into()),
        persisted_to: Some("/home/you/.config/note-processor/config.toml".into()),
        fetch_bytes: Some(309_855_456),
        auto_fetch: AutoFetch::Written,
    });
    assert_eq!(
        serde_json::to_value(&accepted).expect("receipt"),
        json!({ "granted": true, "npRefused": null,
                "changed": ["embeddings.provider", "embeddings.model"],
                "provider": "local-model", "model": "embeddinggemma-2",
                "persistedTo": "/home/you/.config/note-processor/config.toml",
                "fetchBytes": 309_855_456, "autoFetch": "written", "autoFetchError": null })
    );

    // A refusal carries np's sentence and the word `untouched`: the card must be able to say "Jott changed nothing"
    // without inferring it from an absent field.
    let refused = ConsentAnswerDto::from(ConsentReceipt {
        granted: true,
        np_refused: Some("config io: no pinned model".into()),
        changed: vec![],
        provider: None,
        model: None,
        persisted_to: None,
        fetch_bytes: None,
        auto_fetch: AutoFetch::Untouched,
    });
    assert_eq!(refused.auto_fetch, "untouched");
    let value = serde_json::to_value(&refused).expect("refusal");
    assert_eq!(value["npRefused"], json!("config io: no pinned model"));
    assert_eq!(value["autoFetch"], json!("untouched"));
    assert_eq!(value["changed"], json!([]));

    // And the fourth state names the file it could not write, because "written" would be a lie about the guardrail.
    let failed = ConsentAnswerDto::from(ConsentReceipt {
        granted: true,
        np_refused: None,
        changed: vec![],
        provider: Some("local-model".into()),
        model: Some("embeddinggemma-2".into()),
        persisted_to: None,
        fetch_bytes: None,
        auto_fetch: AutoFetch::Failed { sentence: "could not write /x/config.toml: permission denied".into() },
    });
    let value = serde_json::to_value(&failed).expect("failed");
    assert_eq!(value["autoFetch"], json!("failed"));
    assert_eq!(value["autoFetchError"], json!("could not write /x/config.toml: permission denied"));
}

// ---- answering consent through the session (v0.0.15 review). These exist because the free function was tested and
// the session was not: `VaultSession::answer_consent` could — and did — let a second writer raise the guardrail after
// np refused, while the card and the log both said it had been left alone.

/// The button's whole point in one test: np refused, so nothing of ours moved either.
#[tokio::test]
async fn a_refused_yes_leaves_the_guardrail_exactly_where_it_was() {
    let _turn = SERIAL.lock().await;
    let harness = Harness::new();
    harness.write("a.md", b"# A\n");
    let config = harness.data.path().join("np-config.toml");
    let before = "[models]\nauto_fetch = false\n";
    std::fs::write(&config, before).expect("np config");

    let sentence = "config io: no pinned model \"no-such\": this build can name \"embeddinggemma-2\"";
    let (_d, client) = common::spawn(json!({
        "faults": {"embeddings.enable": {"code": -32000, "message": sentence}}
    }))
    .await;
    let (_sentinel, info) = ipc::np_absent();
    let session = VaultSession::open_in_with(
        NpSource::Env(common::repo_root().join("tests/fake-np/np.py")),
        info,
        Some(Arc::new(client)),
        harness.vault.path(),
        Consent::Undecided,
        Some(config.clone()),
        harness.data.path().to_path_buf(),
    );

    let receipt = session.answer_consent(true).await;
    assert_eq!(receipt.np_refused.as_deref(), Some(sentence), "np's words survive to the card");
    assert_eq!(receipt.auto_fetch, AutoFetch::Untouched, "{:?}", receipt.auto_fetch);
    assert_eq!(std::fs::read_to_string(&config).unwrap(), before, "a refused yes must not raise the guardrail");
    session.close().await;
}

/// The other half of the asymmetry: a *no* is Jott's own protection to install, so np refusing to revoke its provider
/// does not get to leave `auto_fetch = true` behind.
#[tokio::test]
async fn a_decline_lowers_the_guardrail_even_when_np_refuses_to_revoke() {
    let _turn = SERIAL.lock().await;
    let harness = Harness::new();
    harness.write("a.md", b"# A\n");
    let config = harness.data.path().join("np-config.toml");
    std::fs::write(&config, "[models]\nauto_fetch = true\n").expect("np config");

    let (_d, client) = common::spawn(json!({
        "faults": {"embeddings.disable": {"code": -32000, "message": "config io: cannot write the config file"}}
    }))
    .await;
    let (_sentinel, info) = ipc::np_absent();
    let session = VaultSession::open_in_with(
        NpSource::Env(common::repo_root().join("tests/fake-np/np.py")),
        info,
        Some(Arc::new(client)),
        harness.vault.path(),
        Consent::Undecided,
        Some(config.clone()),
        harness.data.path().to_path_buf(),
    );

    let receipt = session.answer_consent(false).await;
    assert!(receipt.np_refused.is_some(), "np's refusal is still reported: {:?}", receipt.np_refused);
    assert_eq!(receipt.auto_fetch, AutoFetch::Written, "{:?}", receipt.auto_fetch);
    assert!(!auto_fetch_allowed(&config), "nothing may download after a no");
    session.close().await;
}

/// No child to ask, and a person who said no: Jott's key needs no np, so the guardrail still goes down. A grant with
/// no np is the opposite case — there is no door to open, so nothing of ours moves either.
#[tokio::test]
async fn declining_without_np_still_writes_false_and_granting_it_writes_nothing() {
    let _turn = SERIAL.lock().await;
    let harness = Harness::new();
    harness.write("a.md", b"# A\n");

    let (_sentinel, info) = ipc::np_absent();

    let config = harness.data.path().join("declined.toml");
    std::fs::write(&config, "[models]\nauto_fetch = true\n").expect("np config");
    let session = VaultSession::open_in_with(
        common::unreachable_np(),
        info.clone(),
        None,
        harness.vault.path(),
        Consent::Undecided,
        Some(config.clone()),
        harness.data.path().to_path_buf(),
    );
    let receipt = session.answer_consent(false).await;
    assert!(receipt.np_refused.is_some(), "the window says there was no np to ask");
    assert_eq!(receipt.auto_fetch, AutoFetch::Written, "Jott's own key does not need a child: {:?}", receipt.auto_fetch);
    assert!(!auto_fetch_allowed(&config));
    session.close().await;

    let config = harness.data.path().join("granted.toml");
    let before = "[models]\nauto_fetch = false\n";
    std::fs::write(&config, before).expect("np config");
    let session = VaultSession::open_in_with(
        common::unreachable_np(),
        info,
        None,
        harness.vault.path(),
        Consent::Undecided,
        Some(config.clone()),
        harness.data.path().to_path_buf(),
    );
    let receipt = session.answer_consent(true).await;
    assert!(receipt.np_refused.is_some());
    assert_eq!(receipt.auto_fetch, AutoFetch::Untouched);
    assert_eq!(std::fs::read_to_string(&config).unwrap(), before, "no np, no door: our key stays put");
    session.close().await;
}

/// An accepted grant is still the ordinary path worth pinning at this level: np's keys by np, ours by us.
#[tokio::test]
async fn an_accepted_yes_writes_our_key_once_and_reports_nps() {
    let _turn = SERIAL.lock().await;
    let harness = Harness::new();
    harness.write("a.md", b"# A\n");
    let config = harness.data.path().join("np-config.toml");
    std::fs::write(&config, "[models]\nauto_fetch = false\n").expect("np config");

    let (_d, client) = common::spawn(json!({
        "replies": {"embeddings.enable": {"changed": ["embeddings.provider", "embeddings.model"],
                                          "fetch": {"bytes_total": 309_855_456, "members": ["main"]},
                                          "model": "embeddinggemma-2", "provider": "local-model"}}
    }))
    .await;
    let (_sentinel, info) = ipc::np_absent();
    let session = VaultSession::open_in_with(
        NpSource::Env(common::repo_root().join("tests/fake-np/np.py")),
        info,
        Some(Arc::new(client)),
        harness.vault.path(),
        Consent::Undecided,
        Some(config.clone()),
        harness.data.path().to_path_buf(),
    );

    let receipt = session.answer_consent(true).await;
    assert!(receipt.np_refused.is_none());
    assert_eq!(receipt.changed, vec!["embeddings.provider".to_string(), "embeddings.model".to_string()]);
    assert_eq!(receipt.auto_fetch, AutoFetch::Written);
    let after = std::fs::read_to_string(&config).unwrap();
    assert!(after.contains("auto_fetch = true"), "{after}");
    assert!(!after.contains("[embeddings]"), "Jott wrote into `[embeddings]`: {after}");
    session.close().await;
}

/// The status-line dot asks one question of note-processor — are the model's bytes on this disk? — and answers with
/// what np said, in four distinguishable states. Flattening "np never answered" into `false` would draw a grey dot on
/// a folder whose model is sitting in `%APPDATA%`, which is the same mistake as a green dot on a folder with none.
#[tokio::test]
async fn the_model_dot_reports_what_note_processor_said_about_the_model() {
    let _turn = SERIAL.lock().await;
    // np's own row shape (`state: "present"`), not one Jott invented — the same fixture the consent gate is tested with.
    let present = json!({
        "models": [{ "id": "embeddinggemma-2", "dim": 768, "sha256": null, "prefix_doc": null,
                     "prefix_query": null, "installed_at": 1, "state": "present", "members": [] }],
        "fetching": [], "unclaimed": [], "models_dir_error": null,
        "pinned": [{ "id": "embeddinggemma-2", "bytes_total": 309855456 }]
    });
    let scenario_dir = scenario(json!({ "replies": { "models.list": present } }));

    let harness = Harness::new();
    let config = harness.data.path().join("np-config.toml");
    std::fs::write(
        &config,
        "[models]\nauto_fetch = true\n\n[embeddings]\nprovider = \"local-model\"\nmodel = \"embeddinggemma-2\"\n",
    )
    .expect("config");
    let session = VaultSession::open_in(
        fake_np(),
        info(),
        harness.vault.path(),
        Consent::Granted,
        Some(config),
        harness.data.path().to_path_buf(),
    );

    let status = session.embeddings_status().await;
    assert_eq!(status.provider.as_deref(), Some("local-model"));
    assert_eq!(status.model_id.as_deref(), Some("embeddinggemma-2"));
    assert_eq!(status.present, Some(true), "np listed the model, so the dot says it is here");
    assert_eq!(status.bytes_total, Some(309_855_456));

    // The keys the webview reads, pinned: there is no compiler between this struct and `src/lib/ipc.ts`.
    let wire = serde_json::to_value(&status).expect("serialize");
    assert_eq!(
        wire,
        json!({
            "provider": "local-model",
            "modelId": "embeddinggemma-2",
            "present": true,
            "bytesTotal": 309855456,
            "downloading": null,
            "configUnreadable": false,
            "modelsDirError": null
        })
    );

    assert!(recorded(scenario_dir.path()).iter().any(|m| m == "models.list"), "np was asked");
    session.close().await;
}

/// Configured but not fetched is a state of its own: it is the one the amber dot exists for, and the difference
/// between "waiting on a 296 MiB download" and "nothing is set up".
#[tokio::test]
async fn a_model_that_is_not_on_the_disk_is_reported_absent_rather_than_unknown() {
    let _turn = SERIAL.lock().await;
    let _scenario = scenario(json!({
        "replies": { "models.list": {
            "models": [], "fetching": [], "unclaimed": [], "models_dir_error": null,
            "pinned": [{ "id": "embeddinggemma-2", "bytes_total": 309855456 }]
        }}
    }));

    let harness = Harness::new();
    let config = harness.data.path().join("np-config.toml");
    std::fs::write(
        &config,
        "[models]\nauto_fetch = false\n\n[embeddings]\nprovider = \"local-model\"\nmodel = \"embeddinggemma-2\"\n",
    )
    .expect("config");
    let session = VaultSession::open_in(
        fake_np(),
        info(),
        harness.vault.path(),
        Consent::Undecided,
        Some(config),
        harness.data.path().to_path_buf(),
    );

    let status = session.embeddings_status().await;
    assert_eq!(status.present, Some(false));
    // The price still travels: this is the state where a person decides whether to fetch at all.
    assert_eq!(status.bytes_total, Some(309_855_456));
    session.close().await;
}

/// No np, no answer, no claim. `None` here is the difference between "the model is not on this disk" and "nobody was
/// able to say", and the window draws those two differently (§18: absence is not a verdict).
#[tokio::test]
async fn an_unanswered_question_stays_unknown_instead_of_becoming_absent() {
    let _turn = SERIAL.lock().await;
    let harness = Harness::new();
    let config = harness.data.path().join("np-config.toml");
    std::fs::write(
        &config,
        "[embeddings]\nprovider = \"local-model\"\nmodel = \"embeddinggemma-2\"\n",
    )
    .expect("config");
    let session = VaultSession::open_in(
        NpSource::Env(PathBuf::from("/nonexistent/np")),
        info(),
        harness.vault.path(),
        Consent::Granted,
        Some(config),
        harness.data.path().to_path_buf(),
    );

    let status = session.embeddings_status().await;
    assert_eq!(status.provider.as_deref(), Some("local-model"), "the settings file still says this");
    assert_eq!(status.present, None, "nobody answered, so nothing is claimed either way");
    session.close().await;
}

/// Embeddings nobody allowed are not a download waiting to happen: the question never goes to np at all, and the dot
/// says off from the settings file alone. Asking np here would wake a second child for a folder that will never embed.
#[tokio::test]
async fn embeddings_that_were_never_allowed_are_reported_without_asking_note_processor() {
    let _turn = SERIAL.lock().await;
    let scenario_dir = scenario(json!({}));

    let harness = Harness::new();
    let config = harness.data.path().join("np-config.toml");
    std::fs::write(&config, "[models]\nauto_fetch = false\n").expect("config");
    let session = VaultSession::open_in(
        fake_np(),
        info(),
        harness.vault.path(),
        Consent::Declined,
        Some(config),
        harness.data.path().to_path_buf(),
    );

    let status = session.embeddings_status().await;
    assert_eq!(status.provider, None, "no provider is named, so nothing is configured");
    assert_eq!(status.model_id, None);
    assert_eq!(status.present, None);
    assert!(
        !recorded(scenario_dir.path()).iter().any(|m| m == "models.list"),
        "a folder with no embeddings still woke note-processor: {:?}",
        recorded(scenario_dir.path())
    );
    session.close().await;
}

/// A settings file that will not parse is not a settings file that says "no provider". The consent card already
/// refuses to flatten those two (`SettingsGap::ConfigUnreadable`), and the dot has the same obligation: "embeddings off"
/// about a file nobody could read is an instruction to go turn them on, in a folder where somebody's np is refusing to
/// parse its own config.
#[tokio::test]
async fn an_unreadable_settings_file_is_not_reported_as_embeddings_off() {
    let _turn = SERIAL.lock().await;
    let scenario_dir = scenario(json!({}));

    let harness = Harness::new();
    let config = harness.data.path().join("np-config.toml");
    std::fs::write(&config, "[embeddings\nprovider = \"local-model\"\n").expect("broken toml");
    let session = VaultSession::open_in(
        fake_np(),
        info(),
        harness.vault.path(),
        Consent::Granted,
        Some(config),
        harness.data.path().to_path_buf(),
    );

    let status = session.embeddings_status().await;
    assert!(status.config_unreadable, "a file that fails to parse is unreadable");
    assert_eq!(status.provider, None, "an unreadable file names no provider");
    assert_eq!(status.present, None, "and proves nothing about a download either");
    assert!(
        !recorded(scenario_dir.path()).iter().any(|m| m == "models.list"),
        "a settings file nobody can read still woke note-processor: {:?}",
        recorded(scenario_dir.path())
    );
    session.close().await;
}

/// np answers an unreadable models directory as a **result** carrying `models_dir_error`, not as a fault. Saying
/// "note-processor did not answer" about that would be wrong in a way the person can act on: np is running fine and
/// telling us it cannot look. Its own sentence travels so the dot can quote it.
#[tokio::test]
async fn a_models_directory_that_would_not_list_quotes_note_processors_own_reason() {
    let _turn = SERIAL.lock().await;
    let _scenario = scenario(json!({
        "replies": { "models.list": {
            "models": [], "fetching": [], "unclaimed": [],
            "models_dir_error": "permission denied reading the model store under C:\\Users\\chris\\AppData",
            "pinned": [{ "id": "embeddinggemma-2", "bytes_total": 309855456 }]
        }}
    }));

    let harness = Harness::new();
    let config = harness.data.path().join("np-config.toml");
    std::fs::write(
        &config,
        "[embeddings]\nprovider = \"local-model\"\nmodel = \"embeddinggemma-2\"\n",
    )
    .expect("config");
    let session = VaultSession::open_in(
        fake_np(),
        info(),
        harness.vault.path(),
        Consent::Granted,
        Some(config),
        harness.data.path().to_path_buf(),
    );

    let status = session.embeddings_status().await;
    assert_eq!(status.present, None, "an unlistable directory is nobody's verdict about a download");
    assert_eq!(
        status.models_dir_error.as_deref(),
        Some("permission denied reading the model store under C:\\Users\\chris\\AppData"),
        "np's own sentence is what should be shown"
    );
    session.close().await;
}

/// A price belongs to the model it was quoted for. `only_price()` exists for a fresh np that named nothing and pinned
/// exactly one thing; once a model *has* been named, taking "the table's one entry" as a fallback can only ever attach
/// another model's size to this one — which is how a dot ends up saying *"no-such-model is not on this disk yet (about
/// 296 MiB)"* about bytes nobody priced.
#[tokio::test]
async fn a_price_is_only_ever_quoted_for_the_model_it_belongs_to() {
    let _turn = SERIAL.lock().await;
    let _scenario = scenario(json!({
        "replies": { "models.list": {
            "models": [], "fetching": [], "unclaimed": [], "models_dir_error": null,
            "pinned": [{ "id": "embeddinggemma-2", "bytes_total": 309855456 }]
        }}
    }));

    let harness = Harness::new();
    let config = harness.data.path().join("np-config.toml");
    std::fs::write(
        &config,
        "[embeddings]\nprovider = \"local-model\"\nmodel = \"some-other-model\"\n",
    )
    .expect("config");
    let session = VaultSession::open_in(
        fake_np(),
        info(),
        harness.vault.path(),
        Consent::Granted,
        Some(config),
        harness.data.path().to_path_buf(),
    );

    let status = session.embeddings_status().await;
    assert_eq!(status.model_id.as_deref(), Some("some-other-model"), "np's key is quoted as written");
    assert_eq!(status.present, Some(false), "it is not among the rows np listed");
    assert_eq!(
        status.bytes_total, None,
        "309855456 is embeddinggemma-2's price; quoting it for some-other-model would be Jott's arithmetic"
    );
    session.close().await;
}

#[test]
fn a_window_that_fell_behind_keeps_its_stream() {
    use tokio::sync::broadcast::error::RecvError;
    // The bridge used to end on any error, so one slow paint left the window permanently deaf — including during
    // a model download. `Lagged` names rows nobody read; it does not name a closed session.
    assert!(
        jott_core::ipc::bridge_stays(&RecvError::Lagged(7)),
        "a window that fell behind is a window still there"
    );
    assert!(
        !jott_core::ipc::bridge_stays(&RecvError::Closed),
        "a dropped session is the only real end"
    );
}

#[tokio::test]
async fn the_next_row_arrives_after_a_window_fell_behind() {
    // Surviving the error in principle is not enough: what matters is that the *next* event still gets through.
    let (tx, mut rx) = tokio::sync::broadcast::channel::<u32>(2);
    for n in 0..5 {
        let _ = tx.send(n);
    }
    let error = rx.recv().await.expect_err("the small buffer must report the lag");
    assert!(jott_core::ipc::bridge_stays(&error), "and the policy must keep going: {error:?}");
    // A two-slot buffer that took five rows still holds the last two. The point is not which row survives — it is
    // that the stream carries on instead of ending, so the *next* thing the window needs to hear still arrives.
    assert_eq!(rx.recv().await.unwrap(), 3, "the newest rows are still delivered");
    assert_eq!(rx.recv().await.unwrap(), 4, "and the one after them too");
}

/// A download in flight, read from np's own fetch markers rather than from a progress line that may never have
/// reached the window. Two markers arrive (a vision model is `main` plus an `mmproj`), so the banner names one
/// steadily — and both figures are np's, because a byte count Jott added up would be Jott's arithmetic.
#[tokio::test]
async fn a_download_in_flight_is_reported_from_nps_own_marker() {
    let _turn = SERIAL.lock().await;
    // Shaped from what bundled np 0.5.1 actually answered for `models.list` while fetching: id, bytes, nonce,
    // path, started_at, marker_error — measured, not guessed.
    let listing = json!({
        "models": [],
        "fetching": [
            { "id": "mmproj", "bytes_done": 4_000_000, "bytes_total": 940_000_000u64,
              "nonce": "n-2", "path": "/models/.fetching.mmproj.json", "started_at": 1, "marker_error": null },
            { "id": "embeddinggemma-2", "bytes_done": 184_550_345, "bytes_total": 309_855_456u64,
              "nonce": "n-1", "path": "/models/.fetching.embeddinggemma-2.json", "started_at": 1,
              "marker_error": null },
        ],
        "unclaimed": [], "models_dir_error": null,
        "pinned": [{ "id": "embeddinggemma-2", "bytes_total": 309_855_456 }]
    });
    let scenario_dir = scenario(json!({ "replies": { "models.list": listing } }));

    let harness = Harness::new();
    let config = harness.data.path().join("np-config.toml");
    // A provider has to be configured or the status stops before asking np — it would be answering from the
    // settings file alone, and there would be no marker list to read.
    std::fs::write(
        &config,
        "[models]\nauto_fetch = true\n\n[embeddings]\nprovider = \"local-model\"\nmodel = \"embeddinggemma-2\"\n",
    )
    .expect("config");
    let session = VaultSession::open_in(
        fake_np(),
        info(),
        harness.vault.path(),
        Consent::Granted,
        Some(config),
        harness.data.path().to_path_buf(),
    );

    let status = session.embeddings_status().await;
    let down = status.downloading.as_ref().expect("np said bytes were moving");
    assert_eq!(down.model_id, "embeddinggemma-2", "the same one every poll, not whichever marker was written last");
    assert_eq!(down.bytes_done, 184_550_345);
    assert_eq!(down.bytes_total, Some(309_855_456));

    let wire = serde_json::to_value(&status).expect("serialize");
    assert_eq!(
        wire["downloading"],
        json!({ "modelId": "embeddinggemma-2", "bytesDone": 184_550_345, "bytesTotal": 309_855_456 }),
        "the keys the webview reads, pinned"
    );

    drop(scenario_dir);
}

// ---- issue #1: §11's close-out, asked at quit ---------------------------------------------------------
//
// The close-out existed as a command nothing called, so everything below was unreachable from the shipped app.
// These pins are about the body itself — that it drains, that it asks np rather than killing it, and that a slow
// np cannot hold a window open — because the shell's job is now only to make this call (`tests/shell_quit.rs`).

/// Every method the stand-in was asked for, in order, handshake excluded and `shutdown` kept: the whole point of
/// these three tests is the goodbye, and [`recorded`] filters it out for the tests that do not care about it.
fn asked_before_exit(dir: &Path) -> Vec<String> {
    let text = std::fs::read_to_string(dir.join("calls.ndjson")).unwrap_or_default();
    text.lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter_map(|line| line["method"].as_str().map(str::to_string))
        .filter(|m| m != "initialize")
        .collect()
}

#[tokio::test]
async fn quitting_gives_the_pending_save_its_pass_and_closes_nps_index() {
    let _turn = SERIAL.lock().await;
    let harness = Harness::new();
    harness.write("a.md", b"# A\n");
    let dir = scenario(json!({ "replies": { "scan": serde_json::from_str::<Value>(SCAN_REPORT).unwrap() } }));
    let session = harness.session_with(fake_np());

    // The thing §11 is about: a save made seconds before the window goes away.
    session.save_note("a.md", "# A, edited\n").expect("save");

    tokio::time::timeout(std::time::Duration::from_secs(20), ipc::flush_on_quit(Some(session), None))
        .await
        .expect("the close-out returns on its own");

    let asked = asked_before_exit(dir.path());
    assert!(asked.iter().any(|m| m == "scan"), "the save still owed a pass got it: {asked:?}");
    assert!(asked.iter().any(|m| m == "shutdown"), "np was asked to shut down, not killed holding its index open — the Windows lock friction of §3: {asked:?}");
}

#[tokio::test]
async fn quitting_asks_the_child_that_was_started_at_startup_and_never_claimed() {
    let _turn = SERIAL.lock().await;
    // The window opened and closed without ever opening a folder: the startup handshake's child is still warm,
    // and an np left running after Jott exits keeps its index open on Windows.
    let (scenario_dir, client) = common::spawn(json!({})).await;
    let warm = Arc::new(client);
    assert!(!warm.is_closed(), "the child is running when the window closes");

    tokio::time::timeout(std::time::Duration::from_secs(20), ipc::flush_on_quit(None, Some(Arc::clone(&warm))))
        .await
        .expect("the close-out returns on its own");

    // Polled, not asserted inline: `shutdown_within` waits for the child to exit, but `closed` is set by the
    // reader task when stdout reaches EOF, which lands a moment later — and on a loaded runner that moment is
    // long enough to fail a test about a child that did in fact shut down. Same patience as
    // `a_session_adopts_the_child_the_app_already_started`, which learned this first.
    for _ in 0..40 {
        if warm.is_closed() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(warm.is_closed(), "a warm child nobody claimed was left running");
    drop(scenario_dir);
}

#[tokio::test]
async fn quitting_never_waits_for_a_model_that_is_still_thinking() {
    let _turn = SERIAL.lock().await;
    let harness = Harness::new();
    harness.write("a.md", b"# A\n");
    // An np mid-scan for two minutes: closing is stopping, not waiting (§11's cap), and a window that cannot
    // close is worse than one that closes while np finishes thinking.
    let _scenario = scenario(json!({
        "sleep_ms": { "scan": 120_000 },
        "replies": { "scan": serde_json::from_str::<Value>(SCAN_REPORT).unwrap() },
    }));
    let session = harness.session_with(fake_np());
    session.save_note("a.md", "# A, edited\n").expect("save");

    let started = std::time::Instant::now();
    tokio::time::timeout(std::time::Duration::from_secs(20), ipc::flush_on_quit(Some(session), None))
        .await
        .expect("the close-out bounds itself by the queue's cap rather than by np's");
    assert!(
        started.elapsed() < std::time::Duration::from_secs(15),
        "quitting waited {:?} for a pass that had two minutes of sleep left in it",
        started.elapsed()
    );
}

// ---- issue #2: a registry refusal is an answer, not a shrug ------------------------------------------
//
// `App::open` used to read `register(path).ok().flatten()`, which turned `NotADirectory` into "no warning" and let
// the session be replaced by one pointing at a folder that had been deleted. The sentence the registry wrote is
// the thing the window needs, so it travels intact rather than being dropped on the floor.

#[test]
fn a_refusal_from_the_registry_arrives_as_the_sentence_it_wrote() {
    // §4: name the folder, say what is wrong, no wrapping prose from us on top of it. The registry's own lines
    // already do that, so `From` keeps them byte for byte instead of nesting them in "could not use".
    let missing: ipc::IpcError = StoreError::NotADirectory { path: "/home/ana/notes".into() }.into();
    assert_eq!(missing.kind, ErrorKind::Io);
    assert_eq!(missing.message, "/home/ana/notes is not a folder I can open");

    let unregistered: ipc::IpcError = StoreError::NotRegistered { path: "/tmp/vault".into() }.into();
    assert!(
        unregistered.message.ends_with("is not a registered vault, so there is nothing here to record its consent in"),
        "{}",
        unregistered.message
    );

    // An io failure keeps np's/OS's own reason beside the file it failed on.
    let io: ipc::IpcError = StoreError::Io {
        path: "/home/ana/.local/share/jott/vaults.json".into(),
        source: std::io::Error::new(std::io::ErrorKind::PermissionDenied, "permission denied"),
    }
    .into();
    assert_eq!(io.kind, ErrorKind::Io);
    assert!(io.message.contains("permission denied"), "{}", io.message);
}

// ---- issue #5: a name Windows keeps for a device ----------------------------------------------------

/// The window asks for `nul.md` and gets a refusal rather than a save that went to NUL. This is the same rule as
/// `vault_files.rs`, seen from the wire: the sentence has to arrive with a kind the front end lists, because a kind
/// it does not know falls through to nothing.
#[tokio::test]
async fn a_note_named_after_a_windows_device_is_refused_over_ipc() {
    let _turn = SERIAL.lock().await;
    let harness = Harness::new();
    let session = harness.session();

    for rel in ["nul.md", "notes/AUX.md", "com1.md"] {
        let error = session.save_note(rel, "# typed and lost\n").expect_err(rel);
        assert_eq!(error.kind, ErrorKind::ReservedName, "{rel}: {}", error.message);
        assert!(error.message.contains("Windows"), "the sentence says why: {}", error.message);
    }

    // A note that is merely named *near* one of those stems still saves — the rule is a stem, not a substring.
    session.save_note("null.md", "# a real note\n").expect("null.md is a note");
    let listing = session.listing().expect("listing");
    assert_eq!(listing.nodes.len(), 1, "and it is the only thing in the vault: {:?}", listing.nodes.iter().map(|n| n.rel.clone()).collect::<Vec<_>>());
}
