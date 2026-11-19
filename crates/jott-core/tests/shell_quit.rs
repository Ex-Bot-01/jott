//! Issue #1: §11's close-out — drain the queue up to its cap, save the stamps, ask both np children to shut
//! down — was implemented as a command and never asked. `flush_on_close` sat in the handler list, `flushOnClose`
//! in `src/lib/ipc.ts` had one caller (`saveBeforeUnload`) which had none, `<svelte:window>` bound no unload
//! hook, and the shell's event loop was `.run(|_app, _event| {})`. So the whole path was dead code: a save made
//! seconds before quitting never got its pass, and np died by `kill_on_drop` with its index still open — which is
//! exactly the Windows lock friction §3 is about.
//!
//! The close-out itself cannot be exercised here: it needs a running event loop, and no machine that runs these
//! tests has a display (`shell_window.rs` says the same about the window). What these pins hold is the wiring —
//! that something in the shell *asks* at quit, and that the asking goes through the one body the headless suite
//! covers (`ipc_surface.rs`, `quitting_*`). Structure here, behaviour there; neither alone is enough.

use std::fs;
use std::path::Path;

fn read(rel: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(rel);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{:?}: {e}", path))
}

/// The body of the `.run(...)` callback in `main.rs`, from the closure to the end of the file.
fn run_callback(main: &str) -> &str {
    let at = main.find(".run(").expect("main.rs runs the built app");
    &main[at..]
}

#[test]
fn the_shell_runs_the_close_out_when_it_is_told_to_quit() {
    let main = read("apps/shell/src/main.rs");
    let body = run_callback(&main);

    // An empty callback is the bug itself, spelled out so a regression says why it matters.
    assert!(
        !body.contains("|_app, _event| {}"),
        "the run callback ignores its events, so §11's close-out never runs: a save still in the queue loses \
         its pass and np is killed rather than asked"
    );
    assert!(
        body.contains("RunEvent::ExitRequested"),
        "quit must be handled where an async close-out can actually be awaited; `tauri-runtime-wry-2.12.3` \
         emits `ExitRequested` when the last window is destroyed and waits for this callback to return"
    );
    assert!(
        body.contains("close_for_quit"),
        "the quit path has to run the same body as the `flush_on_close` command, not a second copy of it"
    );
}

#[test]
fn the_command_and_the_quit_path_share_one_close_out_body() {
    let shell = read("apps/shell/src/ipc.rs");
    let at = shell
        .find("pub async fn flush_on_close")
        .expect("the command is still on the surface");
    let body = &shell[at..];

    // One body, two doors. Two copies is how the command came to drift from what §11 asks for in the first place.
    assert!(
        body.contains("flush_on_quit"),
        "`flush_on_close` must delegate to `jott_core::ipc::flush_on_quit`, the function the headless suite \
         covers — a hand-rolled copy here is invisible to every test on this machine"
    );
    assert!(
        !body.contains("session.close().await"),
        "the command keeps its own copy of the close-out; the shell's job is to take the session and the warm \
         child out of their mutexes and hand them to core"
    );
}

#[test]
fn the_close_out_lives_in_core_where_it_can_be_tested() {
    let core = read("crates/jott-core/src/ipc.rs");
    assert!(
        core.contains("pub async fn flush_on_quit"),
        "the close-out belongs in the core, where `ipc_surface.rs` can run it against the stand-in np"
    );
}

#[test]
fn nothing_in_the_window_claims_to_save_on_unload() {
    let ipc = read("src/lib/ipc.ts");
    // `Editor.svelte`'s `pagehide` handler saves through `save_note`, and that is the honest half: an invoke
    // fired from an unload hook may never reach Rust. The helper that promised otherwise had no caller and was
    // the reason issue #1 looked handled.
    assert!(
        !ipc.contains("saveBeforeUnload"),
        "`saveBeforeUnload` promises a save on unload through a channel Tauri does not guarantee; the quit \
         close-out is on the Rust side now"
    );
}
