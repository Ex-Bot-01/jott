//! The command surface as the webview will call it. This is where "is the front end trusted?" and "what
//! does the window actually receive?" get answered, in a test binary, on any machine — because the shell
//! that hosts these commands only builds on a desktop, and a promise you can only check on one operating
//! system is not checked (§19).

mod common;

use common::repo_root;
use jott_core::ingest::queue::{IngestEvent, StateReason};
use jott_core::ipc::{self, ErrorKind, HitDto, NoteContent, RenameOutcome, SearchResults, VaultListing, WriteResult};
use jott_core::np::outcomes::ScanReport;
use jott_core::np::resolve::{NpMissing, NpSource, Os};
use jott_core::np::{NpFeatures, NpInfo};
use jott_core::vault::walk::EntryKind;
use jott_core::ipc::VaultSession;
use jott_core::vault::stamps::StampState;
use jott_core::vault::store::Consent;
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
    let error = session.search("anything", 8).await.expect_err("np is not installed here");
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
    let answer = session.search("A", 8).await.expect("the reader answers");
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
    assert!(macos.message.contains("0.2.0") && macos.message.contains("0.3.1"), "{}", macos.message);
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

    let search = serde_json::to_value(SearchResults {
        hits: vec![HitDto { path: "a.md".into(), snippet: None, kind: "note".into(), score: 1.0, bm25: None, cosine: None }],
        mode: "keyword".into(),
        vectors: false,
        offline: false,
        reason: None,
    })
    .expect("search");
    assert_eq!(search, json!({ "hits": [{"path": "a.md", "snippet": null, "kind": "note", "score": 1.0, "bm25": null, "cosine": null}],
                                 "mode": "keyword", "vectors": false, "offline": false, "reason": null }));

    let note = serde_json::to_value(NoteContent {
        rel: "a.md".into(),
        text: String::new(),
        hash: "h".into(),
        disk_hash: "h".into(),
        disk_mtime_ms: 3,
        mirrored: None,
    })
    .expect("note");
    // Sorted, because `serde_json`'s map is a BTreeMap: the names are the contract, their order is not.
    let mut keys: Vec<&str> = note.as_object().expect("object").keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, vec!["diskHash", "diskMtimeMs", "hash", "mirrored", "rel", "text"]);

    // The status line's events, which `src/lib/statusLine.ts` pattern-matches on.
    let working = IngestEvent::Working { phase: "embedding".into(), done: 3, total: Some(7) };
    assert_eq!(serde_json::to_value(working).expect("working"), json!({ "working": { "phase": "embedding", "done": 3, "total": 7 } }));

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
    assert!(payload.install_hint.contains("0.3.1"), "the hint states the floor: {}", payload.install_hint);

    // A fault sentence from np is kept whole rather than paraphrased (§4).
    let fault = ipc::NpGone::Fault("np said: workspace is locked".into());
    assert_eq!(fault.payload(Os::Linux).message, "np said: workspace is locked");
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
    let (source, info) = ipc::np_absent();
    let session = VaultSession::open_in(source, info, harness.vault.path(), Consent::Undecided, None, harness.data.path().to_path_buf());

    assert_eq!(session.listing().expect("the tree needs no np").nodes.len(), 1);
    session.save_note("b.md", "# B\n").expect("neither does a save");
    assert_eq!(session.search("anything", 8).await.expect_err("np is not here").kind, ErrorKind::NpGone);

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
