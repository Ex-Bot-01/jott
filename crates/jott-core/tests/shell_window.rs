//! The window Jott shows is built by `apps/shell/src/main.rs`, not by `tauri.conf.json`. This file keeps it that
//! way, because both halves of "it opens white, jumps right, then looks right" came from letting the config build it.
//!
//! Two measurements, taken in the crates we compile against rather than remembered:
//!
//! * `tauri-2.12.5/src/app.rs`: `fn setup()` creates every window named in the config (line 2691) and only then runs
//!   our `setup` closure. The window-state plugin restores geometry from its `on_window_ready` hook, which fires
//!   inside that creation — so a config window is already **on screen at the position Windows chose** before the
//!   saved position is applied. That is the jump: appear, then get moved ~200 px. A window built by us with
//!   `.visible(false)` is moved while nobody can see it, and shown once, where it belongs.
//! * The paper colour Jott named in `tauri.conf.json` reached only half of what paints. Config colours are applied
//!   to the **window** (`tauri-runtime-wry-2.12.3/src/lib.rs:889`), while the webview's own colour is taken from
//!   `webview_attributes.background_color` alone (same file, line 4779) — and that attribute is what wry hands to
//!   WebView2 as `SetDefaultBackgroundColor` (`wry-0.57.2/src/webview2/mod.rs:415`). Nothing in the config path ever
//!   sets a webview attribute, so the surface that actually covers the window before the page paints kept WebView2's
//!   default: white. Tauri's builder method sets both halves (`tauri-2.12.5/src/webview/webview_window.rs:1248-1250`),
//!   which is the only reason moving creation into code changes what the first frame looks like.
//!
//! What this file can prove is structure: who creates the window, whether it starts hidden, whether a colour was
//! handed to the thing that reads it. It cannot prove what a screen looks like — no machine running these tests has
//! a display, and jsdom cannot compute style. The visible result is §2b-ix of `docs/testing-on-windows.md`, and the
//! geometry Jott actually showed at is written to the log (`event: "window"`) so the next report can be read rather
//! than guessed at.
//!
//! One pin here is not about the window at all, and sits with these because it was found by reading the same config
//! field's neighbours: the installed build had no developer console, while `docs/testing-on-windows.md` spent sixteen
//! releases telling people to open one. A config key nobody reads (`"devtools": true`) is the same mistake twice in
//! one object.

use std::fs;
use std::path::{Path, PathBuf};

fn repo(rel: &str) -> PathBuf {
    // Tests run with the crate directory as their working directory.
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(rel)
}

fn read(rel: &str) -> String {
    let path = repo(rel);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{:?}: {e}", path))
}

#[test]
fn the_config_names_no_window_so_nothing_appears_before_geometry_is_restored() {
    let config: serde_json::Value = serde_json::from_str(&read("apps/shell/tauri.conf.json"))
        .expect("tauri.conf.json parses");
    let windows = config["app"]["windows"].as_array().expect("app.windows is an array");
    assert!(
        windows.is_empty(),
        "the shell builds its own window; a window in `app.windows` is created and shown by Tauri before the \
         saved position is restored, which is the jump testers reported: {windows:?}"
    );
}

#[test]
fn the_shell_builds_the_window_hidden_painted_and_then_shows_it() {
    let main = read("apps/shell/src/main.rs");

    // Hidden first, so the plugin's restore happens where nobody can watch it.
    assert!(
        main.contains(".visible(false)"),
        "the window must be built with `.visible(false)`; visible-from-birth is what made the jump visible"
    );
    // A colour handed to the builder method that actually forwards it, not to a config field nobody reads.
    assert!(
        main.contains(".background_color("),
        "the pre-paint surface has to be set through `WebviewWindowBuilder::background_color`, the path tauri \
         2.12.5 forwards to wry and on to WebView2"
    );
    // We own visibility, so a saved state can never launch Jott invisible. With `StateFlags::VISIBLE` in play the
    // plugin shows the window only if the *file* said it was visible — and an MSI upgrade that kills Jott mid-run
    // can leave a file saying it was not.
    assert!(
        main.contains("StateFlags::VISIBLE"),
        "the plugin must be told not to own visibility: `StateFlags::all().difference(StateFlags::VISIBLE)`"
    );

    // The order that removes the jump: created hidden (so the plugin's restore lands unseen), shown afterwards.
    let hidden = main.find(".visible(false)").expect("the window starts hidden");
    let shown = main.find(".show()").expect("something shows the window");
    assert!(
        shown > hidden,
        "the window has to be shown after it is built and restored, not before: `.show()` sits at {shown}, \
         `.visible(false)` at {hidden}"
    );
}

#[test]
fn the_paper_behind_the_page_is_the_paper_in_the_stylesheet() {
    // The surface behind the webview is bytes, so a stylesheet edit that changes the palette has to change them
    // too. Same drift class as the np installer pin.
    let css = read("src/app.css");
    let light = css
        .lines()
        .find(|l| l.trim_start().starts_with("--paper: #"))
        .expect("app.css names a light --paper")
        .split('#')
        .nth(1)
        .unwrap_or_default()
        .trim()
        .trim_end_matches(';')
        .trim()
        .to_ascii_lowercase();
    assert_eq!(light, "fafaf9", "the light paper in app.css is what the shell must paint");

    let main = read("apps/shell/src/main.rs");
    assert!(
        main.contains("Color(250, 250, 249, 255)"),
        "app.css says #{light} but the shell does not hand those bytes to the window"
    );
}

#[test]
fn the_installed_build_still_has_a_console() {
    // Everything this release asks a tester to look at comes down to "open the console and paste this". In a
    // release build that key does nothing unless the app asks for it: wry defaults devtools to off outside debug
    // builds (`wry-0.57.2/src/lib.rs:873-875`), and tauri forwards its own `devtools` setting only under
    // `cfg(any(debug_assertions, feature = "devtools"))` (`tauri-runtime-wry-2.12.3/src/lib.rs:5146`). Jott's config
    // said `"devtools": true` for sixteen releases and the installed build ignored it, exactly as it ignored the
    // colour next to it.
    let manifest = read("apps/shell/Cargo.toml");
    let tauri_line = manifest
        .lines()
        .find(|l| l.trim_start().starts_with("tauri = "))
        .expect("the shell depends on tauri");
    assert!(
        tauri_line.contains("devtools"),
        "the installed Jott has no developer console without tauri's `devtools` feature — and every diagnostic in \
         the tester guide assumes one: {tauri_line}"
    );
}

#[test]
fn the_startup_geometry_reaches_the_log_somebody_can_send_us() {
    // Every claim about this fix that cannot be checked here has to be checkable from their log file.
    let shell = read("apps/shell/src/ipc.rs");
    assert!(
        shell.contains("LocalRecord::new(\"window\")"),
        "the window's size, position and whether a saved state existed must be journalled as `event: \"window\"`"
    );

    // And the event has to be listed where every other event is listed — on the `event` field, which is what
    // somebody reads when they find a line in the log — rather than invented at the call site.
    let core = read("crates/jott-core/src/diagnostics.rs");
    let declared = core
        .split("pub struct LocalRecord")
        .nth(1)
        .unwrap_or_default()
        .chars()
        .take(700)
        .collect::<String>();
    assert!(
        declared.contains("`window`"),
        "the event list on `LocalRecord::event` does not name `window`: {declared}"
    );
}
