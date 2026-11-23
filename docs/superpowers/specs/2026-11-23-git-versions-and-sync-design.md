# Versions and sync through the git someone already has — design

Jott saves notes and nothing remembers what they said yesterday. The editor's undo ends when a note closes, the OS
trash only holds deleted files, and an outside edit or a bad paste overwrites the only copy. This adds version history
and an optional remote — GitHub, GitLab, Gitea, anything that speaks git — by driving the `git` installed on the
machine. Jott ships no git and does not require one: without it, Jott says what git would add and where to get it.

## Intent and the why

My reading of what was asked for: the notes folder becomes a normal git repository, so a note can be taken back to an
earlier version, and the whole folder can optionally be pushed to and pulled from a provider as a backup and a way to
use more than one machine. Nothing is bundled — "if you want versions, restore and backups, install git from here and
Jott will pick it up".

Who it's for: somebody who has lost a paragraph to a bad edit and wants it back; somebody with two machines; somebody
who already keeps notes in a git repo and wants the editor to stop pretending it isn't one.

Why in Jott, and not in np: np indexes and searches — its methods are `initialize`, `scan`, `embed`, `search`,
`models.list`, `embeddings.enable/disable` (`np/methods.rs`) — and has never written a note. Every write is already
Jott's (`vault/files.rs`: `write_note`, `create_note`, `rename`, `delete_to_trash`). History records writes, so it
lives with the writer. Putting it in np would make np a second program that changes notes, and only the editor knows
when a "save" happened. The README's rule — nothing in `note-processor` gets edited from here — is untouched.

## Goals

- **Detect git** on every platform, report its version and where it was found, and say plainly when it is missing or
  too old — with the platform's install route and a **Check again** button that needs no restart.
- **Turn on version history** per folder: create a repository in the vault, or adopt the one already there.
- **Versions → Restore** per note: list the versions of the open note, preview what differs from now, restore one.
- **Connect remote** by pasting a URL, then **Sync now**. Login is git's business (Git Credential Manager, ssh-agent),
  never Jott's.
- Every git call isolated from the person's own git configuration where that configuration would break an unattended
  commit, and nowhere else.
- One new crate, `crates/jott-history`, that owns every git invocation. `jott-core` orchestrates; the UI never sees git.

## Non-goals

Branches, a commit graph, blame, staging individual hunks, a merge editor, provider sign-in inside Jott, creating the
remote repository for the user, Git LFS, restoring a deleted note from history (the OS trash covers this today; a
"deleted notes" list is a follow-up), and any git feature in np.

## Anti-goals

- **Losing text.** No `reset --hard`, no force push, no rewritten history, no merge that picks a winner. History is
  append-only; a restore is a new version on top, and a sync conflict keeps both sides.
- **Bytes nobody asked for.** Git must store and check out exactly what Jott wrote: §13's external-change detection
  compares hashes, so a line-ending conversion on checkout would show as a change nobody made.
- **Jott becoming a git client.** A person who knows git uses git; Jott covers save, restore and sync, and leaves the
  repository readable by every other tool.
- **Storing credentials.** No token, password or key goes into Jott's files, logs or diagnostics.
- **Shipping git.** No MinGit in the installer, no GPL obligations, no 34–38 MB added to the download.

## Constraints

- **Git is optional and external.** It is found, never installed. Without it everything else in Jott works as today.
- **Platform reality.** Linux: git is one package away and the deb/AppImage can depend on or recommend it. Windows: not
  present by default. macOS: `/usr/bin/git` is a stub that opens Apple's "install command line developer tools"
  dialog when run, so detection must not execute it blindly (check `xcode-select -p` first). macOS stays a source build,
  as for np.
- **The watcher already ignores `.git`.** Dot-paths are dropped by `vault/watch.rs` (the `.git/HEAD` rule) and
  `vault/walk.rs`, and the tree hides dot-folders (NOTES.md). Files git rewrites in the working tree during a pull or a
  restore are not dot-paths and arrive as outside edits — the path issue #29 already routes to the ingest queue and
  §13's reload/keep-mine handling.
