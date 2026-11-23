//! Consent, written where np will read it. §11 is specific: "Not now" changes `[models]
//! auto_fetch` **in the config file**, because env is frozen at spawn and the child exists before
//! consent does — and np re-reads config per request (§18.6), so flipping the key needs no respawn.
//! Everything else in that file is somebody's own configuration, so it has to come back unchanged.

use jott_core::ingest::consent::{
    answer_consent, auto_fetch_allowed, builtin_default_model, consent_file_is_unreadable, embedding_model_named,
    gap_lines, np_config_path, rerank_top_n, settings_gap, set_auto_fetch, set_rerank_top_n, AutoFetch, ConsentError,
    SettingsGap,
};

mod common;

/// `NP_PATH`-style env games again: one variable, several threads.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn rich_config() -> String {
    "# kept on purpose: this file predates Jott\n\
     [log]\n\
     level = \"info\"  # verbose makes np slow on big vaults\n\
     \n\
     [models]\n\
     base_url = \"https://mirror.example/models\"  # my own mirror\n\
     auto_fetch = true\n\
     \n\
     [embeddings]\n\
     provider = \"local\"\n\
     model = \"bge-m3\"\n"
        .to_string()
}

#[test]
fn auto_fetch_is_yes_until_the_file_says_otherwise() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    assert!(auto_fetch_allowed(&path), "np's own default is yes (measured: `auto_fetch_default`)");

    std::fs::write(&path, "").unwrap();
    assert!(auto_fetch_allowed(&path), "an empty file says nothing, so the default stands");

    std::fs::write(&path, "[models]\nauto_fetch = false\n").unwrap();
    assert!(!auto_fetch_allowed(&path));

    std::fs::write(&path, "[models]\nauto_fetch = true\n").unwrap();
    assert!(auto_fetch_allowed(&path));
}

#[test]
fn declining_changes_one_line_and_nothing_else() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let before = rich_config();
    std::fs::write(&path, &before).unwrap();

    set_auto_fetch(&path, false).unwrap();

    let after = std::fs::read_to_string(&path).unwrap();
    let changed: Vec<(&str, &str)> = before
        .lines()
        .zip(after.lines())
        .filter(|(before, after)| before != after)
        .collect();
    assert_eq!(changed, vec![("auto_fetch = true", "auto_fetch = false")], "the diff is one key: {after}");
    assert_eq!(before.lines().count(), after.lines().count(), "no line added or dropped");
    assert!(after.contains("# kept on purpose"), "comments are the user's, not ours to lose");
    assert!(after.contains("base_url = \"https://mirror.example/models\"  # my own mirror"), "even a trailing one: {after}");
    assert!(after.contains("[log]\nlevel = \"info\"  # verbose makes np slow on big vaults\n"));
}

#[test]
fn granting_writes_true_and_keeps_every_line_that_was_there() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    // No `[models]` at all: the key has to arrive without disturbing what is.
    let before = "[embeddings]\nprovider = \"local\"\nmodel = \"bge-m3\"\n".to_string();
    std::fs::write(&path, &before).unwrap();

    set_auto_fetch(&path, true).unwrap();

    let after = std::fs::read_to_string(&path).unwrap();
    assert!(auto_fetch_allowed(&path), "{after}");
    for line in before.lines() {
        assert!(after.lines().any(|a| a == line), "lost a line: {line}\n{after}");
    }
}

#[test]
fn a_file_that_is_not_there_becomes_the_smallest_one_that_says_no() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested").join("config.toml");

    set_auto_fetch(&path, false).unwrap();

    assert!(!auto_fetch_allowed(&path));
    let text = std::fs::read_to_string(&path).unwrap();
    assert_eq!(text.lines().filter(|l| !l.trim().is_empty() && !l.starts_with('#')).count(), 2, "no invented keys: {text}");
    assert!(text.contains("[models]"));
    assert!(text.contains("auto_fetch = false"));
}

