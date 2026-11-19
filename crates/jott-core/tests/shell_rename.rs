//! A rename reads every note in the vault before it moves one file — the scan behind "what refers to this?"
//! (§18.7) — so it is the one command whose cost grows with the size of the vault rather than with what the person
//! just did (issue #8). Two shapes keep that from getting worse by accident, and both are about where the work
//! happens rather than how fast it is:
//!
//! * the scan shares **one** `PathGuard`, so opening four thousand notes does not mean asking the OS what the vault
//!   folder is four thousand times (`files.rs`'s own counters pin the count; these pins keep the shell honest);
//! * the command runs off the thread that paints. A non-`async` command is resolved inline wherever the IPC message
//!   was handled — `tauri-macros-2.x/src/command/wrapper.rs` wraps such a body in `body_blocking`, which runs it on
//!   the calling context instead of handing it to the async runtime — so a synchronous rename stalled the window
//!   with nothing on screen to say why.

use std::fs;
use std::path::Path;

fn read(rel: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(rel);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{:?}: {e}", path))
}

#[test]
fn the_rename_command_runs_off_the_thread_that_paints() {
    let ipc = read("apps/shell/src/ipc.rs");
    let at = ipc
        .find("pub async fn rename_entry(")
        .expect("`rename_entry` must be async, or its whole-vault scan runs inline in the IPC handler");

    let body = &ipc[at..at + 900];
    assert!(
        body.contains("spawn_blocking"),
        "async alone would put a long blocking scan on an async-runtime worker; the vault walk belongs on the \
         blocking pool"
    );
    // The session has to be cloned out before the closure: `State<'_, App>` borrows the managed value for the
    // request's lifetime, and a blocking task must own what it touches. Written the other way round it does not
    // compile, and the obvious wrong fix is to make the command sync again.
    assert!(
        body.find("let session = app.current()?").is_some() && body.find("spawn_blocking") > body.find("let session"),
        "take the session before spawning, then move it in"
    );
}

#[test]
fn the_one_file_commands_stay_where_they_were() {
    let ipc = read("apps/shell/src/ipc.rs");
    // Not a claim that sync is correct in general — a claim about *this* fix. Each of these touches one file, so
    // an async handoff would add a round trip and change nothing a person can feel; making them all async "for
    // consistency" would also silently reorder how their replies interleave with the watcher's refresh.
    for command in ["save_note", "create_note", "create_folder", "delete_entry", "move_entry"] {
        assert!(
            ipc.contains(&format!("pub fn {command}(")),
            "`{command}` touches one file; it stays synchronous until something measures a reason not to"
        );
    }
}
