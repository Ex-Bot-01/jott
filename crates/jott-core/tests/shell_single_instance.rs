//! Two Jott processes on one vault used to be possible, and each believed it owned the folder. The registry is
//! rewritten whole, so whichever process saved last wrote the list it had loaded at its own startup — a vault the
//! other window added disappeared (`crates/jott-core/src/vault/store.rs` merges now, which is why losing a race no
//! longer costs a recents entry). Stamps are worse: both processes hold their own `BufferedWriter` state and both
//! write the same stamp file, so a note one window marked done can come back as *waiting* after the other writes.
//! `App::open` already refuses two sessions inside one process for exactly this reason — "two queues on one
//! workspace would race over np's lock and both write the same stamp file, so this is not a nicety" — and that
//! reasoning simply stopped at the process boundary.
//!
//! The fix is `tauri-plugin-single-instance`, which issue #7 called the easy way out and which fits the design:
//! `App` holds one session slot and `capabilities/default.json` scopes every permission to a window named `main`,
//! so a second window on a second vault is not a thing Jott can do anyway. The plugin's callback runs in the
//! *first* instance (argv is handed over the named mutex on Windows, the D-Bus name on Linux, the Unix socket on
//! macOS), which is what makes "focus the window that already exists" possible at all.
//!
//! What these pins hold is structure — this box cannot run a GUI, and a second process with no display tells us
//! nothing. They check that the plugin is in the manifest CI builds from, that its callback wakes the window by
//! the same label the shell builds it under, and that nothing releases the lock by a route the plugin's own docs
//! warn against (`tauri-plugin-single-instance-2.5.5/src/lib.rs:56-62`).

use std::fs;
use std::path::Path;

fn read(rel: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(rel);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{:?}: {e}", path))
}

#[test]
fn the_manifest_carries_the_plugin_and_the_lock_pins_its_version() {
    let manifest = read("apps/shell/Cargo.toml");
    assert!(
        manifest.contains("tauri-plugin-single-instance"),
        "without this dependency a second `jott.exe` starts beside the first and both write one vault's stamps"
    );

    // The lock is what CI builds from. A version bump that changes how the lock is taken (or when it is released)
    // should be read in the new source before this file's reasoning is trusted.
    let lock = read("apps/shell/Cargo.lock");
    let at = lock
        .find("name = \"tauri-plugin-single-instance\"")
        .expect("the plugin is in Cargo.lock, so CI builds the version this file describes");
    let version = lock[at..at + 200]
        .lines()
        .find(|l| l.starts_with("version ="))
        .expect("a version line next to the package entry");
    assert!(
        version.contains("2."),
        "the plugin's callback signature and lock lifetime are read from v2 sources: {version}"
    );
}

#[test]
fn a_second_launch_wakes_the_window_that_is_already_there() {
    let main = read("apps/shell/src/main.rs");

    let at = main
        .find("tauri_plugin_single_instance::init(")
        .expect("the plugin has to be installed on the builder, not merely depended upon");
    // Plugins are attached before `.build()`, so the lock is taken before any window exists: an instance that
    // loses the race should exit without ever painting a window.
    assert!(
        main[..at].contains("Builder::default()") || main[..at].contains(".default_plugins"),
        "the single-instance plugin must be attached while building the app, before setup runs"
    );

    let callback = &main[at..at + 1200];
    // The callback fires in the instance that already owns the lock, so it has to reach *its* window. Anything
    // less than these three leaves a Jott that is running and invisible: `set_focus` alone does nothing to a
    // minimized window, and `show` alone leaves it behind other windows on all three platforms.
    for call in ["unminimize", "show", "set_focus"] {
        assert!(callback.contains(call), "a second launch must {call} the existing window");
    }
    assert!(
        callback.contains("\"main\""),
        "the window it wakes is the one `setup` builds, and both spellings of that label have to match"
    );

    // Same label as the real window, or the wake silently does nothing.
    let built = main
        .find("WebviewWindowBuilder::new(app, \"main\"")
        .or_else(|| main.find("WebviewWindowBuilder::new(&app, \"main\""))
        .expect("the shell builds its window under a literal label");
    assert!(built > at, "the window is built in setup, after the plugin is attached");
}

/// The plugin releases its lock in `AppHandle::cleanup_before_exit`, which Tauri runs on a normal exit *and* on
/// `app.exit(0)` — the path issue #1's quit handler takes. It does not run for `std::process::exit`. Getting this
/// wrong is not cosmetic: the lock survives, every later launch decides an instance is already running and quits
/// immediately, and the only way back is logging out (Windows) or clearing a stale D-Bus name (Linux). A quit bug
/// that turned Jott permanently un-launchable would be the worst thing this repo has shipped.
#[test]
fn nothing_releases_the_lock_by_a_route_that_skips_cleanup() {
    let main = read("apps/shell/src/main.rs");
    for forbidden in ["std::process::exit", "process::exit(", ".abort()"] {
        assert!(
            !main.contains(forbidden),
            "`{forbidden}` skips `cleanup_before_exit`, so the single-instance lock would outlive the process \
             and Jott could never start again without logging out"
        );
    }
    assert!(
        !main.contains("tauri_plugin_single_instance::destroy"),
        "releasing the lock by hand invites a window where two instances coexist; Tauri's own cleanup does it"
    );
}