/// The reranker's depth is np's `[search] rerank_top_n`. Writing it adds one key and keeps everything else; a value
/// np would refuse (outside 1..=64) is refused before the file is touched, because np would answer every reranked
/// search with that refusal.
#[test]
fn rerank_top_n_is_one_key_in_nps_file_and_nonsense_never_reaches_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    assert_eq!(rerank_top_n(&path), None, "no file: np's own default applies");
    let before = rich_config();
    std::fs::write(&path, &before).unwrap();

    set_rerank_top_n(&path, 12).unwrap();
    assert_eq!(rerank_top_n(&path), Some(12));
    let after = std::fs::read_to_string(&path).unwrap();
    for line in before.lines() {
        assert!(after.lines().any(|a| a == line), "lost a line: {line}\n{after}");
    }
    assert!(after.contains("[search]\nrerank_top_n = 12"), "{after}");

    for nonsense in [0, 65, -3] {
        let error = set_rerank_top_n(&path, nonsense).expect_err("outside np's range");
        assert!(error.to_string().contains("rerank_top_n"), "{error}");
    }
    assert_eq!(std::fs::read_to_string(&path).unwrap(), after, "a refused value writes nothing");
}

#[test]
fn a_config_jott_cannot_read_answers_no() {
    // np itself would refuse to load this file and say so on its own terms. Jott's reader must not
    // guess permission it cannot see: `true` here means promising a download the child will refuse.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "[models\n  auto_fetch = ").unwrap();
    assert!(!auto_fetch_allowed(&path));
}

#[test]
fn the_model_named_for_embedding_comes_from_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");

    std::fs::write(&path, rich_config()).unwrap();
    assert_eq!(embedding_model_named(&path).as_deref(), Some("bge-m3"));

    std::fs::write(&path, "[embeddings]\nprovider = \"local\"\n").unwrap();
    assert_eq!(embedding_model_named(&path), None, "a provider with no model is not a model");

    std::fs::write(&path, "# model = \"bge-m3\"\n").unwrap();
    assert_eq!(embedding_model_named(&path), None, "a commented-out line is a comment");

    assert_eq!(embedding_model_named(&dir.path().join("absent.toml")), None);
}

#[test]
fn the_config_path_is_nps_rule_not_jotts() {
    let _turn = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();

    #[cfg(windows)]
    {
        // Windows is `%APPDATA%` and nothing else — which is the whole reason the shell's own guess at this
        // path was a bug: git-bash, VS Code and CI all set `HOME`, and np does not look there. The assertion
        // is that a `HOME` pointing somewhere else changes nothing.
        let previous_appdata = std::env::var_os("APPDATA");
        let previous_home = std::env::var_os("HOME");
        std::env::set_var("APPDATA", dir.path());
        std::env::set_var("HOME", dir.path().join("not-here"));

        assert_eq!(np_config_path().unwrap(), dir.path().join("note-processor").join("config.toml"));

        match previous_appdata {
            Some(value) => std::env::set_var("APPDATA", value),
            None => std::env::remove_var("APPDATA"),
        }
        match previous_home {
            Some(home) => std::env::set_var("HOME", home),
            None => std::env::remove_var("HOME"),
        }
    }

    #[cfg(not(windows))]
    {
        std::env::set_var("XDG_CONFIG_HOME", dir.path());
        assert_eq!(np_config_path().unwrap(), dir.path().join("note-processor").join("config.toml"));

        std::env::remove_var("XDG_CONFIG_HOME");
        let previous_home = std::env::var_os("HOME");
        std::env::set_var("HOME", dir.path());
        assert_eq!(
            np_config_path().unwrap(),
            dir.path().join(".config").join("note-processor").join("config.toml")
        );

        match previous_home {
            Some(home) => std::env::set_var("HOME", home),
            None => std::env::remove_var("HOME"),
        }
    }
}

#[test]
fn a_config_that_will_not_parse_is_not_a_config_that_names_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let good = dir.path().join("good.toml");
    let broken = dir.path().join("broken.toml");
    std::fs::write(&good, "[embeddings]\nmodel = \"bge-m3\"\n").unwrap();
    // Half-typed, or truncated by a failed sync: it exists, and it says nothing anyone can act on.
    std::fs::write(&broken, "[embeddings\nmodel = ").unwrap();

    // The consent gate's question, exactly: absent is readable (there is nothing to misread), a file that
    // loads is readable, and one that does not is *not* "no model configured" — reading it that way is how
    // a broken file ends up embedding without asking.
    assert!(!consent_file_is_unreadable(&dir.path().join("absent.toml")));
    assert!(!consent_file_is_unreadable(&good));
    assert!(consent_file_is_unreadable(&broken));
    assert_eq!(embedding_model_named(&broken), None, "and it does name no model, which is the trap");
}

// ---- np's built-in default model, which its config file never mentions ----

