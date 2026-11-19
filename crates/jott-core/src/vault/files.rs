//! Reading and writing the files in someone's vault, byte for byte (§18.8), inside a folder that is
//! the only place any of this is allowed to touch.
//!
//! Two rules hold the whole module together:
//!
//! * **The bytes are the note.** No newline added, no CRLF flattened, no BOM stripped. np hashes raw
//!   bytes (`np-core/src/hash.rs`), so a save that "tidies" tells it the vault changed and re-embeds
//!   it forever — on Windows, every save, forever.
//! * **The vault root is a wall.** Every entry point goes through [`PathGuard`], which refuses `..`,
//!   absolute paths, and a symlink whose target lands outside. A path that escapes is not a bug to
//!   fix later; it is the file someone did not ask you to open.

use crate::app_paths::app_data_dir;
use crate::vault::walk::{note_rels, MAX_NOTE_BYTES};
use crate::vault::watch::SelfWrites;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FileError {
    #[error("{rel} is outside the vault")]
    EscapesRoot { rel: String },
    #[error("{rel} is not there")]
    NotFound { rel: String },
    #[error("{rel} is already there")]
    AlreadyExists { rel: String },
    #[error("cannot move {rel} inside itself")]
    IntoItself { rel: String },
    #[error("{rel} is larger than the editor can hold ({limit} bytes)")]
    TooLarge { rel: String, limit: u64 },
    #[error("{rel} is one of np's own catalog files, which Jott does not write over")]
    NpGenerated { rel: String },
    #[error("{rel} is a name Windows keeps for a device (`nul`, `aux`, `com1` …): the bytes would go to the device, not the folder")]
    ReservedName { rel: String },
    #[error("could not use {rel}: {source}")]
    Io {
        rel: String,
        #[source]
        source: std::io::Error,
    },
}

/// What the UI should do when the file under an open buffer changed (§14).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReloadPolicy {
    /// The bytes on disk are what the buffer already holds, or nothing external happened. Do nothing.
    NoChange,
    /// Somebody else edited it and the buffer has nothing of ours to lose: show the new bytes.
    ReloadSilently,
    /// Somebody else edited it while the buffer holds unsaved typing. Keep the buffer and say so —
    /// §14's "never overwrite unsaved work" is the whole variant.
    KeepAndWarn,
}

/// The policy, as a pure function of what the buffer holds and what the disk says now.
///
/// Hashes decide, because an mtime lies: a sync client or `git checkout` rewrites identical bytes and
/// moves the timestamp with no change to show for it. The timestamps still matter in the other
/// direction — if the disk has not moved since we read it, a difference belongs to the person typing,
/// not to a second editor, and reloading would throw away their work on a false alarm.
pub fn decide(
    buffer_dirty: bool,
    disk_mtime: SystemTime,
    last_read: SystemTime,
    buffer_hash: &str,
    disk_hash: &str,
) -> ReloadPolicy {
    if buffer_hash == disk_hash {
        // Includes the case that matters most: we just wrote these bytes ourselves.
        return ReloadPolicy::NoChange;
    }
    if disk_mtime <= last_read {
        return ReloadPolicy::NoChange;
    }
    if buffer_dirty {
        ReloadPolicy::KeepAndWarn
    } else {
        ReloadPolicy::ReloadSilently
    }
}

// How much work the calls on *this thread* have made the operating system do, for issue #8's tests:
// `(times the vault root was canonicalised, notes read)`. Counters rather than timings, because a CI runner's
// clock says nothing useful and a shape is what this file can answer — rename used to resolve the same directory
// once per note it opened. Thread-local so the suite's parallel tests cannot count each other's syscalls.
#[cfg(test)]
thread_local! {
    pub(crate) static SYSCALLS: std::cell::Cell<(usize, usize)> = const { std::cell::Cell::new((0, 0)) };
}

#[cfg(test)]
pub(crate) fn reset_syscall_counters() {
    SYSCALLS.with(|c| c.set((0, 0)));
}

#[cfg(test)]
pub(crate) fn syscall_counters() -> (usize, usize) {
    SYSCALLS.with(std::cell::Cell::get)
}

#[cfg(test)]
fn counted_canonicalise_root(root: &Path) -> Result<PathBuf, FileError> {
    SYSCALLS.with(|c| c.set((c.get().0 + 1, c.get().1)));
    root.canonicalize().map_err(|source| FileError::Io { rel: root.display().to_string(), source })
}

#[cfg(test)]
fn counted_read(path: &Path) -> std::io::Result<Vec<u8>> {
    SYSCALLS.with(|c| c.set((c.get().0, c.get().1 + 1)));
    std::fs::read(to_open_path(path))
}

