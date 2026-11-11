//! Typed calls to np, and the rule they exist for: `locked`, `offline` and a configuration
//! refusal are **states in a result**, so they arrive as variants — never as an error the UI
//! would render as a failure (spec §5, §11). Every document here is np 0.3.1's own, taken from
//! the captured fixture or from a request this build box made against the released binary.

mod common;

use common::{fixture_result, spawn, vault};
use jott_core::np::methods::{embed, initialize, models_list, scan, search};
use jott_core::np::outcomes::{EmbedOutcome, Mode, ModelState, ScanOutcome};
use jott_core::np::{NpError, NpMissing};
use serde_json::json;
use std::path::Path;

// ---- scan: one state, one report.

#[tokio::test]
async fn a_locked_scan_is_a_state_and_not_a_fault() {
    // np's contention document, verbatim in shape: `LockFailure::document` in np-cli builds
    // exactly these three keys and nothing else — no counts to read as zeros.
    let (_d, client) = spawn(json!({"replies": {
        "scan": {"command": "scan", "locked": true, "waited_ms": 2000}
    }}))
    .await;

    match scan(&client, &vault()).await {
        Ok(ScanOutcome::Locked { waited_ms }) => assert_eq!(waited_ms, 2000),
        Ok(other) => panic!("contention came back as {other:?}, which the UI would render as a result"),
        Err(fault) => panic!("contention came back as a fault: {fault}"),
    }
    client.shutdown().await.unwrap();
}

#[tokio::test]
async fn a_scan_report_carries_nps_counts_and_its_nulls() {
    let (_d, client) = spawn(json!({"replies": {"scan": fixture_result(2)}})).await;

    let ScanOutcome::Applied(report) = scan(&client, &vault()).await.unwrap() else {
        panic!("an ordinary scan is not a locked one");
    };
    assert_eq!((report.added, report.modified, report.moved, report.removed, report.unchanged), (0, 0, 0, 0, 2));
    assert_eq!(report.notes, 2);
    assert_eq!(report.elapsed_ms, 18, "np's own elapsed figure, not a measured-by-us one");
    assert_eq!(report.index_files, ["INDEX.md"]);
    assert_eq!(report.unresolved_links, 1);
    assert_eq!(report.unresolved_targets, ["missing-note"]);
    assert!(report.skipped.is_empty());
    // Present-and-null stays None: "this scan lost nothing" and "this build said nothing" are
    // np's distinction to keep, and a `Some(String::new())` would erase it.
    assert_eq!(report.unindexable, None);
    assert_eq!(report.upgrade_orphans, None);
    assert_eq!(report.merged_notes, None);
    assert_eq!(report.recorded_merged_notes.as_deref(), Some("merged=0;paths="));
    client.shutdown().await.unwrap();
}

#[tokio::test]
async fn a_result_that_is_not_shaped_like_one_is_a_fault_not_a_zero() {
    let (_d, client) = spawn(json!({"replies": {"scan": {"nonsense": true}}})).await;

    let fault = scan(&client, &vault()).await.unwrap_err();
    assert_eq!(fault.code, -32603);
    assert!(
        fault.message.contains("added"),
        "the message should name what is missing, got: {fault}"
    );
    client.shutdown().await.unwrap();
}

// ---- embed: five states, none of them an error dialog.

#[tokio::test]
async fn an_embed_refused_by_configuration_keeps_nps_hint_and_model() {
    let (_d, client) = spawn(json!({"replies": {"embed": fixture_result(4)}})).await;

    let EmbedOutcome::Disabled { key, model_id, hint } = embed(&client, &vault()).await.unwrap() else {
        panic!("np's configuration-disabled is its own state");
    };
    assert_eq!(key.as_deref(), Some("embeddings.model"));
    assert_eq!(model_id.as_deref(), Some("embeddinggemma-2"));
    let hint = hint.expect("this arm names a command that exists");
    assert!(hint.contains("np models install"), "{hint}");
    client.shutdown().await.unwrap();
}

