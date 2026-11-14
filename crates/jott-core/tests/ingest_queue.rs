//! The ingest queue (§11): a save changes bytes, so the vault is indexed and then embedded, in that
//! order, batched per vault because np takes a workspace and nothing else (§18.2). Every state np can
//! answer — `locked`, `offline`, a configuration refusal — requeues or stops short; none of them makes
//! the app claim work that did not happen.

mod common;

use common::{fixture_result, repo_root};
use jott_core::hashes::sha256_file;
use jott_core::ingest::consent::auto_fetch_allowed;
use jott_core::ingest::queue::{backoff_ms, IngestEvent, IngestQueue, StateReason};
use jott_core::np::pool::NpPool;
use jott_core::np::{NpFeatures, NpInfo, NpSource};
use jott_core::vault::stamps::{StampState, Stamps};
use jott_core::vault::store::Consent;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tempfile::TempDir;

/// Every test configures the stand-in through one process-global variable and counts np calls, so
/// they take turns rather than reading each other's recordings.
static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn info() -> NpInfo {
    NpInfo {
        version: "0.3.1".into(),
        protocol: 1,
        methods: ["initialize", "search", "scan", "embed", "models.list", "status.get", "shutdown"]
            .map(str::to_string)
            .to_vec(),
        features: NpFeatures { local_model: true, fetch: true, vision: false },
        data_dir: "/fake/data".into(),
        progress: true,
    }
}

struct Harness {
    // Held so the scenario file, the app-data folder and np's config all outlive the queue.
    _dir: TempDir,
    vault: PathBuf,
    pool: Arc<NpPool>,
    stamps: Arc<Mutex<Stamps>>,
    record: PathBuf,
    np_config: PathBuf,
    appdata: PathBuf,
}

impl Harness {
    /// The caller holds `SERIAL`: setting a process-wide variable is only legitimate one test at a
    /// time.
    async fn new(scenario: Value) -> Harness {
        let dir = tempfile::tempdir().expect("temp dir");
        let vault = dir.path().join("vault");
        std::fs::create_dir_all(vault.join("notes")).unwrap();
        std::fs::write(vault.join("notes/a.md"), "# A\n").unwrap();
        let record = dir.path().join("calls.ndjson");
        std::fs::write(&record, "").unwrap();

        let mut scenario = scenario;
        scenario["record"] = json!(record.display().to_string());
        // The default machine this file tests on is one where the model is already installed and np's config
        // names it, so a test about embedding does not have to stage a consent dialog first. That is a
        // *default*, not a premise: the tests about asking overwrite both the config and this reply, and a
        // fresh np — no config, nothing on disk, a 310 MB pin — has its own test.
        let replies = scenario["replies"].as_object();
        if replies.is_some_and(|replies| !replies.contains_key("models.list")) {
            let mut listing = models_list_missing();
            listing["models"][0]["state"] = json!("present");
            scenario["replies"]["models.list"] = listing;
        }
        let path = dir.path().join("scenario.json");
        std::fs::write(&path, scenario.to_string()).expect("write scenario");
        std::env::set_var("FAKE_NP_SCENARIO", &path);

        let script = repo_root().join("tests/fake-np/np.py");
        let pool = Arc::new(NpPool::new(NpSource::Env(script), info()));
        let appdata = dir.path().join("appdata");
        let np_config = dir.path().join("np-config.toml");
        std::fs::write(&np_config, config_naming("bge-m3")).expect("default np config");
        let stamps = Arc::new(Mutex::new(Stamps::with_root(appdata.clone(), &vault)));
        Harness { _dir: dir, vault, pool, stamps, record, np_config, appdata }
    }

    /// A queue with the production debounce and drain cap shortened, so a sixteen-test file does not
    /// spend two minutes waiting on timers it is not testing. Both real numbers have their own test.
    fn queue(&self) -> IngestQueue {
        IngestQueue::spawn(self.pool.clone(), self.stamps.clone(), self.vault.clone())
            .with_debounce(Duration::from_millis(80))
            .with_drain_cap(Duration::from_millis(600))
            .with_np_config(Some(self.np_config.clone()))
    }

    /// What np was asked for, in order, without the handshake.
    fn calls(&self) -> Vec<String> {
        let Ok(text) = std::fs::read_to_string(&self.record) else { return Vec::new() };
        text.lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .filter_map(|line| line["method"].as_str().map(str::to_string))
            .filter(|m| m != "initialize" && m != "shutdown")
            .collect()
    }

    fn state(&self, rel: &str) -> StampState {
        let hash = sha256_file(&self.vault.join(rel)).expect("the test wrote this note");
        self.stamps.lock().unwrap().state_of(rel, &hash)
    }

    /// What the stamps look like after a restart: same app-data root, nothing in memory.
    fn reloaded(&self) -> Stamps {
        Stamps::with_root(self.appdata.clone(), &self.vault)
    }
}

/// Everything the queue says for `wait`, from a receiver the test subscribed before it started work —
/// a broadcast channel holds them, so nothing needs to be read in step.
async fn collect(rx: &mut tokio::sync::broadcast::Receiver<IngestEvent>, wait: Duration) -> Vec<IngestEvent> {
    let deadline = Instant::now() + wait;
    let mut out = Vec::new();
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return out;
        }
        match tokio::time::timeout(left, rx.recv()).await {
            Ok(Ok(event)) => out.push(event),
            // A lagged receiver means this test is not reading fast enough; what it cares about is
            // already in `out`.
            Ok(_) => continue,
            Err(_) => return out,
        }
    }
}

/// np's measured embed report (`id 4` in the transcript) with the outcome a run that finished reports:
/// as captured it answers `configuration-disabled`, because that is what that vault's config said.
fn embed_current() -> Value {
    let mut report = fixture_result(4);
    report["outcome"] = json!("current");
    report
}