/// Which path to hand the OS for this `rel`, and nothing more. See [`to_open_path`] for the Windows
/// part of that job.
pub struct PathGuard {
    root: PathBuf,
    /// What the OS calls `root`, worked out the first time a name needs it and kept for the rest of this
    /// guard's life. Issue #8: one rename used to ask this question once per note it opened, because every
    /// `read_note` built its own guard, and a vault on a network share or a symlinked mount pays for each
    /// asking. Cached inside one guard rather than globally because the guard *is* the operation — a session
    /// that outlives a remount still gets a fresh answer from the next guard.
    canonical_root: std::sync::OnceLock<PathBuf>,
}

impl PathGuard {
    pub fn new(root: &Path) -> Self {
        Self { root: root.to_path_buf(), canonical_root: std::sync::OnceLock::new() }
    }

    /// The folder this guard bounds, as it was handed over — not canonicalised, because a walk wants the
    /// spelling the caller recognises and only the bound-checking needs the OS's own.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The vault folder as the operating system names it, asked at most once per guard.
    fn canonical_root(&self) -> Result<PathBuf, FileError> {
        if let Some(root) = self.canonical_root.get() {
            return Ok(root.clone());
        }
        // A root that cannot be resolved is not cached: one transient failure must not become the answer for
        // every later name in the same command.
        let root = canonicalise_root(&self.root)?;
        Ok(self.canonical_root.get_or_init(|| root).clone())
    }

    /// The absolute path to open for `rel`, or [`FileError::EscapesRoot`].
    ///
    /// Two checks, because one is not enough: the name itself must stay inside (`..`, absolute paths),
    /// and every existing link along the way must land inside — a `link.md` that points at
    /// `/etc/passwd` is still inside the folder and still somebody else's file.
    pub fn resolve(&self, rel: &str) -> Result<PathBuf, FileError> {
        let escaped = || FileError::EscapesRoot { rel: rel.to_string() };
        if rel.is_empty() || rel.contains('\0') {
            return Err(escaped());
        }
        // Both separator spellings mean the same thing here: a `rel` copied out of a Windows error
        // message is as much a path as one from the tree.
        if rel.starts_with('/') || rel.starts_with('\\') || looks_like_drive_path(rel) {
            return Err(escaped());
        }
        let mut lexical = self.root.clone();
        for part in rel.split(['/', '\\']) {
            match part {
                "" | "." => continue,
                ".." => return Err(escaped()),
                _ => lexical.push(part),
            }
        }

        // The root is what the OS says it is, so a vault reached through a symlink or a mount still
        // bounds itself.
        let canonical_root = self.canonical_root()?;
        let mut cursor = canonical_root.clone();
        for part in rel.split(['/', '\\']).filter(|p| !p.is_empty() && *p != ".") {
            cursor.push(part);
            let Ok(meta) = std::fs::symlink_metadata(to_open_path(&cursor)) else {
                // Nothing here yet, so nothing below it can exist to escape through. This is the
                // normal case for a note being created.
                break;
            };
            if meta.file_type().is_symlink() {
                let real = to_open_path(&cursor).canonicalize().map_err(|source| FileError::Io {
                    rel: cursor.display().to_string(),
                    source,
                })?;
                if !stays_inside(&canonical_root, &real) {
                    return Err(escaped());
                }
            }
        }
        Ok(lexical)
    }
}

fn looks_like_drive_path(rel: &str) -> bool {
    let bytes = rel.as_bytes();
    // `C:\…` and `C:/…`. Checked by hand because on Linux `Path::is_absolute` says no to both, and
    // "not absolute" is not the same as "inside the vault".
    bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic()
}

fn stays_inside(root: &Path, candidate: &Path) -> bool {
    #[cfg(windows)]
    {
        return windows_path_is_under(&root.to_string_lossy(), &candidate.to_string_lossy());
    }
    #[cfg(not(windows))]
    {
        candidate == root || candidate.starts_with(root)
    }
}

/// Windows compares paths case-insensitively, and a long path carries the extended-length prefix
/// while its root may not — so `Path::starts_with` alone calls a note outside the vault it is inside.
/// Written out here (and tested on every platform, because a comparison nobody can run until the
/// target OS boots is a comparison that ships untested) — this is the one that decides whether a
/// symlink gets to reach outside someone's notes folder. Case-folding is ASCII only: two names that
/// differ by non-ASCII case are compared exactly, which can say "outside" where Windows would say
/// "inside". Refusing a note is the survivable direction; reaching past the root is not (§4).
pub fn windows_path_is_under(root: &str, candidate: &str) -> bool {
    // Both separator spellings mean one location on Windows, and a `rel` joined onto a root can hold
    // either, so the comparison is made on one spelling.
    let root = verbatim_stripped(root).replace('/', "\\");
    let candidate = verbatim_stripped(candidate).replace('/', "\\");
    if root.is_empty() || candidate.len() < root.len() {
        return false;
    }
    let (head, rest) = candidate.as_bytes().split_at(root.len());
    // A byte prefix that lands in the middle of a character is not a path prefix at all.
    let Ok(head) = std::str::from_utf8(head) else { return false };
    let continues_into_a_name = rest.is_empty() || rest[0] == b'\\' || rest[0] == b'/';
    head.eq_ignore_ascii_case(root.as_str()) && continues_into_a_name
}

