//! The log the Windows hand-off is asked to send back (Task 11): what Jott asked np, how long it took, and
//! what came back — never what the notes say.

mod common;

use common::repo_root;
use jott_core::diagnostics::{Journal, RequestRecord};
use jott_core::ipc::VaultSession;
use jott_core::np::resolve::NpSource;
use jott_core::np::{NpFeatures, NpInfo};
use jott_core::vault::store::Consent;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// Both the journal tests and the session tests set process-wide state (the stand-in's scenario), so they
/// take turns.
static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn info() -> NpInfo {
    NpInfo {
        version: "0.3.1".into(),
        protocol: 1,
        methods: vec!["initialize", "search", "scan", "embed", "models.list"]
            .into_iter()
            .map(str::to_string)
            .collect(),
        features: NpFeatures { local_model: true, fetch: true, vision: false },
        data_dir: "/fake/data".into(),
        progress: true,
    }
}

fn scenario(value: Value) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("scenario dir");
    std::fs::write(dir.path().join("scenario.json"), value.to_string()).expect("write scenario");
    std::env::set_var("FAKE_NP_SCENARIO", dir.path().join("scenario.json"));
    dir
}

fn fake_np() -> NpSource {
    NpSource::Env(repo_root().join("tests/fake-np/np.py"))
}

const SCAN_REPORT: &str = r#"{"added": 1, "modified": 0, "moved": 0, "removed": 0, "unchanged": 0,
    "skipped": [], "notes": 1, "unresolved_links": 0, "unresolved_targets": [],
    "index_files": ["INDEX.md"], "elapsed_ms": 7, "upgrade_orphans": null, "merged_notes": null,
    "recorded_upgrade_orphans": null, "recorded_merged_notes": null, "unindexable": null}"#;

const SEARCH_REPLY: &str = r#"{"hits": [], "mode": "keyword", "vectors": false, "offline": false,
     "fetched": null, "note": null, "reason": null}"#;

#[test]
fn one_line_per_request_with_the_facts_somebody_needs() {
    let dir = tempfile::tempdir().expect("app data");
    let journal = Journal::open(dir.path());
    assert_eq!(journal.path(), dir.path().join("logs").join("jott.jsonl"), "the path the docs promise");

    journal.record(&RequestRecord::new("search", Path::new("/home/ana/notes"), Duration::from_millis(1_742), "applied")).expect("record");
    let line = jott_core::diagnostics::last_line(&journal.path()).expect("file").expect("one line");
    let record: Value = serde_json::from_str(&line).expect("json");
    assert_eq!(record["method"], "search");
    assert_eq!(record["workspace"], "/home/ana/notes", "which vault was slow is half the answer");
    assert_eq!(record["elapsed_ms"], 1742, "np's time, in milliseconds, unscaled");
    assert_eq!(record["outcome"], "applied");
    assert!(record["at_ms"].is_number(), "so two runs can be told apart");
}

#[test]
fn a_windows_path_is_written_with_forward_slashes() {
    let dir = tempfile::tempdir().expect("app data");
    let journal = Journal::open(dir.path());
    // A log line is one JSON document: backslashes are escapes in there, and a path full of them is a
    // parsing trap for whoever is asked to read the file.
    journal
        .record(&RequestRecord::new("scan", Path::new(r"C:\Users\ana\notes"), Duration::from_millis(3), "locked"))
        .expect("record");
    let record: Value =
        serde_json::from_str(&jott_core::diagnostics::last_line(&journal.path()).expect("file").expect("line")).expect("json");
    assert_eq!(record["workspace"], "C:/Users/ana/notes");
}

#[test]
fn a_fault_keeps_nps_code_and_its_sentence_exactly() {
    let dir = tempfile::tempdir().expect("app data");
    let journal = Journal::open(dir.path());
    // np's own wording, including its punctuation: §4 says show it, and a log that paraphrases is a log
    // that sends someone looking for a message np never sent.
    let sentence = "workspace lock held by another process for 30000 ms";
    journal
        .record(&RequestRecord::fault("embed", Path::new("/v"), Duration::from_secs(30), -32603, sentence))
        .expect("record");

    let record: Value =
        serde_json::from_str(&jott_core::diagnostics::last_line(&journal.path()).expect("file").expect("line")).expect("json");
    assert_eq!(record["outcome"], "fault");
    assert_eq!(record["code"], -32603);
    assert_eq!(record["message"], sentence);
}

#[test]
fn a_line_has_no_field_that_could_hold_note_text() {
    let dir = tempfile::tempdir().expect("app data");
    let journal = Journal::open(dir.path());
    journal.record(&RequestRecord::new("search", Path::new("/v"), Duration::from_millis(1), "applied")).expect("record");
    journal
        .record(&RequestRecord::fault("scan", Path::new("/v"), Duration::from_millis(1), -32603, "np's sentence"))
        .expect("record");

    for record in journal.read_lines().expect("read back") {
        let mut keys: Vec<&str> = record.as_object().expect("object").keys().map(String::as_str).collect();
        keys.sort_unstable();
        let mut allowed: Vec<&str> = RequestRecord::KEYS.to_vec();
        allowed.sort_unstable();
        // The fault line may omit `code`/`message`; nothing may add a field. That is the whole guarantee
        // this task makes about privacy, so it is asserted on the shape rather than on discipline.
        assert!(keys.iter().all(|k| allowed.contains(k)), "unexpected key in {record}");
    }
}