#[tokio::test]
async fn a_switched_off_provider_gets_no_hint_invented_for_it() {
    // Measured against the released 0.3.1 with `provider = "none"`: the refusal carries a key
    // and an id and no hint — a switch thrown on purpose is not an invitation to download.
    let mut report = fixture_result(4);
    report["outcome"] = json!({"configuration-disabled": {"id": "none", "key": "embeddings.provider"}});
    report["model"] = json!(null);
    let (_d, client) = spawn(json!({"replies": {"embed": report}})).await;

    let EmbedOutcome::Disabled { key, model_id, hint } = embed(&client, &vault()).await.unwrap() else {
        panic!("provider-disabled is configuration-disabled");
    };
    assert_eq!(key.as_deref(), Some("embeddings.provider"));
    assert_eq!(model_id.as_deref(), Some("none"));
    assert_eq!(hint, None, "np sent no command, so we write none");
    client.shutdown().await.unwrap();
}

#[tokio::test]
async fn an_embed_that_owed_nothing_is_applied_with_its_counts() {
    let mut report = fixture_result(4);
    report["outcome"] = json!("current");
    report["embedded"] = json!(3);
    report["current"] = json!(7);
    report["chunks_seen"] = json!(7);
    report["image_refusals"] = json!([["notes/diagram.svg", "unsupported-format"]]);
    let (_d, client) = spawn(json!({"replies": {"embed": report}})).await;

    let EmbedOutcome::Applied(done) = embed(&client, &vault()).await.unwrap() else {
        panic!("outcome `current` is work finished");
    };
    assert_eq!(done.embedded, 3);
    assert_eq!(done.current, 7);
    assert_eq!(done.chunks_seen, 7);
    assert_eq!(done.verification, "self-consistent, not verified");
    assert_eq!(done.model.as_deref(), Some("embeddinggemma-2"));
    assert!(!done.offline);
    assert_eq!(
        done.image_refusals,
        vec![jott_core::np::outcomes::ImageRefusal {
            path: "notes/diagram.svg".into(),
            class: "unsupported-format".into()
        }],
        "np names per-file image refusals, and §18.5 says the per-file why comes from here"
    );
    client.shutdown().await.unwrap();
}

#[tokio::test]
async fn an_offline_embed_names_the_model_it_could_not_fetch() {
    let mut report = fixture_result(4);
    report["outcome"] = json!({"offline": {"id": "embeddinggemma-2"}});
    report["offline"] = json!(true);
    let (_d, client) = spawn(json!({"replies": {"embed": report}})).await;

    match embed(&client, &vault()).await.unwrap() {
        EmbedOutcome::Offline { model_id } => assert_eq!(model_id.as_deref(), Some("embeddinggemma-2")),
        other => panic!("offline is its own state, got {other:?}"),
    }
    client.shutdown().await.unwrap();
}

#[tokio::test]
async fn an_absent_artifact_keeps_np_own_command_and_is_never_applied() {
    let mut report = fixture_result(4);
    report["outcome"] = json!({"artifact-absent": {
        "model": "embeddinggemma-2",
        "artifact": "/home/agent/.local/share/workspaces/abc.models/embeddinggemma-2",
        "hint": "install it with `np models install /vault --id embeddinggemma-2 --path <file> --dim 512 --sha256 <digest>`"
    }});
    let (_d, client) = spawn(json!({"replies": {"embed": report}})).await;

    match embed(&client, &vault()).await.unwrap() {
        EmbedOutcome::Refused { state, detail, model_id } => {
            assert_eq!(state, "artifact-absent", "np's own label, kept verbatim");
            assert_eq!(model_id.as_deref(), Some("embeddinggemma-2"));
            assert!(detail.unwrap().contains("np models install"));
        }
        other => panic!("nothing was embedded, so this cannot be Applied: {other:?}"),
    }
    client.shutdown().await.unwrap();
}