/// Drop the `\\?\` / `\\?\UNC\` spelling so two paths can be compared as the locations they name.
fn verbatim_stripped(path: &str) -> String {
    if let Some(rest) = path.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{rest}");
    }
    if let Some(rest) = path.strip_prefix(r"\\?\") { rest.to_string() } else { path.to_string() }
}

/// The path to hand the OS. On Windows this is where a long path gets np's treatment: past
/// `MAX_PATH`, a plain path simply fails, so at 260 characters the extended-length form goes on —
/// once, and only on an absolute path (with that prefix everything after it is literal, so a
/// relative path or a `/` becomes a file nobody can open).
///
/// Measured against np 0.3.1's source: **np has no such rule** (no `\\?\`, no `MAX_PATH` anywhere in
/// its crates), so this is Jott's own, and it means Jott can open a deep note that np still cannot
/// read — which shows up as a note waiting to be embedded, the honest state for "the index has not
/// seen this". The prefix is applied at or above 260 characters only, where a plain path was failing
/// anyway; nothing that worked before changes.
pub fn to_open_path(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        PathBuf::from(extended_length_prefix(&path.to_string_lossy()))
    }
    #[cfg(not(windows))]
    {
        path.to_path_buf()
    }
}

/// The Windows rule on its own, so it is testable on any platform the suite runs on.
pub fn extended_length_prefix(path: &str) -> String {
    const MAX_PATH: usize = 260;
    if path.len() < MAX_PATH || path.starts_with(r"\\?\") {
        return path.to_string();
    }
    let normalized = path.replace('/', "\\");
    if let Some(unc) = normalized.strip_prefix(r"\\") {
        // A share needs `\\?\UNC\server\share\…`; dropping the leading `\\` and re-adding it that way
        // is the only spelling Windows accepts.
        return format!(r"\\?\UNC\{unc}");
    }
    let bytes = normalized.as_bytes();
    let drive_ok =
        bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'\\';
    if !drive_ok {
        return path.to_string();
    }
    format!(r"\\?\{normalized}")
}

/// What Jott wrote itself, process-wide. One app, one writer, so the watcher and the writer agree without
/// any call site having to thread a registry through (§14) — which is also the trap: a session that hands
/// its watcher a *different* registry reports every save of its own as an outside change, so there is one
/// registry here and everybody uses it.
/// Bookkeeping for "we did this": the watcher must not report it back as an outside change, and the tree must
/// not judge the file from the bytes that were there a moment ago. Always both, because they answer the same
/// question about the same event.
fn mark_ours(root: &Path, rel: &str) {
    default_self_writes().record(rel);
    crate::vault::walk::invalidate(&root.join(rel));
}

pub(crate) fn default_self_writes() -> &'static Arc<SelfWrites> {
    static REGISTRY: OnceLock<Arc<SelfWrites>> = OnceLock::new();
    REGISTRY.get_or_init(|| Arc::new(SelfWrites::new()))
}

/// Whether this `rel` was written or deleted by Jott a moment ago — the question Task 10's watcher
/// asks before it reloads anyone's buffer.
pub fn is_our_write(rel: &str) -> bool {
    default_self_writes().is_recent(rel)
}

/// The bytes of a note, exactly as they are on disk.
pub fn read_note(root: &Path, rel: &str) -> Result<Vec<u8>, FileError> {
    read_note_with(&PathGuard::new(root), rel)
}

/// [`read_note`] with a guard already to hand. A command that opens more than one note builds one guard and
/// passes it along, which is the whole of issue #8's first saving: the bound is a property of the vault, not
/// of the note, and checking it per note costs a `canonicalize` each time.
pub fn read_note_with(guard: &PathGuard, rel: &str) -> Result<Vec<u8>, FileError> {
    let path = guard.resolve(rel)?;
    let metadata = std::fs::metadata(to_open_path(&path)).map_err(|source| match source.kind() {
        std::io::ErrorKind::NotFound => FileError::NotFound { rel: rel.to_string() },
        _ => FileError::Io { rel: rel.to_string(), source },
    })?;
    if metadata.len() > MAX_NOTE_BYTES {
        // The tree already marks this `too-large`; saying so here keeps a UI that asked anyway from
        // swallowing 200 MB into a buffer.
        return Err(FileError::TooLarge { rel: rel.to_string(), limit: MAX_NOTE_BYTES });
    }
    read_bytes(&path).map_err(|source| FileError::Io { rel: rel.to_string(), source })
}

/// [`std::fs::canonicalize`] for the vault root, and the only place that call is made here.
#[cfg(not(test))]
fn canonicalise_root(root: &Path) -> Result<PathBuf, FileError> {
    root.canonicalize().map_err(|source| FileError::Io { rel: root.display().to_string(), source })
}