#[test]
fn it_rotates_at_two_mebibytes_and_keeps_two_files() {
    let dir = tempfile::tempdir().expect("app data");
    let journal = Journal::open(dir.path());
    // A message long enough that a few hundred records pass the cap, rather than 10 000 of them.
    let long = "x".repeat(8 * 1024);
    let mut written = 0usize;
    while written < jott_core::diagnostics::ROTATE_AT_BYTES as usize + 64 * 1024 {
        journal.record(&RequestRecord::fault("embed", Path::new("/v"), Duration::from_millis(1), -32603, &long)).expect("record");
        written += long.len();
    }

    let logs = std::fs::read_dir(dir.path().join("logs"))
        .expect("logs dir")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .collect::<Vec<_>>();
    assert!(logs.contains(&"jott.jsonl".to_string()), "the current file is still there: {logs:?}");
    assert!(logs.contains(&"jott.jsonl.1".to_string()), "one older copy is kept: {logs:?}");
    assert!(!logs.iter().any(|name| name.ends_with(".2")), "no third copy: {logs:?}");
    assert!(journal.bytes_on_disk() < jott_core::diagnostics::ROTATE_AT_BYTES, "the newest file starts again after a rotation");

    // And the newest record is readable in it, because a log that rotated away what just happened is
    // useless for the question "what did it do a moment ago".
    let line = jott_core::diagnostics::last_line(&journal.path()).expect("current").expect("a line");
    assert!(line.contains("\"outcome\":\"fault\""), "{line}");
}

#[test]
fn an_unwritable_log_is_nobody_else_problem() {
    // A journal that cannot write reports it once and stays out of the way: a save must not fail because
    // the diagnostics folder is read-only (§4 — this is not something a person can act on in the window).
    let blocker = tempfile::tempdir().expect("blocker");
    let occupied = blocker.path().join("taken");
    std::fs::write(&occupied, b"not a folder").expect("file where a folder wants to be");
    let journal = Journal::open(&occupied);
    assert!(journal.broken(), "creating <file>/logs cannot work");
    assert!(journal.record(&RequestRecord::new("scan", Path::new("/v"), Duration::from_millis(1), "applied")).is_err());
    // And asking again does not panic, does not retry forever, and does not take the app with it.
    assert!(journal.record(&RequestRecord::new("scan", Path::new("/v"), Duration::from_millis(1), "applied")).is_err());
}

#[tokio::test]
async fn a_search_through_the_session_writes_exactly_one_line() {
    let _turn = SERIAL.lock().await;
    let app_data = tempfile::tempdir().expect("app data");
    let vault = tempfile::tempdir().expect("vault");
    std::fs::write(vault.path().join("a.md"), "# A\n").expect("note");
    let _scenario = scenario(json!({ "replies": {"search": serde_json::from_str::<Value>(SEARCH_REPLY).unwrap() } }));

    let journal = Arc::new(Journal::open(app_data.path()));
    let session = VaultSession::open_in(fake_np(), info(), vault.path(), Consent::Declined, None, app_data.path().to_path_buf())
        .with_journal(journal.clone());

    session.search("A", 8).await.expect("the reader answers");

    let lines = journal.read_lines().expect("log reads back");
    assert_eq!(lines.len(), 1, "one request, one line: {lines:?}");
    assert_eq!(lines[0]["method"], "search");
    assert_eq!(lines[0]["outcome"], "applied");
    assert_eq!(lines[0]["workspace"], vault.path().display().to_string().replace('\\', "/"));

    session.close().await;
}

#[tokio::test]
async fn a_refused_search_is_recorded_as_a_fault_with_np_words() {
    let _turn = SERIAL.lock().await;
    let app_data = tempfile::tempdir().expect("app data");
    let vault = tempfile::tempdir().expect("vault");
    let journal = Arc::new(Journal::open(app_data.path()));

    // np is not there at all: the log has to say that, because it is the only record of the attempt.
    let session = VaultSession::open_in(
        NpSource::Env(PathBuf::from("/nonexistent/np")),
        info(),
        vault.path(),
        Consent::Undecided,
        None,
        app_data.path().to_path_buf(),
    )
    .with_journal(journal.clone());

    session.search("anything", 8).await.expect_err("no np here");
    let lines = journal.read_lines().expect("log");
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["outcome"], "fault");
    assert!(lines[0]["message"].as_str().unwrap_or_default().contains("/nonexistent/np"), "{:?}", lines[0]);
}

#[tokio::test]
async fn the_ingest_queue_records_what_it_asked_np_for() {
    let _turn = SERIAL.lock().await;
    let app_data = tempfile::tempdir().expect("app data");
    let vault = tempfile::tempdir().expect("vault");
    std::fs::write(vault.path().join("a.md"), "# A\n").expect("note");
    let _scenario = scenario(json!({ "replies": {"scan": serde_json::from_str::<Value>(SCAN_REPORT).unwrap()} }));

    let journal = Arc::new(Journal::open(app_data.path()));
    let session = VaultSession::open_in(fake_np(), info(), vault.path(), Consent::Declined, None, app_data.path().to_path_buf())
        .with_journal(journal.clone());

    session.save_note("a.md", "# A\n").expect("save");
    session.ingest_all();

    // The queue's own pass goes through the same recorder: a hand-off that asks "was a scan even sent?"
    // gets answered from the file, not from a guess about the code.
    let mut methods = Vec::new();
    for _ in 0..20 {
        methods = journal.read_lines().expect("log").into_iter().filter_map(|l| l["method"].as_str().map(str::to_string)).collect();
        if methods.iter().any(|m| m == "scan") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(methods.contains(&"scan".to_string()), "the scan reached the log: {methods:?}");

    session.close().await;
}