#[tokio::test]
async fn an_outcome_this_build_does_not_know_still_arrives_as_a_state() {
    // np adds arms (F40 added one to 0.3.x). An unknown label must not become Applied — that
    // would claim vectors for a run that reported it stopped short.
    let mut report = fixture_result(4);
    report["outcome"] = json!({"quantum-drift": {"id": "embeddinggemma-2"}});
    let (_d, client) = spawn(json!({"replies": {"embed": report}})).await;

    match embed(&client, &vault()).await.unwrap() {
        EmbedOutcome::Refused { state, model_id, .. } => {
            assert_eq!(state, "quantum-drift");
            assert_eq!(model_id.as_deref(), Some("embeddinggemma-2"));
        }
        other => panic!("an arm we have never seen is not a success: {other:?}"),
    }
    client.shutdown().await.unwrap();
}

// ---- search: the four provenance keys stay together.

#[tokio::test]
async fn a_keyword_only_search_keeps_mode_vectors_and_reason_together() {
    let (_d, client) = spawn(json!({"replies": {"search": fixture_result(3)}})).await;

    let answer = search(&client, &vault(), "hash", 8, Mode::Hybrid).await.unwrap();
    assert_eq!(answer.mode, Mode::Keyword, "np answered which half ran, in `mode`");
    assert!(!answer.vectors);
    assert!(!answer.offline);
    assert_eq!(answer.reason, None);
    assert_eq!(answer.hits.len(), 1);
    let hit = &answer.hits[0];
    assert_eq!(hit.path, "projects/timeline.md");
    assert_eq!(hit.kind, "note");
    assert_eq!(hit.snippet.as_deref(), Some("# Timeline\n\nMoved files are detected by content [hash].\n"));
    assert_eq!(hit.cosine, None, "the vector half did not run — present-and-null, not 0.0");
    assert!(hit.bm25.unwrap() < 0.0);
    client.shutdown().await.unwrap();
}

#[tokio::test]
async fn a_search_that_names_why_a_half_was_missing_says_so() {
    // Measured against released 0.3.1 with `provider = "none"`: mode stays what was asked,
    // vectors is false, and `reason` is the token §10's status line branches on.
    let (_d, client) = spawn(json!({"replies": {"search": {
        "hits": [], "mode": "hybrid", "vectors": false, "offline": false, "fetched": null,
        "note": "embeddings are switched off (`embeddings.provider = \"none\"`): ranked by keyword only",
        "reason": "provider-disabled"
    }}}))
    .await;

    let answer = search(&client, &vault(), "anything", 8, Mode::Hybrid).await.unwrap();
    assert_eq!(answer.mode, Mode::Hybrid);
    assert!(!answer.vectors);
    assert_eq!(answer.reason.as_deref(), Some("provider-disabled"));
    assert!(answer.hits.is_empty());
    client.shutdown().await.unwrap();
}

#[tokio::test]
async fn a_semantic_only_hit_stays_distinguishable_from_a_zero_bm25() {
    let (_d, client) = spawn(json!({"replies": {"search": {
        "hits": [
            {"path": "a.md", "kind": "note", "score": 0.82, "bm25": null, "cosine": 0.82,
             "keyword_rank": null, "semantic_rank": 1, "snippet": null,
             "linked_from": 0, "linked_notes": [], "source_note": null},
            {"path": "b.md", "kind": "note", "score": 0.0, "bm25": 0.0, "cosine": 0.0,
             "keyword_rank": 2, "semantic_rank": null, "snippet": "here",
             "linked_from": 0, "linked_notes": [], "source_note": null}
        ],
        "mode": "hybrid", "vectors": true, "offline": false, "fetched": null, "note": null, "reason": null
    }}}))
    .await;

    let answer = search(&client, &vault(), "meaning", 8, Mode::Hybrid).await.unwrap();
    assert_eq!(answer.hits[0].bm25, None, "a hit only the vector list held has no bm25 at all");
    assert_eq!(answer.hits[0].snippet, None);
    assert_eq!(answer.hits[1].bm25, Some(0.0), "a real zero is a number np computed");
    assert_eq!(answer.hits[1].cosine, Some(0.0));
    client.shutdown().await.unwrap();
}