#[cfg(test)]
fn canonicalise_root(root: &Path) -> Result<PathBuf, FileError> {
    counted_canonicalise_root(root)
}

/// [`std::fs::read`] for a note, and the only place one is opened for reading.
#[cfg(not(test))]
fn read_bytes(path: &Path) -> std::io::Result<Vec<u8>> {
    std::fs::read(to_open_path(path))
}

#[cfg(test)]
fn read_bytes(path: &Path) -> std::io::Result<Vec<u8>> {
    counted_read(path)
}

/// np's catalogs at the vault root (`INDEX.md` and its shards) are not Jott's to write over: they are
/// what np publishes (§9 greys them for exactly that reason), and a note renamed into one of those
/// names would be replaced by generated text on np's next scan — data loss wearing a rename.
fn stays_off_np_catalog(rel: &str) -> Result<(), FileError> {
    let name = rel.rsplit(['/', '\\']).next().unwrap_or(rel);
    if !rel.contains('/') && !rel.contains('\\') && crate::vault::walk::is_generated_index(name) {
        return Err(FileError::NpGenerated { rel: rel.to_string() });
    }
    Ok(())
}

/// A name Windows resolves to a *device* before it looks at the folder at all.
///
/// `CreateFile` matches the segment's stem — the text before the first `.`, case-insensitively — against
/// `con prn aux nul com1…com9 lpt1…lpt9`, extension or not, in the legacy namespace Jott uses for every path under
/// 260 characters. So a note called `nul.md` did not fail: `File::create` opened NUL, the bytes went out, `sync_all`
/// was content, and Jott reported **saved** — while reading `D:\notes\nul` back answered empty and nothing appeared
/// in the folder (issue #5). Renaming a note to `nul` moved it into the device namespace and the tree lost it; and
/// `create_note("aux.md")` answered *already there* for a file nobody has, because `symlink_metadata` succeeded on
/// the device.
///
/// Refused on every platform rather than under `#[cfg(windows)]`, for two reasons. A vault is files: one written on
/// Linux becomes a silent-loss landmine the day it is synced to Windows, which is the shape of bug this issue is.
/// And a rule that answers differently by operating system is how "it worked on my machine" gets filed against
/// somebody else's notes — the same reasoning that refuses np's catalog names everywhere (`stays_off_np_catalog`),
/// not only where np happens to run.
///
/// Deleting such a name stays allowed. On Linux `nul.md` can genuinely exist, and refusing there would strand it in
/// a vault Jott otherwise manages fine.
fn stays_off_device_namespace(rel: &str) -> Result<(), FileError> {
    if rel.split(['/', '\\']).any(is_device_stem) {
        return Err(FileError::ReservedName { rel: rel.to_string() });
    }
    Ok(())
}

/// Whether one path segment's stem is a Windows device. A leading dot is a whole name in `.gitignore`, so its stem
/// is empty and nothing here matches it.
fn is_device_stem(segment: &str) -> bool {
    let stem = segment.split('.').next().unwrap_or("").to_ascii_lowercase();
    matches!(stem.as_str(), "con" | "prn" | "aux" | "nul") || port_stem(&stem)
}

/// `com1`…`com9`, `lpt1`…`lpt9` — spelled out because `com10.md` is a note about the tenth computer, and Windows
/// agrees that it is a file.
fn port_stem(stem: &str) -> bool {
    ["com", "lpt"].iter().any(|prefix| {
        // One digit, 1–9, matched as a byte slice so an empty rest cannot index into itself.
        stem.strip_prefix(prefix).is_some_and(|rest| matches!(rest.as_bytes(), [b'1'..=b'9']))
    })
}

/// Write `bytes` as the whole file. Nothing is appended, converted or re-encoded on the way out.
pub fn write_note(root: &Path, rel: &str, bytes: &[u8]) -> Result<(), FileError> {
    write_note_with(root, rel, bytes, default_self_writes())
}