/// Measured on this machine on 2026-11-12 by running the shipped binary rather than reading a README:
/// `np serve` with no config at all answered a `search` by fetching `embeddinggemma-2` — 309,855,456 bytes —
/// and the `models.list` that followed reported this id, this digest, and dim 768. The point of keeping the
/// numbers is wording: §12's prompt has to name a model and a size *before* anything is downloaded, and a
/// fresh np can name neither until it has fetched one.
#[test]
fn jott_can_name_the_model_np_carries_before_np_is_asked() {
    let model = builtin_default_model();
    assert_eq!(model.id, "embeddinggemma-2");
    assert_eq!(model.bytes, Some(309_855_456));
    assert_eq!(model.sha256, Some("2188ac1deca4b77dffefd603c2776a9d76d9d74ec01841392982ebb840b09135"));
}

/// The pin is wording, not a decision: when np itself reports a model, that answer wins, and Jott's copy is
/// only what the prompt falls back to. If np ever changes its built-in default, a stale string here must be
/// able to change a sentence and nothing else.
#[test]
fn the_built_in_model_is_a_fallback_and_never_the_authority() {
    let model = builtin_default_model();
    assert!(model.bytes.unwrap() > 0, "a prompt that says \"about nothing MB\" is worse than one that says nothing");
    // Nothing in this module treats it as configured: an empty config still names no model.
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "[log]\nlevel = \"info\"\n").expect("write config");
    assert_eq!(embedding_model_named(&path), None);
}

/// The sentence a tester saw — `keyword-only: embeddings.provider is not set` — is true and useless on its own.
/// These four shapes are what makes it actionable, and all four were measured against np 0.4.0 rather than reasoned
/// out: no `[embeddings]` at all means np embeds nothing and reports success doing it; `provider` without `model`
/// makes np fault; both set means nothing stands in the way. A provider somebody chose by hand is the fourth case,
/// and Jott has no advice about it — separation says we ask np for a method (note-processor#7) rather than editing a
/// decision that is not ours.
#[test]
fn a_config_with_no_embeddings_section_is_missing_the_provider() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "[models]\nauto_fetch = true\n").unwrap();
    assert_eq!(settings_gap(Some(&path), Some("embeddinggemma-2")), SettingsGap::ProviderMissing);
    assert_eq!(
        gap_lines(&SettingsGap::ProviderMissing, Some("embeddinggemma-2")),
        vec!["[embeddings]", "provider = \"local-model\"", "model = \"embeddinggemma-2\""]
    );
}

#[test]
fn a_missing_config_is_the_same_gap_as_an_empty_one() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(settings_gap(Some(&dir.path().join("absent.toml")), None), SettingsGap::ProviderMissing);
    assert_eq!(settings_gap(None, None), SettingsGap::ProviderMissing, "no config path is still no provider");
    // Nothing was quoted in the consent offer (a fresh np names no model): the line has to name the model this np
    // build would fetch, from `builtin_default_model`, rather than a placeholder nobody can paste.
    assert!(gap_lines(&SettingsGap::ProviderMissing, None).iter().any(|line| line.contains("embeddinggemma-2")));
}

#[test]
fn a_provider_without_a_model_names_the_model_line_only() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "[embeddings]\nprovider = \"local-model\"\n").unwrap();
    assert_eq!(settings_gap(Some(&path), Some("bge-m3")), SettingsGap::ModelMissing);
    // The provider line is already there. Adding a second `[embeddings]` would be a broken file, and repeating
    // `provider` would be advice about something that is not wrong.
    assert_eq!(gap_lines(&SettingsGap::ModelMissing, Some("bge-m3")), vec!["model = \"bge-m3\""]);
}

#[test]
fn a_provider_someone_else_chose_gets_no_advice() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    // `"none"` is np's explicit off switch and a future spelling is somebody's deliberate choice; both are the same
    // case from here: quote np, suggest nothing.
    for provider in ["none", "something-new"] {
        std::fs::write(&path, format!("[embeddings]\nprovider = \"{provider}\"\nmodel = \"x\"\n")).unwrap();
        assert_eq!(settings_gap(Some(&path), Some("embeddinggemma-2")), SettingsGap::ProviderOther { provider: provider.into() });
        assert!(gap_lines(&SettingsGap::ProviderOther { provider: provider.into() }, Some("embeddinggemma-2")).is_empty(), "{provider} is not ours to change");
    }
}

