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
        let path = dir.path().join("scenario.json");
        std::fs::write(&path, scenario.to_string()).expect("write scenario");
        std::env::set_var("FAKE_NP_SCENARIO", &path);

        let script = repo_root().join("tests/fake-np/np.py");
        let pool = Arc::new(NpPool::new(NpSource::Env(script), info()));
        let appdata = dir.path().join("appdata");
        let np_config = dir.path().join("np-config.toml");
        let stamps = Arc::new(Mutex::new(Stamps::with_root(appdata.clone(), &vault)));
        Harness { _dir: dir, vault, pool, stamps, record, np_config, appdata }
    }

    /// A queue with the production debounce and drain cap shortened, so a sixteen-test file does not
    /// spend two minutes waiting on timers it is not testing. Both real numbers have their own test.
    fn queue(&self) -> IngestQueue {
        IngestQueue::spawn(self.pool.clone(), self.stamps.clone(), self.vault.clone())
            .with_debounce(Duration::from_millis(80))
            .with_drain_cap(Duration::from_millis(600))
            .with_np_config(self.np_config.clone())
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
        IngestEvent::Behind { reason, retry_in_ms } => Some((reason.clone(), *retry_in_ms)),
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

    assert_eq!(h.calls(), vec!["scan".to_string(), "embed".to_string()], "index first: embed reads the index (§18.2)");
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
        vec!["scan".to_string(), "embed".to_string()],
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
        .with_np_config(h.np_config.clone())
        .with_drain_cap(Duration::from_millis(600));

    queue.note_saved("notes/a.md");
    let started = Instant::now();
    queue.flush_now();
    while h.calls().len() < 2 && started.elapsed() < Duration::from_secs(3) {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    assert_eq!(h.calls(), vec!["scan".to_string(), "embed".to_string()]);
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

    assert_eq!(h.calls(), vec!["scan".to_string(), "embed".to_string()], "the stamp says owed, so np is asked");
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

    assert_eq!(h.calls(), vec!["scan".to_string(), "embed".to_string()]);
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

#[tokio::test]
async fn a_vault_with_no_model_configured_is_not_asked_for_consent() {
    let _turn = SERIAL.lock().await;
    // No `[embeddings] model` means there is no artifact to fetch: np answers the embed itself with a
    // configuration state, and offering to download something unnamed is a prompt about nothing.
    let h = Harness::new(json!({
        "replies": {
            "scan": fixture_result(2),
            "embed": {"outcome": {"configuration-disabled": {"key": "[embeddings] model", "hint": "np config: set [embeddings] model"}}}
        }
    }))
    .await;
    std::fs::write(&h.np_config, "[log]\nlevel = \"info\"\n").unwrap();
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
        .with_np_config(h.np_config.clone());

    queue.note_saved("notes/a.md");
    queue.flush_now();
    let started = Instant::now();
    queue.drain().await;

    assert!(started.elapsed() < Duration::from_secs(4), "finishing what was in flight is the point: {:?}", started.elapsed());
    assert_eq!(h.calls(), vec!["scan".to_string(), "embed".to_string()]);
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