/// As [`write_note`], with an explicit memory of our own writes — used by the watcher's tests, and
/// by any second writer that wants its saves suppressed too.
pub fn write_note_with(
    root: &Path,
    rel: &str,
    bytes: &[u8],
    writes: &SelfWrites,
) -> Result<(), FileError> {
    let path = PathGuard::new(root).resolve(rel)?;
    stays_off_np_catalog(rel)?;
    stays_off_device_namespace(rel)?;
    // Recorded first, so an event that arrives while the bytes are still going out is already ours — into the
    // registry the caller is watching, not whichever one happens to be the default. And the tree's digest cache
    // forgets this file: the walk must not judge it from the bytes it just replaced, which would mean resting
    // correctness on a timestamp's resolution.
    writes.record(rel);
    crate::vault::walk::invalidate(&root.join(rel));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|source| FileError::Io { rel: rel.to_string(), source })?;
    }
    // Written in place rather than through a temporary file and a rename: a dot-named temp is invisible
    // to np but a plain one would be indexed as a note, and the plan asked for `File::create` on raw
    // bytes. The cost is honest and stated: a crash mid-save can leave a half-written note, which is
    // what §18.13's mirrored buffer in the app data dir is there to recover.
    let mut file = std::fs::File::create(to_open_path(&path))
        .map_err(|source| FileError::Io { rel: rel.to_string(), source })?;
    use std::io::Write;
    file.write_all(bytes).map_err(|source| FileError::Io { rel: rel.to_string(), source })?;
    // Flush to the device: on Windows a save that is still in the OS cache when the machine sleeps is
    // a lost note, and this is the one place per-keystroke cost does not apply.
    file.sync_all().map_err(|source| FileError::Io { rel: rel.to_string(), source })
}

/// An empty note at `rel`, with the folders it needs. Refuses an existing file rather than truncating
/// somebody's note — "new note" dialogs get their name edited before they are saved.
pub fn create_note(root: &Path, rel: &str) -> Result<(), FileError> {
    let path = PathGuard::new(root).resolve(rel)?;
    stays_off_np_catalog(rel)?;
    stays_off_device_namespace(rel)?;
    if std::fs::symlink_metadata(to_open_path(&path)).is_ok() {
        return Err(FileError::AlreadyExists { rel: rel.to_string() });
    }
    write_note(root, rel, b"")
}

/// A note that moved, plus the links that pointed at where it was.
#[derive(Debug, Clone)]
pub struct RenameReport {
    /// Detection only in M1 (§13): these are shown to the user before the rename, never rewritten.
    /// Plan 2 gets np's resolver and the rewriting; this shape exists so that plan does not reshape
    /// this task.
    pub incoming_links: Vec<LinkRef>,
    /// How many notes the scan had to open to answer (§18.7's "wider than np's resolution"). Carried into the
    /// journal by the shell, because issue #8's whole complaint is that nobody has ever measured this against
    /// a vault with thousands of notes — and a log line saying `scanned 4312` turns "renaming feels slow" into a
    /// number the next person can act on.
    pub scanned_notes: usize,
}

/// One note whose text refers to a path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkRef {
    /// The note holding the link, vault-relative with `/`.
    pub src: String,
    /// 1-based, because it is shown to a person.
    pub line: u32,
    /// The target as written, with np's `|alias`, `#heading` and `^block` parts removed (§18.7).
    pub target: String,
}

/// A folder for new notes to live in. Refuses a name that is taken, because "New folder" pressed twice
/// should not quietly merge two folders into one.
pub fn create_folder(root: &Path, rel: &str) -> Result<(), FileError> {
    let path = PathGuard::new(root).resolve(rel)?;
    stays_off_np_catalog(rel)?;
    stays_off_device_namespace(rel)?;
    if std::fs::symlink_metadata(to_open_path(&path)).is_ok() {
        return Err(FileError::AlreadyExists { rel: rel.to_string() });
    }
    mark_ours(root, rel);
    std::fs::create_dir_all(to_open_path(&path)).map_err(|source| FileError::Io { rel: rel.to_string(), source })
}

/// Move one file or folder within the vault and report what referred to it.
pub fn rename(root: &Path, from: &str, to: &str) -> Result<RenameReport, FileError> {
    let guard = PathGuard::new(root);
    let source = guard.resolve(from)?;
    let target = guard.resolve(to)?;
    stays_off_np_catalog(to)?;
    stays_off_device_namespace(to)?;
    if std::fs::symlink_metadata(to_open_path(&source)).is_err() {
        return Err(FileError::NotFound { rel: from.to_string() });
    }
    if std::fs::symlink_metadata(to_open_path(&target)).is_ok() {
        // Never overwrite: a rename that silently deletes the target is the one mistake a file
        // manager is not forgiven.
        return Err(FileError::AlreadyExists { rel: to.to_string() });
    }
    if path_is_under(from, to) {
        return Err(FileError::IntoItself { rel: from.to_string() });
    }
    // One guard for the whole command: it resolved both ends of the move, and it resolves every note the scan
    // opens. The scan runs before the move on purpose — after it, `from` no longer exists to search for.
    let (incoming_links, scanned_notes) = incoming_pointing_at(&guard, from);
    move_within(from, to, source, target)?;
    Ok(RenameReport { incoming_links, scanned_notes })
}