#[test]
fn provider_and_model_together_leave_nothing_to_say() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "[embeddings]\nprovider = \"local-model\"\nmodel = \"bge-m3\"\n").unwrap();
    assert_eq!(settings_gap(Some(&path), Some("embeddinggemma-2")), SettingsGap::Ready);
    assert!(gap_lines(&SettingsGap::Ready, Some("embeddinggemma-2")).is_empty());
}

/// A half-edited `config.toml` is the state a person gets into by editing it themselves — and np answers it with a
/// parse error naming the file and the line (measured against 0.4.0: *"config parse error in …/config.toml: TOML
/// parse error at line 1, column 12 … unclosed table, expected `]`"*). Jott must not answer that state with a claim
/// about what the file says.
#[test]
fn an_unparsable_config_says_the_file_is_unreadable_and_offers_no_lines() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "[embeddings\nprovider = \"local-model\n").unwrap();

    assert_eq!(settings_gap(Some(&path), Some("embeddinggemma-2")), SettingsGap::ConfigUnreadable);
    assert!(
        gap_lines(&SettingsGap::ConfigUnreadable, Some("embeddinggemma-2")).is_empty(),
        "handing somebody lines to paste into a file that does not parse is advice, not help"
    );
}

#[test]
fn a_missing_file_is_not_the_same_as_an_unreadable_one() {
    // No file at all means np's defaults answer, which is a readable state: the card should talk about the missing
    // provider, not about a file that isn't there.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    assert_eq!(settings_gap(Some(&path), None), SettingsGap::ProviderMissing);
}

// ---- the answer goes through np (note-processor#7). The rule these tests exist for: a consent click must never
// leave Jott having written np's provider or model by hand, and a refusal must leave Jott's own key untouched.

fn enable_ok(model: &str) -> serde_json::Value {
    serde_json::json!({"changed": ["embeddings.provider", "embeddings.model"],
                       "fetch": {"bytes_total": 309_855_456, "members": ["main"]},
                       "model": model,
                       "persisted_to": "/home/you/.config/note-processor/config.toml",
                       "provider": "local-model"})
}

#[tokio::test]
async fn saying_yes_opens_the_door_first_and_writes_only_our_own_key_second() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.toml");
    std::fs::write(&config, "[models]\nauto_fetch = false\n").unwrap();
    let record = dir.path().join("calls.ndjson");
    std::fs::write(&record, "").unwrap();

    let (_d, client) = common::spawn(serde_json::json!({
        "record": record.display().to_string(),
        "replies": {"embeddings.enable": enable_ok("embeddinggemma-2")}
    }))
    .await;

    let receipt = answer_consent(&client, Some(&config), true, "embeddinggemma-2").await;

    assert!(receipt.granted, "this was the yes button");
    assert!(receipt.np_refused.is_none(), "np said yes: {:?}", receipt.np_refused);
    assert_eq!(receipt.changed, vec!["embeddings.provider".to_string(), "embeddings.model".to_string()],
               "the card lists what np changed, in np's order");
    assert_eq!(receipt.provider.as_deref(), Some("local-model"));
    assert_eq!(receipt.fetch_bytes, Some(309_855_456), "np's price, quoted");
    assert_eq!(
        receipt.persisted_to.as_deref(),
        Some(std::path::Path::new("/home/you/.config/note-processor/config.toml")),
        "np names the file it wrote, so the card can name it too"
    );

    // Jott's own key, and nothing else: np's two keys came from np over the pipe, so the file must show no hand of ours
    // in `[embeddings]` — which is what §4 of the design is about.
    let after = std::fs::read_to_string(&config).unwrap();
    assert!(after.contains("auto_fetch = true"), "{after}");
    assert!(!after.contains("[embeddings]"), "Jott wrote into `[embeddings]`: {after}");
    assert!(matches!(receipt.auto_fetch, AutoFetch::Written), "{:?}", receipt.auto_fetch);

    let lines = std::fs::read_to_string(&record).unwrap();
    assert_eq!(lines.matches("\"embeddings.enable\"").count(), 1, "{lines}");
    client.shutdown().await.unwrap();
}

