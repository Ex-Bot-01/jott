//! One remote, `origin`, one branch: connect it, and sync with it.
//!
//! Sync never picks a winner. When the same note changed on both sides, the local text stays where it is and the
//! remote's is written beside it, named after where it came from — the "never guess" rule from the editor's link
//! handling, applied to two machines (spec: *Sync*).

use crate::error::HistoryError;
use crate::locate::Git;
use crate::repo::Repo;
use crate::run::{args, failed};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectOutcome {
    pub branch: String,
    /// The remote had nothing yet; the first sync pushes everything.
    pub remote_was_empty: bool,
}

/// A note both sides changed: `rel` keeps this machine's text, `remote_copy` holds the remote's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeptBoth {
    pub rel: String,
    pub remote_copy: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SyncOutcome {
    UpToDate,
    #[serde(rename_all = "camelCase")]
    Pushed { commits: u32 },
    /// The remote's versions came in. `paths` are the files that changed on disk.
    #[serde(rename_all = "camelCase")]
    Pulled { commits: u32, paths: Vec<String> },
    #[serde(rename_all = "camelCase")]
    Merged { pulled: u32, pushed: u32, paths: Vec<String>, kept_both: Vec<KeptBoth> },
}

enum Pushed {
    Done,
    /// The remote moved since the fetch.
    Behind,
}

impl Repo {
    pub fn remote_url(&self) -> Result<Option<String>, HistoryError> {
        let output = self.git().try_run(self.place(), args!["remote", "get-url", "origin"])?;
        Ok(output.ok().then(|| output.stdout_text().trim().to_string()).filter(|url| !url.is_empty()))
    }

    /// Check the address answers and the login works, then make it `origin`. A remote whose history has nothing in
    /// common with this one is refused and not kept.
    pub fn connect(&self, url: &str) -> Result<ConnectOutcome, HistoryError> {
        let url = url.trim();
        check_url(url)?;
        let net = self.network_git()?;
        let listing = net.run(self.place(), args!["ls-remote", url])?;
        let remote_was_empty = listing.stdout_text().trim().is_empty();
        if self.remote_url()?.is_some() {
            self.git().run(self.place(), args!["remote", "set-url", "origin", url])?;
        } else {
            self.git().run(self.place(), args!["remote", "add", "origin", url])?;
        }
        let branch = self.branch()?;
        if !remote_was_empty {
            if let Err(error) = net.run(self.place(), args!["fetch", "-q", "origin"]) {
                self.forget_remote();
                return Err(error);
            }
            let remote_ref = remote_ref(&branch);
            if self.has_commits()? && self.ref_exists(&remote_ref)? {
                let base = self.git().try_run(self.place(), args!["merge-base", "HEAD", remote_ref])?;
                if base.status == Some(1) {
                    self.forget_remote();
                    return Err(HistoryError::UnrelatedHistory);
                }
            }
        }
        Ok(ConnectOutcome { branch, remote_was_empty })
    }

    pub fn disconnect(&self) -> Result<(), HistoryError> {
        if self.remote_url()?.is_some() {
            self.git().run(self.place(), args!["remote", "remove", "origin"])?;
        }
        Ok(())
    }

    fn forget_remote(&self) {
        let _ = self.git().try_run(self.place(), args!["remote", "remove", "origin"]);
    }

