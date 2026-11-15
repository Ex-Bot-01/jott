//! The log the Windows hand-off is asked to send back (Task 11): what Jott asked np, how long it took, and
//! what came back — never what the notes say.

mod common;

use common::repo_root;
use jott_core::diagnostics::{now_ms, Journal, LocalRecord, RequestRecord, RunRecord};
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
fn it_rotates_at_two_mebibytes_and_keeps_two_older_copies() {
    let dir = tempfile::tempdir().expect("app data");
    let journal = Journal::open(dir.path());
    // A message long enough that a few hundred records pass the cap, rather than 10 000 of them.
    let long = "x".repeat(8 * 1024);
    let mut written = 0usize;
    while written < jott_core::diagnostics::ROTATE_AT_BYTES as usize + 64 * 1024 {
        journal.record(&RequestRecord::fault("embed", Path::new("/v"), Duration::from_millis(1), -32603, &long)).expect("record");
        written += long.len();
    }

    // A second rotation: "keeps two older copies" means `.1` moves to `.2` and the older `.2` goes, which
    // only happens on the second crossing. Counted in bytes written rather than in the size of the live
    // file, since that is what a rotation resets (`bytes_on_disk` would never reach the cap again).
    let mut second_pass = 0usize;
    while second_pass < jott_core::diagnostics::ROTATE_AT_BYTES as usize + 64 * 1024 {
        journal.record(&RequestRecord::fault("embed", Path::new("/v"), Duration::from_millis(1), -32603, &long)).expect("record");
        second_pass += long.len();
    }

    let logs = std::fs::read_dir(dir.path().join("logs"))
        .expect("logs dir")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .collect::<Vec<_>>();
    assert!(logs.contains(&"jott.jsonl".to_string()), "the current file is still there: {logs:?}");
    assert!(logs.contains(&"jott.jsonl.1".to_string()), "one older copy is kept: {logs:?}");
    assert!(logs.contains(&"jott.jsonl.2".to_string()), "the copy before that survives one rotation: {logs:?}");
    assert!(!logs.iter().any(|name| name.ends_with(".3")), "and nothing further back: {logs:?}");
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

    // Asked with word-beginning matching on purpose: whether np widened a search is a fact about one **answer**, and
    // the journal's key set stays closed (§18.13). A widened request logs the same shape as any other.
    session.search("A", 8, true).await.expect("the reader answers");

    let lines = journal.read_lines().expect("log reads back");
    assert_eq!(lines.len(), 1, "one request, one line: {lines:?}");
    assert_eq!(lines[0]["method"], "search");
    assert_eq!(lines[0]["outcome"], "applied");
    assert_eq!(lines[0]["workspace"], vault.path().display().to_string().replace('\\', "/"));
    let keys: Vec<&str> = lines[0].as_object().expect("a line is an object").keys().map(String::as_str).collect();
    for forbidden in ["prefix", "prefix_used", "prefix_terms", "matched_exactly", "query"] {
        assert!(!keys.contains(&forbidden), "the search line grew a field ({forbidden}): {keys:?}");
    }

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

    session.search("anything", 8, false).await.expect_err("no np here");
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

// ---- the trace: this run's backend activity, kept in memory for the window to show ----

use jott_core::diagnostics::{Trace, TraceEvent, TraceKind};

fn request(method: &str, at_ms: u64) -> TraceEvent {
    TraceEvent {
        at_ms,
        kind: TraceKind::Request,
        method: Some(method.to_string()),
        outcome: Some("applied"),
        elapsed_ms: Some(40),
        detail: None,
    }
}

/// The drawer is opened on a run that has already been going for ten minutes, and the interesting part is what
/// happened last. Oldest first, because the window shows newest at the top and should not have to know that.
#[test]
fn the_ring_keeps_the_most_recent_events_oldest_first() {
    let trace = Trace::new(3);
    for at in 1..=5 {
        trace.push(request("scan", at));
    }
    let kept: Vec<u64> = trace.recent().into_iter().map(|e| e.at_ms).collect();
    assert_eq!(kept, vec![3, 4, 5], "three kept, the newest three");
}

/// A window that opens late still gets the events after it subscribed, and a window that stopped reading (a
/// closed drawer, a hung webview) must not be able to break the run.
#[tokio::test]
async fn subscribers_see_what_comes_next_and_lagging_is_harmless() {
    let trace = Arc::new(Trace::new(4));
    let mut rx = trace.subscribe();

    trace.push(request("embed", 10));
    let got = tokio::time::timeout(Duration::from_millis(500), rx.recv())
        .await
        .expect("the event arrives")
        .expect("the channel is open");
    assert_eq!(got.method.as_deref(), Some("embed"));

    // Nobody reading while the ring wraps around twice: still no panic, still no block.
    for at in 0..12 {
        trace.push(request("scan", at));
    }
    assert_eq!(trace.recent().len(), 4);
}

/// The same rule the journal lives by, applied to what crosses into the webview: a closed set of keys, and none
/// of them a place a note's text could go. This is the test that fails when someone adds `snippet` "just for
/// debugging" three months from now — which is exactly when nobody is thinking about it.
#[test]
fn the_trace_carries_fields_and_never_contents() {
    let filled = TraceEvent {
        at_ms: 1,
        kind: TraceKind::Progress,
        method: Some("embed".into()),
        outcome: Some("applied"),
        elapsed_ms: Some(2),
        detail: Some("chunks 7/7".into()),
    };
    // Compared sorted: `serde_json` keeps an object's keys in its own order, and what is being pinned here is
    // *which* keys exist, not the order they happen to arrive in.
    let mut keys: Vec<String> = serde_json::to_value(&filled)
        .expect("an event serializes")
        .as_object()
        .expect("an object")
        .keys()
        .cloned()
        .collect();
    keys.sort();
    let mut expected: Vec<String> = TraceEvent::KEYS.iter().map(|key| key.to_string()).collect();
    expected.sort();
    assert_eq!(keys, expected);

    for forbidden in ["text", "snippet", "body", "contents", "query", "path"] {
        assert!(
            !TraceEvent::KEYS.contains(&forbidden),
            "`{forbidden}` would put someone's note text on a screen that gets screenshotted"
        );
    }
}

/// `kind` is how the window groups rows. Lowercase, so the frontend can switch on it without learning Rust's
/// spelling of a variant.
#[test]
fn kinds_arrive_lowercase() {
    for (kind, word) in [
        (TraceKind::Request, "request"),
        (TraceKind::Progress, "progress"),
        (TraceKind::Connection, "connection"),
        (TraceKind::Consent, "consent"),
        (TraceKind::Queue, "queue"),
    ] {
        assert_eq!(serde_json::to_value(kind).unwrap(), json!(word));
    }
}

// ---- B8: the run boundary, and B9: the file work around np ----

/// A log that spans an upgrade cannot be read as two runs without a line saying where one run started, on which
/// build, against which np binary. "It worked yesterday" is answerable only if yesterday's line names yesterday's
/// `np.exe`.
#[test]
fn a_run_line_says_which_build_ran_against_which_np() {
    let dir = tempfile::tempdir().expect("app data");
    let journal = Journal::open(dir.path());
    let run = RunRecord {
        jott: "0.0.12".into(),
        np: Some("0.3.1".into()),
        protocol: Some(1),
        np_path: Some("D:/Jott/np/np.exe".into()),
        np_source: "bundled".into(),
        platform: "windows-x86_64".into(),
        state_root: "C:/Users/chris/AppData/Roaming/jott".into(),
        state_root_is_fallback: false,
        at_ms: now_ms(),
    };
    journal.record_run(&run).expect("the run line is written");

    let lines = journal.read_lines().unwrap();
    assert_eq!(lines.len(), 1);
    // Sorted on both sides: `serde_json` hands object keys back in order, and the contract is the *set* of keys.
    let mut keys: Vec<&str> = lines[0].as_object().unwrap().keys().map(String::as_str).collect();
    keys.sort_unstable();
    let mut expected = RunRecord::KEYS.to_vec();
    expected.sort_unstable();
    assert_eq!(keys, expected);
    assert_eq!(lines[0]["np_source"], "bundled");
    // Forward slashes in the file even for a Windows path: one rule for every path Jott writes down.
    assert_eq!(lines[0]["np_path"], "D:/Jott/np/np.exe");
}

/// The question a tester asks that no np request can answer: "my note vanished". Saves, deletions, replays and
/// refusals are the events, and `rel` is allowed here precisely because *which file* is the question. What is not
/// allowed is the file's contents — the same rule as everywhere else, stated next to the key list.
#[test]
fn a_local_line_names_the_file_and_never_its_contents() {
    let dir = tempfile::tempdir().expect("app data");
    let journal = Journal::open(dir.path());
    journal
        .record_local(&LocalRecord::new("save").with_rel("notes/a.md").with_bytes(4))
        .expect("the line is written");
    journal
        .record_local(&LocalRecord::new("deleted").with_detail("moved to Jott's own folder, refused: no backend"))
        .expect("the second line too");

    let lines = journal.read_lines().unwrap();
    assert_eq!(lines.len(), 2);
    for line in &lines {
        let keys: Vec<&str> = line.as_object().unwrap().keys().map(String::as_str).collect();
        for key in &keys {
            assert!(LocalRecord::KEYS.contains(key), "{key} is not in the local line's contract: {line}");
        }
        assert!(
            keys.contains(&"event") && keys.contains(&"at_ms"),
            "the two keys that are never optional: {line}"
        );
    }
    assert_eq!(lines[0]["event"], "save");
    assert_eq!(lines[0]["rel"], "notes/a.md");
    assert_eq!(lines[0]["bytes"], 4);
    assert!(lines[0].get("detail").is_none(), "an absent detail is not written as null");
    assert_eq!(lines[1]["detail"].as_str().unwrap(), "moved to Jott's own folder, refused: no backend");

    // The privacy rule, stated as a refusal rather than a hope.
    for forbidden in ["text", "snippet", "body", "contents", "query"] {
        assert!(!LocalRecord::KEYS.contains(&forbidden), "{forbidden} must never become a key");
    }
}

/// Both shapes land in the same file, and rotation counts bytes across them: the log is one artifact a person
/// attaches, not three.
#[test]
fn the_shapes_share_one_file_and_one_rotation() {
    let dir = tempfile::tempdir().expect("app data");
    let journal = Journal::open(dir.path());
    journal.record_run(&RunRecord::for_test("0.0.12")).expect("run");
    journal.record(&RequestRecord::new("scan", Path::new("/v"), Duration::from_millis(5), "applied")).unwrap();
    journal.record_local(&LocalRecord::new("index-now")).unwrap();

    let lines = journal.read_lines().unwrap();
    assert_eq!(lines.len(), 3);
    assert!(lines[0].get("jott").is_some(), "the run line came first");
    assert!(lines[1].get("method").is_some(), "then the request");
    assert!(lines[2].get("event").is_some(), "then the local event");
}

/// Answering the consent question writes a key into np's own config, so the log says which key and which file —
/// "Jott touched my configuration" is exactly the thing a person should be able to check afterwards. Keys and a
/// destination only: values are quoted as `true`/`false` for that one key, never anything a note contains.
#[tokio::test]
async fn answering_consent_records_the_key_and_where_it_went() {
    let _turn = SERIAL.lock().await;
    let app_data = tempfile::tempdir().expect("app data");
    let vault = tempfile::tempdir().expect("vault");
    std::fs::write(vault.path().join("a.md"), b"# A\n").expect("note");
    let np_config = app_data.path().join("config.toml");

    // Answering consent now talks to np over the pipe, so this test needs a stand-in of its own: `FAKE_NP_SCENARIO`
    // is process-global and another test in this binary may well have left it pointing at a tempdir that no longer
    // exists. Inheriting that was a crash with a Python traceback, which is a fair punishment but a poor fixture.
    let _scenario = scenario(json!({"replies": {"embeddings.enable": {
        "changed": ["embeddings.provider", "embeddings.model"],
        "fetch": {"bytes_total": 309_855_456, "members": ["main"]},
        "model": "embeddinggemma-2",
        "persisted_to": "/home/you/.config/note-processor/config.toml",
        "provider": "local-model"
    }}}));

    let journal = Arc::new(Journal::open(app_data.path()));
    let session = VaultSession::open_in(
        fake_np(),
        info(),
        vault.path(),
        Consent::Undecided,
        Some(np_config.clone()),
        app_data.path().to_path_buf(),
    )
    .with_journal(journal.clone());

    session.answer_consent(true).await;

    let lines = journal.read_lines().expect("log reads back");
    let consent = lines
        .iter()
        .find(|line| line["event"] == "consent")
        .unwrap_or_else(|| panic!("a consent answer is file work worth logging: {lines:?}"));
    let detail = consent["detail"].as_str().unwrap_or_default();
    assert!(detail.contains("granted"), "what was answered: {detail}");
    assert!(detail.contains("models.auto_fetch"), "which key Jott writes: {detail}");
    assert!(detail.contains("config.toml"), "and which file it goes to: {detail}");
    // The line now says what *np* moved, not just what Jott did: after note-processor#7 the two are different facts,
    // and an audit of somebody's machine should show both.
    assert!(detail.contains("embeddings.provider"), "what np changed itself: {detail}");

    let raw = std::fs::read_to_string(app_data.path().join("logs").join("jott.jsonl")).expect("log file");
    // "kumquat" and "# A" are the note's own words; the model id is the other thing this line must not carry — a
    // settings decision is auditable from the key and the file, and the plan asked for exactly this guard.
    for forbidden in ["kumquat", "# A", "embeddinggemma"] {
        assert!(!raw.contains(forbidden), "note text reached the log: {forbidden}");
    }

    session.close().await;
}

/// A refusal is the case the log gets wrong easiest: np's words name model ids, and "the button said no" is not the
/// same fact as "the file was written". So the file carries keys, the method called, and what happened to Jott's own
/// key — and np's sentence goes only to the drawer row, where a person is looking at it right now.
#[tokio::test]
async fn a_refused_answer_logs_the_method_and_our_key_without_nps_words() {
    let _turn = SERIAL.lock().await;
    let app_data = tempfile::tempdir().expect("app data");
    let vault = tempfile::tempdir().expect("vault");
    std::fs::write(vault.path().join("a.md"), b"# A\n").expect("note");
    let np_config = app_data.path().join("config.toml");
    std::fs::write(&np_config, "[models]\nauto_fetch = false\n").expect("np config");

    // np refuses a model id this build cannot use — and names the ones it can, which is exactly what must not end up
    // in a file somebody else reads years later.
    let _scenario = scenario(json!({
        "faults": {"embeddings.enable": {
            "code": -32000,
            "message": "config io: no pinned model \"bge-m3\": this build can name \"embeddinggemma-2\""
        }}
    }));

    let journal = Arc::new(Journal::open(app_data.path()));
    let session = VaultSession::open_in(
        fake_np(),
        info(),
        vault.path(),
        Consent::Undecided,
        Some(np_config.clone()),
        app_data.path().to_path_buf(),
    )
    .with_journal(journal.clone());

    session.answer_consent(true).await;

    let lines = journal.read_lines().expect("log reads back");
    let consent = lines
        .iter()
        .find(|line| line["event"] == "consent")
        .unwrap_or_else(|| panic!("a consent answer is file work worth logging: {lines:?}"));
    let detail = consent["detail"].as_str().unwrap_or_default();

    // Which door was knocked on, and what became of our key.
    assert!(detail.contains("embeddings.enable"), "an audit needs to know which method ran: {detail}");
    assert!(detail.contains("models.auto_fetch left as it was"), "the guardrail did not move: {detail}");
    // And not what np said: its sentence names model ids, and the card quotes it on screen instead.
    let raw = std::fs::read_to_string(app_data.path().join("logs").join("jott.jsonl")).expect("log file");
    for forbidden in ["embeddinggemma", "bge-m3", "no pinned model"] {
        assert!(!raw.contains(forbidden), "np\'s sentence reached the log file: {forbidden}");
    }
    // A refusal must never be logged as a write, whatever the button was.
    assert!(!detail.contains("writes models.auto_fetch"), "{detail}");

    // The drawer row does carry np\'s words — byte for byte, quoted, because that is where they belong (v0.0.11).
    let rows = session.trace().recent();
    let row = rows
        .iter()
        .rev()
        .find(|row| row.detail.as_deref().is_some_and(|d| d.contains("consent")))
        .expect("the drawer saw the answer too");
    assert!(
        row.detail.as_deref().unwrap_or_default().contains("no pinned model \"bge-m3\""),
        "np\'s own words belong in the row: {:?}",
        row.detail
    );
    session.close().await;
}

/// A decline whose write *failed* must not log a write. This is the state where np will not touch its unparsable file
/// and the OS will not let us write ours.
#[tokio::test]
async fn a_decline_whose_write_failed_does_not_log_a_write() {
    let _turn = SERIAL.lock().await;
    let app_data = tempfile::tempdir().expect("app data");
    let vault = tempfile::tempdir().expect("vault");
    std::fs::write(vault.path().join("a.md"), b"# A\n").expect("note");
    // A regular file where the config's parent directory should be: no answer gets written anywhere.
    std::fs::write(app_data.path().join("blocked"), b"not a directory").expect("blocker");
    let np_config = app_data.path().join("blocked/config.toml");

    let _scenario = scenario(json!({
        "faults": {"embeddings.disable": {"code": -32000, "message": "config io: cannot write the config file"}}
    }));

    let journal = Arc::new(Journal::open(app_data.path()));
    let session = VaultSession::open_in(
        fake_np(),
        info(),
        vault.path(),
        Consent::Undecided,
        Some(np_config),
        app_data.path().to_path_buf(),
    )
    .with_journal(journal.clone());

    session.answer_consent(false).await;

    let lines = journal.read_lines().expect("log reads back");
    let detail = lines
        .iter()
        .find(|line| line["event"] == "consent")
        .and_then(|line| line["detail"].as_str())
        .expect("a consent row");
    assert!(detail.contains("embeddings.disable"), "{detail}");
    assert!(detail.contains("could not write models.auto_fetch"), "say the failure, not the intent: {detail}");
    assert!(!detail.contains("wrote models.auto_fetch = false"), "{detail}");
    session.close().await;
}
