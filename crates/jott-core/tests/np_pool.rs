//! The pool: a writer connection for `scan`/`embed`, and a reader spawned only when a search
//! actually needs one. §18.1 measured a search queued behind an embed at **102.1 s** on one
//! connection and 0.00 s from a second process, so the two-connection shape is not an
//! optimisation — it is the difference between a status line that updates and a window that
//! freezes.

mod common;

use common::repo_root;
use jott_core::np::outcomes::{EmbedOutcome, Mode, ScanOutcome};
use jott_core::np::pool::{Connection, NpPool};
use jott_core::np::{NpFeatures, NpInfo, NpSource};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;

/// Every test here both counts children and configures the stand-in through a process-global
/// variable, so they take turns rather than counting each other's processes.
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

/// Point the stand-in at `scenario` and hand back the pool that will spawn from it. The caller
/// holds `SERIAL`, which is what makes setting a process-wide variable here legitimate.
fn pool_answering(scenario: Value) -> (tempfile::TempDir, Arc<NpPool>) {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("scenario.json");
    std::fs::write(&path, scenario.to_string()).expect("write scenario");
    std::env::set_var("FAKE_NP_SCENARIO", &path);
    let script = repo_root().join("tests/fake-np/np.py");
    (dir, Arc::new(NpPool::new(NpSource::Env(script), info())))
}

fn vault() -> PathBuf {
    PathBuf::from("/fake/vault")
}

/// Live `np` stand-in children of *this* process. Zombie states are skipped: an exited child
/// waiting to be reaped holds no workspace lock and answers nothing.
///
/// Linux-only because it reads `/proc`. Windows is where this app runs for real (spec §19), and
/// the behaviour those counts witness — spawn on demand, replace a dead connection — is asserted
/// from the client side by the tests below, which run everywhere.
#[cfg(unix)]
fn live_children() -> usize {
    let me = std::process::id().to_string();
    std::fs::read_dir("/proc")
        .expect("/proc")
        .flatten()
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.is_empty() || !name.chars().all(|c| c.is_ascii_digit()) {
                return false;
            }
            let Ok(cmdline) = std::fs::read(entry.path().join("cmdline")) else { return false };
            if !String::from_utf8_lossy(&cmdline).replace('\0', " ").contains("fake-np/np.py") {
                return false;
            }
            let Ok(stat) = std::fs::read_to_string(entry.path().join("stat")) else { return false };
            // `pid (comm) state ppid …`, and a comm can hold spaces and brackets — so read from
            // the closing bracket rather than splitting the line from the front.
            let after = stat.rsplit(')').next().unwrap_or_default();
            let fields: Vec<&str> = after.split_whitespace().collect();
            let state = fields.first().copied().unwrap_or("?");
            let ppid = fields.get(1).copied().unwrap_or("");
            state != "Z" && ppid == me
        })
        .count()
}

/// Children appear and disappear asynchronously (the stand-in has to exec, and an exited one has
/// to be reaped), so counts are waited for rather than assumed at the instant of the call.
#[cfg(unix)]
async fn wait_for_live_children(want: usize) {
    for _ in 0..100 {
        if live_children() == want {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("wanted {want} live np children, ended up with {}", live_children());
}

const SCAN_REPORT: &str = r#"{"added": 0, "modified": 0, "moved": 0, "removed": 0, "unchanged": 1,
    "skipped": [], "notes": 1, "unresolved_links": 0, "unresolved_targets": [],
    "index_files": ["INDEX.md"], "elapsed_ms": 7, "upgrade_orphans": null, "merged_notes": null,
    "recorded_upgrade_orphans": null, "recorded_merged_notes": null, "unindexable": null}"#;

const SEARCH_REPLY: &str = r#"{"hits": [], "mode": "keyword", "vectors": false, "offline": false,
    "fetched": null, "note": null, "reason": null}"#;

#[cfg(unix)]
#[tokio::test]
async fn a_scan_spawns_one_child_and_a_search_adds_exactly_one_more() {
    let _turn = SERIAL.lock().await;
    let (_dir, pool) = pool_answering(json!({
        "replies": {"scan": serde_json::from_str::<Value>(SCAN_REPORT).unwrap(),
                     "search": serde_json::from_str::<Value>(SEARCH_REPLY).unwrap()}
    }));

    // Asking for a pool asks for nothing: no child until there is work for one to do.
    assert_eq!(live_children(), 0, "constructing the pool spawns nothing");

    assert!(matches!(pool.scan(&vault()).await.unwrap(), ScanOutcome::Applied(_)));
    wait_for_live_children(1).await;

    pool.search(&vault(), "anything", 8).await.unwrap();
    wait_for_live_children(2).await;

    // The second search reuses the reader: a child per query would pay np's ~115 MB model map
    // every time (spec §7).
    pool.search(&vault(), "again", 8).await.unwrap();
    assert_eq!(live_children(), 2, "a second search must not spawn a third child");

    pool.close().await.unwrap();
    wait_for_live_children(0).await;

    // A save that lands after shutdown is refused rather than answered by one last child the
    // app has stopped waiting for.
    assert!(pool.scan(&vault()).await.is_err(), "a closed pool spawns nothing more");
    assert_eq!(live_children(), 0);
}

