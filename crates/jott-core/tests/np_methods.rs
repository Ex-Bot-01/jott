//! Typed calls to np, and the rule they exist for: `locked`, `offline` and a configuration
//! refusal are **states in a result**, so they arrive as variants — never as an error the UI
//! would render as a failure (spec §5, §11). Every document here is np 0.3.1's own, taken from
//! the captured fixture or from a request this build box made against the released binary.

mod common;

use common::{fixture_result, spawn, vault};
use jott_core::np::methods::{embed, embeddings_disable, embeddings_enable, initialize, models_list, scan, search};
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
    // `-32603` used to live here, borrowed from np's internal-error code on the reasoning that the honest reading was
    // the same. It is not: `IpcError::np_fault` maps that code to `NpGone`, "go install np", so a report we could not
    // read told someone to install a program that had just answered them (issue #11). Jott's own code now carries
    // the shape faults, and this pin is what keeps the two from collapsing back together.
    assert_eq!(fault.code, jott_core::np::SHAPE_FAULT_CODE, "ours, not np's: {fault}");
    assert_ne!(fault.code, -32603, "that one means nothing came back at all");
    assert!(jott_core::np::is_shape_fault(&fault));
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

    let answer = search(&client, &vault(), "hash", 8, Mode::Hybrid, false).await.unwrap();
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

    let answer = search(&client, &vault(), "anything", 8, Mode::Hybrid, false).await.unwrap();
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

    let answer = search(&client, &vault(), "meaning", 8, Mode::Hybrid, false).await.unwrap();
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

    let answer = search(&client, &vault(), "one\ntwo", 5, Mode::Keyword, false).await.unwrap();
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

    let mode = search(&client, &vault(), "q", 5, Mode::Keyword, false).await.unwrap_err();
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
        // No scenario keys: progress is opt-in on every connection now, which is the default behaviour.
        "initialize_requires": ["workspace", "notifications"]
    }))
    .await;

    let info = initialize(&client, Path::new("/fake/vault"), true).await.unwrap();
    // The stand-in emulates the np this repo pins; if it ever answers with something Jott would refuse, every
    // protocol test here would be testing a backend nobody ships.
    jott_core::np::handshake::check_version(&info.version).unwrap_or_else(|why| panic!("stand-in np {} refused: {why}", info.version));
    assert_eq!(info.protocol, 1);
    assert!(info.progress, "the connection asked for notifications and np said yes");
    assert_eq!(info.data_dir, "/fake/data", "the path the consent prompt names");
    assert!(info.has_method("models.list"));
    client.shutdown().await.unwrap();
}