/// Move a file or folder — including a folder full of notes — somewhere else in the vault.
pub fn move_entry(root: &Path, from: &str, to: &str) -> Result<(), FileError> {
    let guard = PathGuard::new(root);
    let source = guard.resolve(from)?;
    let target = guard.resolve(to)?;
    stays_off_np_catalog(to)?;
    stays_off_device_namespace(to)?;
    if std::fs::symlink_metadata(to_open_path(&source)).is_err() {
        return Err(FileError::NotFound { rel: from.to_string() });
    }
    if std::fs::symlink_metadata(to_open_path(&target)).is_ok() {
        return Err(FileError::AlreadyExists { rel: to.to_string() });
    }
    // `inbox` → `inbox/2026/inbox` would move a folder into the space it occupies. Windows answers
    // with a generic access error; naming it here is kinder and testable on every platform.
    if path_is_under(from, to) {
        return Err(FileError::IntoItself { rel: from.to_string() });
    }
    move_within(from, to, source, target)?;
    Ok(())
}

fn path_is_under(from: &str, to: &str) -> bool {
    let from_parts: Vec<&str> = from.split(['/', '\\']).filter(|p| !p.is_empty() && *p != ".").collect();
    let to_parts: Vec<&str> = to.split(['/', '\\']).filter(|p| !p.is_empty() && *p != ".").collect();
    to_parts.len() > from_parts.len() && to_parts.starts_with(&from_parts)
}

fn move_within(from: &str, to: &str, source: PathBuf, target: PathBuf) -> Result<(), FileError> {
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|source| FileError::Io { rel: to.to_string(), source })?;
    }
    // The folder and everything inside it, on both sides. A watcher sees `notes/2026/a.md` disappear and
    // `archive/2026/a.md` appear; suppressing only the folder's own name leaves every child arriving as an
    // outside change, and the window offering to reload notes nobody touched (§14).
    for rel in moved_rels(&source, from, to) {
        default_self_writes().record(&rel);
    }
    default_self_writes().record(from);
    default_self_writes().record(to);
    // A move renames everything underneath it, so the digest cache is dropped whole rather than re-keyed one
    // child at a time: correctness first, and a re-walk of even a large vault measured 40 ms.
    crate::vault::walk::invalidate_tree();
    std::fs::rename(to_open_path(&source), to_open_path(&target))
        .map_err(|source| FileError::Io { rel: from.to_string(), source })
}

/// Every note inside the thing being moved, named on the side it leaves and the side it arrives on. If
/// `source` is a single note rather than a folder, that is just its two names.
fn moved_rels(source: &Path, from: &str, to: &str) -> Vec<String> {
    let subs = match crate::vault::walk::note_rels(source) {
        Ok(subs) => subs,
        // Not a folder, or unreadable: the two names we already have are all there is to say.
        Err(_) => return Vec::new(),
    };
    let mut rels = Vec::with_capacity(subs.len() * 2);
    for sub in subs {
        rels.push(format!("{from}/{sub}"));
        rels.push(format!("{to}/{sub}"));
    }
    rels
}

/// Where a deleted file went.
#[derive(Debug, Clone)]
pub enum Trashed {
    /// The operating system's own trash. Jott cannot read it back and does not claim to.
    Os,
    /// `Jott`'s folder inside the app data dir, because the operating system refused to take it — a headless
    /// Linux box, a disabled Recycle Bin, a network share. The bytes are at `to`, and `reason` is what the
    /// operating system said when asked, which is the difference between "where is my note" and "why is it not
    /// in the Recycle Bin".
    AppDir { to: PathBuf, reason: String },
}

impl Trashed {
    /// What to tell the person, in one sentence that names where the note is. Both arms are a success — the note
    /// is out of the vault either way — but only one of them is where they would go looking for it, so the other
    /// says why and where instead of leaving them to find out by opening Recycle Bin.
    pub fn sentence(&self) -> String {
        match self {
            Trashed::Os => "moved to the system trash".to_string(),
            Trashed::AppDir { to, reason } if reason.is_empty() => {
                format!("moved to Jott's own folder: {}", to.display())
            }
            Trashed::AppDir { to, reason } => format!(
                "moved to Jott's own folder ({}), because the system trash refused: {reason}",
                to.display()
            ),
        }
    }
}

/// Move a file or folder out of the vault. Never deletes: the only promise Jott makes about a note it
/// did not write is that it can be got back.
pub fn delete_to_trash(root: &Path, rel: &str) -> Result<Trashed, FileError> {
    let path = PathGuard::new(root).resolve(rel)?;
    stays_off_np_catalog(rel)?;
    if std::fs::symlink_metadata(to_open_path(&path)).is_err() {
        return Err(FileError::NotFound { rel: rel.to_string() });
    }
    mark_ours(root, rel);
    match trash::delete(to_open_path(&path)) {
        Ok(()) => Ok(Trashed::Os),
        Err(os_refusal) => {
            // The reason goes into the returned path's face rather than being swallowed: Task 10 says
            // "moved to Jott's folder" and can quote why it was not the Recycle Bin.
            let dest = trash_folder().map_err(|source| FileError::Io { rel: rel.to_string(), source })?;
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "note".to_string());
            let target = unique_in(&dest, &name);
            std::fs::create_dir_all(&dest).map_err(|source| FileError::Io { rel: rel.to_string(), source })?;
            move_tree(&path, &target).map_err(|source| FileError::Io { rel: rel.to_string(), source })?;
            Ok(Trashed::AppDir { to: target, reason: os_refusal.to_string() })
        }
    }
}