- **np ignores `.git` and `.gitignore`** (confirmed by np's owner, 2026-11-23). Nothing to file there.
- **No console windows.** Every spawn on Windows carries `CREATE_NO_WINDOW`, as `np/client.rs` does for np.
- **No shell.** Arguments go as an argv array, paths after `--`. Nothing typed by a user is ever interpolated into a
  command line.
- **Machine-readable output only.** `--porcelain=v2 -z`, `--format` with explicit separators, `-z` wherever paths are
  listed. Git's human sentences are shown to the person verbatim in failure messages, never parsed.
- **Measured, not assumed:** gix (gitoxide) cannot push yet and libgit2 adds a C build; that is why this drives the git
  binary. MinGit 2.56.1 is 33.7–38.0 MB zipped; that is why it is not shipped.

## Approaches

**A — drive the installed git binary (chosen).** Complete (push, merge, credentials) and proven; nothing shipped, no
license work, the repository is plain git for every other tool. Trades: versions only exist for people who install git,
and Jott has to defend against configuration it did not write.

**B — gix compiled in, git only for sync.** History for everyone with a few MB and no install; sync still needs the
binary. Two implementations of one feature, and gix's missing push leaves the hard half to the binary anyway.
Worth revisiting if history should be on by default for people without git.

**C — ship MinGit in the Windows installer.** No install barrier, a known version. Adds 34–38 MB and GPLv2
obligations for the copy shipped (license, notices, matching source). Rejected for size.

**D — put git into np.** Rejected: np would start writing notes, git work would wait on np releases, and np does not
know when a save happens (see *Intent*).

## Finding git

Order, the same shape as np's (`np/resolve.rs`): **a path set in Settings → the program search path → the platform's
usual install locations.** A configured path that is wrong is an error of its own, never a fallthrough — someone chose
a binary, and quietly using another would commit with something they did not pick.

Usual locations, checked by existence before anything is run:

| Platform | Locations |
|---|---|
| Windows | `%ProgramFiles%\Git\cmd\git.exe`, `%LocalAppData%\Programs\Git\cmd\git.exe` |
| macOS | `/opt/homebrew/bin/git`, `/usr/local/bin/git`, then `/usr/bin/git` only if `xcode-select -p` succeeds |
| Linux | `PATH` is enough |

The usual locations are what make **Check again** work without a restart: a Windows install updates the PATH of new
processes, not of the Jott that is already running.

Found → `git --version`, parsed against a floor of **2.28** — the oldest git with `init -b`, with
`--pathspec-from-file` (2.26) and `%as` (2.25) under it — which accepts Ubuntu 22.04's 2.34.1 and current Git for
Windows. Too old → the same kind of sentence np's floor produces: found version, needed version, where to get a newer
one. `safe.directory` passed with `-c` is honoured only by newer gits; on older ones a vault that fails the ownership
check gets git's own sentence, which names the fix.

**Not found** → the Settings section reads, in substance:

> Jott can keep versions of your notes, restore an earlier one, and back them up to GitHub, GitLab or Gitea — using
> git. Git isn't installed on this computer. Install it from https://git-scm.com/downloads and press **Check again**.

with the platform's short route underneath: Windows `winget install --id Git.Git -e`; macOS `xcode-select --install`
or `brew install git`; Linux the distribution's package manager. The same sentence appears **once** as a dismissible
hint outside Settings; after that it lives only in Settings. Never a modal, never on every launch.

## Isolation from the person's git configuration

Git reads system, global and repository config. Some of it breaks an unattended commit; some of it is exactly what sync
needs (Git for Windows puts `credential.helper=manager` in the *system* config). So Jott does **not** discard
configuration files — it overrides the specific keys on every call with `-c`, which outranks every file:

| Override | Why |
|---|---|
| `core.autocrlf=false`, `core.safecrlf=false` | bytes in = bytes out; §13's hashes stay true |
| `commit.gpgsign=false`, `tag.gpgsign=false` | no key prompt, no failure from an unattended commit |
| `core.hooksPath=<empty dir in app data>` | a global hook must not run on every autosave |
| `core.quotepath=false` | non-ASCII note names come back as names |
| `core.fsmonitor=false` | no daemon started on the user's behalf |
| `safe.directory=<vault>` | vaults on another drive or a network share pass git's ownership check — for this call only, never written to anyone's global config |

Plus environment: `GIT_TERMINAL_PROMPT=0` (a missing credential helper fails instead of hanging on a terminal nobody
sees), `GIT_SSH_COMMAND=ssh -o BatchMode=yes` unless the person set their own, `GIT_OPTIONAL_LOCKS=0` for background
status reads.

**Identity.** Commits carry the person's own `user.name`/`user.email` when git has them. When it has none, Jott asks
once in the panel and writes the answer into that repository's `.git/config` only.

## Version history

**Turning it on.** No repository → `git init` with branch `main`. A repository at the vault's root → "This folder
already uses git" and **Use existing history**; Jott never commits while that repository is mid-merge, mid-rebase or
has staged changes it did not stage. A vault that is a *subfolder* of another repository is refused, naming that
repository: autosave commits landing in somebody's project repository are not a thing to do to them, and a nested
repository inside it would hide the notes from the outer one. Either way, np's generated files (`INDEX.md`,
`INDEX-*.md`) go into `info/exclude` — repository-local, invisible, never written into the person's notes — and Jott
re-adds them whenever it opens a repo, so a fresh clone on another machine gets them too.