#[tokio::test]
async fn a_search_does_not_wait_for_the_embedding_job() {
    let _turn = SERIAL.lock().await;
    // 1500 ms is the plan's number, and it is generous in the wrong direction: §18.1 measured
    // 102 s for this mistake on one connection.
    let (_dir, pool) = pool_answering(json!({
        "sleep_ms": {"embed": 1500},
        "replies": {
            "embed": {"model": "embeddinggemma-2", "chunks_seen": 4, "current": 4, "embedded": 4,
                       "refused_budget": 0, "busy_gap": 0, "errors": 0, "images_seen": 0,
                       "images_embedded": 0, "images_refused": 0, "images_pruned": 0,
                       "images_unchecked": 0, "images_sweep_ran": true, "image_refusals": [],
                       "verification": "self-consistent, not verified", "offline": false,
                       "outcome": "current"},
            "search": serde_json::from_str::<Value>(SEARCH_REPLY).unwrap()
        }
    }));

    let embedding = {
        // The pool is used from several tasks at once in the app: a status line, a save, an overlay.
        let vault = vault();
        let pool = pool.clone();
        tokio::spawn(async move { pool.embed(&vault).await })
    };

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let started = std::time::Instant::now();
    let answer = pool.search(&vault(), "now", 8).await.unwrap();
    let waited = started.elapsed();
    assert!(
        waited < std::time::Duration::from_millis(300),
        "the search queued behind the embed: it waited {waited:?} (spec §18.1 measured 102.1 s for this)"
    );
    assert!(!answer.vectors);

    assert!(matches!(embedding.await.unwrap().unwrap(), EmbedOutcome::Applied(_)));
    pool.close().await.unwrap();
}

#[tokio::test]
async fn progress_says_which_connection_a_line_came_from() {
    let _turn = SERIAL.lock().await;
    let (_dir, pool) = pool_answering(json!({
        "replies": {"scan": serde_json::from_str::<Value>(SCAN_REPORT).unwrap(),
                     "search": {"hits": [], "mode": "hybrid", "vectors": true, "offline": false,
                                 "fetched": null, "note": null, "reason": null}}
    }));

    let mut notes = pool.progress();
    pool.scan(&vault()).await.unwrap();
    pool.search(&vault(), "meaning", 8).await.unwrap();

    let mut seen = Vec::new();
    for _ in 0..64 {
        let Ok(note) = notes.try_recv() else { break };
        seen.push((note.connection, note.note.command));
    }
    assert!(
        seen.contains(&(Connection::Writer, "scan".to_string())),
        "the writer's own line: {seen:?}"
    );
    assert!(
        seen.contains(&(Connection::Reader, "search".to_string())),
        "the reader's own line: {seen:?}"
    );
    // §7's reason for tagging: an embed's `embedding 3/7` must never be drawn against a
    // search's counters, and one merged stream cannot tell them apart without this.
    assert!(
        !seen.contains(&(Connection::Reader, "scan".to_string())),
        "a scan runs on the writer only: {seen:?}"
    );
    pool.close().await.unwrap();
}

#[tokio::test]
async fn a_reader_that_died_is_replaced_and_the_writer_never_notices() {
    let _turn = SERIAL.lock().await;
    // The stand-in answers the first search and exits on the second, which is what a killed or
    // crashed child looks like from here: an answer that never comes.
    let (_dir, pool) = pool_answering(json!({
        "die_on_call": {"search": 2},
        "replies": {
            "search": serde_json::from_str::<Value>(SEARCH_REPLY).unwrap(),
            "scan": serde_json::from_str::<Value>(SCAN_REPORT).unwrap()
        }
    }));

    pool.scan(&vault()).await.unwrap();
    pool.search(&vault(), "first", 8).await.unwrap();

    // Bounded so a regression reports itself: a request that never comes back is the frozen
    // window this task exists to prevent, not a test that runs slowly.
    let second = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        pool.search(&vault(), "second", 8),
    )
    .await;
    // `Err` here is the timeout itself: no answer, because nothing was left to send one.
    let Ok(fault) = second else {
        panic!("the request outlived the child that was answering it")
    };
    assert!(fault.is_err(), "a dead child is not an empty result: {fault:?}");

    // The next search finds a fresh reader and answers. Nothing here asks the user to restart.
    pool.search(&vault(), "third", 8).await.unwrap();
    // And the writer was never touched by any of it.
    assert!(matches!(pool.scan(&vault()).await.unwrap(), ScanOutcome::Applied(_)));

    pool.close().await.unwrap();
}

#[tokio::test]
async fn a_search_asks_for_hybrid_because_that_is_what_the_overlay_wants() {
    let _turn = SERIAL.lock().await;
    // The stand-in answers only a `hybrid` request and refuses any other mode by name, so this
    // fails if the pool quietly changes what it sends (§10).
    let (_dir, pool) = pool_answering(json!({
        "require_mode": "hybrid",
        "replies": {"search": {"hits": [], "mode": "hybrid", "vectors": true, "offline": false,
                                 "fetched": null, "note": null, "reason": null}}
    }));

    let answer = pool.search(&vault(), "both halves", 12).await.unwrap();
    assert_eq!(answer.mode, Mode::Hybrid);
    pool.close().await.unwrap();
}
