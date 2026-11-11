//! The folder walk: the vault on disk turned into the tree in §9.
//!
//! Three rules hold it together.
//!
//! **Nothing vanishes.** A directory that cannot be read appears as one refused entry, because an
//! empty-looking folder is a lie about someone's notes. (A file that disappears between listing
//! and reading is not reported — there is no size, no mtime and no content to speak of, and np's
//! own walk makes the same call.)
//!
//! **No invented states.** A marker comes from np's verdict through [`Stamps`], or from something
//! this walk measured itself. Files the walk cannot judge — folders, images, attachments, np's own
//! generated catalogs — carry no marker at all rather than a comfortable `Waiting`.
//!
//! **Order is bytes.** Sorted by the bytes of `rel`, with `/` as the separator on every platform,
//! so the same vault lists the same way on Windows and Linux and matches the paths np reports.

use super::stamps::{StampState, Stamps};
use crate::hashes::sha256_file;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use thiserror::Error;
use walkdir::WalkDir;

/// Above this size a "note" is not something Jott can put in a text buffer and read, so it is
/// named and shown rather than hashed into the ground. Chosen as an editor limit, not as a claim
/// about anyone's corpus: 32 MiB of markdown is millions of words.
pub const MAX_NOTE_BYTES: u64 = 32 * 1024 * 1024;

/// What a tree row is, in the vocabulary §9 uses. A file with no extension np cares about is an
/// `Attachment`: np parses none of them and Jott edits none of them, but they are in someone's
/// notes folder and hiding them to keep our taxonomy tidy is the wrong trade.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    Folder,
    Note,
    Image,
    Attachment,
    /// `INDEX.md` and its shards: written by np, greyed in the tree, never edited here.
    Generated,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeEntry {
    /// Workspace-relative path with `/` separators — the same spelling np uses in its reports.
    pub rel: String,
    pub kind: EntryKind,
    /// Bytes of a file; `0` for a folder, which has no size worth showing.
    pub size: u64,
    pub mtime: SystemTime,
    /// The marker §9 draws. `None` means this row has no index state to show, not that it is
    /// waiting for one.
    pub state: Option<StampState>,
}

