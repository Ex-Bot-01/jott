//! The command surface as the webview will call it. This is where "is the front end trusted?" and "what
//! does the window actually receive?" get answered, in a test binary, on any machine — because the shell
//! that hosts these commands only builds on a desktop, and a promise you can only check on one operating
//! system is not checked (§19).

mod common;

use common::repo_root;
use jott_core::diagnostics::{TraceEvent, TraceKind};
use jott_core::ingest::consent::auto_fetch_allowed;
use jott_core::ingest::queue::{IngestEvent, StateReason};
use jott_core::ipc::{self, ConnectionState, ErrorKind, HitDto, NoteContent, RenameOutcome, SearchResults, VaultListing, WriteResult};
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

    session.search("kumquat", 8).await.expect("the stand-in answers");

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

    let err = session.search("anything", 8).await.expect_err("np is not here");
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

    session.search("kumquat", 8).await.expect("the stand-in answers");
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
