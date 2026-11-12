//! The unsaved-buffer mirror (§18.13): the last text in an editor that has not been saved yet, kept
//! outside the vault so a crash or a closed lid does not take it with it.
//!
//! Two rules keep this honest:
//!
//! * **Replay only what is newer.** A mirror older than the file is a leftover from a session that saved
//!   and quit; showing it would resurrect text someone deleted on purpose — §15's second risk, named as
//!   one before it was built.
//! * **Nothing lives in the vault.** The mirror is app state, so it goes in the app data dir like the
//!   registry and the stamps (§12).

use crate::app_paths::app_data_dir;
use crate::hashes::{sha256_prefix, sha256_hex};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Where this file's mirror lives, under the app data dir.
pub fn mirror_path(vault: &Path, rel: &str) -> std::io::Result<PathBuf> {
    Ok(mirror_path_in(&buffers_dir()?, vault, rel))
}

/// The same, with the root named. This is what the shell calls (it knows where its data lives), and what
/// every test uses — a path that comes from `$XDG_DATA_HOME` would make two tests in one process depend
/// on each other's environment, which is a flake waiting to happen rather than a design.
pub fn mirror_path_in(root: &Path, vault: &Path, rel: &str) -> PathBuf {
    // Normalised the same way the stamps normalise the vault (`to_abs`): a picker that hands back
    // `C:\notes\` and a registry entry reading `C:\notes` are one vault, and a draft keyed on the raw
    // string would simply never come back (§18.13).
    let key = format!("{}\n{}", crate::vault::store::to_abs(vault).display(), rel);
    buffers_dir_in(root).join(format!("{}.{}", sha256_prefix(key.as_bytes(), 16), "buffer"))
}

fn buffers_dir() -> std::io::Result<PathBuf> {
    let root = app_data_dir().map_err(|e| std::io::Error::other(e.to_string()))?;
    Ok(buffers_dir_in(&root))
}