fn trash_folder() -> Result<PathBuf, std::io::Error> {
    let dir = app_data_dir().map_err(|e| std::io::Error::other(e.to_string()))?.join("trash");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// A name in the trash folder that is not already taken. Two notes deleted in the same second keep
/// both: overwriting the earlier one would be a deletion Jott did promise not to make.
fn unique_in(dir: &Path, name: &str) -> PathBuf {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    let mut candidate = dir.join(format!("{stamp}-{name}"));
    let mut n = 1;
    while candidate.exists() {
        candidate = dir.join(format!("{stamp}-{n}-{name}"));
        n += 1;
    }
    candidate
}

fn move_tree(source: &Path, target: &Path) -> Result<(), std::io::Error> {
    match std::fs::rename(source, target) {
        Ok(()) => Ok(()),
        // The app data dir and the vault can be on different devices (a USB vault, a network share),
        // where rename says "cross-device link" and only a copy will do.
        Err(_) => {
            copy_tree(source, target)?;
            let removed = if source.is_dir() { std::fs::remove_dir_all(source) } else { std::fs::remove_file(source) };
            // If the copy worked and the original would not go, keep both rather than deleting the
            // copy: two copies is recoverable, zero is not.
            match removed {
                Ok(()) => Ok(()),
                Err(_) if target.exists() => Ok(()),
                Err(e) => Err(e),
            }
        }
    }
}

fn copy_tree(source: &Path, target: &Path) -> Result<(), std::io::Error> {
    if source.is_dir() {
        std::fs::create_dir_all(target)?;
        for entry in std::fs::read_dir(source)? {
            let entry = entry?;
            let name = entry.file_name();
            copy_tree(&entry.path(), &target.join(name))?;
        }
        return Ok(());
    }
    std::fs::copy(source, target)?;
    Ok(())
}

/// Which notes hold something that could refer to `rel`.
///
/// Deliberately **wider** than np's resolution (§18.7): it lists what might point at the file rather
/// than promising what does, because this feeds a confirmation dialog before a rename and an
/// under-report breaks links in silence. Plan 2 replaces the guess with np's own ladder and the
/// rewriting that goes with it (§13).
/// Also reports how many notes it opened, which is the number issue #8 asks about: nothing here can skip a
/// note before reading it (a link target can name any file from any folder), so the only honest saving is not
/// reading twice — and a count is how anyone tells whether that has started happening again.
fn incoming_pointing_at(guard: &PathGuard, rel: &str) -> (Vec<LinkRef>, usize) {
    let Ok(notes) = note_rels(guard.root()) else { return (Vec::new(), 0) };
    let mut found = Vec::new();
    let mut scanned = 0usize;
    for note in notes {
        if note == rel {
            continue;
        }
        scanned += 1;
        let Ok(bytes) = read_note_with(guard, &note) else { continue };
        let Ok(text) = std::str::from_utf8(&bytes) else { continue };
        for (offset, target) in link_targets(text) {
            if points_at(&target, rel) {
                found.push(LinkRef { src: note.clone(), line: line_at(text, offset), target });
            }
        }
    }
    (found, scanned)
}

/// Link targets in a note, stripped the way np strips them before placing them: no `|alias`, no
/// `#heading`, no `^block`, trimmed (§18.7). Byte offset of the target's start comes along for the
/// line number.
fn link_targets(text: &str) -> Vec<(usize, String)> {
    let mut out: Vec<(usize, String)> = Vec::new();

    // `[[wikilink|alias]]`
    let mut from = 0;
    while let Some(found) = text[from..].find("[[") {
        let start = from + found + 2;
        let Some(end) = text[start..].find("]]") else { break };
        let raw = &text[start..start + end];
        if !raw.is_empty() {
            out.push((start, strip_target(raw)));
        }
        from = start + end;
    }

    // `[label](target "title")` and `![alt](target)`
    let mut from = 0;
    while let Some(found) = text[from..].find("](") {
        let start = from + found + 2;
        let Some(end) = text[start..].find(')') else { break };
        let raw = text[start..start + end].trim();
        // A `<…>` target may hold spaces; otherwise a title starts at the first space.
        let raw = match raw.strip_prefix('<').and_then(|r| r.strip_suffix('>')) {
            Some(angle) => angle,
            None => raw.split_whitespace().next().unwrap_or(raw),
        };
        if !raw.is_empty() {
            out.push((start, strip_target(raw)));
        }
        from = start + end;
    }

    // Link *definitions* (`[ref]: path`) are not scanned: they are rare in notes people type by hand,
    // and Plan 2 reads the grammar properly with np's resolver (§18.7). M1 warns about what it sees.
    out.retain(|(_, target)| !is_external(target));
    out.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);
    out.sort_by_key(|(offset, _)| *offset);
    out
}

