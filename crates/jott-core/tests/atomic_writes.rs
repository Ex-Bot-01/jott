//! Issue #4: every file Jott rewrites whole goes temp-then-rename — except the one file it does not own.
//!
//! `VaultStore::save` says the rule out loud ("a temp file in the same directory, then one rename. A crash leaves
//! either the old file or the new one, never half of either"), and `Stamps::save` repeats it in its own words.
//! `set_auto_fetch` (now `set_key`, which also writes `[search] rerank_top_n`) wrote np's `config.toml` with a plain `std::fs::write`: a crash or a power loss mid-write left
//! a truncated file, np then answered its own *config parse error in <path>*, Jott reported `configUnreadable`,
//! and the two keys that carry the consent answer — `embeddings.provider` and `embeddings.model`, written by np
//! itself since note-processor#7 — went with it. It is also the file nobody owns: it may hold somebody's mirror
//! URL, their log level, comments, and keys Jott has never heard of.
//!
//! A truncated file cannot be produced on a test machine on purpose — you cannot crash a process mid-`write` from
//! the outside at will — so this pins the rule where it is checkable, in the source, for all three writers. The
//! behaviour that *is* checkable (a failed write leaves the original whole; a successful one keeps the file's own
//! permission bits) lives with the rest of the consent tests in `np_consent.rs`.

use std::fs;
use std::path::Path;

fn read(rel: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(rel);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{:?}: {e}", path))
}

/// The body of one function, found by its signature and closed by counting braces. A line-based cut would stop at
/// the first `match` arm that happens to sit at four spaces and report a function as shorter than it is.
fn body(file: &str, signature: &str) -> String {
    let source = read(file);
    let at = source.find(signature).unwrap_or_else(|| panic!("{signature} is gone from {file}"));
    let rest = &source[at..];
    let open = rest.find('{').expect("a function body opens");
    let mut depth = 0usize;
    for (index, ch) in rest.char_indices().skip(open) {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return rest[..=index].to_string();
                }
            }
            _ => {}
        }
    }
    panic!("{signature} in {file} never closes");
}

/// Every whole-file rewrite in Jott: the registry, the marker map, and np's config.
#[test]
fn a_rewritten_file_is_written_aside_and_moved_into_place() {
    let writers = [
        ("crates/jott-core/src/vault/store.rs", "fn save(&self, entries: &[VaultEntry])"),
        ("crates/jott-core/src/vault/stamps.rs", "pub fn save(&self) -> Result<(), StoreError>"),
        ("crates/jott-core/src/ingest/consent.rs", "fn set_key(path: &Path, table: &'static str"),
    ];
    for (file, signature) in writers {
        let name = file.rsplit('/').next().unwrap();
        let body = body(file, signature);
        assert!(
            body.contains("tmp-") && body.contains("rename"),
            "{name}: a rewrite of a whole file has to land beside it and be renamed over the old one — that is \
             what makes \"either the old file or the new one, never half of either\" true. A crash during a plain \
             write leaves np answering a config parse error in someone's own settings file."
        );
        assert!(
            !body.contains("std::fs::write(path,"),
            "{name}: still writes the destination in place; write the temp, then rename"
        );
    }
}

/// The temp has to be *beside* the file — a rename across filesystems is not atomic, and `/tmp` frequently is
/// another one. All three writers put it in the destination's own directory today; this says so out loud.
#[test]
fn the_temporary_lives_in_the_same_directory_as_the_file() {
    for (file, signature) in [
        ("crates/jott-core/src/vault/store.rs", "fn save(&self, entries: &[VaultEntry])"),
        ("crates/jott-core/src/vault/stamps.rs", "pub fn save(&self) -> Result<(), StoreError>"),
        ("crates/jott-core/src/ingest/consent.rs", "fn set_key(path: &Path, table: &'static str"),
    ] {
        let body = body(file, signature);
        assert!(
            body.contains("parent()") || body.contains("dir.join"),
            "{}: the temp file must be built from the destination's parent, not from a path of ours",
            file.rsplit('/').next().unwrap()
        );
    }
}