#[tokio::test]
async fn a_newline_in_a_snippet_does_not_split_the_frame() {
    // np's snippets carry the note's line breaks (§5), so one response line holds `\n` as an
    // escape. One raw newline in that line would desynchronise every later response.
    let (_d, client) = spawn(json!({"replies": {
        "search": {"hits": [
            {"path": "a.md", "kind": "note", "score": 1.0, "bm25": -0.5, "cosine": null,
             "keyword_rank": 1, "semantic_rank": null, "snippet": "one\ntwo\nthree",
             "linked_from": 0, "linked_notes": [], "source_note": null}
        ], "mode": "keyword", "vectors": false, "offline": false, "fetched": null, "note": null, "reason": null},
        "scan": fixture_result(2)
    }}))
    .await;

    let answer = search(&client, &vault(), "one\ntwo", 5, Mode::Keyword).await.unwrap();
    assert_eq!(answer.hits[0].snippet.as_deref(), Some("one\ntwo\nthree"));
    // The stream is still aligned: the next request gets its own answer, not the tail of that one.
    assert!(matches!(scan(&client, &vault()).await.unwrap(), ScanOutcome::Applied(_)));
    client.shutdown().await.unwrap();
}

// ---- models.list: the consent gate's only source of "is the artifact here".

#[tokio::test]
async fn models_list_reports_each_row_and_where_its_artifact_stands() {
    // The released 0.3.1 row shape, eight keys (`model_json` in np-cli).
    let (_d, client) = spawn(json!({"replies": {"models.list": {
        "models": [{"id": "embeddinggemma-2", "dim": 512, "sha256": "a".repeat(64),
                    "prefix_doc": "doc: ", "prefix_query": "q: ", "installed_at": 1_762_000_000,
                    "state": "present", "members": []}],
        "fetching": [], "unclaimed": [], "models_dir_error": null
    }}}))
    .await;

    let listing = models_list(&client, &vault()).await.unwrap();
    assert_eq!(listing.rows.len(), 1);
    let row = &listing.rows[0];
    assert_eq!(row.id, "embeddinggemma-2");
    assert_eq!(row.dim, 512);
    assert_eq!(row.state, ModelState::Present);
    assert_eq!(row.installed_at, 1_762_000_000);
    assert_eq!(listing.models_dir_error, None);
    assert!(listing.artifact_present("embeddinggemma-2"));
    assert!(!listing.artifact_present("some-other-model"));
    client.shutdown().await.unwrap();
}

#[tokio::test]
async fn an_unreadable_models_directory_is_not_reported_as_an_empty_shelf() {
    // np keeps this a result with `models_dir_error` filled rather than a fault. Answering
    // "no rows" here would ask the user to download 295 MiB they already have (§18.6).
    let (_d, client) = spawn(json!({"replies": {"models.list": {
        "models": [], "fetching": [], "unclaimed": [],
        "models_dir_error": "cannot list /home/agent/.local/share/models: Permission denied (os error 13)"
    }}}))
    .await;

    let listing = models_list(&client, &vault()).await.unwrap();
    assert!(listing.rows.is_empty());
    let err = listing.models_dir_error.expect("np's sentence about the directory");
    assert!(err.contains("Permission denied"), "{err}");
    client.shutdown().await.unwrap();
}