/// np's stripping, in np's order (§18.7): alias, then heading, then block ref, then trim.
fn strip_target(raw: &str) -> String {
    let mut target = raw;
    if let Some((head, _)) = target.split_once('|') {
        target = head;
    }
    for marker in ['#', '^'] {
        if let Some((head, _)) = target.split_once(marker) {
            target = head;
        }
    }
    target.trim().to_string()
}

fn is_external(target: &str) -> bool {
    // `https://…`, `mailto:…`, `//share/…` — a scheme or a bare share never means a file in the vault,
    // and its last segment can still share a name with one.
    if target.starts_with("//") {
        return true;
    }
    match target.find(':') {
        Some(i) if i > 0 => target[..i].chars().all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.') && target[..i].starts_with(char::is_alphabetic),
        _ => false,
    }
}

/// Whether a written target could mean this vault-relative path: exactly, case-insensitively, or by
/// basename with and without its extension — the three ways a person writes a link that np then has to
/// place. Case-folded throughout, because M1 would rather over-report than miss (§18.7's ambiguity
/// rules belong to the resolver, not to this warning).
fn points_at(target: &str, rel: &str) -> bool {
    let target = target.trim_end_matches('/');
    if target.is_empty() {
        return false;
    }
    if target.eq_ignore_ascii_case(rel) {
        return true;
    }
    let (Some(t), Some(r)) = (file_name(target), file_name(rel)) else { return false };
    if t.eq_ignore_ascii_case(r) {
        return true;
    }
    // `[[q3-review]]` names a note without its extension, and so does `q3-review`.
    stem_of(t).eq_ignore_ascii_case(stem_of(r))
}

fn file_name(path: &str) -> Option<&str> {
    path.rsplit(['/', '\\']).next().filter(|n| !n.is_empty())
}

/// A basename without its extension — np indexes basenames both ways (§18.7). A leading dot is not a
/// stem boundary, but the tree excludes dot-names anyway.
fn stem_of(name: &str) -> &str {
    match name.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() => stem,
        _ => name,
    }
}

fn line_at(text: &str, offset: usize) -> u32 {
    text[..offset.min(text.len())].bytes().filter(|b| *b == b'\n').count() as u32 + 1
}

#[cfg(test)]
mod tests {
    //! What one rename costs, in syscalls rather than milliseconds (issue #8). The report says "unmeasured,
    //! so treat this as a shape problem", and a shape is what a counter can pin: a `HashCache::stats`-shaped
    //! answer to "did this do work it did not have to do?", on a fixture small enough that no timing would
    //! have meant anything. Timers here would fail on a loaded runner and pass for the wrong reason; these fail
    //! because a directory was resolved five times instead of once — twice for the move itself, then once per
    //! note the scan opened, which is exactly what issue #8 describes.

    use super::{rename, reset_syscall_counters, syscall_counters};
    use std::path::Path;

    /// A vault with notes at the root and two folders deep, one of which refers to the note being renamed.
    fn vault(dir: &Path) {
        for rel in ["top.md", "a/middle.md", "a/b/deep.md", "a/b/other.md"] {
            let path = dir.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, "# note\n").unwrap();
        }
        std::fs::write(dir.join("a/b/other.md"), "see [[deep]] and [text](../middle.md)\n").unwrap();
    }

    #[test]
    fn one_rename_resolves_the_vault_root_once() {
        let tmp = tempfile::tempdir().unwrap();
        vault(tmp.path());
        reset_syscall_counters();

        let report = rename(tmp.path(), "a/b/deep.md", "a/b/deeper.md").expect("the rename happens");
        let (canonicalisations, _) = syscall_counters();

        // One guard, one answer: `from`, `to` and every note the scan opens describe the same directory.
        // Before issue #8 this was 2 + one per note read, because each `read_note` built its own guard and
        // canonicalised the root again — a rename of one file asking the OS what the vault is, N times.
        assert_eq!(canonicalisations, 1, "the scan must share the guard that resolved the move");
        assert_eq!(report.incoming_links.len(), 1, "and the scan still finds the link that refers to it");
    }

    #[test]
    fn one_rename_reads_each_other_note_once() {
        let tmp = tempfile::tempdir().unwrap();
        vault(tmp.path());
        reset_syscall_counters();

        rename(tmp.path(), "top.md", "topper.md").expect("the rename happens");
        let (_, reads) = syscall_counters();

        // Four notes, one of them the file being moved: three reads. This is the floor while the scan has to
        // open every note — a link target can name any file from any folder, so nothing can be skipped before
        // it is read (issue #8 says so, and it is right) — but it is also the floor *once*, not once per
        // guard or once per separator spelling. A second rename that reads six notes is a regression.
        assert_eq!(reads, 3);
    }
}
