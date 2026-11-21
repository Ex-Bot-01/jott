//! The pre-release gate against a real note-processor, driven the way Jott drives it.
//!
//! ```text
//! JOTT_REAL_NP=apps/shell/np/np cargo test -p jott-core --test np_real_session
//! ```
//!
//! Without the variable every test here says so and passes: CI has no np on purpose (the absence tests are half of
//! what that suite checks), and a test suite that starts downloading a 14 MB backend to prove a client works is its
//! own bug. `scripts/fetch-np` puts the exact binary an installer will carry at `apps/shell/np/np`, so the command
//! above asks the shipped artefact the question, not a build off somebody's branch.
//!
//! Why this file exists beside `np_serve_args.rs`: that gate shakes hands and stops. Every np release so far was
//! accepted on the handshake — which is exactly why it proves too little: 0.6.0 changed nothing in `initialize` and
//! two things in what `scan` answers afterwards, 0.6.1 changed neither, and 0.5.1's headline change (#6, prefix
//! widening) lived only in a `search` document. A release note says what np intends; this file asks whether a note Jott wrote actually comes
//! back.

use jott_core::ipc::{self, VaultSession};
use jott_core::np::NpSource;
use jott_core::vault::stamps::StampState;
use jott_core::vault::store::Consent;
use std::path::PathBuf;
use std::time::Duration;

/// The binary to ask, or `None` when this machine was not told. A path that does not exist is a hard failure
/// rather than a skip: "the gate passed" must never mean "the variable was typo'd".
fn real_np() -> Option<PathBuf> {
    let path = std::env::var_os("JOTT_REAL_NP").map(PathBuf::from)?;
    assert!(path.is_file(), "JOTT_REAL_NP names {}, which is not a file", path.display());
    Some(path)
}