#[tokio::test]
async fn a_model_state_this_build_has_never_seen_keeps_np_label() {
    let row = json!({"id": "m", "dim": 8, "sha256": null, "prefix_doc": "", "prefix_query": "",
                     "installed_at": 1, "state": "reconstituted", "members": []});
    let (_d, client) = spawn(json!({"replies": {"models.list": {
        "models": [row], "fetching": [], "unclaimed": [], "models_dir_error": null
    }}}))
    .await;

    let listing = models_list(&client, &vault()).await.unwrap();
    assert_eq!(listing.rows[0].state, ModelState::Other("reconstituted".into()));
    client.shutdown().await.unwrap();
}

// ---- faults: np's sentence is the useful part, so it travels verbatim.

#[tokio::test]
async fn a_fault_for_an_unknown_method_keeps_nps_list_of_what_it_answers() {
    let (_d, client) = spawn(json!({"faults": {"models.list": {
        "code": -32601,
        "message": "unknown method `links.list`: this build answers status.get, initialize, search, scan, embed, models.list, shutdown"
    }}}))
    .await;

    let fault = models_list(&client, &vault()).await.unwrap_err();
    assert_eq!(fault.code, -32601);
    assert_eq!(
        fault.message,
        "unknown method `links.list`: this build answers status.get, initialize, search, scan, embed, models.list, shutdown"
    );
    client.shutdown().await.unwrap();
}

#[tokio::test]
async fn both_flavours_of_bad_params_keep_their_own_sentence() {
    let (_d, client) = spawn(json!({"faults": {
        "search": {"code": -32602, "message": "unknown mode `telepathy`: write hybrid, semantic or keyword"},
        "scan": {"code": -32602, "message": "params need a \"query\" string"}
    }}))
    .await;

    let mode = search(&client, &vault(), "q", 5, Mode::Keyword).await.unwrap_err();
    assert_eq!((mode.code, mode.message.as_str()), (-32602, "unknown mode `telepathy`: write hybrid, semantic or keyword"));
    let query = scan(&client, &vault()).await.unwrap_err();
    assert_eq!((query.code, query.message.as_str()), (-32602, "params need a \"query\" string"));
    client.shutdown().await.unwrap();
}

// ---- initialize: the handshake, asked for properly.

#[tokio::test]
async fn initialize_names_the_workspace_and_asks_for_progress() {
    // The stand-in refuses unless both params arrive (§18.6: workspace is a request param,
    // never config) and reports progress support only when asked for it, as real np does.
    let (_d, client) = spawn(json!({
        "initialize_requires": ["workspace", "notifications"],
        "initialize_progress_by_param": true
    }))
    .await;

    let info = initialize(&client, Path::new("/fake/vault"), true).await.unwrap();
    assert_eq!(info.version, "0.3.1");
    assert_eq!(info.protocol, 1);
    assert!(info.progress, "the connection asked for notifications and np said yes");
    assert_eq!(info.data_dir, "/fake/data", "the path the consent prompt names");
    assert!(info.has_method("models.list"));
    client.shutdown().await.unwrap();
}

#[tokio::test]
async fn initialize_without_notifications_promises_no_progress() {
    let (_d, client) = spawn(json!({
        "initialize_requires": ["workspace", "notifications"],
        "initialize_progress_by_param": true
    }))
    .await;

    let info = initialize(&client, Path::new("/fake/vault"), false).await.unwrap();
    assert!(!info.progress, "a connection that did not ask must not be promised a status line");
    client.shutdown().await.unwrap();
}

#[tokio::test]
async fn initialize_refuses_a_build_below_the_floor() {
    let (_d, client) = spawn(json!({"initialize": {
        "np": "0.2.3", "protocol": 1, "data_dir": "/fake/data",
        "features": {"fetch": true, "local_model": true, "vision": false},
        "methods": ["initialize", "search", "scan", "embed", "models.list", "shutdown", "status.get"]
    }}))
    .await;

    match initialize(&client, &vault(), true).await {
        Err(NpError::Missing(NpMissing::TooOld { found })) => assert_eq!(found, "0.2.3"),
        other => panic!("0.2.3 is below the floor, got {other:?}"),
    }
    client.shutdown().await.unwrap();
}
