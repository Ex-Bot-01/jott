//! Issue #2: picking a folder from recents that had been deleted threw away the vault that was working.
//!
//! `App::open` registered the folder and dropped the reason:
//!
//! ```ignore
//! let warning = self.store.register(path).ok().flatten();
//! ```
//!
//! `StoreError::NotADirectory` is a refusal, and `.ok().flatten()` read it as "no warning". The session was
//! replaced anyway, the command still answered `Err` (from `listing()`, one line later), and so core moved on while
//! the window kept drawing the old rows — every command after that answered about a folder nobody could open, until
//! *Open…* picked a folder that exists. The fix is one decision made before anything is switched, which is exactly
//! the kind of thing no test on this machine can run: `App` lives in the shell, and the shell needs WebView2 or
//! webkit to compile at all. So this file pins the ordering that *is* checkable — refuse first, switch second —
//! and `ipc_surface.rs::a_refusal_from_the_registry_arrives_as_the_sentence_it_wrote` pins what the refusal says.

use std::fs;
use std::path::Path;

fn read(rel: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(rel);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{:?}: {e}", path))
}

/// The body of `App::open`, up to its own closing brace. Nested blocks in it close at eight spaces or more, so the
/// first four-space `}` ends the function.
fn open_body() -> String {
    let shell = read("apps/shell/src/ipc.rs");
    let at = shell.find("async fn open(").expect("`App::open` is still there");
    let body = &shell[at..];
    match body.find("\n    }\n") {
        Some(end) => body[..end].to_string(),
        None => body.to_string(),
    }
}

#[test]
fn a_folder_that_is_not_there_is_refused_without_touching_the_session() {
    let body = open_body();

    let registered = body.find(".register(").expect("open still registers the folder");
    let statement_end = registered + body[registered..].find(';').expect("the register call is a statement");
    let statement = &body[registered..statement_end];

    // `.ok().flatten()` is the bug, in one expression: it turns "this folder is not there" into "nothing to report".
    assert!(!statement.contains(".ok()"), "`register` answered a refusal and open still switched vaults: {statement}");
    assert!(
        statement.contains('?') || statement.contains("map_err") || statement.contains("return Err"),
        "the registry's refusal has to reach the window as an error, or a bad pick silently keeps the old listing \
         while core points somewhere else: {statement}"
    );

    // And it has to happen *before* the swap, so a bad pick leaves the working vault alone.
    let replaced = body.find("self.session).replace(").expect("open still installs the new session");
    assert!(
        registered < replaced,
        "the folder is registered after the session is replaced, so nothing can refuse the switch"
    );
}

#[test]
fn a_refused_open_still_leaves_the_nested_vault_warning_as_it_was() {
    // §8.2: a vault inside a vault is legal and gets one sentence about the second index it means. Issue #2's fix
    // must not have traded that sentence away to get the refusal — the warning half still comes from `register`.
    let body = open_body();
    assert!(
        body.contains("RegisterWarning::NestedVault"),
        "the nested-vault warning is built from `register`'s answer; the error half was what needed fixing"
    );
}
