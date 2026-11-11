#![allow(dead_code)] // each test binary compiles its own copy and uses a different part of it
//! Shared harness for the tests that talk to `tests/fake-np/np.py`.
//!
//! Kept out of each test file so every scenario goes through one spawn path: the stand-in is
//! configured by an environment variable, and a test binary runs its tests on several threads.

use jott_core::np::client::NpClient;
use serde_json::Value;
use std::path::PathBuf;
use tempfile::TempDir;

pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .to_path_buf()
}

/// One `result` document out of np 0.3.1's captured transcript, by the response id it answered.
///
/// Scenario documents come from here rather than from typing: a mapping test that retypes the
/// fixture can drift from what was measured and still stay green.
pub fn fixture_result(id: i64) -> Value {
    let text = std::fs::read_to_string(repo_root().join("tests/fixtures/np-serve-transcript.ndjson"))
        .expect("the captured np transcript");
    for line in text.lines() {
        let Ok(line) = serde_json::from_str::<Value>(line) else { continue };
        if line.get("id").and_then(Value::as_i64) == Some(id) && line.get("result").is_some() {
            return line["result"].clone();
        }
    }
    panic!("no response to id {id} in the fixture");
}

/// Held across "point the stand-in at this scenario" and "exec it", for the reason in `spawn`.
static SPAWN: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Spawn fake np answering `scenario`. The returned `TempDir` keeps the scenario file alive.
pub async fn spawn(scenario: Value) -> (TempDir, NpClient) {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("scenario.json");
    std::fs::write(&path, scenario.to_string()).expect("write scenario");
    let script = repo_root().join("tests/fake-np/np.py");

    // FAKE_NP_SCENARIO is process-global, and a child copies its environment at exec — so the
    // lock has to cover both lines. Without it, one test's scenario becomes another's and the
    // failures land on whichever test lost the race.
    let guard = SPAWN.lock().await;
    std::env::set_var("FAKE_NP_SCENARIO", &path);
    let client = NpClient::spawn(&script).await.expect("fake np starts");
    drop(guard);
    (dir, client)
}

/// A workspace path for tests that never touch a disk: the stand-in answers from its scenario
/// and reads nothing, so no test needs a real folder to name one.
pub fn vault() -> PathBuf {
    PathBuf::from("/fake/vault")
}