The on/off decision is remembered per vault in `state/<vault key>/history.json`, beside the stamps: a `.git` that is already there is not consent. (Not in `vaults.json`: its two-window merge rules are about recents and consent, and this needs none of them.)

**Cloud-synced folders.** OneDrive, Dropbox, Google Drive, iCloud Drive, Box, Nextcloud/ownCloud and Syncthing copy a
folder file by file; a `.git` in such a folder gets its object files and refs copied out of order between machines,
which is how repositories end up corrupt. Jott detects these folders — from the services' own markers and settings
(OneDrive's environment variables, Dropbox's `info.json` and `.dropbox`, Nextcloud's `.sync_*.db`, Syncthing's
`.stfolder`, the platform's `iCloudDrive`, `Mobile Documents`, `CloudStorage`, `My Drive` and `Box` folders) — and
when the vault sits in one:

- the repository lives in **Jott's app data** (`state/<vault key>/history.git`), with the vault as its work tree, so nothing
  git owns is inside the synced folder;
- **Connect remote** is not offered — the cloud service already syncs the notes, and two syncers on one set of files
  fight; history is per machine, which is what the panel says;
- a `.git` that is already in such a folder is the person's own choice: **Use existing history** stays available,
  with the risk said once beside the button.

**Large files.** A file over **50 MB** is left out of versions — GitHub warns at 50 MB and refuses at 100 MB, and a
history that cannot be pushed is a history that fails later, far from the cause. Such files stay on disk untouched;
the panel lists each one with its size under *Not kept in versions*. A tracked file that grows past the limit keeps
its last version under it.

**When a version is made.** After saves, once the vault has been quiet for a while (default 60 s, a setting); when
Jott closes; immediately before a restore and before a sync. Never per keystroke, never per save. A version contains
every changed path in the vault, including edits made outside Jott. Message: `Jott: edited a.md, notes/b.md`
(shortened past a handful of names), so `git log` stays readable in other tools.

**Versions → Restore.** For the open note: `git log --follow` lists its versions (date, message); choosing one shows
a line diff against the current text; **Restore** reads the old bytes (`git show <version>:<path>`) and writes them
through `write_note` — the same atomic save as typing — then commits `Jott: restored a.md to the version of <date>`.
An unsaved buffer is saved first or the restore is cancelled; the person chooses, nothing is discarded silently.

## Sync

**Connect remote.** Paste a URL → `git ls-remote` proves it answers and the login works → it becomes `origin`.
A URL with credentials in it (`https://token@host/…`) is refused with the reason, before it reaches `.git/config` or a
log. An empty remote gets the first push. A remote with history unrelated to the local one is refused with a sentence;
combining two histories is a git job, not a button.

**Sync now.** Make a version → `git fetch` → if only one side moved, fast-forward or push → if both moved, `git merge`:
- no overlapping notes: the merge commits by itself, then push;
- the same note changed on both sides: **keep both** — Jott's text stays at its path, the remote's is written beside
  it as `name (from <remote host>, <date>).md`, the merge is committed, and the panel lists every such pair. No side is
  chosen for anyone; this is §13's "never guess" applied to sync.

A push rejected because the remote moved meanwhile repeats fetch-and-merge once, then stops and says so. Notes git
changed on disk reach open buffers through the existing external-change handling: unmodified buffers reload,
modified ones show `changed on disk`.

**Sync on open and close** is a setting, off by default. The panel always shows the last sync's outcome and time.

## Where things live

| What | Where |
|---|---|
| History, remote URL | the repository's own git dir — nothing duplicated in Jott |
| The repository of a cloud-synced vault | `state/<vault key>/history.git` in the app data dir |
| History on/off, quiet interval, sync on open/close | per vault, `state/<vault key>/history.json` |
| Chosen git path | per app, `git.json` in the app data dir |
| Empty hooks dir | app data dir |
| Journal lines | `diagnostics.rs`, same rules as today: git found/missing/version/source, version made (count of paths), sync outcome. URLs logged without userinfo, never a credential. The diagnostics drawer gains a git row next to np's. |

**Crate boundary.** `jott-history` spawns git and returns data: found/version/source, repo state, versions of a
path, the bytes of a path at a version, sync outcomes as an enum with git's sentence attached. It does not depend on
`jott-core` and never writes a note; `jott-core` calls it and does every write through `vault/files.rs`. New IPC
commands are thin: git status / check again, enable history, list versions, preview, restore, connect, sync.

## Verification

- `jott-history` integration tests run real git in temp dirs: a bare repository in another temp dir is the remote, so
  no test touches the network. Covered: detection order and the wrong-configured-path error; the `-c` overrides beating
  a hostile global config (`autocrlf=true`, `gpgsign=true`, a failing hook) set through `GIT_CONFIG_GLOBAL` in the
  test; init vs adopt; quiet-period commits; restore bytes identical to the old version; fast-forward, clean merge and
  keep-both on a conflicting note; push retry; a URL with credentials refused; a vault inside another repository
  refused; the app-data repository of a cloud-synced vault leaving nothing in the vault; a file over the limit left
  out and reported. Cloud detection is a pure function over an injected environment and file probe, tested per
  service without any service installed.
- CI runners already have git. A test that cannot find git fails with that sentence rather than skipping.
- The Windows hand test (`docs/testing-on-windows.md`) gains: Jott open without git → install git → **Check again**
  finds it without a restart.

## Decided as mine, correct me freely

- **Off until asked.** History starts only when the person presses **Turn on version history**; Jott never runs
  `git init` on its own.
- **The repository is the vault's own `.git`**, not a hidden one in app data — the point is a folder every git tool
  and provider understands.
- **60 s quiet period** as the default, adjustable.
- **Keep both on conflict**, sibling file named after the remote's host and date.
- **Sync is manual by default**; on open/close is one opt-in toggle. No timer.
- **One remote, `origin`, one branch.** Whatever branch is checked out when history is adopted is the one Jott uses.

## Left to the builder

The wording of every sentence (tested, as everywhere); where the one-time hint sits; how the diff preview is drawn;
how long a message gets before it is shortened; whether `CREATE_NO_WINDOW` is shared with `np/client.rs` or repeated.

## Known unknowns, tracked

- **Cloud services not on the list.** Detection covers the services named above; a folder synced by something else
  gets an in-vault `.git`. Adding one is a marker and a test.
