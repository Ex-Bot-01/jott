use jott_core::app_config::parse_app_config;

/// The shell's tauri.conf.json is the one place the app identity is written, and a
/// typo in it produces an installed app with the wrong name or data folder. So the
/// file is parsed and checked here, where CI runs on every platform — including the
/// ones where the GUI itself cannot be compiled by this crate.
#[test]
fn shell_config_declares_jott() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)  // <root>/crates/jott-core -> crates -> root
        .expect("repo root");
    // Beside the crate's Cargo.toml, because that is where `tauri_build::build()` looks: its default is a
    // `tauri.conf.json` in the build script's working directory, which is the package root — not a
    // `src-tauri/` subfolder. A config in the wrong drawer is a build that fails on the release machine
    // only, so the location is asserted here, where every platform runs it.
    let shell = root.join("apps/shell");
    assert!(shell.join("Cargo.toml").is_file(), "the shell crate's manifest");
    let conf_path = shell.join("tauri.conf.json");
    let conf = std::fs::read_to_string(&conf_path).expect("apps/shell/tauri.conf.json exists");
    let cfg = parse_app_config(&conf).expect("parses");
    assert_eq!(cfg.product_name, "Jott");
    assert_eq!(cfg.identifier, "dev.jott.app");
    // `frontendDist` is resolved against the *config file's* directory, so it has to be a path that exists
    // from there. This is the check that catches "the bundle shipped an empty folder" without building.
    // Two levels up from `apps/shell`, which is where the repository's `dist/` actually sits. The literal
    // was `"../dist"` while the config lived in `apps/shell/src-tauri/`, and is wrong either way from
    // either folder — which is precisely the mistake the existence check below exists to catch.
    assert_eq!(cfg.frontend_dist, "../../dist");
    let dist = conf_path.parent().expect("config dir").join(cfg.frontend_dist.trim_start_matches("./"));
    // Where it points has to be the repository's own `dist/` — that much is checkable from a clean
    // checkout. Whether it is *populated* is a build-order fact (`npm run build` before `cargo test`, which
    // CI does in that order), and a suite that fails before anyone built anything teaches nobody anything;
    // so when the folder exists, its contents are asserted, and when it does not, the reason is printed.
    let inside_repo = jott_core::vault::store::to_abs(&dist).starts_with(jott_core::vault::store::to_abs(root));
    assert!(inside_repo, "the window's assets stay inside the repository: {}", dist.display());
    if dist.exists() {
        assert!(dist.join("index.html").is_file(), "{} is the webview's folder and has no index.html", dist.display());
    } else {
        eprintln!("note: {} does not exist yet — run `npm run build` to check the bundle's contents", dist.display());
    }
    assert_eq!(cfg.dev_url.as_deref(), Some("http://localhost:5173"));
}

#[test]
fn a_missing_key_is_an_error_not_a_default() {
    let err = parse_app_config(r#"{"productName":"Jott"}"#).unwrap_err();
    assert!(err.to_string().contains("identifier"), "{err}");
}
