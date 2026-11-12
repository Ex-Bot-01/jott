use jott_core::app_config::parse_app_config;

fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2) // <root>/crates/jott-core -> crates -> root
        .expect("repo root")
        .to_path_buf()
}

/// The shell crate's folder: its manifest and its config sit side by side there.
fn shell_dir() -> std::path::PathBuf {
    repo_root().join("apps/shell")
}

/// The shell's tauri.conf.json is the one place the app identity is written, and a typo in it produces an
/// installed app with the wrong name or data folder. So the file is parsed and checked here, where CI runs
/// on every platform — including the ones where the GUI itself cannot be compiled by this crate.
#[test]
fn shell_config_declares_jott() {
    // Beside the crate's Cargo.toml, because that is where `tauri_build::build()` looks: its default is a
    // `tauri.conf.json` in the build script's working directory, which is the package root — not a
    // `src-tauri/` subfolder. A config in the wrong drawer is a build that fails on the release machine
    // only, so the location is asserted here, where every platform runs it.
    let shell = shell_dir();
    assert!(shell.join("Cargo.toml").is_file(), "the shell crate's manifest");
    let conf_path = shell.join("tauri.conf.json");
    let conf = std::fs::read_to_string(&conf_path).expect("apps/shell/tauri.conf.json exists");
    let cfg = parse_app_config(&conf).expect("parses");
    assert_eq!(cfg.product_name, "Jott");
    assert_eq!(cfg.identifier, "dev.jott.app");
    assert_eq!(cfg.dev_url.as_deref(), Some("http://localhost:5173"));

    // `frontendDist` is resolved against the *config file's* directory, so it has to be a path that works
    // from there: two levels up from `apps/shell`, which is where the repository's `dist/` actually sits.
    // It read `"../dist"` while the config lived in `apps/shell/src-tauri/` — wrong from either folder, and
    // the webview would have loaded an empty directory. Hence the checks below rather than a string match.
    assert_eq!(cfg.frontend_dist, "../../dist");
    let dist = conf_path
        .parent()
        .expect("config dir")
        .join(cfg.frontend_dist.trim_start_matches("./"));
    // Where it points has to be inside this repository — that much is checkable from a clean checkout.
    // Whether it is *populated* is a build-order fact (`npm run build` before `cargo test`, which CI does in
    // that order), and a suite that fails before anybody built anything teaches nobody anything. So the
    // contents are asserted when the folder exists, and when it does not, the reason is printed.
    assert!(
        jott_core::vault::store::to_abs(&dist).starts_with(jott_core::vault::store::to_abs(&repo_root())),
        "the window's assets stay inside the repository: {}",
        dist.display()
    );
    if dist.exists() {
        assert!(
            dist.join("index.html").is_file(),
            "{} is the webview's folder and has no index.html",
            dist.display()
        );
    } else {
        eprintln!("note: {} does not exist yet — run `npm run build` to check the bundle's contents", dist.display());
    }

    // An installer with no icon is an installer people distrust, and Windows wants a `.ico` in particular.
    assert!(
        shell.join("icons/icon.ico").is_file(),
        "the Windows bundle needs apps/shell/icons/icon.ico"
    );
}

#[test]
fn the_version_is_written_once() {
    // `tauri-build` refuses to bundle when `tauri.conf.json` and the crate's manifest disagree about the
    // version, and it refuses on the release machine only. So the two are compared here, where a bump that
    // forgot one of them fails long before a tag does.
    let shell = shell_dir();
    let conf: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(shell.join("tauri.conf.json")).expect("apps/shell/tauri.conf.json exists"),
    )
    .expect("the config parses");
    let manifest = std::fs::read_to_string(shell.join("Cargo.toml")).expect("apps/shell/Cargo.toml exists");
    let cargo_version = manifest
        .lines()
        .find_map(|line| {
            line.trim()
                .strip_prefix("version = \"")
                .and_then(|rest| rest.strip_suffix('"'))
        })
        .expect("a version in apps/shell/Cargo.toml");
    assert_eq!(
        conf["version"].as_str().expect("a version in tauri.conf.json"),
        cargo_version,
        "the bundle's version and the crate's version have to be one number"
    );
}

#[test]
fn a_missing_key_is_an_error_not_a_default() {
    let err = parse_app_config(r#"{"productName":"Jott"}"#).unwrap_err();
    assert!(err.to_string().contains("identifier"), "{err}");
}