/// A refusal is the interesting case: np said no, so Jott must not have touched anything. Writing `auto_fetch = true`
/// after a refusal would leave the guardrail off and the door shut — the worst of both, and invisible.
#[tokio::test]
async fn a_refusal_leaves_jotts_own_key_alone_and_keeps_np_sentence() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.toml");
    std::fs::write(&config, "[models]\nauto_fetch = false\n").unwrap();
    let sentence = "config io: no pinned model \"no-such\": this build can name \"embeddinggemma-2\"";
    let (_d, client) = common::spawn(serde_json::json!({
        "faults": {"embeddings.enable": {"code": -32000, "message": sentence}}
    }))
    .await;

    let receipt = answer_consent(&client, Some(&config), true, "no-such").await;

    assert_eq!(receipt.np_refused.as_deref(), Some(sentence), "np's words, byte for byte");
    assert!(receipt.changed.is_empty(), "nothing changed, so nothing is claimed: {:?}", receipt.changed);
    let after = std::fs::read_to_string(&config).unwrap();
    assert_eq!(after, "[models]\nauto_fetch = false\n", "a refusal must not leave the guardrail off");
    assert!(matches!(receipt.auto_fetch, AutoFetch::Untouched), "{:?}", receipt.auto_fetch);
    client.shutdown().await.unwrap();
}

/// An unparsable config is np's to refuse and np's to explain. Jott never parses it here — that is what
/// `settings_gap` is for when *showing* — so the file has to come back with the same bytes it went in with, and np's
/// sentence has to survive intact for the card to quote.
#[tokio::test]
async fn a_file_np_cannot_read_is_left_byte_for_byte_as_it_was() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.toml");
    let broken = "[embeddings\nprovider = \"local-model\n";
    std::fs::write(&config, broken).unwrap();
    let sentence = "config io: config parse error in /home/you/.config/note-processor/config.toml: \
        TOML parse error at line 1, column 12";
    let (_d, client) = common::spawn(serde_json::json!({
        "faults": {"embeddings.enable": {"code": -32000, "message": sentence}}
    }))
    .await;

    let receipt = answer_consent(&client, Some(&config), true, "embeddinggemma-2").await;

    assert_eq!(receipt.np_refused.as_deref(), Some(sentence));
    assert_eq!(std::fs::read_to_string(&config).unwrap(), broken, "Jott edited a file it was refused at");
    client.shutdown().await.unwrap();
}

/// Saying no revokes the provider but keeps the model someone chose, and **always** leaves `auto_fetch = false`
/// behind — even when np refused to revoke. A decline that left the download guardrail up would be a bug that costs
/// somebody 296 MiB.
#[tokio::test]
async fn saying_no_keeps_the_guardrail_down_even_when_np_refuses_to_revoke() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.toml");
    std::fs::write(&config, "[models]\nauto_fetch = true\n").unwrap();
    let sentence = "config io: cannot write the config file";
    let (_d, client) = common::spawn(serde_json::json!({
        "faults": {"embeddings.disable": {"code": -32000, "message": sentence}}
    }))
    .await;

    let receipt = answer_consent(&client, Some(&config), false, "embeddinggemma-2").await;

    assert_eq!(receipt.np_refused.as_deref(), Some(sentence));
    assert!(matches!(receipt.auto_fetch, AutoFetch::Written), "the guardrail is Jott's own job: {:?}", receipt.auto_fetch);
    assert!(!auto_fetch_allowed(&config), "np refused, and yet nothing may download");
    client.shutdown().await.unwrap();
}

/// A decline np accepted reports np's answer, including the model it kept. `changed` naming one key is the proof that
/// revoking is not erasing.
#[tokio::test]
async fn a_decline_np_accepted_says_which_key_moved_and_which_model_stayed() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.toml");
    std::fs::write(&config, "[models]\nauto_fetch = true\n").unwrap();
    let (_d, client) = common::spawn(serde_json::json!({
        "replies": {"embeddings.disable": {"changed": ["embeddings.provider"], "fetch": null,
                                            "model": "embeddinggemma-2", "provider": "none"}}
    }))
    .await;

    let receipt = answer_consent(&client, Some(&config), false, "embeddinggemma-2").await;

    assert!(receipt.np_refused.is_none());
    assert_eq!(receipt.changed, vec!["embeddings.provider".to_string()]);
    assert_eq!(receipt.provider.as_deref(), Some("none"));
    assert_eq!(receipt.model.as_deref(), Some("embeddinggemma-2"), "no is not a deletion");
    assert_eq!(receipt.fetch_bytes, None);
    assert!(!auto_fetch_allowed(&config));
    client.shutdown().await.unwrap();
}

