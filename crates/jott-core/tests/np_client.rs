use jott_core::np::client::NpClient;
use jott_core::np::frame::Frame;
use jott_core::np::ProgressNote;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2).unwrap().to_path_buf()
}

/// The captured np 0.3.1 transcript is the spec for this module: 22 lines, a mix of
/// responses, faults (one with a null id), and notifications interleaved anywhere.
#[tokio::test]
async fn replays_the_captured_transcript() {
    let text = std::fs::read_to_string(repo_root().join("tests/fixtures/np-serve-transcript.ndjson"))
        .expect("fixture");
    let mut responses = 0;
    let mut faults = 0;
    let mut notes = 0;
    let mut null_id = 0;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        match Frame::decode(line) {
            Frame::Response { .. } => responses += 1,
            Frame::Fault { id, fault } => {
                faults += 1;
                if id.is_none() {
                    null_id += 1;
                }
                assert!(!fault.message.is_empty(), "np's sentence survives");
            }
            Frame::Note(_) => notes += 1,
            Frame::Ignored => panic!("no blank lines in the fixture"),
        }
    }
    // Two sessions are in the file: a clean one (initialize, scan, search, embed) and a
    // second process asking about a workspace someone else is embedding.
    assert_eq!(responses, 8, "every result-bearing line, both sessions");
    assert_eq!(faults, 4, "-32601, two -32602, one -32700");
    assert_eq!(null_id, 1, "the parse error has no id to echo");
    assert_eq!(notes, 10, "np/progress lines only, matched by method and absence of id");
    assert_eq!(responses + faults + notes, 22);
}

fn fake_np(scenario: Option<&std::path::Path>) -> PathBuf {
    let script = repo_root().join("tests/fake-np/np.py");
    if let Some(s) = scenario {
        std::env::set_var("FAKE_NP_SCENARIO", s);
    } else {
        std::env::remove_var("FAKE_NP_SCENARIO");
    }
    script
}

#[tokio::test]
async fn requests_are_answered_by_id_while_notifications_flow_past() {
    let client = NpClient::spawn(&fake_np(None)).await.unwrap();
    let mut notes = client.notifications();
    let init = client.request("initialize", serde_json::json!({"notifications": true})).await.unwrap();
    assert_eq!(init["protocol"], 1);

    // Two requests in flight at once: each must get its own body, not whichever arrives next.
    let a = client.request("scan", serde_json::json!({"workspace": "/x"}));
    let b = client.request("search", serde_json::json!({"workspace": "/x", "query": "q"}));
    let (ra, rb) = tokio::join!(a, b);
    assert!(ra.unwrap_err().message.contains("scan"), "unknown-method fault for scan");
    assert!(rb.unwrap_err().message.contains("search"), "unknown-method fault for search");

    // The notifications we generated are still on the stream, tagged with their request.
    let mut seen = Vec::new();
    while let Ok(n) = notes.try_recv() {
        let ProgressNote { command, phase, .. } = n;
        seen.push((command, phase));
    }
    assert!(seen.iter().any(|(c, _)| c == "initialize"), "{seen:?}");
    client.shutdown().await.unwrap();
}

#[tokio::test]
async fn a_slow_embed_does_not_block_a_search_on_another_connection() {
    let dir = tempfile::tempdir().unwrap();
    let scenario = dir.path().join("s.json");
    std::fs::write(
        &scenario,
        serde_json::json!({"sleep_ms": {"embed": 1200},
                           "replies": {"embed": {"embedded": 1, "current": 0},
                                       "search": {"hits": [], "mode": "keyword", "vectors": false,
                                                  "offline": false, "reason": null}}})
        .to_string(),
    )
    .unwrap();

    let writer = NpClient::spawn(&fake_np(Some(&scenario))).await.unwrap();
    let reader = NpClient::spawn(&fake_np(Some(&scenario))).await.unwrap();
    let embed = writer.request("embed", serde_json::json!({"workspace": "/x"}));
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let started = std::time::Instant::now();
    let hit = reader.request("search", serde_json::json!({"workspace": "/x", "query": "q"})).await.unwrap();
    assert!(started.elapsed() < std::time::Duration::from_millis(400), "reader waited {:?}", started.elapsed());
    assert_eq!(hit["mode"], "keyword");
    drop(embed); // the sleep is still running; this test is about the reader not waiting for it
    writer.shutdown().await.unwrap();
    reader.shutdown().await.unwrap();
}