fn scan_and_embed() -> Value {
    json!({ "replies": { "scan": fixture_result(2), "embed": embed_current() } })
}

fn behind(events: &[IngestEvent]) -> Option<(StateReason, u64)> {
    events.iter().find_map(|e| match e {
        IngestEvent::Behind { reason, retry_in_ms } => Some((*reason, *retry_in_ms)),
        _ => None,
    })
}

// ---- batching (spec §18.2: np takes a workspace, so a batch is the only shape there is)

#[tokio::test]
async fn one_save_becomes_one_scan_then_one_embed() {
    let _turn = SERIAL.lock().await;
    let h = Harness::new(scan_and_embed()).await;
    let queue = h.queue();
    let mut rx = queue.events();

    queue.note_saved("notes/a.md");
    let events = collect(&mut rx, Duration::from_millis(900)).await;

    assert_eq!(h.calls(), vec!["scan".to_string(), "models.list".to_string(), "embed".to_string()], "index first: embed reads the index (§18.2)");
    assert!(events.iter().any(|e| matches!(e, IngestEvent::Indexed(_))), "the tree needs to hear about the scan: {events:?}");
    assert!(events.iter().any(|e| matches!(e, IngestEvent::Embedded(_))), "and about the embed: {events:?}");
}

#[tokio::test]
async fn np_counter_lines_reach_the_status_line_as_they_came() {
    let _turn = SERIAL.lock().await;
    let h = Harness::new(json!({
        "replies": {"scan": fixture_result(2), "embed": embed_current()},
        "progress_counters": {"embed": [{"phase": "embedding", "chunks_done": 3, "chunks_total": 7}]}
    }))
    .await;
    let queue = h.queue();
    let mut rx = queue.events();

    queue.note_saved("notes/a.md");
    let events = collect(&mut rx, Duration::from_millis(900)).await;

    assert!(
        events.contains(&IngestEvent::Working { phase: "embedding".into(), done: 3, total: Some(7) }),
        "\"embedding 3/7 chunks\" (§11) is np's numbers, passed through: {events:?}"
    );
}

#[tokio::test]
async fn a_phase_without_counters_is_not_reported_as_zero_percent() {
    let _turn = SERIAL.lock().await;
    let h = Harness::new(json!({
        "replies": {"scan": fixture_result(2), "embed": embed_current()},
        "progress_counters": {"scan": [{"phase": "indexing"}]}
    }))
    .await;
    let queue = h.queue();
    let mut rx = queue.events();

    queue.note_saved("notes/a.md");
    let events = collect(&mut rx, Duration::from_millis(900)).await;

    assert!(
        events.contains(&IngestEvent::Working { phase: "indexing".into(), done: 0, total: None }),
        "no counters means \"unknown\", and `total: None` is what the status line renders as a label alone (§18.12): {events:?}"
    );
}

#[tokio::test]
async fn ten_saves_in_the_window_are_one_pair() {
    let _turn = SERIAL.lock().await;
    let h = Harness::new(scan_and_embed()).await;
    let queue = h.queue();

    for n in 0..10 {
        let rel = format!("notes/{n}.md");
        std::fs::write(h.vault.join(&rel), format!("# Note {n}\n")).unwrap();
        queue.note_saved(&rel);
    }
    tokio::time::sleep(Duration::from_millis(700)).await;

    assert_eq!(
        h.calls(),
        vec!["scan".to_string(), "models.list".to_string(), "embed".to_string()],
        "ten saves, one owed pass — np has no per-note call to make (§18.2)"
    );
}

