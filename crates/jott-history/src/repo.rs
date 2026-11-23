//! A vault's repository: turning history on, making versions, and reading old ones.
//!
//! History is append-only. Nothing here resets, rewrites or force-anything; a restore is the caller writing old
//! bytes through its own save path and asking for a new version on top.

use crate::error::{Busy, HistoryError};
use crate::locate::Git;
use crate::run::{args, Place};
use serde::Serialize;
use std::path::{Path, PathBuf};

/// Files above this are left out of versions. GitHub warns at 50 MB and refuses at 100 MB, and a history that
/// cannot be pushed is a failure found later, far from its cause.
pub const MAX_FILE_BYTES: u64 = 50 * 1024 * 1024;

/// np's generated catalogue, kept out of history in `info/exclude` — never in a file in the person's notes.
const EXCLUDES: [&str; 2] = ["/INDEX.md", "/INDEX-*.md"];
const EXCLUDE_HEADER: &str = "# Added by Jott: note-processor's generated catalogue is rebuilt, not versioned.";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Layout {
    /// The vault's own `.git`.
    InVault,
    /// A repository in Jott's app data with the vault as its work tree — for cloud-synced folders.
    AppData,
}

/// What is there before anything is turned on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Discovery {
    Nothing,
    /// A repository at the vault's root. Adopting it is the person's call.
    AtRoot,
    /// The vault is a subfolder of another repository. Refused.
    Inside { top: PathBuf },
    /// Jott's own app-data repository for this vault.
    AppData,
}