    /// Bring the remote's versions in and this machine's out. The caller makes a version of unsaved changes first.
    pub fn sync(&self) -> Result<SyncOutcome, HistoryError> {
        let url = self.remote_url()?.ok_or(HistoryError::NoRemote)?;
        if let Some(busy) = self.busy()? {
            return Err(HistoryError::Busy(busy));
        }
        let net = self.network_git()?;
        let branch = self.branch()?;
        let remote_ref = remote_ref(&branch);
        // Twice at most: a push that loses a race fetches and merges once more, then stops and says so.
        for _ in 0..2 {
            net.run(self.place(), args!["fetch", "-q", "origin"])?;
            if !self.ref_exists(&remote_ref)? {
                if !self.has_commits()? {
                    return Ok(SyncOutcome::UpToDate);
                }
                let commits = self.count("HEAD")?;
                match self.push(&net, &branch)? {
                    Pushed::Done => return Ok(SyncOutcome::Pushed { commits }),
                    Pushed::Behind => continue,
                }
            }
            let before = self.head()?;
            let (ahead, behind) = match &before {
                Some(_) => self.ahead_behind(&remote_ref)?,
                None => (0, self.count(&remote_ref)?),
            };
            match (ahead, behind) {
                (0, 0) => return Ok(SyncOutcome::UpToDate),
                (0, behind) => {
                    self.git().run(self.place(), args!["merge", "--ff-only", "-q", remote_ref])?;
                    let paths = self.changed_since(before.as_deref())?;
                    return Ok(SyncOutcome::Pulled { commits: behind, paths });
                }
                (ahead, 0) => match self.push(&net, &branch)? {
                    Pushed::Done => return Ok(SyncOutcome::Pushed { commits: ahead }),
                    Pushed::Behind => continue,
                },
                (ahead, behind) => {
                    let kept_both = self.merge_keeping_both(&remote_ref, &url)?;
                    let paths = self.changed_since(before.as_deref())?;
                    match self.push(&net, &branch)? {
                        Pushed::Done => {
                            return Ok(SyncOutcome::Merged { pulled: behind, pushed: ahead, paths, kept_both })
                        }
                        Pushed::Behind => continue,
                    }
                }
            }
        }
        Err(HistoryError::PushRaced)
    }

    fn merge_keeping_both(&self, remote_ref: &str, url: &str) -> Result<Vec<KeptBoth>, HistoryError> {
        if self.identity()?.is_none() {
            return Err(HistoryError::NeedsIdentity);
        }
        let merge_args = args!["merge", "--no-edit", "-q", remote_ref];
        let output = self.git().try_run(self.place(), merge_args)?;
        if output.ok() {
            return Ok(Vec::new());
        }
        let unmerged = self.unmerged()?;
        if unmerged.is_empty() {
            // Refused before it started (a local file in the way, say), or failed some other way: leave nothing open.
            let _ = self.git().try_run(self.place(), args!["merge", "--abort"]);
            return Err(failed(merge_args, &output));
        }
        let date = self.git().run(self.place(), args!["log", "-1", "--format=%as", remote_ref])?.stdout_text();
        let label = format!("from {}, {}", remote_label(url), date.trim());
        let work_tree = self.place().work_tree.clone();
        let mut kept = Vec::new();
        for (rel, stages) in unmerged {
            match (stages.get(&2), stages.get(&3)) {
                (Some(ours), Some(theirs)) => {
                    let copy = sibling(&rel, &label, |candidate| work_tree.join(candidate).exists());
                    self.write_blob(ours, &rel)?;
                    self.write_blob(theirs, &copy)?;
                    self.git().run(self.place(), args!["add", "--", rel, copy])?;
                    kept.push(KeptBoth { rel, remote_copy: copy });
                }
                (Some(only), None) | (None, Some(only)) => {
                    self.write_blob(only, &rel)?;
                    self.git().run(self.place(), args!["add", "--", rel])?;
                }
                (None, None) => {
                    self.git().run(self.place(), args!["rm", "-q", "--cached", "--ignore-unmatch", "--", rel])?;
                }
            }
        }
        self.git().run(self.place(), args!["commit", "-q", "--no-edit", "--no-verify"])?;
        Ok(kept)
    }