/// Wait until the writer's first scan has marked `rel` indexed, and say what was refused if it never does.
///
/// Why a session waits rather than searching straight away: a reader-only child and the writer creating the same
/// brand-new `.np/index.db` at the same moment is a race inside np, measured on 0.5.1 *and* 0.6.0 — one of them
/// answers `sqlite: table files already exists` (note-processor#18, filed with the recorded transcripts). A person
/// typing in the search box while a folder indexes for the first time can hit it, so this file used to be an
/// occasional witness. It waits now, because a release gate that fails one run in eight teaches nobody anything; the
/// race itself is tracked as jott#23 and deliberately not papered over here.
///
/// Re-checked for 0.6.1 with two children scanning ten brand-new vaults at once: no refusal on either build, so the
/// race did not reproduce. That is "did not see it today", not "fixed" — it came up about once in eight inside Jott's
/// real flow, and #18 stays open until np says schema creation is serialised.
async fn await_indexed(session: &std::sync::Arc<VaultSession>, rel: &str) {
    for _ in 0..120 {
        let listing = session.listing().expect("the tree lists");
        if listing.nodes.iter().any(|n| n.rel == rel && n.state == Some(StampState::Indexed)) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let listing = session.listing().expect("the tree lists");
    let states: Vec<_> = listing.nodes.iter().map(|n| (n.rel.clone(), n.state.clone())).collect();
    panic!("{rel} never reached `indexed`: {states:?}");
}

/// Open a folder, write a note through Jott's own save path, ask for an index, and search for a word that exists
/// only in that note. Every step is the one the window takes; nothing is stubbed.
#[tokio::test]
async fn a_real_np_indexes_a_note_jott_just_wrote_and_finds_it_again() {
    let _turn = SERIAL.lock().await;
    let Some(path) = real_np() else {
        eprintln!("JOTT_REAL_NP unset: skipping the real-session check");
        return;
    };

    let tmp = tempfile::tempdir().expect("temp dir");
    let vault = tmp.path().join("vault");
    std::fs::create_dir_all(&vault).expect("vault folder");

    // The warm child from the handshake becomes the session's writer, exactly as it does at startup: one np per
    // vault, not one per question (§18.6).
    let source = NpSource::Bundled(path);
    let (warm, info) = ipc::read_np_info_warm(&source).await.expect("the real np answers the handshake");
    assert!(info.methods.iter().any(|m| m == "scan"), "np must offer `scan`: {:?}", info.methods);

    let session: std::sync::Arc<VaultSession> = VaultSession::open_in_with(
        source,
        info,
        Some(warm),
        &vault,
        Consent::Declined,
        None,
        tmp.path().join("appdata"),
    );

    let text = "# Grandmother\n\nA recipe with kestrel sugar folded through the top.\n";
    session.save_note("grandmother.md", text).expect("the note saves");
    session.ingest_all();
    await_indexed(&session, "grandmother.md").await;

    // Keyword mode, because this box has no embedding artifact installed and a meanings-only answer here would be a
    // statement about the machine rather than about np's protocol.
    let answers = session.search("kestrel", 8, false, "keyword").await.expect("search answers");
    let (rel, exact) = match answers.hits.first() {
        Some(hit) => (hit.path.clone(), hit.matched_exactly),
        None => panic!("no hit for a word written into the note seconds ago: {:?}", answers),
    };
    assert_eq!(rel, "grandmother.md", "np indexes against the vault-relative name (§18.6)");
    // `Option<bool>`, because "did not say" and "said no" are different sentences (§4). np answers it on both shipped
    // candidates, and says yes: the word is in the bytes verbatim, so this is an exact match rather than a widened one
    // — the distinction every result row's mark is built on.
    assert_eq!(exact, Some(true), "an exact hit stays exact on the np being shipped");

    // Closing is part of the gate: §11's close-out asks np to shut down rather than killing it holding its index
    // open, which on Windows is the difference between a clean next launch and lock friction.
    tokio::time::timeout(Duration::from_secs(20), session.close())
        .await
        .expect("the close-out returns on its own");
}

/// 0.6.0 added `attachments` and `attachment_examples` to the same document Jott parses strictly, and 0.6.1 answers
/// with them too. Here the point is not that the keys exist — `np_methods.rs` pins that against recorded bytes — but
/// that a vault holding them still indexes and searches end to end, so a strict reader cannot turn a new key into
/// stopped indexing (issue #11).
#[tokio::test]
async fn a_vault_full_of_attachments_still_indexes_and_searches() {
    let _turn = SERIAL.lock().await;
    let Some(path) = real_np() else {
        eprintln!("JOTT_REAL_NP unset: skipping the real-session check");
        return;
    };

    let tmp = tempfile::tempdir().expect("temp dir");
    let vault = tmp.path().join("vault");
    std::fs::create_dir_all(vault.join("sub")).expect("vault folders");
    // Three attachments for one note: the shape np 0.6.0 started counting, and the one a 0.5.1 report described as
    // though the files did not exist.
    for rel in ["plan.org", "scratch.txt", "sub/spec.rst"] {
        std::fs::write(vault.join(rel), b"not markdown, never parsed\n").expect("attachment");
    }

    let source = NpSource::Bundled(path);
    let (warm, info) = ipc::read_np_info_warm(&source).await.expect("the real np answers the handshake");
    let session = VaultSession::open_in_with(
        source,
        info,
        Some(warm),
        &vault,
        Consent::Declined,
        None,
        tmp.path().join("appdata"),
    );

    session.save_note("kestrel.md", "# Kestrel\n\nThe sugar came from a kestrel orchard.\n").expect("the note saves");
    session.ingest_all();
    await_indexed(&session, "kestrel.md").await;

    let answers = session.search("orchard", 8, false, "keyword").await.expect("search answers");
    assert!(!answers.hits.is_empty(), "the attachments did not stop the one note being found: {answers:?}");

    // The tree keeps its own opinion about what is a note: `.org` and `.rst` are rows with no marker, whatever np
    // now counts in its report.
    let listing = session.listing().expect("the tree lists");
    let kinds: Vec<(String, String)> =
        listing.nodes.iter().map(|e| (e.rel.clone(), format!("{:?}", e.kind))).collect();
    for rel in ["plan.org", "scratch.txt", "sub/spec.rst"] {
        assert!(kinds.iter().any(|(r, k)| r == rel && k == "Attachment"), "{rel} is an attachment row: {kinds:?}");
    }

    tokio::time::timeout(Duration::from_secs(20), session.close()).await.expect("the close-out returns");
}

static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Issue #29's promise against the binary that actually ships, not a stand-in: a note changed **outside** Jott has
/// to reach np by itself. Before the fix this test spent its whole budget waiting — the editor reloaded the text,
/// the watcher reported the path, and nothing downstream ever asked np to look, which left a green marker and an
/// `up to date` status line describing bytes np had never read.
#[tokio::test]
async fn a_real_np_notices_a_note_changed_outside_jott() {
    let _turn = SERIAL.lock().await;
    let Some(path) = real_np() else {
        eprintln!("JOTT_REAL_NP unset: skipping the real-session check");
        return;
    };

    let tmp = tempfile::tempdir().expect("temp dir");
    let vault = tmp.path().join("vault");
    std::fs::create_dir_all(&vault).expect("vault folder");

    let source = NpSource::Bundled(path);
    let (warm, info) = ipc::read_np_info_warm(&source).await.expect("the real np answers the handshake");
    let session = VaultSession::open_in_with(
        source,
        info,
        Some(warm),
        &vault,
        Consent::Declined,
        None,
        tmp.path().join("appdata"),
    );

    session.save_note("shelf.md", "# Shelf\n\nNothing on it yet.\n").expect("the note saves");
    session.ingest_all();
    await_indexed(&session, "shelf.md").await;

    // Wait out Jott's own-write window first — §14 keeps a path Jott itself wrote quiet for 200 ms so the watcher
    // does not report our save back as somebody else's change. Measured while writing this gate: with one small
    // note, np indexes fast enough that `await_indexed` comes back ~30 ms after the save, and an edit made in the
    // next breath lands inside that window and is swallowed — correctly, but it measures the suppression instead of
    // the wiring. A second editor does not write in Jott's own 200 ms; neither does this test.
    tokio::time::sleep(Duration::from_millis(400)).await;

    // Somebody else edits the file: no save inside Jott, no manual "Index now", nothing typed in the window.
    std::fs::write(vault.join("shelf.md"), "# Shelf\n\nThe quarkkessel stands where the jug was.\n")
        .expect("outside edit");

    // The marker has to come back on its own — `state_of` reads the digest on disk, so this waits for the *new*
    // bytes rather than accepting the old stamp.
    await_indexed(&session, "shelf.md").await;

    let answers = session.search("quarkkessel", 8, false, "keyword").await.expect("search answers");
    assert!(
        answers.hits.iter().any(|hit| hit.path == "shelf.md"),
        "words written outside Jott never reached np: {:?}",
        answers.hits.iter().map(|hit| (&hit.path, hit.matched_exactly)).collect::<Vec<_>>()
    );

    tokio::time::timeout(Duration::from_secs(20), session.close())
        .await
        .expect("the close-out returns on its own");
}