#[derive(Debug, Error)]
pub enum WalkError {
    #[error("{path} is not a folder I can open")]
    NotAFolder { path: PathBuf },
    #[error("could not read the vault at {}: {source}", .path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Walk `root` and return every row the tree shows, sorted by `rel` bytes. The root itself is not
/// a row — it is the tree's header — and its own name is never held against it: a vault at
/// `~/.notes` is a vault.
pub fn walk(root: &Path, stamps: &Stamps) -> Result<Vec<TreeEntry>, WalkError> {
    if !root.is_dir() {
        return Err(WalkError::NotAFolder { path: root.to_path_buf() });
    }
    let mut entries: Vec<TreeEntry> = Vec::new();
    // Folders whose contents could not be listed. The row for the folder itself usually already
    // exists — its parent listed it — so the refusal is written onto that row here rather than
    // added as a second, contradictory row for the same folder.
    let mut unreadable: Vec<String> = Vec::new();
    for item in WalkDir::new(root).min_depth(1).follow_links(false) {
        match item {
            Ok(dir_entry) => {
                let path = dir_entry.path();
                let Some(rel) = rel_of(root, path) else { continue };
                if is_hidden_below_root(&rel) {
                    // np excludes dot-names at every depth; matching it keeps the tree and the
                    // index talking about the same corpus.
                    continue;
                }
                entries.push(entry_for(&dir_entry, &rel, stamps));
            }
            Err(source) => {
                // A walk error always names the path it tripped over except in cases walkdir
                // itself cannot name; those are skipped rather than guessed at.
                let Some(path) = source.path().map(Path::to_path_buf) else { continue };
                if source.depth() == 0 {
                    return Err(WalkError::Io { path, source: io_error(&source) });
                }
                if path.is_dir() {
                    if let Some(rel) = rel_of(root, &path) {
                        unreadable.push(rel);
                    }
                }
                // A file whose metadata or name we could not read is not reportable: there is no
                // row to draw without a path or a size. np's walk drops the same case.
            }
        }
    }
    for rel in unreadable {
        let refused = Some(StampState::Refused("unreadable-dir".into()));
        match entries.iter_mut().find(|entry| entry.rel == rel) {
            Some(entry) => entry.state = refused,
            None => entries.push(TreeEntry {
                rel,
                kind: EntryKind::Folder,
                size: 0,
                mtime: SystemTime::UNIX_EPOCH,
                state: refused,
            }),
        }
    }
    entries.sort_by(|a, b| a.rel.as_bytes().cmp(b.rel.as_bytes()));
    Ok(entries)
}

/// The `rel`s of the notes np indexes — the same corpus the tree draws markers for, without the
/// rows. Used by rename's link scan, which wants every note's text and nothing else.
///
/// Dot-names and anything under `processed` are excluded because np excludes them from its index
/// (measured in 0.3.1: `walk::is_excluded`), so a link written there is not in the index either;
/// np's own generated catalogs are excluded because they hold no links anybody wrote.
pub fn note_rels(root: &Path) -> Result<Vec<String>, WalkError> {
    if !root.is_dir() {
        return Err(WalkError::NotAFolder { path: root.to_path_buf() });
    }
    let mut notes = Vec::new();
    for item in WalkDir::new(root).min_depth(1).follow_links(false) {
        let Ok(dir_entry) = item else { continue };
        if !dir_entry.file_type().is_file() {
            continue;
        }
        let Some(rel) = rel_of(root, dir_entry.path()) else { continue };
        if is_hidden_below_root(&rel) || is_under_processed(&rel) {
            continue;
        }
        let name = dir_entry.file_name().to_string_lossy();
        if !rel.contains('/') && is_generated_index(&name) {
            continue;
        }
        if dir_entry.path().extension().is_some_and(|e| e.eq_ignore_ascii_case("md")) {
            notes.push(rel);
        }
    }
    notes.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    Ok(notes)
}

fn entry_for(dir_entry: &walkdir::DirEntry, rel: &str, stamps: &Stamps) -> TreeEntry {
    let path = dir_entry.path();
    let metadata = dir_entry.metadata().ok();
    let size = metadata.as_ref().map(|m| if m.is_dir() { 0 } else { m.len() }).unwrap_or(0);
    // A filesystem that will not answer for an mtime gets no timestamp rather than a plausible one;
    // the tree shows "unknown" for these (Task 10) instead of a date nobody measured.
    let mtime = metadata
        .as_ref()
        .and_then(|m| m.modified().ok())
        .unwrap_or(SystemTime::UNIX_EPOCH);

    if dir_entry.file_type().is_dir() {
        return TreeEntry { rel: rel.to_string(), kind: EntryKind::Folder, size, mtime, state: None };
    }

    let name = dir_entry.file_name().to_string_lossy().into_owned();
    // np writes its catalogs at the workspace root alone; a nested `INDEX-old.md` is an ordinary
    // note, and matching exact case keeps someone's `index.md` a note too.
    if !rel.contains('/') && is_generated_index(&name) {
        return TreeEntry { rel: rel.to_string(), kind: EntryKind::Generated, size, mtime, state: None };
    }

    let extension = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let kind = match extension.as_str() {
        "md" => EntryKind::Note,
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" => EntryKind::Image,
        _ => EntryKind::Attachment,
    };
    // Only notes have a state to show: np's index rows are notes, and an image row would need the
    // vision feature the released binaries do not carry (§18.7).
    let state = (kind == EntryKind::Note && !is_under_processed(rel)).then(|| note_state(path, rel, stamps));
    TreeEntry { rel: rel.to_string(), kind, size, mtime, state }
}

/// What the walk itself can prove about a note's bytes, and what it has to ask the stamps about.
fn note_state(path: &Path, rel: &str, stamps: &Stamps) -> StampState {
    let Ok(metadata) = path.metadata() else {
        return StampState::Refused("unreadable".into());
    };
    if metadata.len() > MAX_NOTE_BYTES {
        return StampState::Refused("too-large".into());
    }
    let Ok(bytes) = std::fs::read(path) else {
        return StampState::Refused("unreadable".into());
    };
    if std::str::from_utf8(&bytes).is_err() {
        // Jott's refusal, not np's: np indexes such bytes lossily. The editor holds text, so this
        // file cannot be opened here — and the tree says that instead of claiming a state nobody
        // measured.
        return StampState::Refused("not-utf8".into());
    }
    match sha256_file(path) {
        Ok(hash) => stamps.state_of(rel, &hash),
        Err(_) => StampState::Refused("unreadable".into()),
    }
}

/// The path under the root, in np's spelling: `/` separated, and text (a name that is not valid
/// UTF-8 arrives lossily, which is how np displays it too).
fn rel_of(root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(root).ok()?;
    let mut parts: Vec<String> = Vec::new();
    for component in relative.components() {
        if let std::path::Component::Normal(name) = component {
            parts.push(name.to_string_lossy().into_owned());
        }
    }
    if parts.is_empty() {
        return None;
    }
    Some(parts.join("/"))
}

/// Whether this path sits under a `processed` folder. np's walk excludes that name at every depth
/// (measured in 0.3.1: `walk::is_excluded`), so nothing below it will ever be indexed — and Jott
/// still shows the folder, because hiding someone's files is not how you avoid a wrong marker.
/// What goes missing is the marker, which would otherwise promise an index that never comes (§4).
fn is_under_processed(rel: &str) -> bool {
    rel.split('/').any(|part| part == "processed")
}

fn is_hidden_below_root(rel: &str) -> bool {
    rel.split('/').any(|part| part.starts_with('.'))
}

/// np's own rule (`np-core/src/index_names.rs`): `INDEX.md`, or a name that starts `INDEX-` and
/// ends `.md`, matched at exact case because those names are np's to write.
pub fn is_generated_index(name: &str) -> bool {
    let Some(stem) = name.strip_suffix(".md") else { return false };
    stem == "INDEX" || stem.strip_prefix("INDEX").is_some_and(|rest| rest.starts_with('-'))
}

/// The OS error behind a walk failure, rebuilt rather than cloned (`io::Error` is not `Clone`).
fn io_error(source: &walkdir::Error) -> std::io::Error {
    match source.io_error().and_then(|e| e.raw_os_error()) {
        Some(code) => std::io::Error::from_raw_os_error(code),
        None => std::io::Error::other(source.to_string()),
    }
}
