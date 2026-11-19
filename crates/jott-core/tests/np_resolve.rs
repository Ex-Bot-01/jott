use jott_core::np::resolve::{install_hint, min_version_text, resolve_np, NpMissing, NpSource, Os};
use std::fs;
use std::path::Path;

/// Make an executable at `dir/np` (or `np.exe`) that answers nothing — resolution is
/// about *where* np is, not what it says.
fn fake_np(dir: &Path) -> std::path::PathBuf {
    let p = dir.join(if cfg!(windows) { "np.exe" } else { "np" });
    fs::write(&p, b"#!/bin/sh\nexit 0\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
    }
    p
}

/// `NP_PATH` is one variable for the whole process and these tests run on several threads, so the
/// two that set it take turns: without this, one test's override vanishes under another's
/// assertion and the failure lands on whichever test lost the race.
static NP_PATH_TURN: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// What `NP_PATH` was before this test touched it, put back at the end.
fn with_np_path<T>(value: Option<std::path::PathBuf>, run: impl FnOnce() -> T) -> T {
    let previous = std::env::var_os("NP_PATH");
    match value {
        Some(v) => std::env::set_var("NP_PATH", v),
        None => std::env::remove_var("NP_PATH"),
    }
    let out = run();
    match previous {
        Some(v) => std::env::set_var("NP_PATH", v),
        None => std::env::remove_var("NP_PATH"),
    }
    out
}

#[test]
fn np_path_wins_and_is_not_silently_forgiven_when_wrong() {
    let _turn = NP_PATH_TURN.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    // The override lives somewhere that is *not* the app dir, or it would also be a
    // bundled np and the test would prove nothing about precedence.
    let bin = dir.path().join("bin");
    fs::create_dir_all(&bin).unwrap();
    let app_dir = dir.path().join("app");
    fs::create_dir_all(&app_dir).unwrap();
    let good = fake_np(&bin);
    let found = with_np_path(Some(good.clone()), || resolve_np(&app_dir));
    assert!(matches!(&found, Ok(NpSource::Env(p)) if *p == good), "{found:?}");

    // A bad override must not fall through to PATH: the user said which binary, and a
    // silent fallback would have Jott index with a different np than they installed.
    let refused = with_np_path(Some(bin.join("nope")), || resolve_np(&app_dir));
    match refused {
        Err(NpMissing::OverrideMissing(p)) => assert!(p.ends_with("nope")),
        other => panic!("expected OverrideMissing, got {other:?}"),
    }
}

#[test]
fn bundled_np_is_used_before_path() {
    let _turn = NP_PATH_TURN.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let app_dir = dir.path().join("app");
    fs::create_dir_all(&app_dir).unwrap();
    let bundled = fake_np(&app_dir);
    let found = with_np_path(None, || resolve_np(&app_dir));
    assert!(matches!(&found, Ok(NpSource::Bundled(p)) if *p == bundled), "{found:?}");
}

#[test]
fn a_named_binary_beats_the_one_we_shipped() {
    let _turn = NP_PATH_TURN.lock().unwrap_or_else(|e| e.into_inner());
    // The case that only exists now that installers carry np inside them: the app ships a binary, and a
    // person points `NP_PATH` at one they built or installed themselves. Whichever we shipped is a file;
    // their override is a decision, and a decision has to beat a file or the variable is a lie (§14).
    let dir = tempfile::tempdir().unwrap();
    let app_dir = dir.path().join("app");
    fs::create_dir_all(&app_dir).unwrap();
    let _bundled = fake_np(&app_dir);
    let own = dir.path().join("own-build");
    fs::create_dir_all(&own).unwrap();
    let chosen = fake_np(&own);

    let found = with_np_path(Some(chosen.clone()), || resolve_np(&app_dir));
    assert!(matches!(&found, Ok(NpSource::Env(p)) if *p == chosen), "bundled np won over an explicit NP_PATH: {found:?}");
}

#[test]
fn hints_name_the_asset_for_that_platform() {
    let win = install_hint(&NpMissing::NotFound, Os::Windows);
    assert!(win.contains("x86_64-pc-windows-gnu-cross.zip"), "{win}");
    let linux = install_hint(&NpMissing::NotFound, Os::Linux);
    assert!(linux.contains("sha256sum -c SHA256SUMS.txt"), "{linux}");
    let old = install_hint(&NpMissing::TooOld { found: "0.2.3".into() }, Os::Linux);
    // Names what was found and what is needed, both from constants — a hint quoting a floor the code does not enforce
    // sends someone to fetch a build Jott would then refuse.
    assert!(old.contains("0.2.3") && old.contains(&min_version_text()), "{old}");
}