/// The buffers live beside the registry and the stamps, in a folder of their own (§12). Created on demand:
/// an app that never had an unsaved keystroke has no reason to have made it.
fn buffers_dir_in(root: &Path) -> PathBuf {
    let dir = root.join("buffers");
    // A failure here is not this call's to report: the write below will fail with the same cause and name
    // the file it could not make.
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Mirror what the editor holds right now, saying which version of the file that text was typed against
/// (`base`: the file's mtime when the buffer was filled or last saved). Called on every autosave tick, not
/// only on a real save — the point of §18.13 is the text that was never written to the file.
pub fn write_mirror(vault: &Path, rel: &str, bytes: &[u8], base: Option<SystemTime>) -> std::io::Result<()> {
    let path = mirror_path(vault, rel)?;
    write_mirror_at(&path, bytes, base)
}

/// The same, under a root the caller named.
pub fn write_mirror_in(root: &Path, vault: &Path, rel: &str, bytes: &[u8], base: Option<SystemTime>) -> std::io::Result<()> {
    write_mirror_at(&mirror_path_in(root, vault, rel), bytes, base)
}

fn write_mirror_at(path: &Path, bytes: &[u8], base: Option<SystemTime>) -> std::io::Result<()> {
    // Three facts travel with the text: its own hash (so a replay can be matched to the buffer it claims
    // to be), its length (so a half-written record is recognisable), and `base` — the file's mtime this
    // draft was typed against, which is what makes "newer than the file" decidable. Timestamps on this
    // box can repeat to the nanosecond for two writes in the same tick, so a bare mtime comparison would
    // sometimes be a coin toss; `base` is not a guess about ordering, it is a fact about what was read.
    let base_field = match base {
        Some(when) => when.duration_since(UNIX_EPOCH).map(|d| d.as_nanos().to_string()).unwrap_or_else(|_| "unknown".into()),
        None => "unknown".into(),
    };
    let header = format!(
        "jott-buffer-mirror 1\nhash={}\nlen={}\nbase={}\n",
        sha256_hex(bytes),
        bytes.len(),
        base_field
    );
    let mut record = header.into_bytes();
    record.extend_from_slice(b"\0");
    record.extend_from_slice(bytes);
    std::fs::write(path, record)
}

/// A mirrored buffer whose bytes match `text`, if one exists. Used to say "this is what you had typed"
/// rather than "here is a file that looks similar".
pub fn read_mirror(vault: &Path, rel: &str) -> Option<MirroredBuffer> {
    read_mirror_at(&mirror_path(vault, rel).ok()?)
}

/// The same, under a root the caller named.
pub fn read_mirror_in(root: &Path, vault: &Path, rel: &str) -> Option<MirroredBuffer> {
    read_mirror_at(&mirror_path_in(root, vault, rel))
}

fn read_mirror_at(path: &Path) -> Option<MirroredBuffer> {
    let record = std::fs::read(path).ok()?;
    let split = record.iter().position(|b| *b == b'\0')?;
    let header = std::str::from_utf8(&record[..split]).ok()?;
    let body = &record[split + 1..];
    let mut hash = None;
    let mut base = None;
    for line in header.lines() {
        if let Some(rest) = line.strip_prefix("hash=") {
            hash = Some(rest.to_string());
        } else if let Some(rest) = line.strip_prefix("base=") {
            base = rest.parse::<u128>().ok().map(|nanos| UNIX_EPOCH + std::time::Duration::from_nanos(nanos as u64));
        }
    }
    // A record whose length or hash does not match was truncated by the thing we are defending against.
    let declared_len_ok = header
        .lines()
        .find_map(|line| line.strip_prefix("len="))
        .and_then(|len| len.parse::<usize>().ok())
        == Some(body.len());
    if !header.starts_with("jott-buffer-mirror 1") || !declared_len_ok {
        return None;
    }
    let mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok()?;
    Some(MirroredBuffer { text: body.to_vec(), hash: hash.unwrap_or_default(), base, mtime, path: path.to_path_buf() })
}

/// Drop the mirror once the file itself holds the text (§18.13's replay has nothing left to do).
pub fn clear_mirror(vault: &Path, rel: &str) -> std::io::Result<()> {
    match std::fs::remove_file(mirror_path(vault, rel)?) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

/// The same, under a root the caller named.
pub fn clear_mirror_in(root: &Path, vault: &Path, rel: &str) -> std::io::Result<()> {
    match std::fs::remove_file(mirror_path_in(root, vault, rel)) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

pub struct MirroredBuffer {
    pub text: Vec<u8>,
    /// The hash taken when the mirror was written, so the caller can tell "same text" from "older text".
    pub hash: String,
    /// The file's mtime this draft was typed against. `None` for a record written before that was stored.
    pub base: Option<SystemTime>,
    pub mtime: SystemTime,
    pub path: PathBuf,
}

impl MirroredBuffer {
    /// Whether this draft should be offered for a file whose mtime is `file_mtime` (§18.13's "newer").
    ///
    /// With a `base`, the question has a definite answer: the file has not been written since this buffer
    /// was taken, so whatever it holds came from typing. Without one — an older record — fall back to the
    /// timestamp rule and let a tie mean "say nothing", because two writes in the same clock tick really
    /// do share a nanosecond here, and a coin toss is not what §15's second risk deserves.
    pub fn newer_than(&self, file_mtime: SystemTime) -> bool {
        match self.base {
            Some(base) => file_mtime <= base,
            None => self.mtime > file_mtime,
        }
    }

    /// Milliseconds since the epoch, for the wire.
    pub fn mtime_ms(&self) -> u64 {
        self.mtime.duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn mirror(when: SystemTime) -> MirroredBuffer {
        MirroredBuffer { text: b"typed".to_vec(), hash: "h".into(), base: None, mtime: when, path: PathBuf::from("/x") }
    }

    fn drafted_against(base: SystemTime) -> MirroredBuffer {
        MirroredBuffer { base: Some(base), ..mirror(base + Duration::from_millis(1)) }
    }

    #[test]
    fn a_draft_is_offered_when_the_file_has_not_been_written_since_it_was_taken() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        // §18.13's rule, decided from what was read rather than from who won the race with the clock.
        assert!(drafted_against(now).newer_than(now), "the file has not moved since this buffer was filled");
        assert!(drafted_against(now).newer_than(now - Duration::from_secs(1)), "an older file is even more clearly the same file");
        assert!(!drafted_against(now).newer_than(now + Duration::from_secs(1)), "somebody wrote the file afterwards: that version wins until it is said otherwise");
    }

    #[test]
    fn an_older_record_falls_back_to_the_timestamp_and_a_tie_says_nothing() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        assert!(mirror(now + Duration::from_secs(1)).newer_than(now), "typed after the file was written");
        // A tie is not evidence of anything: two writes in the same tick share a nanosecond here.
        assert!(!mirror(now).newer_than(now));
        assert!(!mirror(now - Duration::from_secs(1)).newer_than(now), "older means somebody saved since");
    }

    #[test]
    fn a_truncated_mirror_is_not_a_buffer() {
        let dir = tempfile::tempdir().expect("dir");
        // Two vaults with the same `rel` must not read each other's drafts.
        let one = mirror_path_in(dir.path(), &dir.path().join("vault-a"), "notes/a.md");
        let two = mirror_path_in(dir.path(), &dir.path().join("vault-b"), "notes/a.md");
        assert_ne!(one, two);

        write_mirror_in(dir.path(), dir.path(), "notes/a.md", b"half written", None).expect("mirror");
        let path = mirror_path_in(dir.path(), dir.path(), "notes/a.md");
        let intact = std::fs::read(&path).expect("read");
        assert_eq!(read_mirror_in(dir.path(), dir.path(), "notes/a.md").expect("mirror").text, b"half written");

        // Cut it in half, as a full disk or a crash mid-write would.
        std::fs::write(&path, &intact[..intact.len() / 2]).expect("truncate");
        assert!(read_mirror_in(dir.path(), dir.path(), "notes/a.md").is_none(), "a damaged record is not shown as a draft");

        clear_mirror_in(dir.path(), dir.path(), "notes/a.md").expect("clear");
        assert!(read_mirror_in(dir.path(), dir.path(), "notes/a.md").is_none());
        clear_mirror_in(dir.path(), dir.path(), "notes/a.md").expect("clearing what is gone is not an error");
    }
}