    fn write_blob(&self, sha: &str, rel: &str) -> Result<(), HistoryError> {
        let bytes = self.git().run(self.place(), args!["cat-file", "blob", sha])?.stdout;
        let path = self.place().work_tree.join(rel);
        let io = |source| HistoryError::Io { path: path.clone(), source };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(io)?;
        }
        std::fs::write(&path, bytes).map_err(io)
    }

    /// `path → stage → blob` from `git ls-files -u -z` (`<mode> <sha> <stage>\t<path>`).
    fn unmerged(&self) -> Result<BTreeMap<String, BTreeMap<u8, String>>, HistoryError> {
        let raw = self.git().run(self.place(), args!["ls-files", "-u", "-z"])?.stdout;
        let mut out: BTreeMap<String, BTreeMap<u8, String>> = BTreeMap::new();
        for entry in raw.split(|b| *b == 0).filter(|e| !e.is_empty()) {
            let text = String::from_utf8_lossy(entry);
            let Some((meta, path)) = text.split_once('\t') else { continue };
            let mut parts = meta.split(' ');
            let (_mode, sha, stage) = (parts.next(), parts.next(), parts.next());
            if let (Some(sha), Some(Ok(stage))) = (sha, stage.map(str::parse::<u8>)) {
                out.entry(path.to_string()).or_default().insert(stage, sha.to_string());
            }
        }
        Ok(out)
    }

    fn push(&self, net: &Git, branch: &str) -> Result<Pushed, HistoryError> {
        let push_args = args!["push", "--porcelain", "--set-upstream", "origin", branch];
        let output = net.try_run(self.place(), push_args)?;
        if output.ok() {
            return Ok(Pushed::Done);
        }
        // `--porcelain` marks a refused ref with `!`; `[rejected]` is the remote being ahead, while
        // `[remote rejected]` is the server saying no (a protected branch, a file too large) — that one is git's
        // sentence for the person, not a race to retry.
        let raced = output.stdout_text().lines().any(|line| line.starts_with('!') && line.contains("[rejected]"));
        if raced {
            Ok(Pushed::Behind)
        } else {
            Err(failed(push_args, &output))
        }
    }

    fn branch(&self) -> Result<String, HistoryError> {
        let output = self.git().try_run(self.place(), args!["symbolic-ref", "--short", "-q", "HEAD"])?;
        let name = output.stdout_text().trim().to_string();
        if output.ok() && !name.is_empty() {
            Ok(name)
        } else {
            Err(HistoryError::Git {
                command: "symbolic-ref".into(),
                said: "the repository is not on a branch (a detached HEAD); check out a branch to sync".into(),
            })
        }
    }

    fn ref_exists(&self, name: &str) -> Result<bool, HistoryError> {
        Ok(self.git().try_run(self.place(), args!["rev-parse", "--verify", "-q", name])?.ok())
    }

    fn head(&self) -> Result<Option<String>, HistoryError> {
        let output = self.git().try_run(self.place(), args!["rev-parse", "--verify", "-q", "HEAD"])?;
        Ok(output.ok().then(|| output.stdout_text().trim().to_string()))
    }

    fn count(&self, range: &str) -> Result<u32, HistoryError> {
        let output = self.git().run(self.place(), args!["rev-list", "--count", range])?;
        Ok(output.stdout_text().trim().parse().unwrap_or(0))
    }

    fn ahead_behind(&self, remote_ref: &str) -> Result<(u32, u32), HistoryError> {
        let range = format!("HEAD...{remote_ref}");
        let output = self.git().run(self.place(), args!["rev-list", "--left-right", "--count", range])?;
        let text = output.stdout_text();
        let mut parts = text.split_whitespace().map(|n| n.parse().unwrap_or(0));
        Ok((parts.next().unwrap_or(0), parts.next().unwrap_or(0)))
    }

    fn changed_since(&self, before: Option<&str>) -> Result<Vec<String>, HistoryError> {
        let output = match before {
            Some(before) => self.git().run(self.place(), args!["diff", "--name-only", "-z", before, "HEAD"])?,
            None => self.git().run(self.place(), args!["ls-tree", "-r", "--name-only", "-z", "HEAD"])?,
        };
        Ok(output
            .stdout
            .split(|b| *b == 0)
            .filter(|p| !p.is_empty())
            .map(|p| String::from_utf8_lossy(p).into_owned())
            .collect())
    }

    /// The git used for network calls. ssh is told not to prompt — there is no terminal to prompt on — unless the
    /// person has already said how ssh should run.
    fn network_git(&self) -> Result<Git, HistoryError> {
        let git = self.git().clone();
        let chosen_env = std::env::var_os("GIT_SSH_COMMAND").is_some() || std::env::var_os("GIT_SSH").is_some();
        let chosen_config = self.git().try_run(self.place(), args!["config", "--get", "core.sshCommand"])?.ok();
        Ok(if chosen_env || chosen_config { git } else { git.with_env("GIT_SSH_COMMAND", "ssh -o BatchMode=yes") })
    }
}