/// Two files state which np Jott wants and nothing but this test makes them agree: `packaging/np-pin.json` is what
/// the installer downloads, `install_hint` is what a person with an ambient np is told to fetch. A hint naming an
/// older asset than the one in the installer sends someone to install a build Jott then refuses.
#[test]
fn the_hint_names_the_np_the_installer_ships() {
    let pin_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packaging/np-pin.json");
    let raw = std::fs::read_to_string(&pin_path).unwrap_or_else(|e| panic!("{}: {e}", pin_path.display()));
    let pin: serde_json::Value = serde_json::from_str(&raw).expect("the pin file is JSON");
    let version = pin["version"].as_str().expect("the pin names a version");

    for (os, key) in [(Os::Windows, "x86_64-pc-windows-msvc"), (Os::Linux, "x86_64-unknown-linux-gnu")] {
        let asset = pin["assets"][key]["archive"].as_str().unwrap_or_else(|| panic!("no pinned {key} asset"));
        let hint = install_hint(&NpMissing::NotFound, os);
        assert!(hint.contains(version), "the installer ships np {version}, the hint says otherwise: {hint}");
        assert!(hint.contains(asset), "the hint names an asset other than the shipped {asset}: {hint}");
    }

    // And the floor a refusal quotes is that same release, so "too old" points at something that will be accepted.
    let too_old = install_hint(&NpMissing::TooOld { found: "0.3.1".into() }, Os::Windows);
    assert!(too_old.contains(version), "{too_old}");
}

#[test]
fn version_floor_is_compared_numerically() {
    use jott_core::np::handshake::{check_version, MIN_NP};
    assert_eq!(MIN_NP, (0, 5, 1));
    assert!(check_version("0.5.1").is_ok());
    assert!(check_version("0.10.0").is_ok(), "string compare would reject this");
    assert!(matches!(check_version("0.2.9"), Err(NpMissing::TooOld { .. })));
    assert!(matches!(check_version("nightly-2026"), Err(NpMissing::NoProtocol)));
}

/// Every release below 0.5.1 is behind, each for its own measured reason: 0.3.1 could not say whether a folder had
/// ever been indexed or which half of a search ran (#5), re-indexed a whole vault over one trailing backslash (#4) and
/// quoted no price for the download a consent prompt has to name (#3); 0.4.0 fixed all three but had neither
/// `embeddings.enable` (#7) nor prefix matching (#6). Jott still *reads* an older answer — every newer key is optional
/// on the wire — but a backend that cannot take a consent answer is not one to install by default.
#[test]
fn every_release_below_the_floor_is_refused_by_name() {
    use jott_core::np::handshake::check_version;
    for version in ["0.3.1", "0.4.0", "0.5.0"] {
        let Err(NpMissing::TooOld { found }) = check_version(version) else {
            panic!("{version} is below the floor and must be refused, not accepted");
        };
        assert_eq!(found, version, "the refusal names what it found: {found}");
    }
}

/// 0.5.1 is the floor because two things Jott used to work around became np's own job, and a build that cannot do them
/// would leave the window explaining settings it could have written itself:
///
/// - `embeddings.enable` / `embeddings.disable` (note-processor#7) — measured on 0.5.1: the answer names which keys it
///   changed, the price of the fetch, and the file it wrote; an id this build does not pin is refused rather than
///   half-applied.
/// - prefix matching (#6) — `{"prefix": true}` reaches *grandmother* from `grand` and says so in `prefix_used`,
///   `prefix_terms` and each hit's `matched_exactly`.
/// - and 0.5.1 specifically, not 0.5.0: the provenance comment it writes into someone's `config.toml` names the method
///   over `np serve` instead of a terminal command nobody typed (#10). An audit trail that quotes a lie is worse than
///   no audit trail, and Jott quotes it.
#[test]
fn np_0_5_0_is_below_the_floor_and_says_which_version_to_get() {
    use jott_core::np::handshake::check_version;
    assert!(matches!(check_version("0.5.0"), Err(NpMissing::TooOld { .. })));
    assert!(check_version("0.5.1").is_ok());
    assert!(check_version("0.6.0").is_ok(), "a newer np is still np");

    let floor = jott_core::np::resolve::min_version_text();
    assert_eq!(floor, "0.5.1", "the sentence a refusal quotes and the constant must be one number");
}

/// What goes inside the installer, named exactly — asset names are what `scripts/fetch-np` downloads, and a typo in
/// one is a CI job that fails on somebody's release afternoon rather than here.
///
/// The shipped release and the floor are deliberately different numbers now: 0.6.0 goes in the box, 0.5.1 stays
/// acceptable, because nothing in Jott needs what 0.6.0 added and a person whose np came from an older installer is
/// not running something broken. The invariant that matters is one-directional, so it is asserted as one: ship at or
/// above the floor, never below.
#[test]
fn the_bundled_asset_names_the_release_we_require() {
    use jott_core::np::handshake::check_version;
    use jott_core::np::resolve::{asset_for, NP_RELEASE_VERSION};
    assert_eq!(NP_RELEASE_VERSION, "0.6.0");
    assert_eq!(asset_for(Os::Windows).as_deref(), Some("np-0.6.0-x86_64-pc-windows-gnu-cross.zip"));
    assert_eq!(asset_for(Os::Linux).as_deref(), Some("np-0.6.0-x86_64-unknown-linux-gnu.tar.gz"));
    assert!(check_version(NP_RELEASE_VERSION).is_ok(), "the installer must not ship a build Jott would refuse");
}