#[derive(Debug, Clone)]
pub struct Repo {
    git: Git,
    place: Place,
    layout: Layout,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LargeFile {
    pub rel: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitOutcome {
    /// The new version, or `None` when nothing had changed.
    pub version: Option<String>,
    /// Every path the version records, added, changed or removed.
    pub paths: Vec<String>,
    /// Files left out for size; reported every time, so the panel's list is current.
    pub too_large: Vec<LargeFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Version {
    pub id: String,
    /// Seconds since the epoch, as the commit records it.
    pub at: i64,
    pub message: String,
    /// The note's path in that version — different from today's when it has been renamed since.
    pub rel: String,
}

impl Repo {
    /// What exists for `vault` today. `app_dir` is where this vault's app-data repository would be.
    pub fn discover(git: &Git, vault: &Path, app_dir: &Path) -> Result<Discovery, HistoryError> {
        if is_git_dir(app_dir) {
            return Ok(Discovery::AppData);
        }
        if vault.join(".git").exists() {
            return Ok(Discovery::AtRoot);
        }
        let output = git.run_bare(Some(vault), args!["rev-parse", "--show-toplevel"])?;
        if output.ok() {
            let top = PathBuf::from(output.stdout_text().trim());
            if !same_dir(&top, vault) {
                return Ok(Discovery::Inside { top });
            }
            return Ok(Discovery::AtRoot);
        }
        Ok(Discovery::Nothing)
    }

    /// The repository for a vault whose history is already on, or [`HistoryError::NotEnabled`] if there is none.
    pub fn open(git: &Git, vault: &Path, app_dir: &Path) -> Result<Repo, HistoryError> {
        let repo = match Repo::discover(git, vault, app_dir)? {
            Discovery::AppData => Repo::at(git, app_dir, vault, Layout::AppData),
            Discovery::AtRoot => Repo::at(git, &vault.join(".git"), vault, Layout::InVault),
            Discovery::Inside { top } => return Err(HistoryError::InsideRepository { top }),
            Discovery::Nothing => return Err(HistoryError::NotEnabled),
        };
        repo.ensure_excludes()?;
        Ok(repo)
    }

    /// Turn history on: adopt the repository at the root, or create one — in the vault, or in `app_dir` when the
    /// vault is cloud-synced.
    pub fn enable(git: &Git, vault: &Path, app_dir: &Path, cloud_synced: bool) -> Result<Repo, HistoryError> {
        match Repo::discover(git, vault, app_dir)? {
            Discovery::Inside { top } => Err(HistoryError::InsideRepository { top }),
            Discovery::AtRoot | Discovery::AppData => Repo::open(git, vault, app_dir),
            Discovery::Nothing => {
                let (git_dir, layout) = if cloud_synced {
                    if let Some(parent) = app_dir.parent() {
                        std::fs::create_dir_all(parent)
                            .map_err(|source| HistoryError::Io { path: parent.to_path_buf(), source })?;
                    }
                    (app_dir.to_path_buf(), Layout::AppData)
                } else {
                    (vault.join(".git"), Layout::InVault)
                };
                let repo = Repo::at(git, &git_dir, vault, layout);
                git.run(&repo.place, args!["init", "-q", "-b", "main"])?;
                repo.ensure_excludes()?;
                Ok(repo)
            }
        }
    }

    fn at(git: &Git, git_dir: &Path, vault: &Path, layout: Layout) -> Repo {
        Repo {
            git: git.clone(),
            place: Place { git_dir: git_dir.to_path_buf(), work_tree: vault.to_path_buf() },
            layout,
        }
    }

    pub fn layout(&self) -> Layout {
        self.layout
    }

    pub fn git_dir(&self) -> &Path {
        &self.place.git_dir
    }

    pub(crate) fn git(&self) -> &Git {
        &self.git
    }

    pub(crate) fn place(&self) -> &Place {
        &self.place
    }

    /// A path inside the repository's metadata, resolved by git (a `.git` *file* points somewhere else).
    pub(crate) fn git_path(&self, name: &str) -> Result<PathBuf, HistoryError> {
        let output = self.git.run(&self.place, args!["rev-parse", "--git-path", name])?;
        let path = PathBuf::from(output.stdout_text().trim());
        Ok(if path.is_absolute() { path } else { self.place.work_tree.join(path) })
    }

    fn ensure_excludes(&self) -> Result<(), HistoryError> {
        let path = self.git_path("info/exclude")?;
        let io = |source| HistoryError::Io { path: path.clone(), source };
        let existing = std::fs::read_to_string(&path).unwrap_or_default();
        let missing: Vec<&str> = EXCLUDES.iter().copied().filter(|line| !existing.lines().any(|l| l.trim() == *line)).collect();
        if missing.is_empty() {
            return Ok(());
        }
        let mut text = existing;
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        if !text.contains(EXCLUDE_HEADER) {
            text.push_str(EXCLUDE_HEADER);
            text.push('\n');
        }
        for line in missing {
            text.push_str(line);
            text.push('\n');
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(io)?;
        }
        std::fs::write(&path, text).map_err(io)
    }

    /// The name and email git will put on versions, from any of its config files.
    pub fn identity(&self) -> Result<Option<(String, String)>, HistoryError> {
        let get = |key: &str| -> Result<Option<String>, HistoryError> {
            let output = self.git.try_run(&self.place, args!["config", "--get", key])?;
            let value = output.stdout_text().trim().to_string();
            Ok((output.ok() && !value.is_empty()).then_some(value))
        };
        Ok(match (get("user.name")?, get("user.email")?) {
            (Some(name), Some(email)) => Some((name, email)),
            _ => None,
        })
    }

    /// Record a name and email in this repository's own config — never anyone's global one.
    pub fn set_identity(&self, name: &str, email: &str) -> Result<(), HistoryError> {
        self.git.run(&self.place, args!["config", "--local", "user.name", name])?;
        self.git.run(&self.place, args!["config", "--local", "user.email", email])?;
        Ok(())
    }

    /// Why Jott should not commit right now, if there is a reason.
    pub fn busy(&self) -> Result<Option<Busy>, HistoryError> {
        // The per-worktree git dir holds these markers; one call finds it even when `.git` is a file.
        let output = self.git.run(&self.place, args!["rev-parse", "--absolute-git-dir"])?;
        let dir = PathBuf::from(output.stdout_text().trim());
        for (name, busy) in [
            ("MERGE_HEAD", Busy::Merging),
            ("rebase-merge", Busy::Rebasing),
            ("rebase-apply", Busy::Rebasing),
            ("CHERRY_PICK_HEAD", Busy::CherryPicking),
            ("REVERT_HEAD", Busy::Reverting),
        ] {
            if dir.join(name).exists() {
                return Ok(Some(busy));
            }
        }
        let staged = self.git.try_run(&self.place, args!["diff", "--cached", "--quiet"])?;
        Ok((staged.status == Some(1)).then_some(Busy::StagedElsewhere))
    }

    pub(crate) fn has_commits(&self) -> Result<bool, HistoryError> {
        Ok(self.git.try_run(&self.place, args!["rev-parse", "--verify", "-q", "HEAD"])?.ok())
    }

    /// Make a version of everything that changed in the vault, if anything did. `message` replaces the default
    /// `Jott: saved …` line.
    pub fn commit_if_dirty(&self, message: Option<&str>) -> Result<CommitOutcome, HistoryError> {
        if let Some(busy) = self.busy()? {
            return Err(HistoryError::Busy(busy));
        }
        let changed = self.changed_paths()?;
        let mut paths = Vec::new();
        let mut too_large = Vec::new();
        for rel in changed {
            match std::fs::metadata(self.place.work_tree.join(&rel)) {
                Ok(meta) if meta.is_file() && meta.len() > MAX_FILE_BYTES => {
                    too_large.push(LargeFile { rel, bytes: meta.len() });
                }
                _ => paths.push(rel),
            }
        }
        if paths.is_empty() {
            return Ok(CommitOutcome { version: None, paths, too_large });
        }
        if self.identity()?.is_none() {
            return Err(HistoryError::NeedsIdentity);
        }
        let mut list = Vec::new();
        for rel in &paths {
            list.extend_from_slice(rel.as_bytes());
            list.push(0);
        }
        self.git.run_with_input(&self.place, args!["add", "-A", "--pathspec-from-file=-", "--pathspec-file-nul"], &list)?;
        if self.git.try_run(&self.place, args!["diff", "--cached", "--quiet"])?.ok() {
            return Ok(CommitOutcome { version: None, paths: Vec::new(), too_large });
        }
        let text = message.map(str::to_string).unwrap_or_else(|| saved_message(&paths));
        self.git.run(&self.place, args!["commit", "-q", "--no-verify", "-m", text])?;
        let head = self.git.run(&self.place, args!["rev-parse", "HEAD"])?.stdout_text().trim().to_string();
        Ok(CommitOutcome { version: Some(head), paths, too_large })
    }

    /// Every path git sees as changed, including deletions and files nobody has added yet. Unmerged paths mean a
    /// merge is open, which [`busy`](Self::busy) already refused.
    fn changed_paths(&self) -> Result<Vec<String>, HistoryError> {
        let output = self.git.run(&self.place, args!["status", "--porcelain=v2", "-z", "--untracked-files=all"])?;
        Ok(parse_status(&output.stdout))
    }

    /// The versions of one note, newest first, following it across renames.
    pub fn versions(&self, rel: &str) -> Result<Vec<Version>, HistoryError> {
        if !self.has_commits()? {
            return Ok(Vec::new());
        }
        let output = self.git.run(
            &self.place,
            args!["log", "--follow", "--name-only", "-z", "--format=%x1e%H%x1f%at%x1f%s", "--", rel],
        )?;
        Ok(parse_log(&output.stdout))
    }

    /// The bytes of `rel` as `version` recorded them. `rel` is the path *in that version* ([`Version::rel`]).
    pub fn bytes_at(&self, version: &str, rel: &str) -> Result<Vec<u8>, HistoryError> {
        if version.len() < 7 || !version.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(HistoryError::BadVersion(format!("\"{version}\" is not a version Jott recorded")));
        }
        let spec = format!("{version}:{rel}");
        Ok(self.git.run(&self.place, args!["cat-file", "blob", spec])?.stdout)
    }
}

fn saved_message(paths: &[String]) -> String {
    const SHOWN: usize = 3;
    let mut text = format!("Jott: saved {}", paths.iter().take(SHOWN).cloned().collect::<Vec<_>>().join(", "));
    if paths.len() > SHOWN {
        text.push_str(&format!(" and {} more", paths.len() - SHOWN));
    }
    text
}

/// Paths from `git status --porcelain=v2 -z`. A rename contributes both names.
pub(crate) fn parse_status(raw: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    let mut fields = raw.split(|b| *b == 0).filter(|f| !f.is_empty());
    while let Some(entry) = fields.next() {
        let text = String::from_utf8_lossy(entry);
        let path = match entry.first() {
            Some(b'1') => text.splitn(9, ' ').nth(8),
            Some(b'2') => {
                let path = text.splitn(10, ' ').nth(9).map(str::to_string);
                if let Some(original) = fields.next() {
                    out.push(String::from_utf8_lossy(original).into_owned());
                }
                if let Some(path) = path {
                    out.push(path);
                }
                continue;
            }
            Some(b'u') => text.splitn(11, ' ').nth(10),
            Some(b'?') => text.strip_prefix("? "),
            _ => None,
        };
        if let Some(path) = path {
            out.push(path.to_string());
        }
    }
    out.sort();
    out.dedup();
    out
}

/// `git log --name-only -z --format=%x1e%H%x1f%at%x1f%s`: each record is `\x1e<id>\x1f<at>\x1f<subject>\0\n<path>\0`.
pub(crate) fn parse_log(raw: &[u8]) -> Vec<Version> {
    let text = String::from_utf8_lossy(raw);
    text.split('\u{1e}')
        .filter(|record| !record.is_empty())
        .filter_map(|record| {
            let (header, rest) = record.split_once('\0')?;
            let mut parts = header.splitn(3, '\u{1f}');
            let id = parts.next()?.to_string();
            let at = parts.next()?.parse().ok()?;
            let message = parts.next().unwrap_or_default().to_string();
            let rel = rest.trim_start_matches('\n').split('\0').find(|p| !p.is_empty())?.to_string();
            Some(Version { id, at, message, rel })
        })
        .collect()
}

fn is_git_dir(dir: &Path) -> bool {
    dir.join("HEAD").is_file() && dir.join("objects").is_dir()
}

fn same_dir(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_lists_changes_renames_and_new_files() {
        let raw = b"1 .M N... 100644 100644 100644 aaa bbb notes/a b.md\x002 R. N... 100644 100644 100644 aaa bbb R100 new.md\x00old.md\x00? fresh.md\x00";
        assert_eq!(parse_status(raw), vec!["fresh.md", "new.md", "notes/a b.md", "old.md"]);
    }

    #[test]
    fn log_records_carry_the_path_each_version_had() {
        let raw = b"\x1eabc123\x1f1795452257\x1ftwo\x00\nb.md\x00\x1edef456\x1f1795452200\x1fone\x00\na.md\x00";
        let versions = parse_log(raw);
        assert_eq!(versions.len(), 2);
        assert_eq!((versions[0].id.as_str(), versions[0].rel.as_str(), versions[0].at), ("abc123", "b.md", 1795452257));
        assert_eq!((versions[1].message.as_str(), versions[1].rel.as_str()), ("one", "a.md"));
    }

    #[test]
    fn the_default_message_names_a_few_paths_and_counts_the_rest() {
        let paths: Vec<String> = ["a.md", "b.md", "c.md", "d.md", "e.md"].map(String::from).to_vec();
        assert_eq!(saved_message(&paths), "Jott: saved a.md, b.md, c.md and 2 more");
        assert_eq!(saved_message(&paths[..1]), "Jott: saved a.md");
    }
}
