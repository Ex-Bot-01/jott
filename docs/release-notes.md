# Jott 0.0.33 — a note can be taken back to what it said yesterday

An editor saves what you type and forgets what it replaced: undo ends when a note closes, the OS trash only holds
deleted files, and a bad paste overwrites the only copy. This adds version history and an optional remote by driving
**the git already installed on the machine**. Jott ships no git — nothing is bundled, no 34 MB added to the download,
no second licence to carry — and it does not require one: without git, every other part of Jott behaves exactly as it
did in 0.0.32, and Settings says what git would add and where to get it.

## What you'll see

- **Said once, then never again.** Open a folder and one dismissible line appears — *Jott can keep versions of your
  notes, restore earlier ones and back them up — once git is installed.* with **Show me**. Dismiss it and it stays
  dismissed; after that the offer lives only in Settings. Never a modal, never on every launch.
- **Settings → Versions & sync.** It names the git it found and where it came from — a path you chose, then the
  program search path, then the usual install folders — or gives the platform's own route (`Download:`, *or run*
  `winget install --id Git.Git -e --source winget`) beside **Check again**, which finds a git installed while Jott
  was open. A chosen path that does not work is an error of its own, never a quiet fallthrough to a different binary,
  and a git below **2.28** names the version it found and the one it needs.
- **Turn on version history**, per folder, only when you press it — Jott never runs `git init` behind you. A folder
  that already uses git says so (*This folder already uses git. Jott can save versions into that history.*) and offers
  **Use existing history**; one with somebody else's work staged is left alone rather than committed into.
- **versions** beside the open note's name lists its versions across renames. Pick one: a diff against what is there
  now, where *green lines come back with a restore; red lines are in the note now and would go*. Then
  **Restore this version**.
- **A restore is not a rewrite.** It saves what the note says now as a version first, then writes the old bytes through
  the same atomic save as typing — so the list has a *restored* version on top and the restore can be undone the same
  way. An unsaved buffer is saved first or nothing happens: *Save your changes first — a restore must not overwrite
  them.*
- **Sync.** Paste the address of an empty repository on GitHub, GitLab, Gitea or any git server, **Connect**,
  **Sync now**. git asks for the login itself (Credential Manager, ssh-agent) and Jott never sees or stores it. An
  address with a login written into it is refused before anything is sent: *This address has a login written into it.
  Use it without the login — git will ask for it.*
- **A folder in OneDrive, Dropbox, Google Drive, iCloud, Box, Nextcloud or Syncthing** keeps its repository in Jott's
  own data folder instead of inside the synced folder, and says why: *two syncers on the same files fight*. Versions
  stay on that computer.

Versions are made after a minute without changes (adjustable, 10 s to 1 h) and when Jott closes — never per keystroke,
never per save — with messages like `Jott: edited a.md, notes/b.md`, so `git log` stays readable in a real git client.

## What it refuses

- **Nothing is thrown away.** No `reset --hard`, no force push, no rewritten history, no merge that picks a winner.
  Same note changed on both machines: yours stays at its path and the other text arrives beside it as
  `name (from github.com, <date>).md`, and the panel lists every such pair.
- **A vault inside somebody else's repository** is refused, naming that repository — autosave commits landing in your
  project's history is not something to do to you.
- **Files over 50 MB** stay on disk untouched and are left out of versions, listed under *Not kept in versions —
  larger than 50 MB*, because a history that cannot be pushed fails later and far from the cause.
- **np's generated `INDEX.md` files are never versioned**, through git's repository-local `info/exclude`, so nothing is
  written into your notes to achieve it. Jott re-adds them on open, which is why a fresh clone on a second machine
  gets the same treatment.
- **Your own git configuration stays yours.** Only the settings that would break an unattended commit are overridden,
  per call: line-ending conversion, signing, a global hook running on every autosave, path quoting, `fsmonitor`, and
  `safe.directory` for the one call instead of written into your global config. The credential helper is kept — that is
  how sync authenticates. With no `user.name` anywhere, Jott asks once and writes the answer into that repository only.
- Branches, a commit graph, blame, staging hunks, a merge editor, provider sign-in inside Jott, creating the remote for
  you, Git LFS: not here. The folder is a plain git repository, so use git for those.

## What I measured before shipping it

Linux, git 2.53.0 in front of the suites, and `np` deliberately off `PATH` so the tests about np being absent fail for
the right reason. The 13 integration tests in the new `jott-history` crate drive **real git in temp folders**, with a
bare repository in another temp folder standing in for the remote — no test touches the network or a real provider:

| behaviour | test |
|---|---|
| turning history on makes a repository in the vault and a version of what is already there | `turning_history_on_…` |
| a hostile global config (`autocrlf=true`, `gpgsign=true`, a failing hook) cannot break or change an unattended commit | `a_hostile_global_config_…` |
| a note's versions survive a rename, and its old bytes come back byte for byte | `every_version_of_a_note_is_listed_across_a_rename_…` |
| two machines through a remote, and a note both sides changed, keep **both** texts | `two_machines_sync_through_a_remote_…` |
| different notes changed on both sides merge without producing copies | `changes_to_different_notes_on_two_machines_merge_…` |
| a cloud-synced vault leaves nothing git-owned inside the folder | `a_cloud_synced_vault_keeps_its_repository_in_app_data_…` |
| an address with a login in it never reaches `.git/config` | `an_address_with_a_login_in_it_never_reaches_the_repository` |

Plus, at unit level: detection order and the wrong-chosen-path error, Apple's `/usr/bin/git` stub never being run
without the developer tools, a Windows git install found without a restart, and the remote's copy always landing beside
a note rather than on top of one. The journal (`jott.jsonl`) gains `version`, `restored`, `sync` and
`remote-connected` lines: path counts and host names only, never note text and never the path half of an address.

**Not measured here, and I am not claiming it:** no Windows machine sat in front of this release, and no real provider
was contacted — the remote in every test is a local bare repository. `docs/testing-on-windows.md` §2b-zj is the sheet
for exactly those gaps: git installed while Jott is open, OneDrive, GitHub's real credential prompt, two machines.

## What this release deliberately does not claim

- **No history for people without git.** That is the trade made openly for shipping nothing: install git and the
  feature is there, decline it and Jott is what it was. If the barrier turns out to be too high, the alternative is a
  compiled-in git library that cannot push — `docs/superpowers/specs/2026-11-23-git-versions-and-sync-design.md`
  records why that was not chosen today.
- **No claim that sync is safe against every cloud service.** Detection covers the seven services named above; a
  folder synced by something else gets a `.git` inside it like any other folder. Adding one is a marker and a test.
- **The np floor stays 0.5.1** and nothing about indexing, search or reranking changed in this release.

## Numbers

442 Rust tests — 409 in `jott-core` across its 30 binaries, 33 in the new `jott-history` — with
`cargo clippy --all-targets -- -D warnings` clean on both crates, 539 front-end tests green, `svelte-check` at 0 errors
and 0 warnings, and `vite build` clean. CI runs all of it again on this tag, history crate included, and builds no
installer until it passes; the installers and their `SHA256SUMS.txt` are what CI publishes below.

## Upgrading

Install over 0.0.32. Nothing re-indexes, nothing re-embeds and no ranking model is fetched again. History is **off
until you turn it on in a folder**, so this upgrade puts no `.git` in your vault and downloads nothing you did not ask
for; the only new thing on first launch is the one line about versions, which goes away when you dismiss it.
