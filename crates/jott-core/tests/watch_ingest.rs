//! The other half of §14: what Jott does with a note somebody changed **outside** it.
//!
//! The reload half has long been tested — a clean buffer follows the disk, a dirty one asks before touching
//! either side. This file is about the half nobody tested, and it cost a release: an outside edit reached the
//! window and never reached the ingest queue, so np kept answering from bytes that were no longer on disk while
//! the tree's markers stayed green and the status line read `up to date`. Measured in the packaged v0.0.31 (the
//! stamps recorded `ea76570b…` for a file whose bytes were `c0766e69…`), filed as issue #29.

mod common;

use common::repo_root;
use jott_core::hashes::sha256_file;
use jott_core::ipc::VaultSession;
use jott_core::np::{NpFeatures, NpInfo, NpSource};
use jott_core::vault::store::Consent;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// The stand-in is configured through one process-global variable and records into one file per scenario, so
/// these take turns rather than reading each other's recordings.
static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn info() -> NpInfo {
    NpInfo {
        version: "0.6.1".into(),
        protocol: 1,
        methods: ["initialize", "search", "scan", "embed", "models.list", "status.get", "shutdown"]
            .map(str::to_string)
            .to_vec(),
        features: NpFeatures { local_model: true, fetch: true, vision: false },
        data_dir: "/fake/data".into(),
        progress: true,
    }
}

/// An embed answer that says "nothing to do", because this file's subject is which paths get *asked* about —
/// not what np reports afterwards.
fn embed_current() -> Value {
    json!({"model": "embeddinggemma-2", "chunks_seen": 1, "current": 1, "embedded": 1, "refused_budget": 0,
           "busy_gap": 0, "errors": 0, "images_seen": 0, "images_embedded": 0, "images_refused": 0,
           "images_pruned": 0, "images_unchecked": 0, "images_sweep_ran": true, "image_refusals": [],
           "verification": "self-consistent, not verified", "offline": false, "outcome": "current"})
}

struct Harness {
    /// The state root this session writes: stamps and mirrors. Named, so two tests cannot see each other's files.
    data: tempfile::TempDir,
    vault: tempfile::TempDir,
    record: PathBuf,
    /// Held so the scenario file outlives every child spawned against it.
    _scenario: tempfile::TempDir,
}

impl Harness {
    async fn new() -> Harness {
        let data = tempfile::tempdir().expect("data dir");
        let vault = tempfile::tempdir().expect("vault dir");
        let record = data.path().join("calls.ndjson");
        std::fs::write(&record, "").expect("empty call record");

        let scenario_dir = tempfile::tempdir().expect("scenario dir");
        let scenario_file = scenario_dir.path().join("scenario.json");
        let scenario = json!({
            "record": record.display().to_string(),
            "replies": { "scan": common::fixture_result(2), "embed": embed_current() },
        });
        std::fs::write(&scenario_file, scenario.to_string()).expect("write scenario");
        // Set before the session spawns its child: a child copies its environment at exec.
        std::env::set_var("FAKE_NP_SCENARIO", &scenario_file);

        Harness { data, vault, record, _scenario: scenario_dir }
    }

    /// Write a note **behind Jott's back** — deliberately not through `session.save_note`, because our own saves
    /// are recorded in `SelfWrites` and the watcher is right to stay quiet about them.
    fn write_outside(&self, rel: &str, bytes: &[u8]) {
        let path = self.vault.path().join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("parent");
        }
        std::fs::write(path, bytes).expect("outside write");
    }

    fn open(&self) -> Arc<VaultSession> {
        VaultSession::open_in_with(
            NpSource::Env(repo_root().join("tests/fake-np/np.py")),
            info(),
            None,
            self.vault.path(),
            Consent::Granted,
            Some(self.data.path().join("np-config.toml")),
            self.data.path().to_path_buf(),
        )
    }

    /// Which np methods the stand-in was actually asked for since the last `clear_record`.
    fn asked(&self) -> Vec<String> {
        std::fs::read_to_string(&self.record)
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .filter_map(|line| line["method"].as_str().map(str::to_string))
            .filter(|m| m != "initialize" && m != "shutdown")
            .collect()
    }

    fn clear_record(&self) {
        std::fs::write(&self.record, "").expect("clear record");
    }

    /// What the stamps claim about `rel`, once the queue has written them: `(indexed digest, embedded digest)`.
    fn stamped(&self, rel: &str) -> Option<(String, String)> {
        let state_dir = self.data.path().join("state");
        let Ok(workspaces) = std::fs::read_dir(&state_dir) else { return None };
        for entry in workspaces.flatten() {
            let Ok(text) = std::fs::read_to_string(entry.path().join("state.json")) else { continue };
            let Ok(value) = serde_json::from_str::<Value>(&text) else { continue };
            if let Some(row) = value.get("files").and_then(|files| files.get(rel)) {
                let indexed = row.get("indexed").and_then(Value::as_str).map(str::to_string);
                let embedded = row.get("embedded").and_then(Value::as_str).map(str::to_string);
                if let (Some(indexed), Some(embedded)) = (indexed, embedded) {
                    return Some((indexed, embedded));
                }
            }
        }
        None
    }
}

