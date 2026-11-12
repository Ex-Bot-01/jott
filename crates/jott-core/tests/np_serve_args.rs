//! The bug this file exists for: Jott ran np with no subcommand.
//!
//! `np` on its own prints its usage to stderr, exits 2, and never reads stdin — so the shipped installer
//! produced a window saying "np ended before answering", dressed up by our own hint as a version problem.
//! Every protocol test in this repository talks to `tests/fake-np/np.py`, which speaks NDJSON with any
//! arguments at all, so 190 green tests could not see it. These two do: the stand-in is told to have real
//! np's manners, and then only a client that says `serve` gets an answer.

mod common;

use common::spawn;
use jott_core::np::client::NpClient;
use jott_core::np::handshake::{check_version, parse_initialize};
use jott_core::np::resolve::Os;
use serde_json::json;

#[tokio::test]
async fn np_is_told_to_serve_before_anything_is_asked() {
    let (_dir, client) = spawn(json!({
        "require_args": ["serve"],
        "initialize": {
            "data_dir": "/fake/data",
            "features": {"fetch": true, "local_model": true, "vision": false},
            "methods": ["initialize"],
            "np": "0.3.1",
            "protocol": 1
        }
    }))
    .await;

    let answer = client
        .request("initialize", json!({ "notifications": false }))
        .await
        .expect("a child started with `serve` answers initialize");
    assert_eq!(answer["np"], "0.3.1");
}

#[tokio::test]
async fn what_np_said_on_stderr_travels_with_the_failure() {
    // The alternative is the report we got: "np ended before answering", with no clue that np had printed a
    // whole sentence explaining itself two feet away.
    let (_dir, client) = spawn(json!({
        "die_with_stderr": "error: could not open the index: disk quota exceeded\n",
        "die_code": 3
    }))
    .await;

    let fault = client
        .request("initialize", json!({ "notifications": false }))
        .await
        .expect_err("a child that exits without answering owes a fault, not silence");
    assert!(
        fault.message.contains("disk quota exceeded"),
        "np's own words are missing: {}",
        fault.message
    );
    assert!(fault.message.starts_with("np ended before answering"), "{}", fault.message);
}

#[tokio::test]
async fn a_child_that_never_answers_is_not_reported_as_an_old_np() {
    use jott_core::ipc::NpGone;

    let payload = NpGone::Fault("np ended before answering".into()).payload(Os::Windows);
    assert!(!payload.message.contains("too old"), "{}", payload.message);
    assert!(
        !payload.install_hint.to_lowercase().contains("too old"),
        "the hint blames a version nobody measured: {}",
        payload.install_hint
    );
    // It still has to say what to do next, not just that something failed.
    assert!(payload.install_hint.contains("About"), "{hint}", hint = payload.install_hint);
}

/// The same question asked of the real binary, when one is installed.
///
/// `JOTT_REAL_NP=/usr/local/bin/np cargo test -p jott-core --test np_serve_args -- --ignored`
///
/// Without the variable it says so and passes: CI has no np, and a test suite must not start downloading one
/// to prove a client works. Run it yourself before a release — skipping it is exactly what shipped 0.0.3
/// through 0.0.5 with a broken bridge.
#[tokio::test]
async fn the_real_np_answers_when_told_to_serve() {
    let Some(path) = std::env::var_os("JOTT_REAL_NP") else {
        eprintln!("JOTT_REAL_NP unset: skipping the real-binary check");
        return
    };
    let client = NpClient::spawn(std::path::Path::new(&path)).await.expect("real np starts");
    let answer = client.request("initialize", json!({ "notifications": true })).await.expect("real np answers");
    let info = parse_initialize(&answer).expect("and its handshake document is one we understand");
    check_version(&info.version).unwrap_or_else(|why| panic!("np {} refused: {why}", info.version));
    assert!(info.methods.contains(&"search".to_string()), "methods: {:?}", info.methods);
}