/// np opened the door and Jott had no file to write its own key into. The receipt has to say "nowhere to write"
/// rather than claim the guardrail was set — this is the case where an env-only np install leaves `auto_fetch` at its
/// default of *yes*, so a false "Written" would be exactly the wrong reassurance.
#[tokio::test]
async fn with_no_config_path_jott_says_it_wrote_nothing() {
    let (_d, client) = common::spawn(serde_json::json!({
        "replies": {"embeddings.enable": enable_ok("embeddinggemma-2")}
    }))
    .await;

    let receipt = answer_consent(&client, None, true, "embeddinggemma-2").await;
    assert!(receipt.np_refused.is_none(), "np itself said yes");
    assert_eq!(receipt.auto_fetch, AutoFetch::NoConfigPath, "{:?}", receipt.auto_fetch);
    client.shutdown().await.unwrap();
}

/// A write that fails is reported as failing, with the sentence naming the file. `Written` here would tell somebody
/// their guardrail is down when it is still up — and np did say yes, so this is a half-done answer the card has to be
/// able to show.
#[tokio::test]
async fn a_failed_write_of_our_own_key_is_not_reported_as_written() {
    let dir = tempfile::tempdir().unwrap();
    // A regular file where the config's parent directory should be: no amount of `auto_fetch` gets written there.
    std::fs::write(dir.path().join("blocked"), b"not a directory").unwrap();
    let blocked = dir.path().join("blocked/config.toml");
    let (_d, client) = common::spawn(serde_json::json!({
        "replies": {"embeddings.enable": enable_ok("embeddinggemma-2")}
    }))
    .await;

    let receipt = answer_consent(&client, Some(&blocked), true, "embeddinggemma-2").await;

    let AutoFetch::Failed { sentence } = &receipt.auto_fetch else {
        panic!("a write that could not happen is not `Written`: {:?}", receipt.auto_fetch);
    };
    assert!(sentence.contains("blocked/config.toml"), "the sentence names the file it failed on: {sentence}");
    client.shutdown().await.unwrap();
}

#[test]
fn a_consent_error_names_the_file_it_failed_on() {
    // The card quotes these sentences, so they have to name a path rather than say "io error".
    let err = ConsentError::NotATable { path: "/home/you/.config/note-processor/config.toml".into(), table: "models" };
    let said = err.to_string();
    assert!(said.contains("note-processor/config.toml"), "{said}");
    assert!(said.contains("[models]"), "{said}");
}

// ---- issue #4: rewriting a file nobody owns ---------------------------------------------------------

/// The rewrite goes through a temporary beside the destination, and a temporary is a **new** file — so it would
/// arrive with the umask's mode and rename that over whatever the person had chosen. `config.toml` can hold a
/// mirror URL somebody did not want read; the point of issue #4 is that Jott must not be the reason it changes.
#[cfg(unix)]
#[test]
fn a_rewrite_keeps_the_permission_bits_the_file_already_had() {
    use std::os::unix::fs::PermissionsExt;
    let _turn = SERIAL.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "[models]\nauto_fetch = true\n").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();

    set_auto_fetch(&path, false).unwrap();

    assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600, "the rewrite changed who can read someone's settings file");
    assert!(!auto_fetch_allowed(&path), "and the key still landed");
}

/// A write that cannot happen must cost nothing. The directory is the one thing in the way here, so neither the
/// temp nor an in-place open can succeed — and the file a person has stays byte for byte what it was.
#[cfg(unix)]
#[test]
fn a_write_that_cannot_happen_leaves_the_file_a_person_has_intact() {
    use std::os::unix::fs::PermissionsExt;
    let _turn = SERIAL.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let before = rich_config();
    std::fs::write(&path, &before).unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o500)).unwrap();

    let error = set_auto_fetch(&path, false).expect_err("a folder nobody can write into refuses the write");
    assert!(matches!(error, ConsentError::Write { .. }), "{error}");

    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), before, "np's config survived an attempt that failed");
}

/// The temporary is a file in the same folder as np's config, and np indexes this folder — so a leftover would be
/// the one thing this rewrite must not leave behind. (`.tmp-<pid>` is not a `.md`, but "invisible to np" was the
/// reason `write_note_with` writes in place, and it is worth a line to keep the promise here too.)
#[test]
fn a_successful_rewrite_leaves_no_temporary_behind() {
    let _turn = SERIAL.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "[models]\nauto_fetch = true\n").unwrap();

    set_auto_fetch(&path, true).unwrap();

    let leftovers: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| name != "config.toml")
        .collect();
    assert!(leftovers.is_empty(), "stray files beside np's config: {leftovers:?}");
}