#[tokio::test]
async fn flush_now_does_not_wait_for_the_debounce() {
    let _turn = SERIAL.lock().await;
    let h = Harness::new(json!({
        "replies": {"scan": fixture_result(2), "embed": embed_current()},
        "sleep_ms": {"scan": 150, "embed": 150}
    }))
    .await;
    // The production debounce, so "now" is measured against the timer it skips.
    let queue = IngestQueue::spawn(h.pool.clone(), h.stamps.clone(), h.vault.clone())
        .with_np_config(Some(h.np_config.clone()))
        .with_drain_cap(Duration::from_millis(600));

    queue.note_saved("notes/a.md");
    let started = Instant::now();
    queue.flush_now();
    while h.calls().len() < 2 && started.elapsed() < Duration::from_secs(3) {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    assert_eq!(h.calls(), vec!["scan".to_string(), "models.list".to_string(), "embed".to_string()]);
    assert!(started.elapsed() < Duration::from_millis(900), "flush means now: {:?}", started.elapsed());
}

#[tokio::test]
async fn the_timers_are_the_ones_the_plan_asks_for() {
    assert_eq!(IngestQueue::DEBOUNCE, Duration::from_secs(1), "§11: debounce ~1 s after typing stops");
    assert_eq!(IngestQueue::DRAIN_CAP, Duration::from_secs(5), "§11: closing never blocks on embedding");
}

// ---- not calling np for nothing (§11: a write that changed nothing triggers nothing)

#[tokio::test]
async fn a_save_that_changed_no_bytes_calls_np_at_all() {
    let _turn = SERIAL.lock().await;
    let h = Harness::new(scan_and_embed()).await;
    let hash = sha256_file(&h.vault.join("notes/a.md")).unwrap();
    h.stamps.lock().unwrap().mark_embedded("notes/a.md", &hash);

    let queue = h.queue();
    queue.note_saved("notes/a.md");
    tokio::time::sleep(Duration::from_millis(500)).await;

    assert!(h.calls().is_empty(), "Ctrl+S on an unmodified buffer asked np anyway: {:?}", h.calls());
}

#[tokio::test]
async fn a_saved_note_that_was_only_indexed_goes_back_for_embedding() {
    let _turn = SERIAL.lock().await;
    let h = Harness::new(scan_and_embed()).await;
    let hash = sha256_file(&h.vault.join("notes/a.md")).unwrap();
    // Indexed but never embedded: the bytes are unchanged, and the work is still owed.
    h.stamps.lock().unwrap().mark_indexed("notes/a.md", &hash);

    let queue = h.queue();
    queue.note_saved("notes/a.md");
    tokio::time::sleep(Duration::from_millis(600)).await;

    assert_eq!(h.calls(), vec!["scan".to_string(), "models.list".to_string(), "embed".to_string()], "the stamp says owed, so np is asked");
}

// ---- states are states (§11: locked and offline requeue; a refusal quotes np)

#[tokio::test]
async fn a_locked_scan_says_behind_and_retries() {
    let _turn = SERIAL.lock().await;
    let h = Harness::new(json!({"replies": {"scan": {"command": "scan", "locked": true, "waited_ms": 5000}}}))
    .await;
    let queue = h.queue().with_backoff_floor(Duration::from_millis(300));
    let mut rx = queue.events();

    queue.note_saved("notes/a.md");
    let events = collect(&mut rx, Duration::from_millis(1_200)).await;

    let (reason, retry_in) = behind(&events).unwrap_or_else(|| panic!("a locked scan must arrive as Behind: {events:?}"));
    assert_eq!(reason, StateReason::Locked { waited_ms: 5000 });
    assert_eq!(retry_in, backoff_ms(0), "the first retry is the first rung");
    assert_eq!(h.state("notes/a.md"), StampState::Waiting, "nothing was indexed, so nothing is claimed");
    assert!(
        h.calls().iter().filter(|m| m.as_str() == "scan").count() >= 2,
        "the retry is a real second request: {:?}",
        h.calls()
    );
}

#[test]
fn the_backoff_ladder_is_the_plans_and_it_stops_rising() {
    assert_eq!(backoff_ms(0), 2_000);
    assert_eq!(backoff_ms(1), 5_000);
    assert_eq!(backoff_ms(2), 15_000);
    assert_eq!(backoff_ms(3), 60_000);
    assert_eq!(backoff_ms(40), 60_000, "capped: a vault will not be rescanned every two seconds for an hour");
}

#[tokio::test]
async fn an_offline_embed_leaves_the_note_indexed_not_embedded() {
    let _turn = SERIAL.lock().await;
    let h = Harness::new(json!({
        "replies": {
            "scan": fixture_result(2),
            "embed": {"outcome": {"offline": {"id": "bge-m3"}}},
        }
    }))
    .await;
    let queue = h.queue();
    let mut rx = queue.events();

    queue.note_saved("notes/a.md");
    let events = collect(&mut rx, Duration::from_millis(900)).await;

    assert!(
        matches!(behind(&events), Some((StateReason::Offline, _))),
        "offline is a state the status line shows: {events:?}"
    );
    assert_eq!(h.state("notes/a.md"), StampState::Indexed, "indexed is true; embedded would be a lie");
}

#[tokio::test]
async fn a_refusal_carries_nps_sentence_unchanged() {
    let _turn = SERIAL.lock().await;
    let sentence = "workspace `/fake/vault` is mid-migration: run `np migrate` first";
    let h = Harness::new(json!({
        "replies": {"scan": fixture_result(2)},
        "faults": {"embed": {"code": -32000, "message": sentence}}
    }))
    .await;
    let queue = h.queue();
    let mut rx = queue.events();

    queue.note_saved("notes/a.md");
    let events = collect(&mut rx, Duration::from_millis(900)).await;

    let message = events
        .iter()
        .find_map(|e| match e {
            IngestEvent::Refused { message } => Some(message.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("a -32000 must arrive as Refused: {events:?}"));
    assert_eq!(message, sentence, "np's words, not ours (§11)");
    assert_eq!(h.state("notes/a.md"), StampState::Indexed, "the scan did happen; the embed did not");
}

#[tokio::test]
async fn an_embed_that_reported_errors_does_not_claim_vectors() {
    let _turn = SERIAL.lock().await;
    let mut report = fixture_result(4);
    report["errors"] = json!(3);
    let h = Harness::new(json!({"replies": {"scan": fixture_result(2), "embed": report}}))
    .await;
    let queue = h.queue();

    queue.note_saved("notes/a.md");
    tokio::time::sleep(Duration::from_millis(700)).await;

    assert_eq!(
        h.state("notes/a.md"),
        StampState::Indexed,
        "np's aggregate says some chunks failed and does not say which — so this note is not promised vectors"
    );
}

// ---- consent (§11: asked once per vault, decided from `models.list`, answered in np's config file)

fn models_list_missing() -> Value {
    json!({
        "models": [{
            "id": "bge-m3", "dim": 1024, "sha256": null, "prefix_doc": null, "prefix_query": null,
            "installed_at": 0, "state": "missing", "members": []
        }],
        "fetching": [], "unclaimed": [], "models_dir_error": null
    })
}

fn config_naming(model: &str) -> String {
    format!("[embeddings]\nprovider = \"local\"\nmodel = \"{model}\"\n")
}

#[tokio::test]
async fn no_artifact_and_no_decision_asks_once_and_never_embeds() {
    let _turn = SERIAL.lock().await;
    let h = Harness::new(json!({"replies": {"scan": fixture_result(2), "models.list": models_list_missing()}}))
    .await;
    std::fs::write(&h.np_config, config_naming("bge-m3")).unwrap();
    let queue = h.queue();
    let mut rx = queue.events();

    queue.note_saved("notes/a.md");
    tokio::time::sleep(Duration::from_millis(500)).await;
    queue.note_saved("notes/a.md");
    let events = collect(&mut rx, Duration::from_millis(700)).await;

    let asks: Vec<&IngestEvent> = events.iter().filter(|e| matches!(e, IngestEvent::NeedsConsent { .. })).collect();
    assert_eq!(asks.len(), 1, "once per vault per session, not once per save: {events:?}");
    match asks[0] {
        IngestEvent::NeedsConsent { model_id, bytes, data_dir } => {
            assert_eq!(model_id.as_deref(), Some("bge-m3"), "the model np's config names, not one we guessed");
            assert_eq!(data_dir, &PathBuf::from("/fake/data"), "the folder the prompt quotes");
            assert!(bytes.is_none(), "np 0.3.1 sends no artifact size; Jott invents none (§4)");
        }
        _ => unreachable!(),
    }
    let calls = h.calls();
    assert!(calls.contains(&"scan".to_string()), "indexing is not gated: {calls:?}");
    assert!(!calls.contains(&"embed".to_string()), "no download without an answer: {calls:?}");
    assert_eq!(h.state("notes/a.md"), StampState::Indexed, "indexed is true even so");
}

#[tokio::test]
async fn declining_writes_the_config_and_keeps_indexing() {
    let _turn = SERIAL.lock().await;
    let h = Harness::new(json!({"replies": {"scan": fixture_result(2), "models.list": models_list_missing()}}))
    .await;
    std::fs::write(&h.np_config, config_naming("bge-m3")).unwrap();
    let queue = h.queue();
    queue.set_consent(Consent::Declined);

    queue.note_saved("notes/a.md");
    tokio::time::sleep(Duration::from_millis(700)).await;

    let calls = h.calls();
    assert_eq!(calls, vec!["scan".to_string()], "keyword search keeps working fully (§11)");
    assert!(!auto_fetch_allowed(&h.np_config), "the answer goes in np's file, not in env (§11)");
    assert_eq!(h.state("notes/a.md"), StampState::Indexed);
}

#[tokio::test]
async fn granting_writes_the_config_then_embeds() {
    let _turn = SERIAL.lock().await;
    let h = Harness::new(json!({
        "replies": {"scan": fixture_result(2), "embed": embed_current(), "models.list": models_list_missing()}
    }))
    .await;
    std::fs::write(&h.np_config, config_naming("bge-m3")).unwrap();
    let queue = h.queue();
    queue.set_consent(Consent::Granted);

    queue.note_saved("notes/a.md");
    tokio::time::sleep(Duration::from_millis(700)).await;

    assert_eq!(
        h.calls(),
        vec!["scan".to_string(), "embed".to_string()],
        "a decision already made is not asked about again: no `models.list` on this path"
    );
    assert!(auto_fetch_allowed(&h.np_config), "[Download] means the key is written (§11)");
    assert_eq!(h.state("notes/a.md"), StampState::Embedded, "this time the vectors are real");
}

#[tokio::test]
async fn an_answer_arriving_later_runs_the_work_it_was_waiting_for() {
    let _turn = SERIAL.lock().await;
    let h = Harness::new(json!({
        "replies": {"scan": fixture_result(2), "embed": embed_current(), "models.list": models_list_missing()}
    }))
    .await;
    std::fs::write(&h.np_config, config_naming("bge-m3")).unwrap();
    let queue = h.queue();

    queue.note_saved("notes/a.md");
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert_eq!(h.calls(), vec!["scan".to_string(), "models.list".to_string()], "held at the gate");

    // The user presses [Download].
    queue.set_consent(Consent::Granted);
    tokio::time::sleep(Duration::from_millis(700)).await;

    assert_eq!(
        h.calls(),
        vec!["scan".to_string(), "models.list".to_string(), "embed".to_string()],
        "the owed pass runs, not a fresh save — and granting does not go and ask about artifacts again"
    );
}

#[tokio::test]
async fn an_artifact_already_on_disk_needs_no_consent() {
    let _turn = SERIAL.lock().await;
    let mut listing = models_list_missing();
    listing["models"][0]["state"] = json!("present");
    let h = Harness::new(json!({
        "replies": {"scan": fixture_result(2), "embed": embed_current(), "models.list": listing}
    }))
    .await;
    std::fs::write(&h.np_config, config_naming("bge-m3")).unwrap();
    let queue = h.queue();
    let mut rx = queue.events();

    queue.note_saved("notes/a.md");
    let events = collect(&mut rx, Duration::from_millis(900)).await;

    assert!(!events.iter().any(|e| matches!(e, IngestEvent::NeedsConsent { .. })), "nothing to ask: {events:?}");
    assert_eq!(h.calls(), vec!["scan".to_string(), "models.list".to_string(), "embed".to_string()]);
}

/// note-processor#3, fixed in 0.4.0: `models.list` carries `pinned`, priced from the table compiled into *that*
/// build — so a vision-enabled build quoting 865 MB gets a prompt that says 865 MB instead of Jott's own idea of a
/// number. np's quote wins; Jott's pin is what a nameless, unquoted model falls back to.
#[tokio::test]
async fn the_prompt_quotes_the_price_np_gave_and_not_jotts_own_number() {
    let _turn = SERIAL.lock().await;
    let quoted = 864_676_512u64;
    let h = Harness::new(json!({
        "replies": {
            "scan": fixture_result(2),
            "models.list": {
                "models": [], "fetching": [], "unclaimed": [], "models_dir_error": null,
                "pinned": [{ "id": "embeddinggemma-2-vision", "bytes_total": quoted, "members": ["main", "mmproj"] }]
            },
            "embed": embed_current()
        }
    }))
    .await;
    // np's config names a model this build has no pin for at all: without `pinned` the prompt would have to say
    // nothing about size. With it, the number on screen is np's.
    std::fs::write(&h.np_config, config_naming("embeddinggemma-2-vision")).unwrap();
    let queue = h.queue();
    let mut rx = queue.events();

    queue.note_saved("notes/a.md");
    let events = collect(&mut rx, Duration::from_millis(900)).await;

    let ask = events
        .iter()
        .find_map(|e| match e {
            IngestEvent::NeedsConsent { model_id, bytes, .. } => Some((model_id.clone(), *bytes)),
            _ => None,
        })
        .unwrap_or_else(|| panic!("an uninstalled model is a question: {events:?}"));
    assert_eq!(ask.0.as_deref(), Some("embeddinggemma-2-vision"));
    assert_eq!(ask.1, Some(quoted), "np priced it; Jott repeats rather than competes");
}

#[tokio::test]
async fn a_fresh_np_that_names_no_model_is_still_asked_before_any_download() {
    let _turn = SERIAL.lock().await;
    // The premise this replaces was wrong, and it was wrong in the direction that matters. A fresh np has no
    // config file at all and so names no model — but 0.3.1 carries a built-in pin and will fetch it: measured
    // here, a bare `np serve` downloaded 309,855,456 bytes on the first *search*, unprompted. "Nothing named"
    // therefore means "ask about np's own default", not "nothing to ask".
    let h = Harness::new(json!({
        "replies": {
            "scan": fixture_result(2),
            "models.list": {"models": [], "fetching": [], "unclaimed": [], "models_dir_error": null},
            "embed": embed_current()
        }
    }))
    .await;
    std::fs::write(&h.np_config, "[log]\nlevel = \"info\"\n").unwrap();
    let queue = h.queue();
    let mut rx = queue.events();

    queue.note_saved("notes/a.md");
    let events = collect(&mut rx, Duration::from_millis(900)).await;

    let asked: Vec<&IngestEvent> = events
        .iter()
        .filter(|e| matches!(e, IngestEvent::NeedsConsent { .. }))
        .collect();
    assert_eq!(asked.len(), 1, "a fresh np is exactly the case §12's prompt exists for: {events:?}");
    match asked[0] {
        IngestEvent::NeedsConsent { model_id, bytes, data_dir } => {
            assert_eq!(model_id.as_deref(), Some("embeddinggemma-2"), "named from np's own pin, not invented");
            assert_eq!(*bytes, Some(309_855_456), "the size in the prompt is the size np fetches");
            assert!(!data_dir.as_os_str().is_empty(), "and it names the folder it goes to");
        }
        _ => unreachable!(),
    }
    assert_eq!(h.calls(), vec!["scan".to_string(), "models.list".to_string()], "asking is not doing: {events:?}");
    assert_eq!(h.state("notes/a.md"), StampState::Indexed, "keyword search still arrives while the question waits");
}

/// Once np has the artifact on disk there is no download to consent to, whatever its config says — and this is
/// the case where Jott's pin must *not* speak: np named a different model and np's answer is the one shown.
#[tokio::test]
async fn a_model_np_already_has_is_never_a_question_and_its_own_name_wins() {
    let _turn = SERIAL.lock().await;
    let mut listing = models_list_missing();
    listing["models"][0]["state"] = json!("present");
    let h = Harness::new(json!({
        "replies": {"scan": fixture_result(2), "embed": embed_current(), "models.list": listing}
    }))
    .await;
    std::fs::write(&h.np_config, config_naming("bge-m3")).unwrap();
    let queue = h.queue();
    let mut rx = queue.events();

    queue.note_saved("notes/a.md");
    let events = collect(&mut rx, Duration::from_millis(900)).await;

    assert!(!events.iter().any(|e| matches!(e, IngestEvent::NeedsConsent { .. })), "{events:?}");
    assert_eq!(h.calls(), vec!["scan".to_string(), "models.list".to_string(), "embed".to_string()]);
}

/// np *refusing* for a configuration reason is still a state to display, not a download to consent to — the
/// distinction the old "no model named" test was really protecting. Here np names nothing but its own report
/// says `configuration-disabled`, which happens after consent when np's config disables embeddings outright.
#[tokio::test]
async fn a_refusal_about_configuration_is_a_state_and_not_an_error() {
    let _turn = SERIAL.lock().await;
    let mut listing = models_list_missing();
    listing["models"][0]["state"] = json!("present");
    let h = Harness::new(json!({
        "replies": {
            "scan": fixture_result(2),
            "models.list": listing,
            "embed": {"outcome": {"configuration-disabled": {"key": "[embeddings] model", "hint": "np config: set [embeddings] model"}}}
        }
    }))
    .await;
    std::fs::write(&h.np_config, config_naming("bge-m3")).unwrap();
    let queue = h.queue();
    let mut rx = queue.events();

    queue.note_saved("notes/a.md");
    let events = collect(&mut rx, Duration::from_millis(900)).await;

    assert!(!events.iter().any(|e| matches!(e, IngestEvent::NeedsConsent { .. })), "{events:?}");
    assert!(
        events.iter().any(|e| matches!(e, IngestEvent::Disabled { key: Some(key), .. } if key == "[embeddings] model")),
        "the configuration state has to reach the status line: {events:?}"
    );
    assert_eq!(h.state("notes/a.md"), StampState::Indexed);
}

// ---- closing (§11: never blocks on embedding)

#[tokio::test]
async fn drain_finishes_the_pass_it_started_and_saves_the_stamps() {
    let _turn = SERIAL.lock().await;
    let h = Harness::new(json!({
        "replies": {"scan": fixture_result(2), "embed": embed_current()},
        "sleep_ms": {"scan": 300, "embed": 300}
    }))
    .await;
    // The real cap: finishing in-flight work is what `drain` is for.
    let queue = IngestQueue::spawn(h.pool.clone(), h.stamps.clone(), h.vault.clone())
        .with_np_config(Some(h.np_config.clone()));

    queue.note_saved("notes/a.md");
    queue.flush_now();
    let started = Instant::now();
    queue.drain().await;

    assert!(started.elapsed() < Duration::from_secs(4), "finishing what was in flight is the point: {:?}", started.elapsed());
    assert_eq!(h.calls(), vec!["scan".to_string(), "models.list".to_string(), "embed".to_string()]);
    let reloaded = h.reloaded();
    let hash = sha256_file(&h.vault.join("notes/a.md")).unwrap();
    assert_eq!(reloaded.state_of("notes/a.md", &hash), StampState::Embedded, "closing must not lose what happened");
}

#[tokio::test]
async fn drain_gives_up_at_its_cap_and_leaves_the_note_waiting() {
    let _turn = SERIAL.lock().await;
    let h = Harness::new(json!({
        "replies": {"scan": fixture_result(2), "embed": embed_current()},
        "sleep_ms": {"scan": 9_000}
    }))
    .await;
    let queue = h.queue();

    queue.note_saved("notes/a.md");
    queue.flush_now();
    let started = Instant::now();
    queue.drain().await;

    assert!(started.elapsed() < Duration::from_secs(2), "closing never blocks on embedding (§11): {:?}", started.elapsed());
    assert_eq!(h.state("notes/a.md"), StampState::Waiting, "nothing finished, so nothing is claimed");
}

#[tokio::test]
async fn a_note_that_is_not_on_disk_anymore_is_not_queued() {
    let _turn = SERIAL.lock().await;
    let h = Harness::new(scan_and_embed()).await;
    let queue = h.queue();

    // A save for a file that is gone: the next walk will show it vanished, and there is nothing here
    // for np to index.
    queue.note_saved("notes/gone.md");
    tokio::time::sleep(Duration::from_millis(400)).await;

    assert!(h.calls().is_empty(), "a whole-vault pass for a file that is not there: {:?}", h.calls());
}

#[tokio::test]
async fn closing_a_queue_with_nothing_owed_answers_at_once() {
    let _turn = SERIAL.lock().await;
    let h = Harness::new(json!({})).await;
    // A cap long enough that answering at the cap would be obvious: production's 5 s is what a person
    // closing the app feels, and the test queue shortens it for other tests' sake.
    let queue = IngestQueue::spawn(h.pool.clone(), h.stamps.clone(), h.vault.clone())
        .with_debounce(Duration::from_millis(80))
        .with_drain_cap(Duration::from_secs(3));

    // The drain cap is for work in flight. With nothing owed, a close that waits the full cap is a window
    // that appears to hang on quit (§11).
    let started = Instant::now();
    tokio::time::timeout(Duration::from_secs(5), queue.drain_now()).await.expect("an idle queue drains");
    assert!(started.elapsed() < Duration::from_millis(1_000), "an idle close waited {:?} — that is the drain cap, not work", started.elapsed());
}

#[tokio::test]
async fn two_batches_held_at_the_gate_are_both_embedded_once_it_is_answered() {
    let _turn = SERIAL.lock().await;
    let h = Harness::new(json!({
        "replies": {"scan": fixture_result(2), "models.list": models_list_missing(), "embed": embed_current()}
    }))
    .await;
    std::fs::write(&h.np_config, config_naming("bge-m3")).unwrap();
    for rel in ["notes/first.md", "notes/second.md"] {
        std::fs::write(h.vault.join(rel), format!("# {}\n", rel)).unwrap();
    }
    let queue = h.queue();

    // Two batches, both parked at the consent gate: the second must not replace the first.
    queue.note_saved("notes/first.md");
    tokio::time::sleep(Duration::from_millis(300)).await;
    queue.note_saved("notes/second.md");
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(!h.calls().contains(&"embed".to_string()), "nothing embeds before an answer");

    queue.set_consent(Consent::Granted);
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if h.state("notes/first.md") == StampState::Embedded && h.state("notes/second.md") == StampState::Embedded {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    for rel in ["notes/first.md", "notes/second.md"] {
        assert_eq!(h.state(rel), StampState::Embedded, "{rel} was dropped on the way through the gate");
    }
    assert!(h.calls().contains(&"embed".to_string()), "granting consent runs what was held: {:?}", h.calls());
    queue.drain_now().await;
}

#[tokio::test]
async fn reopening_a_vault_does_not_rewrite_nps_configuration() {
    let _turn = SERIAL.lock().await;
    let h = Harness::new(json!({})).await;
    // A hand-edited config: comments and all, exactly as someone left them.
    let text = "# mine\n[embeddings]\nprovider = \"local\"\nmodel = \"bge-m3\"\n";
    std::fs::write(&h.np_config, text).unwrap();

    // §12: Jott writes np's file to record a *decision*, not to re-assert one it already has.
    let queue = h.queue();
    queue.seed_consent(Consent::Granted);
    assert_eq!(std::fs::read_to_string(&h.np_config).unwrap(), text, "seeding is memory only");

    // An answer still writes, and writes it once: the second identical answer changes nothing.
    queue.set_consent(Consent::Granted);
    let after_answer = std::fs::read_to_string(&h.np_config).unwrap();
    assert!(after_answer.contains("auto_fetch"), "the decision is recorded where np reads it: {after_answer}");
    assert!(after_answer.contains("# mine"), "np's file, with the person's comment still in it: {after_answer}");
    queue.set_consent(Consent::Granted);
    assert_eq!(std::fs::read_to_string(&h.np_config).unwrap(), after_answer);
    queue.drain_now().await;
}

#[tokio::test]
async fn quitting_with_the_consent_banner_up_does_not_wait_for_the_drain_cap() {
    let _turn = SERIAL.lock().await;
    // The first-run path, and the one a person actually hits: save a note, the banner appears, nobody
    // answers, and the window is closed. Same fixture as the two-batch test above, because that is the
    // shape that really parks work at the gate rather than merely failing to start any.
    let h = Harness::new(json!({
        "replies": {"scan": fixture_result(2), "models.list": models_list_missing()}
    }))
    .await;
    std::fs::write(&h.np_config, config_naming("bge-m3")).unwrap();
    std::fs::write(h.vault.join("notes/first.md"), "# First\n").unwrap();
    let queue = IngestQueue::spawn(h.pool.clone(), h.stamps.clone(), h.vault.clone())
        .with_debounce(Duration::from_millis(80))
        .with_drain_cap(Duration::from_secs(3))
        .with_np_config(Some(h.np_config.clone()));

    queue.note_saved("notes/first.md");
    let mut events = queue.events();
    let deadline = Instant::now() + Duration::from_secs(10);
    let asked = loop {
        if Instant::now() > deadline {
            break false;
        }
        if let Ok(Ok(IngestEvent::NeedsConsent { .. })) =
            tokio::time::timeout(Duration::from_millis(200), events.recv()).await
        {
            break true;
        }
    };
    assert!(asked, "the gate never asked, so this test would be timing nothing");

    // The gate only holds work that scan confirmed. If the note were never indexed there would be no
    // embed owed and nothing parked, and this test would be measuring an idle queue — which has its own
    // test above.
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline && h.state("notes/first.md") != StampState::Indexed {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(h.state("notes/first.md"), StampState::Indexed, "scan did not index the note, so nothing is parked");

    // Work parked at a gate is not work in flight: a drain cannot answer on someone's behalf, and those
    // notes are already `Indexed`. Waiting the cap here is a quit that looks like a hang (§11).
    let started = Instant::now();
    tokio::time::timeout(Duration::from_secs(5), queue.drain_now())
        .await
        .expect("a queue with only parked work drains");
    assert!(started.elapsed() < Duration::from_millis(1_000), "quitting with the banner up waited {:?} — that is the drain cap, not work", started.elapsed());
}

#[tokio::test]
async fn a_note_saved_twice_while_held_is_stamped_with_its_newest_bytes() {
    let _turn = SERIAL.lock().await;
    let h = Harness::new(json!({
        "replies": {"scan": fixture_result(2), "models.list": models_list_missing(), "embed": embed_current()}
    }))
    .await;
    std::fs::write(&h.np_config, config_naming("bge-m3")).unwrap();
    let queue = h.queue();

    queue.note_saved("notes/a.md");
    let mut events = queue.events();
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        // A short timeout on each receive, so a gate that never speaks cannot hang the test.
        if let Ok(Ok(IngestEvent::NeedsConsent { .. })) =
            tokio::time::timeout(Duration::from_millis(200), events.recv()).await
        {
            break;
        }
    }

    // Edited again while the banner is up. What np will embed after the answer is what is on disk then, so
    // the held entry has to carry the second version's hash: stamping the first would leave the note
    // answering "not embedded" about content that is embedded, and the tree would sit on `●` until that
    // note is saved again for some other reason.
    std::fs::write(h.vault.join("notes/a.md"), "# A, rewritten while you were deciding\n").unwrap();
    queue.note_saved("notes/a.md");
    tokio::time::sleep(Duration::from_millis(400)).await;

    queue.set_consent(Consent::Granted);
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        if h.state("notes/a.md") == StampState::Embedded {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(h.state("notes/a.md"), StampState::Embedded, "the stamp describes the bytes np embedded");
    queue.drain_now().await;
}

// ---- asking again (§12's decision belongs to the person, not to the first session) ----

/// "Stay keyword-only" answers the question that was asked *then*. It should not be a life sentence: a month
/// later the same vault may want semantic search, and there has to be a way back that is not editing a TOML
/// file by hand. So the window can ask whether there is still something to offer.
#[tokio::test]
async fn an_earlier_no_can_be_asked_about_again() {
    let _turn = SERIAL.lock().await;
    let h = Harness::new(json!({
        "replies": {"scan": fixture_result(2), "models.list": models_list_missing(), "embed": embed_current()}
    }))
    .await;
    std::fs::write(&h.np_config, config_naming("bge-m3")).unwrap();
    let queue = h.queue();
    queue.set_consent(Consent::Declined);

    let offer = queue.offer().await.expect("the bytes are still not on disk, so there is still a question");
    assert_eq!(offer.model_id.as_deref(), Some("bge-m3"), "np's own answer, not Jott's pin");
    assert!(!offer.data_dir.as_os_str().is_empty(), "and it names where they would go");
    // `models.list` is how it knows — that call is the whole point. Asking is still not indexing, and still
    // not fetching: np's answer empty-handed is what makes an offer honest.
    assert_eq!(h.calls(), vec!["models.list".to_string()], "asking is not working: {offer:?}");
}

/// The other arm: nothing to offer when the model is already installed. Offering a download of bytes someone
/// has is how §18.6's duplicate-fetch trap starts, and it would make the button a lie.
#[tokio::test]
async fn nothing_is_offered_when_the_bytes_are_already_there() {
    let _turn = SERIAL.lock().await;
    let h = Harness::new(json!({"replies": {"scan": fixture_result(2), "embed": embed_current()}})).await;
    let queue = h.queue();

    assert_eq!(queue.offer().await, None, "the default harness reports the model present");

    // And a decision of yes is not a question either: np fetches on its own once it is allowed to.
    queue.set_consent(Consent::Granted);
    assert_eq!(queue.offer().await, None);
}

// ---- the trace: what the drawer shows about this queue's work ----

use jott_core::diagnostics::{Trace, TraceKind};

/// The window's answer to "what did np just do" has to be the same facts the journal wrote, or the two surfaces
/// disagree in front of someone reporting a bug — which is the one thing §11 forbids.
#[tokio::test]
async fn every_request_and_np_sentence_reaches_the_trace() {
    let _turn = SERIAL.lock().await;
    let sentence = "workspace `/fake/vault` is mid-migration: run `np migrate` first";
    let h = Harness::new(json!({
        "replies": {"scan": fixture_result(2)},
        "faults": {"embed": {"code": -32000, "message": sentence}}
    }))
    .await;
    let queue = h.queue();
    let trace = Arc::new(Trace::new(50));
    queue.set_trace(trace.clone());

    queue.note_saved("notes/a.md");
    tokio::time::sleep(Duration::from_millis(700)).await;

    let rows = trace.recent();
    let scan = rows
        .iter()
        .find(|row| row.method.as_deref() == Some("scan") && row.kind == TraceKind::Request)
        .unwrap_or_else(|| panic!("the scan belongs here too: {rows:?}"));
    assert_eq!(scan.outcome, Some("applied"));
    assert!(scan.elapsed_ms.is_some(), "and it says how long it took");

    let embed = rows
        .iter()
        .find(|row| row.method.as_deref() == Some("embed") && row.kind == TraceKind::Request)
        .unwrap_or_else(|| panic!("a faulted embed belongs here: {rows:?}"));
    assert_eq!(embed.outcome, Some("fault"));
    assert_eq!(embed.detail.as_deref(), Some(sentence), "np's words, byte for byte (§4)");
}

/// A fetch sends one progress line per chunk of bytes. Forwarded as they arrive, that is a thousand rows a
/// second into a debugging screen and a webview choking on its own log — so the trace gets one row per phase
/// change, and at most one per second otherwise.
#[tokio::test]
async fn progress_lines_are_coalesced_not_forwarded_one_by_one() {
    let _turn = SERIAL.lock().await;
    let h = Harness::new(json!({
        "replies": {"scan": fixture_result(2), "embed": embed_current()},
        "progress_counters": {
            "embed": [
                {"phase": "locking"},
                {"phase": "embedding", "chunks_done": 1, "chunks_total": 3},
                {"phase": "embedding", "chunks_done": 2, "chunks_total": 3},
                {"phase": "completed"}
            ]
        }
    }))
    .await;
    let queue = h.queue();
    let trace = Arc::new(Trace::new(50));
    queue.set_trace(trace.clone());

    queue.note_saved("notes/a.md");
    tokio::time::sleep(Duration::from_millis(900)).await;

    let embed_rows: Vec<_> = trace
        .recent()
        .into_iter()
        .filter(|row| row.kind == TraceKind::Progress && row.method.as_deref() == Some("embed"))
        .collect();
    assert_eq!(
        embed_rows.iter().map(|r| r.detail.as_deref()).collect::<Vec<_>>(),
        // Four notifications from np, three rows: two `embedding` lines inside one second are one row, and the
        // counters shown are the first of them.
        vec![Some("locking"), Some("embedding chunks 1/3"), Some("completed")],
        "{embed_rows:?}"
    );
}

/// The two things a person watching the drawer will ask about that are not requests: did my answer count, and
/// why is it waiting again.
#[tokio::test]
async fn answers_and_retries_are_rows_too() {
    let _turn = SERIAL.lock().await;
    let h = Harness::new(json!({"replies": {"scan": {"command": "scan", "locked": true, "waited_ms": 5000}}})).await;
    let queue = h.queue();
    let trace = Arc::new(Trace::new(50));
    queue.set_trace(trace.clone());

    queue.note_saved("notes/a.md");
    tokio::time::sleep(Duration::from_millis(700)).await;
    queue.set_consent(Consent::Granted);
    tokio::time::sleep(Duration::from_millis(200)).await;

    let rows = trace.recent();
    assert!(
        rows.iter().any(|row| row.kind == TraceKind::Queue && row.detail.as_deref() == Some("locked")),
        "a retry has to be visible in the drawer, not only in the status line: {rows:?}"
    );
    assert!(
        rows.iter().any(|row| row.kind == TraceKind::Consent && row.detail.as_deref() == Some("granted")),
        "and so does the answer someone gave: {rows:?}"
    );
}

/// The ladder exists so a vault locked by somebody else is not knocked on every two seconds for an hour. It was
/// also never coming back down: the counter reset only when an embed finished, so one `configuration-disabled`
/// answer — or any pass that indexed and then stopped short — left the *next* lock waiting 60 s for a two-second
/// problem. A committed index is the fact that matters: the index is not behind any more.
#[tokio::test]
async fn a_committed_index_brings_the_retry_wait_back_down() {
    let _turn = SERIAL.lock().await;
    let locked = || json!({"command": "scan", "locked": true, "waited_ms": 1000});
    let h = Harness::new(json!({
        "sequence": {
            // call 1 locked, call 2 (the retry) commits, call 3 is a fresh lock after a real pass.
            "scan": [{"reply": locked()}, {"reply": fixture_result(2)}, {"reply": locked()}]
        },
        "replies": { "embed": {"outcome": {"configuration-disabled": {"key": "[embeddings] model"}}} }
    }))
    .await;
    let queue = h.queue().with_backoff_floor(Duration::from_millis(300));
    let mut rx = queue.events();

    queue.note_saved("notes/a.md");
    let first = collect(&mut rx, Duration::from_millis(1_500)).await;
    let (_, wait) = behind(&first).expect("the locked scan says behind");
    assert_eq!(wait, backoff_ms(0), "the first lock is the first rung");
    assert!(
        first.iter().any(|e| matches!(e, IngestEvent::Indexed(_))),
        "and the retry got through: {first:?}"
    );

    // A new note, a new lock: the ladder starts again, because in between np indexed the vault.
    std::fs::write(h.vault.join("notes/b.md"), "# B\n").unwrap();
    queue.note_saved("notes/b.md");
    let after = collect(&mut rx, Duration::from_millis(1_500)).await;
    let (_, wait) = behind(&after).unwrap_or_else(|| panic!("the third scan is locked again: {after:?}"));
    assert_eq!(wait, backoff_ms(0), "a committed index resets the ladder, so this is rung one again");
}
