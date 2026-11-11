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
    let conf = std::fs::read_to_string(root.join("apps/shell/src-tauri/tauri.conf.json"))
        .expect("apps/shell/src-tauri/tauri.conf.json exists");
    let cfg = parse_app_config(&conf).expect("parses");
    assert_eq!(cfg.product_name, "Jott");
    assert_eq!(cfg.identifier, "dev.jott.app");
    assert_eq!(cfg.frontend_dist, "../dist");
    assert_eq!(cfg.dev_url.as_deref(), Some("http://localhost:5173"));
}

#[test]
fn a_missing_key_is_an_error_not_a_default() {
    let err = parse_app_config(r#"{"productName":"Jott"}"#).unwrap_err();
    assert!(err.to_string().contains("identifier"), "{err}");
}