fn remote_ref(branch: &str) -> String {
    format!("refs/remotes/origin/{branch}")
}

/// Refuse an address with a login in it before it reaches `.git/config` or a log.
pub fn check_url(url: &str) -> Result<(), HistoryError> {
    if url.is_empty() || url.starts_with('-') || url.chars().any(char::is_control) {
        return Err(HistoryError::BadUrl(url.to_string()));
    }
    if let Some((scheme, rest)) = url.split_once("://") {
        let authority = rest.split('/').next().unwrap_or_default();
        if let Some((userinfo, _)) = authority.rsplit_once('@') {
            let http = matches!(scheme.to_ascii_lowercase().as_str(), "http" | "https");
            if http || userinfo.contains(':') {
                return Err(HistoryError::CredentialsInUrl);
            }
        }
    }
    Ok(())
}

/// Where a remote lives, for naming the copy a conflict keeps: `github.com`, or `remote` for a folder.
fn remote_label(url: &str) -> String {
    let host = if let Some((_, rest)) = url.split_once("://") {
        let authority = rest.split('/').next().unwrap_or_default();
        let host = authority.rsplit_once('@').map_or(authority, |(_, host)| host);
        host.split(':').next().unwrap_or_default().to_string()
    } else if let Some((before, _)) = url.split_once(':').filter(|(before, _)| before.len() > 1 && !before.contains('/')) {
        // scp-like `git@github.com:owner/repo`; a one-letter "host" is a Windows drive.
        before.rsplit_once('@').map_or(before, |(_, host)| host).to_string()
    } else {
        String::new()
    };
    let clean: String = host.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-')).collect();
    if clean.is_empty() || clean == "file" {
        "remote".to_string()
    } else {
        clean
    }
}

/// `notes/a.md` → `notes/a (from github.com, 2026-11-23).md`, numbered when that name is taken.
fn sibling(rel: &str, label: &str, taken: impl Fn(&str) -> bool) -> String {
    let (dir, file) = rel.rsplit_once('/').map_or(("", rel), |(dir, file)| (dir, file));
    let (stem, ext) = match file.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem, format!(".{ext}")),
        _ => (file, String::new()),
    };
    let prefix = if dir.is_empty() { String::new() } else { format!("{dir}/") };
    let mut n = 1;
    loop {
        let counter = if n == 1 { String::new() } else { format!(" {n}") };
        let candidate = format!("{prefix}{stem} ({label}){counter}{ext}");
        if !taken(&candidate) {
            return candidate;
        }
        n += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logins_in_an_address_are_refused_and_plain_addresses_pass() {
        for bad in ["https://ghp_token@github.com/a/b.git", "https://user:pw@gitea.local/a/b", "ssh://u:pw@host/a", "-uhack"] {
            assert!(check_url(bad).is_err(), "{bad}");
        }
        for good in ["https://github.com/a/b.git", "git@github.com:a/b.git", "ssh://git@host:22/a/b", "/srv/git/notes.git"] {
            assert!(check_url(good).is_ok(), "{good}");
        }
    }

    #[test]
    fn a_remote_is_named_by_its_host() {
        assert_eq!(remote_label("https://github.com/a/b.git"), "github.com");
        assert_eq!(remote_label("git@gitlab.com:a/b.git"), "gitlab.com");
        assert_eq!(remote_label("ssh://git@gitea.local:2222/a/b"), "gitea.local");
        assert_eq!(remote_label("/srv/git/notes.git"), "remote");
        assert_eq!(remote_label(r"C:\git\notes.git"), "remote");
        assert_eq!(remote_label("file:///srv/git/notes.git"), "remote");
    }

    #[test]
    fn the_remote_copy_sits_beside_the_note_and_never_replaces_another_file() {
        let label = "from github.com, 2026-11-23";
        assert_eq!(sibling("notes/a.md", label, |_| false), "notes/a (from github.com, 2026-11-23).md");
        assert_eq!(sibling("todo", label, |_| false), "todo (from github.com, 2026-11-23)");
        let taken = |c: &str| c == "a (from github.com, 2026-11-23).md";
        assert_eq!(sibling("a.md", label, taken), "a (from github.com, 2026-11-23) 2.md");
    }
}
