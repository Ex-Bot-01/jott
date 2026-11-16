//! The window Jott shows is built by `apps/shell/src/main.rs`, not by `tauri.conf.json`. This file keeps it that
//! way, and keeps straight *why* — which matters, because I had the why wrong once already and shipped the wrong
//! sentence into four files.
//!
//! What the pinned crates actually say (`apps/shell/Cargo.lock` names these exact versions):
//!
//! * **The move is real.** `tauri-2.12.5/src/app.rs:2691` builds every window named in the config, and such a window
//!   is made visible during creation — `visible` defaults to true (`tauri-utils-2.10.2/src/config.rs:2072`), is applied
//!   by the runtime (`tauri-runtime-wry-2.12.3/src/lib.rs:857`) and set in tao's `init`. Only then does our `setup`
//!   closure run (`app.rs:2697`). The window-state plugin restores geometry from `on_window_ready`
//!   (`tauri-plugin-window-state-2.5.0/src/lib.rs:433`), which runs synchronously inside that creation, and it does so
//!   with ordinary `set_position`/`set_size` calls (`:213`, `:230`). A window that has a saved state therefore appears
//!   where Windows put it and then moves. Built hidden, restored during `build()`, shown afterwards: the move happens
//!   where nobody can see it.
//! * **The precondition is the part I overclaimed.** The plugin restores only when the label is in its file
//!   (`:198-244`); with no `.window-state.json` it takes the else-branch at `:245` and never moves anything. So this
//!   fix addresses launches *with* a saved state — including an upgrade install, which keeps
//!   `%APPDATA%\dev.jott.desktop\.window-state.json` while counting as "freshly installed". A jump on a genuinely
//!   first run is something else, DPI renegotiation being the live candidate
//!   (`tao-0.37.2/src/platform_impl/windows/event_loop.rs:1837+`, which moves windows after `show()`).
//! * **The colour was never the problem.** I wrote that a config colour reached only the window. Wrong:
//!   `tauri-runtime-2.12.2/src/webview.rs:498` fills `WebviewAttributes::background_color` from the same config field,
//!   and that attribute is precisely what `tauri-runtime-wry-2.12.3/src/lib.rs:4779` hands to wry for WebView2's
//!   `SetDefaultBackgroundColor`. `v0.0.18`'s config spelled it correctly (`git show v0.0.18:apps/shell/tauri.conf.json`:
//!   `"backgroundColor": [250, 250, 249, 255]`). So the colour lives in code now only because this is the sole place a
//!   code-built window can be given one — and **the white first frame remains unexplained.** What has been ruled out:
//!   a GDI white erase (tao paints its own background in `WM_ERASEBKGND`, `event_loop.rs:1042-1050`, with a NULL class
//!   brush at `:641`; wry's container class is likewise NULL-brushed, `wry-0.57.2/src/webview2/mod.rs:222-235`). What
//!   is left: WebView2 runtimes without `ICoreWebView2ControllerOptions3`, where the colour lands later
//!   (`wry webview2/mod.rs:409-431`, `:464-468`), and WebView2's own surface before its first frame.
//!
//! One pin here is not about the window, and sits with these because it was found reading the same config object: the
//! installed build had no developer console, while `docs/testing-on-windows.md` spent sixteen releases telling people
//! to open one. See [`the_installed_build_still_has_a_console`].
//!
//! What these pins can prove is structure — who creates the window, whether it starts hidden, whether the show is
//! unconditional, which bytes were handed to what. None of it proves what a screen looks like: no machine running
//! these tests has a display, and jsdom cannot compute style. §2b-ix of `docs/testing-on-windows.md` asks a person to
//! look, and the geometry Jott showed at is journalled (`event: "window"`) so the next report can be read rather than
//! described.

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

    // Hidden first, so the plugin's restore happens where nobody can watch it — and nothing may turn it visible
    // again before `build()`, since the last `visible` call on the chain wins (`webview_window.rs:934-936`).
    let built = main.find(".build()?").expect("the window is built in setup");
    let head = &main[..built];
    assert!(
        head.contains(".visible(false)"),
        "the window must be built with `.visible(false)`; visible-from-birth is what made the jump visible"
    );
    assert_eq!(
        head.matches(".visible(").count(),
        1,
        "a second `.visible(..)` on the builder chain would silently win over the first one"
    );
    // A colour handed to the builder — pinned to the stylesheet elsewhere, and worth keeping here even though it is
    // no longer claimed to fix the white frame.
    assert!(
        main.contains(".background_color("),
        "with no config window, this is the only place the surface behind the page can be set"
    );
    // The plugin must not own visibility. Pinned as an expression rather than a name: leaving `StateFlags::VISIBLE`
    // in a comment while the call dropped the difference would otherwise stay green.
    assert!(
        main.contains("StateFlags::all().difference(StateFlags::VISIBLE)"),
        "the plugin has to be built with exactly this flag set, or it decides whether Jott is visible at all"
    );

    // The show is unconditional: no condition, no task, no timer between building the window and showing it. That is
    // what keeps "we never ship an invisible app" true rather than merely intended — decision 5 in the plan for this
    // release refuses the usual show-after-paint trick for exactly this reason.
    let after = &main[built..];
    let shown = after.find(".show()").expect("something shows the window");
    let between = &after[..shown];
    for forbidden in ["if ", "spawn", "sleep", "await"] {
        assert!(
            !between.contains(forbidden),
            "`{forbidden}` between building and showing means Jott can start with no window on screen"
        );
    }
}

#[test]
fn the_window_keeps_the_shape_the_config_gave_it() {
    // The numbers came out of `tauri.conf.json` at v0.0.18 (`git show v0.0.18:apps/shell/tauri.conf.json`) and went
    // into code, where nothing but this test would notice one being dropped or retyped.
    let main = read("apps/shell/src/main.rs");
    for needle in [".inner_size(1200.0, 760.0)", ".min_inner_size(720.0, 480.0)"] {
        assert!(main.contains(needle), "the window lost a size the config had: {needle}");
    }
    // Every permission in `apps/shell/capabilities/default.json` is scoped to this label, and it is the key the
    // plugin saves geometry under. A rename would deny IPC and forget where the window lives, quietly.
    assert!(main.contains("\"main\""), "the window's label is part of the permission model");
    let caps = read("apps/shell/capabilities/default.json");
    assert!(caps.contains("\"main\""), "the capability file no longer names the window Jott builds");
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
    // builds (`wry-0.57.2/src/lib.rs:872-875`, applied unconditionally at `webview2/mod.rs:651`), and tauri forwards
    // its own setting only under `cfg(any(debug_assertions, feature = "devtools"))`
    // (`tauri-runtime-wry-2.12.3/src/lib.rs:5146-5148`). Jott's config said `"devtools": true` for sixteen releases
    // and the installed build ignored it, exactly as a config colour would have been ignored had I been right about
    // colours: the difference between those two sentences is one `impl From` in a crate nobody thought to grep.
    let manifest = read("apps/shell/Cargo.toml");
    let tauri_line = manifest
        .lines()
        .find(|l| l.trim_start().starts_with("tauri = "))
        .expect("the shell depends on tauri");
    assert!(
        tauri_line.contains("features = [") && tauri_line.contains("\"devtools\""),
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
    // Named by the plugin's own constant, so the sentence cannot drift away from the file it reports on.
    assert!(
        shell.contains("tauri_plugin_window_state::DEFAULT_FILENAME"),
        "the log line must ask the plugin what its state file is called, not guess a filename"
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
