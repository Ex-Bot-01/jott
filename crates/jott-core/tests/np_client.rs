mod common;

use common::{repo_root, spawn};
use jott_core::np::frame::Frame;
use jott_core::np::ProgressNote;
use serde_json::json;

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

#[tokio::test]
async fn requests_are_answered_by_id_while_notifications_flow_past() {
    let (_dir, client) = spawn(serde_json::json!({})).await;
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
    let scenario = serde_json::json!({"sleep_ms": {"embed": 1200},
                       "replies": {"embed": {"embedded": 1, "current": 0},
                                   "search": {"hits": [], "mode": "keyword", "vectors": false,
                                              "offline": false, "reason": null}}});

    let (_wd, writer) = spawn(scenario.clone()).await;
    let (_rd, reader) = spawn(scenario).await;
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

/// A child that takes a request and never answers is the one failure this client had no answer for: the search
/// overlay spun forever, and only quitting escaped. §18.1 measured a healthy search answering in milliseconds, so
/// "still waiting after a minute" is not a slow search — it is a wedged child.
#[tokio::test]
async fn a_child_that_never_answers_is_given_up_on() {
    let (_dir, client) = spawn(json!({"silent": ["search"], "replies": {"scan": {"added": 0, "notes": 0}}})).await;

    let started = std::time::Instant::now();
    let fault = client
        .request_within("search", serde_json::json!({"workspace": "/tmp/vault", "query": "x"}), Some(std::time::Duration::from_millis(300)))
        .await
        .expect_err("waiting forever is not an outcome");
    assert!(fault.message.contains("did not answer"), "{fault:?}");
    assert!(fault.message.contains("search"), "and names which request gave up: {fault:?}");
    assert!(started.elapsed() < std::time::Duration::from_secs(5), "gave up rather than hung");

    // The child is still there and still serving: what timed out was one request, not the connection. Killing it
    // would be a worse answer — np may be heads-down on something else entirely.
    let later = client
        .request_within("scan", serde_json::json!({"workspace": "/tmp/vault"}), Some(std::time::Duration::from_secs(10)))
        .await;
    assert!(later.is_ok(), "the next request on the same child still works: {later:?}");
}

/// Which calls are allowed to take how long is a policy, and policy belongs where a test can read it. Scan and
/// embed deliberately have no deadline: embedding a large vault legitimately takes a long time, and killing that
/// would lose real work to protect against a stall the drain cap already bounds at close.
#[test]
fn prompt_calls_have_a_deadline_and_long_ones_do_not() {
    use jott_core::np::client::deadline_for;
    for method in ["initialize", "models.list", "status.get"] {
        assert!(deadline_for(method).is_some(), "{method} is supposed to answer promptly");
    }
    assert!(deadline_for("search").is_some(), "a spinning search overlay is what people report as a frozen app");
    assert_eq!(deadline_for("scan"), None, "an index pass on a big vault is not a stall");
    assert_eq!(deadline_for("embed"), None, "and neither is embedding one");
}