#[tokio::test]
async fn initialize_without_notifications_promises_no_progress() {
    let (_d, client) = spawn(json!({
        // No scenario keys: progress is opt-in on every connection now, which is the default behaviour.
        "initialize_requires": ["workspace", "notifications"]
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

// ---- np 0.4.0: an answer says what ran, whether there was anything to run against, and what a download costs ----

/// Filed as note-processor#5 and fixed in 0.4.0: `indexed`, `keyword_used` and `semantic_used` turn "no matches"
/// into three different facts. Measured off the released binary — a never-scanned folder answers `indexed: false`
/// with np's own sentence in `note`.
#[tokio::test]
async fn a_search_answer_says_whether_the_folder_was_ever_indexed() {
    let (_d, client) = spawn(json!({"replies": {"search": {
        "hits": [],
        "indexed": false,
        "keyword_used": true,
        "semantic_used": false,
        "mode": "hybrid",
        "vectors": false,
        "offline": false,
        "reason": "no-index",
        "note": "this workspace has never been scanned — run `np scan …`"
    }}}))
    .await;

    let answer = search(&client, &vault(), "kumquat", 8, Mode::Hybrid, false).await.unwrap();
    assert_eq!(answer.indexed, Some(false), "nothing matched and nothing *could* have: different sentences");
    assert_eq!(answer.keyword_used, Some(true));
    assert_eq!(answer.semantic_used, Some(false), "the meaning half did not run, whatever `mode` says");
    assert_eq!(answer.note.as_deref(), Some("this workspace has never been scanned — run `np scan …`"));
    client.shutdown().await.unwrap();
}

/// The same code still talks to 0.3.1, which sends none of those keys. Absent stays absent rather than becoming
/// `false`: a search box that claimed "not indexed" because a field was missing would be inventing a diagnosis (§4).
#[tokio::test]
async fn an_older_np_leaves_the_new_keys_absent_rather_than_false() {
    let (_d, client) = spawn(json!({"replies": {"search": fixture_result(3)}})).await;

    let answer = search(&client, &vault(), "hash", 8, Mode::Hybrid, false).await.unwrap();
    assert_eq!(answer.indexed, None);
    assert_eq!(answer.keyword_used, None);
    assert_eq!(answer.semantic_used, None);
    assert_eq!(answer.note, None);
    client.shutdown().await.unwrap();
}

/// note-processor#3: `models.list` carries `pinned`, priced from the sha256 table compiled into *this* build — so a
/// GUI can name the download before it exists without keeping a copy of the number in its own source.
#[tokio::test]
async fn models_list_prices_the_model_this_build_would_fetch() {
    // Both halves are what np 0.4.0 actually answered here: the row is a present model's, and `pinned` quotes the
    // configured model at the size compiled into that build.
    let (_d, client) = spawn(json!({"replies": {"models.list": {
        "models": [{
            "id": "embeddinggemma-2", "dim": 768,
            "sha256": "2188ac1deca4b77dffefd603c2776a9d76d9d74ec01841392982ebb840b09135",
            "installed_at": 1_794_647_641, "state": "present",
            "prefix_doc": "", "prefix_query": "", "members": []
        }],
        "pinned": [{ "id": "embeddinggemma-2", "bytes_total": 309_855_456, "members": ["main"] }],
        "fetching": [],
        "unclaimed": [],
        "models_dir_error": null
    }}}))
    .await;

    let listing = models_list(&client, &vault()).await.unwrap();
    let pinned = listing.pinned.first().unwrap_or_else(|| panic!("0.4.0 names a price: {:?}", listing.pinned));
    assert_eq!(pinned.id, "embeddinggemma-2");
    assert_eq!(pinned.bytes_total, Some(309_855_456), "the number the consent prompt should quote");
    assert_eq!(listing.price_of("embeddinggemma-2"), Some(309_855_456));
    assert_eq!(listing.price_of("some-other-model"), None, "a model np did not quote has no price, not a zero");
    client.shutdown().await.unwrap();
}

#[tokio::test]
async fn an_np_that_prices_nothing_leaves_the_price_unnamed() {
    // Measured against 0.4.0 with no config file and an empty data folder: np names nothing and quotes nothing.
    // Jott's own pin is then the only number a prompt can quote — see `builtin_default_model`.
    let (_d, client) = spawn(json!({"replies": {"models.list": {
        "models": [],
        "fetching": [],
        "unclaimed": [],
        "pinned": [],
        "models_dir_error": null
    }}}))
    .await;

    let listing = models_list(&client, &vault()).await.unwrap();
    assert!(listing.pinned.is_empty(), "an np that quotes nothing has no price — which is not the same as 0 bytes");
    assert_eq!(listing.price_of("embeddinggemma-2"), None);
    assert_eq!(listing.only_price(), None, "and there is no single entry to guess from");
    client.shutdown().await.unwrap();
}

/// np says what it fetched, in its own record: which model, and how many bytes each of its members was. That is the
/// only honest source for "model ready · 296 MiB" on a banner — Jott must not add up a download it did not watch. Two
/// member shapes matter: every member naming a size totals up; one without a size has no total at all, because half a
/// promise is worse than none.
#[tokio::test]
async fn an_embed_reports_the_model_it_fetched_and_what_that_cost() {
    let mut report = fixture_result(4);
    report["outcome"] = json!("current");
    // Measured against np 0.4.0's embed document, verbatim in shape:
    report["fetched"] = json!({"id": "embeddinggemma-2",
                               "members": [{"bytes": 309_855_456, "digest-verified": true, "name": "main"}],
                               "origin": "compiled-in-pin"});
    let (_d, client) = spawn(json!({"replies": {"embed": report}})).await;

    let EmbedOutcome::Applied(done) = embed(&client, &vault()).await.unwrap() else {
        panic!("outcome `current` is work finished");
    };
    assert_eq!(
        done.fetched,
        Some(jott_core::np::outcomes::FetchedRecord { id: "embeddinggemma-2".into(), bytes_total: Some(309_855_456) }),
        "the banner quotes np's numbers, not ours",
    );
}

#[tokio::test]
async fn a_fetch_that_named_no_sizes_totals_to_nothing() {
    let mut report = fixture_result(4);
    report["outcome"] = json!("current");
    // One member announces no size (np omits the key rather than guessing), and both carry np's digest verdict.
    report["fetched"] = json!({"id": "some-model",
                               "members": [{"name": "main", "digest-verified": true},
                                           {"bytes": 1_024, "name": "mmproj", "digest-verified": true}],
                               "origin": "compiled-in-pin"});
    let (_d, client) = spawn(json!({"replies": {"embed": report}})).await;

    let EmbedOutcome::Applied(done) = embed(&client, &vault()).await.unwrap() else {
        panic!("outcome `current` is work finished");
    };
    let fetched = done.fetched.expect("np said it fetched something");
    assert_eq!(fetched.id, "some-model");
    assert_eq!(fetched.bytes_total, None, "one member without a size means the total is unknown, not 1 KiB");
}

#[tokio::test]
async fn an_embed_that_fetched_nothing_says_so_by_being_null() {
    // The fixture's own document carries `"fetched": null` — the ordinary case, and the one that must not become an
    // empty record the window could read as "downloaded 0 bytes".
    let mut report = fixture_result(4);
    report["outcome"] = json!("current");
    let (_d, client) = spawn(json!({"replies": {"embed": report}})).await;

    let EmbedOutcome::Applied(done) = embed(&client, &vault()).await.unwrap() else {
        panic!("outcome `current` is work finished");
    };
    assert_eq!(done.fetched, None);
}

// ---- the two consent doors (note-processor#7). Every document below was answered by np 0.5.1 on this box.

/// "Allow the download" now has somewhere to go: np writes its own two keys and says which ones, where, and what the
/// fetch will cost. Jott's job is to pass that answer on, not to describe settings it did not write.
#[tokio::test]
async fn enabling_embeddings_reports_what_np_changed_and_what_it_costs() {
    // The stand-in appends `{"method":…, "params":…}` per call when a scenario names a record file, which is the only
    // way to see what Jott actually sent: a param dropped on the floor would consent to something else, silently.
    let record = std::env::temp_dir().join(format!("jott-enable-{}.ndjson", std::process::id()));
    let _ = std::fs::remove_file(&record);
    let (_d, client) = spawn(json!({
        "record": record.display().to_string(),
        "replies": {"embeddings.enable": {
        "changed": ["embeddings.provider", "embeddings.model"],
        "fetch": {"bytes_total": 309_855_456, "members": ["main"]},
        "model": "embeddinggemma-2",
        "persisted_to": "/home/you/.config/note-processor/config.toml",
        "provider": "local-model"
    }}}))
    .await;

    let answer = embeddings_enable(&client, "embeddinggemma-2").await.expect("np accepted the answer");
    assert_eq!(answer.changed, vec!["embeddings.provider".to_string(), "embeddings.model".to_string()]);
    assert_eq!(answer.provider.as_deref(), Some("local-model"));
    assert_eq!(answer.model.as_deref(), Some("embeddinggemma-2"));
    assert_eq!(answer.fetch_bytes, Some(309_855_456), "the price is np's number, not ours");
    assert_eq!(answer.persisted_to.as_deref(), Some(Path::new("/home/you/.config/note-processor/config.toml")));

    // …and it asked about the model we named.
    let lines = std::fs::read_to_string(&record).expect("the stand-in recorded its calls");
    let asked: Vec<serde_json::Value> = lines
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("one call per line"))
        .filter(|call: &serde_json::Value| call["method"] == json!("embeddings.enable"))
        .collect();
    assert_eq!(asked.len(), 1, "one answer, one call: {lines}");
    assert_eq!(asked[0]["params"]["model"], json!("embeddinggemma-2"), "{}", asked[0]);
    let _ = std::fs::remove_file(&record);

    client.shutdown().await.unwrap();
}

/// note-processor#11: a meanings-only search in a folder with no vectors answers `"result": null`. That is not a
/// malformed answer — it is an empty one with no words attached, and the difference decides what a person does next.
/// The generic shape fault would say np's reply was broken; the useful reading names the known cause and the way out.
#[tokio::test]
async fn a_search_answered_with_nothing_at_all_says_why_and_names_the_issue() {
    let (_d, client) = spawn(json!({"replies": {"search": null}})).await;

    let fault = search(&client, &vault(), "grandmother", 8, Mode::Semantic, false).await.expect_err("null is not an answer");
    let message = fault.message.to_lowercase();
    assert!(fault.message.contains("note-processor#11"), "the sentence should point at the open issue: {fault}");
    assert!(message.contains("vector"), "it should name the known cause: {fault}");
    assert!(
        !fault.message.contains("not shaped like one"),
        "calling np's mute answer malformed blames the wrong thing: {fault}"
    );
    // This sentence is quoted verbatim into the window's problem line, so it has to survive being read there. A
    // string continued across source lines by padding each one with spaces arrives as one long line of holes — and
    // `contains` checks cannot see that, because every word is still present.
    assert!(!fault.message.contains("  "), "the printed sentence has runs of padding in it: {fault:?}");
    assert!(!fault.message.contains('\n'), "one sentence, one line: {fault:?}");

    client.shutdown().await.expect("shutdown");
}

/// Revoking is not erasing: np switches the provider off and **keeps** which model was chosen, so saying no twice in a
/// row does not lose the choice someone made once. `changed` naming one key is that fact on the wire.
#[tokio::test]
async fn disabling_keeps_the_model_and_says_which_key_moved() {
    let (_d, client) = spawn(json!({"replies": {"embeddings.disable": {
        "changed": ["embeddings.provider"],
        "fetch": null,
        "model": "embeddinggemma-2",
        "persisted_to": "/home/you/.config/note-processor/config.toml",
        "provider": "none"
    }}}))
    .await;

    let answer = embeddings_disable(&client).await.expect("np took the answer back");
    assert_eq!(answer.changed, vec!["embeddings.provider".to_string()]);
    assert_eq!(answer.provider.as_deref(), Some("none"));
    assert_eq!(answer.model.as_deref(), Some("embeddinggemma-2"), "disable is not forget");
    assert_eq!(answer.fetch_bytes, None);

    client.shutdown().await.unwrap();
}

/// Measured on 0.5.1: enabling what is already enabled writes no bytes and answers `changed: []`. That is a success
/// with an empty list, and the card must be able to tell it apart from "nothing happened because we did not ask".
#[tokio::test]
async fn an_enable_that_changed_nothing_is_still_an_answer() {
    let (_d, client) = spawn(json!({"replies": {"embeddings.enable": {
        "changed": [], "fetch": null, "model": "embeddinggemma-2",
        "persisted_to": "/home/you/.config/note-processor/config.toml", "provider": "local-model"
    }}}))
    .await;

    let answer = embeddings_enable(&client, "embeddinggemma-2").await.expect("already on is not an error");
    assert!(answer.changed.is_empty(), "np wrote nothing: {answer:?}");
    assert_eq!(answer.provider.as_deref(), Some("local-model"));
    assert_eq!(answer.fetch_bytes, None, "no price quoted because no fetch is owed");

    client.shutdown().await.unwrap();
}

/// A config file np itself cannot read is left untouched — the refusal measured on 0.5.1 names the parse problem and
//  the path. Jott shows it byte for byte; a paraphrase would lose the line number that makes it fixable.
#[tokio::test]
async fn np_refuses_to_write_a_file_it_cannot_read_and_says_which() {
    let sentence = "config io: config parse error in /home/you/.config/note-processor/config.toml: TOML parse error at line 1, column 12";
    let (_d, client) = spawn(json!({"faults": {"embeddings.enable": {"code": -32000, "message": sentence}}}))
        .await;

    let Err(fault) = embeddings_enable(&client, "embeddinggemma-2").await else {
        panic!("an unloadable config must not look like consent recorded");
    };
    assert_eq!(fault.code, -32000);
    assert_eq!(fault.message, sentence, "np's sentence, unchanged: {}", fault.message);

    client.shutdown().await.unwrap();
}

/// An id this build does not pin is refused by name, naming the ids it does. Jott passes that through rather than
/// retrying with its own guess — the compiled-in default is not a fact about someone's binary.
#[tokio::test]
async fn an_unpinned_model_is_refused_with_the_names_np_will_accept() {
    let sentence = "config io: no pinned model \"no-such-model\": this build can name \"embeddinggemma-2\" \
        — enable one of those, or install bytes you already hold with `np models install`";
    let (_d, client) = spawn(json!({"faults": {"embeddings.enable": {"code": -32000, "message": sentence}}}))
        .await;

    let Err(fault) = embeddings_enable(&client, "no-such-model").await else {
        panic!("np refuses this, so Jott must not report success");
    };
    assert_eq!(fault.message, sentence);

    client.shutdown().await.unwrap();
}

/// `persisted_to` is optional on the wire. Absent means np did not say where — it does **not** mean "the file we
/// guessed", and the card has to be able to say "note-processor recorded it" without naming a path that may be wrong.
#[tokio::test]
async fn a_missing_persisted_to_stays_unknown() {
    let (_d, client) = spawn(json!({"replies": {"embeddings.enable": {
        "changed": ["embeddings.provider", "embeddings.model"], "fetch": null,
        "model": "embeddinggemma-2", "provider": "local-model"
    }}}))
    .await;

    let answer = embeddings_enable(&client, "embeddinggemma-2").await.expect("answer parsed");
    assert_eq!(answer.persisted_to, None, "silence is not a path");
    assert_eq!(answer.model.as_deref(), Some("embeddinggemma-2"));

    client.shutdown().await.unwrap();
}

/// The stand-in answers these two doors the way 0.5.1 does with no scenario at all, so a later test that only cares
/// about Jott's behaviour cannot accidentally configure a backend nobody ships.
#[tokio::test]
async fn the_stand_in_opens_the_door_without_being_told_to() {
    let (_d, client) = spawn(json!({})).await;

    let answer = embeddings_enable(&client, "embeddinggemma-2").await.expect("default behaviour");
    assert_eq!(answer.changed, vec!["embeddings.provider".to_string(), "embeddings.model".to_string()]);
    assert!(answer.provider_set(), "np switched embeddings on: {answer:?}");

    let back = embeddings_disable(&client).await.expect("and took it back");
    assert_eq!(back.changed, vec!["embeddings.provider".to_string()]);
    assert!(!back.provider_set());
    assert_eq!(back.model.as_deref(), Some("embeddinggemma-2"), "disable kept the choice");

    client.shutdown().await.unwrap();
}

/// A guard on the stand-in itself, asked raw: 0.5.1 refuses a parameter it cannot use instead of answering `ok`, and
/// a fixture that accepted typos would let Jott believe a consent answer had been recorded when np ignored it.
#[tokio::test]
async fn the_stand_in_refuses_a_parameter_np_would_refuse() {
    let (_d, client) = spawn(json!({})).await;

    let Err(fault) = client.request("embeddings.enable", json!({ "modle": "embeddinggemma-2" })).await else {
        panic!("a misspelled parameter must not be reported as consent recorded");
    };
    assert_eq!(fault.code, -32602, "{fault}");

    client.shutdown().await.unwrap();
}

// ---- prefix matching (note-processor#6). Measured against np 0.5.1 on a vault holding "grandmother":
// `grand` finds nothing until the request asks for word beginnings, and then np says which words it widened.

fn grand_note_reply(prefix: bool) -> serde_json::Value {
    if prefix {
        json!({"hits": [{"path": "a.md", "snippet": "\u{2026}my [grandmother] kept\u{2026}", "kind": "note",
                          "score": 1.1, "bm25": null, "cosine": 0.82, "matched_exactly": false}],
               "mode": "semantic", "vectors": true, "offline": false, "reason": null,
               "indexed": true, "keyword_used": true, "semantic_used": true,
               "prefix_used": true, "prefix_terms": ["grand"]})
    } else {
        json!({"hits": [], "mode": "hybrid", "vectors": true, "offline": false, "reason": null,
               "indexed": true, "keyword_used": true, "semantic_used": true,
               "prefix_used": false, "prefix_terms": []})
    }
}

/// The default answer says plainly that nothing was widened. Jott must be able to tell "np did not widen" from
/// "np did not say", because the first is a fact to print and the second is silence about an older build.
#[tokio::test]
async fn an_ordinary_search_says_nothing_was_widened() {
    let record = std::env::temp_dir().join(format!("jott-prefix-off-{}.ndjson", std::process::id()));
    let _ = std::fs::remove_file(&record);
    let (_d, client) = spawn(json!({
        "record": record.display().to_string(),
        "replies": {"search": grand_note_reply(false)}
    }))
    .await;

    let answer = search(&client, &vault(), "grand", 8, Mode::Hybrid, false).await.expect("np answered");
    assert_eq!(answer.prefix_used, Some(false), "np said it did not widen");
    assert_eq!(answer.prefix_terms, Vec::<String>::new());

    // The parameter went out. A `prefix` that is never sent is the whole feature missing with every assertion above
    // still passing, which is precisely why this test reads the wire rather than trusting the call site.
    let lines = std::fs::read_to_string(&record).unwrap();
    let sent: Vec<serde_json::Value> = lines
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("one call per line"))
        .collect();
    let search = sent.iter().find(|call| call["method"] == json!("search")).expect("a search was recorded");
    assert_eq!(search["params"]["prefix"], json!(false), "{search}");

    let _ = std::fs::remove_file(&record);
    client.shutdown().await.unwrap();
}

/// With the flag on, np widens and **says** which words it widened. `prefix_terms` is what lets the window say
/// "`grand` → grandmother" instead of claiming the note matched the typing.
#[tokio::test]
async fn a_widened_search_names_the_words_np_used() {
    let record = std::env::temp_dir().join(format!("jott-prefix-on-{}.ndjson", std::process::id()));
    let _ = std::fs::remove_file(&record);
    let (_d, client) = spawn(json!({
        "record": record.display().to_string(),
        "replies": {"search": grand_note_reply(true)}
    }))
    .await;

    let answer = search(&client, &vault(), "grand", 8, Mode::Hybrid, true).await.expect("np widened");
    assert_eq!(answer.prefix_used, Some(true));
    assert_eq!(answer.prefix_terms, vec!["grand".to_string()]);
    assert_eq!(answer.hits.len(), 1);
    assert_eq!(answer.hits[0].matched_exactly, Some(false), "this row was reached by a word beginning");

    let lines = std::fs::read_to_string(&record).unwrap();
    let search = lines
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("one call per line"))
        .find(|call| call["method"] == json!("search"))
        .expect("a search was recorded");
    assert_eq!(search["params"]["prefix"], json!(true), "{search}");

    let _ = std::fs::remove_file(&record);
    client.shutdown().await.unwrap();
}

/// A hit with no `matched_exactly` key has never been word-tested — a vector-only hit. `None`, not `true`: reading
/// absence as exactness would mark a meaning-match as a word match, which is the same lie in the other direction.
#[tokio::test]
async fn a_hit_np_never_word_tested_stays_unknown() {
    let (_d, client) = spawn(json!({"replies": {"search": json!({
        "hits": [{"path": "b.md", "snippet": null, "kind": "note", "score": 0.6, "bm25": null, "cosine": 0.71}],
        "mode": "semantic", "vectors": true, "offline": false, "reason": null
    })}}))
    .await;

    let answer = search(&client, &vault(), "grand", 8, Mode::Semantic, false).await.expect("answered");
    assert_eq!(answer.hits[0].matched_exactly, None, "absence is not exactness");
    // An np that predates the flag answers nothing about widening — and that stays unknown rather than "did not".
    assert_eq!(answer.prefix_used, None, "silence from an older np is not a No");
    assert!(answer.prefix_terms.is_empty());

    client.shutdown().await.unwrap();
}

/// `Mode::LABELS` and `Mode::label` are two halves of one contract, and `from_label` is the third: the list is what a
/// refusal offers as alternatives, so a variant added to `label()` but not to `LABELS` would leave the sentence
/// silently omitting a mode the app can actually use. Nothing else reads those three together, hence this.
#[test]
fn every_mode_is_listed_where_a_refusal_lists_them() {
    let modes = [Mode::Keyword, Mode::Hybrid, Mode::Semantic];
    let labels: Vec<&str> = modes.iter().map(Mode::label).collect();
    assert_eq!(Mode::LABELS.to_vec(), labels, "the list a refusal prints must name every mode in its own order");
    for label in Mode::LABELS {
        let parsed = Mode::from_label(label).unwrap_or_else(|| panic!("{label} is listed but cannot be parsed back"));
        assert_eq!(parsed.label(), label, "{label} parses into a different mode than it spells");
    }
    // And the door stays shut on anything else: `None`, not a quiet default.
    for junk in ["", "fuzzy", "HYBRID", "auto"] {
        assert!(Mode::from_label(junk).is_none(), "{junk} is not a mode and must not parse");
    }
}

/// Issue #11: one renamed counter in a future np's scan answer. The document is present, parseable, and shorter
/// than expected — which used to be reported under `-32603`, the code that means *no answer came*. That mapping
/// told a person to install a np that was sitting there answering their requests, while indexing stopped for the
/// session. `ScanReport` stays strict on purpose (a `{}` must not parse as an all-zero scan and mark every note
/// indexed on the strength of nothing); what changed is which sentence comes out, and where it points.
#[tokio::test]
async fn a_scan_answer_missing_one_counter_says_the_answer_was_unreadable() {
    let mut short = fixture_result(2);
    assert!(short.as_object_mut().expect("an object").remove("modified").is_some(), "the fixture carries the key this test removes");

    let (_d, client) = spawn(json!({ "replies": { "scan": short } })).await;
    let fault = scan(&client, &vault()).await.expect_err("a report missing a counter is not a report");

    assert!(jott_core::np::is_shape_fault(&fault), "this is Jott failing to read an answer: {fault}");
    assert_ne!(fault.code, -32603, "`-32603` means nothing came back; something did: {fault}");
    assert!(fault.message.contains("modified"), "the sentence names the key — the whole answer to \"what did np change?\": {fault}");

    // The half that reaches a person. `NpGone` says *go install np*; np is running.
    let error = jott_core::ipc::IpcError::np_fault(&fault);
    assert_eq!(error.kind, jott_core::ipc::ErrorKind::NpShape, "an unreadable answer is its own kind: {error:?}");

    // And the case that *is* np being gone keeps its own sentence, so the two cannot collapse back together.
    let gone = jott_core::ipc::IpcError::np_fault(&jott_core::np::NpFault { code: -32603, message: "np died mid-request".into() });
    assert_eq!(gone.kind, jott_core::ipc::ErrorKind::NpGone, "\"no answer came\" still means that: {gone:?}");
}

// ---- np 0.6.0: two keys appear, and one strict parser reads both releases (issue: the version bump)

/// The `scan` answer measured from the published `np-0.6.0-x86_64-unknown-linux-gnu` binary on a vault of three
/// notes and two attachments (`plan.org`, `scratch.txt`), recorded here rather than copied from the release page:
///
/// ```text
/// {"added":5,"attachment_examples":["plan.org","scratch.txt"],"attachments":2,"notes":3,…}
/// ```
///
/// `ScanReport` is deliberately strict about the eleven fields Jott reads — issue #11 kept it that way, because a
/// `{}` must not parse as an all-zero scan and mark every note indexed on the strength of nothing. What makes this
/// safe is the *other* half of that shape: no `deny_unknown_fields`, so a key Jott has never heard of is silence
/// rather than a fault. That tolerance is one-directional, and this test is what pins which direction it runs in —
/// an added key is accepted today; a renamed one would still fault, now as `NpShape` rather than as "np is missing".
#[tokio::test]
async fn a_scan_answer_from_np_0_6_0_parses_and_the_new_keys_are_ignored() {
    let measured_on_0_6_0 = json!({
        "added": 5, "elapsed_ms": 23, "index_files": ["INDEX.md"], "merged_notes": null, "modified": 0, "moved": 0,
        "notes": 3, "recorded_merged_notes": "merged=0;paths=",
        "recorded_upgrade_orphans": "headings=0,tags=0,items=0,links_src=0,links_dst_null=0",
        "removed": 0, "skipped": [], "unchanged": 5, "unindexable": null,
        "unresolved_links": 1, "unresolved_targets": ["missing.md"], "upgrade_orphans": null,
        "attachments": 2, "attachment_examples": ["plan.org", "scratch.txt"],
    });

    let (_d, client) = spawn(json!({ "replies": { "scan": measured_on_0_6_0 } })).await;
    let outcome = scan(&client, &vault()).await.expect("np 0.6.0's answer parses");
    let ScanOutcome::Applied(report) = outcome else { panic!("the document is not a lock: {outcome:?}") };
    assert_eq!(report.notes, 3, "the counters Jott reads are unchanged");
    assert_eq!(report.unresolved_targets, vec!["missing.md".to_string()]);

    // The same vault answered by 0.5.1 carries neither key, and absence is the ordinary case for every install that
    // has not upgraded yet — one parser, both releases, no `attachments: 0` invented for a build that never said it.
    let mut from_0_5_1 = measured_on_0_6_0.clone();
    let map = from_0_5_1.as_object_mut().expect("an object");
    map.remove("attachments");
    map.remove("attachment_examples");
    let (_d2, client2) = spawn(json!({ "replies": { "scan": from_0_5_1 } })).await;
    let older = scan(&client2, &vault()).await.expect("and so does 0.5.1's");
    let ScanOutcome::Applied(older) = older else { panic!("not a lock: {older:?}") };
    assert_eq!(older.notes, 3, "the same fields, the same reading");
    client.shutdown().await.unwrap();
}