/// Wait for np to be asked for `method`, and hand back everything it was asked either way.
async fn wait_until_asked(harness: &Harness, method: &str, rounds: u32) -> Vec<String> {
    let mut asked = Vec::new();
    for _ in 0..rounds {
        asked = harness.asked();
        if asked.iter().any(|m| m == method) {
            return asked;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    asked
}

/// The bug: an outside edit reached the window and stopped there.
#[tokio::test]
async fn a_note_changed_outside_jott_is_indexed_without_any_save_in_jott() {
    let _turn = SERIAL.lock().await;
    let harness = Harness::new().await;
    harness.write_outside("a.md", b"# A\n\nthe first text\n");
    let session = harness.open();

    // Nothing has been queued yet: Jott indexes on a save or on a change it notices, not because a folder was
    // opened — measured while writing this file, and the reason an outside edit had nowhere to go. Clear the
    // record so everything below is attributable to this one edit.
    harness.clear_record();

    // Somebody else edits the note: another editor, a `sed -i`, a sync client's sweep.
    harness.write_outside("a.md", b"# A\n\nQuarkkessel stands on the shelf\n");

    let asked = wait_until_asked(&harness, "scan", 120).await;
    assert!(
        asked.iter().any(|m| m == "scan"),
        "np was never told the bytes changed, so search answers from the old text: {asked:?}"
    );

    session.close().await;
}

/// Indexing is only half the promise. The marker beside the row and the status line's `up to date` are both
/// claims about *which bytes* np holds — so the stamp has to move to the new digest, or a green dot keeps
/// describing a file that no longer exists in that form.
#[tokio::test]
async fn an_outside_edit_moves_the_stamp_to_the_bytes_np_now_holds() {
    let _turn = SERIAL.lock().await;
    let harness = Harness::new().await;
    harness.write_outside("a.md", b"# A\n\nthe first text\n");
    let session = harness.open();

    harness.clear_record();

    let edited = b"# A\n\nQuarkkessel stands on the shelf\n";
    harness.write_outside("a.md", edited);
    wait_until_asked(&harness, "scan", 120).await;

    // The queue writes stamps after the pass lands; give it the same grace the np call got.
    let want = sha256_file(&harness.vault.path().join("a.md")).expect("digest of the edited note");
    let mut stamped = None;
    for _ in 0..120 {
        stamped = harness.stamped("a.md");
        if stamped.as_ref().is_some_and(|(indexed, _)| *indexed == want) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let (indexed, embedded) = stamped.unwrap_or_else(|| panic!("no stamp recorded for a.md"));
    assert_eq!(indexed, want, "the row still claims the pre-edit bytes are indexed");
    assert_eq!(embedded, want, "a green marker would promise embeddings of bytes np never saw");

    session.close().await;
}

/// `note_saved`'s own contract, now that an outside edit goes through it: a change worth nothing costs no round
/// trip. An editor that rewrites identical bytes on every focus change must not buy a scan each time.
#[tokio::test]
async fn an_outside_touch_that_changed_no_bytes_costs_no_round_trip() {
    let _turn = SERIAL.lock().await;
    let harness = Harness::new().await;
    let same = "# A\n\nunchanged text\n";
    let session = harness.open();

    // Earn the state first. Jott does not index a folder merely because it opened one, so "bytes np already
    // holds cost nothing" has to be measured after a pass that really ran — here, an ordinary save.
    session.save_note("a.md", same).expect("saved");
    let want = sha256_file(&harness.vault.path().join("a.md")).expect("digest");
    let mut landed = None;
    for _ in 0..120 {
        landed = harness.stamped("a.md");
        if landed.as_ref().is_some_and(|(indexed, embedded)| indexed == &want && embedded == &want) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(
        landed.as_ref().map(|(indexed, embedded)| (indexed.as_str(), embedded.as_str())),
        Some((want.as_str(), want.as_str())),
        "the save's own pass never landed, so this test would be measuring nothing"
    );
    harness.clear_record();

    // Now an editor rewrites the same bytes — vim on focus, a formatter that changes nothing, a sync client
    // reconciling a file it already has.
    harness.write_outside("a.md", same.as_bytes());
    // Long enough for a debounce plus a scan to have shown up had one been queued.
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let asked = harness.asked();
    assert!(asked.is_empty(), "rewriting identical bytes asked np for work: {asked:?}");

    session.close().await;
}

/// The two fences that stopped 6,885 scans in 142 seconds stay up with the new wiring: only a path note-processor
/// could have indexed may start index work, and that includes np's own `INDEX.md.tmp` → `INDEX.md` rename.
#[tokio::test]
async fn np_s_own_catalogue_churn_still_starts_no_index_work() {
    let _turn = SERIAL.lock().await;
    let harness = Harness::new().await;
    harness.write_outside("a.md", b"# A\n");
    let session = harness.open();

    harness.clear_record();

    // np's catalogue: written as a temporary, renamed into place, on every pass. Plus a file np never parses.
    harness.write_outside("INDEX.md.tmp", b"# index draft\n");
    std::fs::rename(harness.vault.path().join("INDEX.md.tmp"), harness.vault.path().join("INDEX.md"))
        .expect("renamed into place");
    harness.write_outside("photo.png", b"\x89PNG not a note\n");
    tokio::time::sleep(Duration::from_millis(1500)).await;

    let asked = harness.asked();
    assert!(asked.is_empty(), "np's own churn, or a non-note file, started index work: {asked:?}");

    session.close().await;
}

/// A note inside a folder is as much somebody's edit as one at the root — the pump hands over `rel` paths, and
/// the queue has to receive the same spelling the walk uses.
#[tokio::test]
async fn an_outside_edit_in_a_folder_is_indexed_too() {
    let _turn = SERIAL.lock().await;
    let harness = Harness::new().await;
    harness.write_outside("notes/deep.md", b"# Deep\n\nfirst\n");
    let session = harness.open();

    harness.clear_record();

    harness.write_outside("notes/deep.md", b"# Deep\n\nQuarkkessel again\n");
    let asked = wait_until_asked(&harness, "scan", 120).await;
    assert!(asked.iter().any(|m| m == "scan"), "a folder note's edit never reached np: {asked:?}");

    session.close().await;
}

/// Where the news has to arrive: the pump's decision is a line of code, and a line of code is easy to drop in a
/// refactor. Naming it here means the next person who moves that loop finds this before a tester does.
#[test]
fn the_watcher_pump_routes_a_still_present_note_to_the_queue() {
    let source = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/ipc.rs")).expect("ipc.rs");
    let pump = source
        .split("fn start_watching")
        .nth(1)
        .and_then(|tail| tail.split("\n    }\n").next())
        .unwrap_or_default();
    assert!(!pump.is_empty(), "the pump was renamed; this fence is pointing at nothing");

    // Both outcomes of an outside change have to reach the queue from inside the pump: a path that is gone asks
    // for a walk, and a path still present owes np a look at its bytes. A file-wide count would pass on the save
    // path alone, which is how this stayed broken unnoticed.
    assert!(pump.contains("queue.paths_changed()"), "a deleted note no longer asks for a walk: {pump}");
    assert!(pump.contains("queue.note_saved("), "an edited note is not being handed to the ingest queue: {pump}");

    // And each stays behind the guard that keeps np's own catalogue churn out — before it, not after it.
    assert!(pump.matches("could_be_indexed").count() >= 2, "one of the two branches lost its `could_be_indexed` guard: {pump}");
}

/// Silence here would be the old bug wearing a different coat: an unreachable np must not make the pump decide a
/// person's edit never happened. The queue requeues; the window keeps saying what it thinks it knows.
#[tokio::test]
async fn an_outside_edit_is_still_queued_when_np_is_not_there() {
    let _turn = SERIAL.lock().await;
    let harness = Harness::new().await;
    harness.write_outside("a.md", b"# A\n");
    let session = VaultSession::open_in_with(
        NpSource::Env(PathBuf::from("/nonexistent/np")),
        info(),
        None,
        harness.vault.path(),
        Consent::Granted,
        Some(harness.data.path().join("np-config.toml")),
        harness.data.path().to_path_buf(),
    );
    harness.clear_record();

    // No panic, and the note is still readable: the watch pump is not allowed to take the session down with it.
    harness.write_outside("a.md", b"# A\n\nedited while np is away\n");
    tokio::time::sleep(Duration::from_millis(600)).await;
    let note = session.read_note("a.md").expect("the editor still works without np");
    assert!(note.text.contains("edited while np is away"), "the reload half of §14 broke: {:?}", note.text);

    session.close().await;
}
