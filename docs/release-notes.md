# Jott 0.0.29 — every open issue closed, and a newer engine in the box

Eleven issues, all of them real, all of them now fixed. Most are small and some are not: what you typed at the end of
a session used to be thrown away when you quit; clicking a recent folder that no longer exists replaced the vault you
were working in; on Windows, saving a note in a subfolder often did not re-index it. Alongside them the
note-processor inside the installer is now **0.6.0**, which is the part of this release you will not have to do
anything about.

## What you'll see

- **Quitting saves.** Close the window or press Ctrl+Q and the last few seconds of typing go to disk first, with the
  index closed politely rather than killed mid-write (§11's close-out). It was already written — `flush_on_close`
  existed — but nothing ever called it, so it never ran. If a note looked unchanged after you reopened Jott, this was
  why.
- **A missing recent folder no longer costs you your work.** Clicking a recents row whose folder has been moved or
  unmounted now says "that path is not a folder" and leaves the vault you had open exactly where it was. It used to
  replace your session with the failure, which turned a stale row in a list into losing your place.
- **Nested notes re-index on Windows.** A note at `projects\ideas\x.md` now updates its index entry when you save it.
  The tree measured the file one way and the invalidation another — `D:\notes\a/b.md` against `D:\notes\a\b.md` — so
  on Windows every note in a subfolder was re-indexed from bytes that were no longer there, or not at all. Search
  results for those notes could lag by however long it had been since something else dirtied the folder.
- **Names Windows would eat are refused.** `nul`, `aux.md`, `com1.txt`, `lpt9.log` cannot be created on Windows in any
  form, and on Linux or macOS a file with one of those names is a file that vanishes the moment the vault travels:
  writing it succeeds there and writes to a device instead of to disk. Jott refuses them at the name box and at the
  disk, and says which rule it's applying. Deleting a file that already has such a name still works everywhere — that
  is the way out.
- **The window has a Content-Security-Policy.** It shipped with `csp: null` since the first release. Scripts are
  same-origin, no remote anything, framing refused; inline styles stay allowed because CodeMirror writes them and an
  over-strict style policy fails as an unstyled editor rather than as an error.
- **One Jott per machine.** Starting a second one now wakes the first window — including from the taskbar, where
  clicking a minimised Jott looked like nothing happening — and refuses to start. Two processes used to share one
  vault's stamps and recents file by last writer winning silently.
- **Renaming stops freezing the window.** A rename read every other note in the folder inside a synchronous command,
  and asked the operating system what the vault root was once per note — 206 syscalls for a five-note folder before a
  single byte moved. It now runs off the UI thread with one root resolution and one read per note, and the journal says
  `renamed: from a.md; scanned 143 notes` so a slow rename in a big vault is measurable instead of mysterious.
- **Your unsaved drafts are yours.** App-data files — the registry of folders you've opened, per-vault index stamps,
  buffer mirrors, and the diagnostic journal — were created with whatever umask your session happened to have, which
  on a shared Linux box can mean another user reading the half-written note you never saved. They are `0600` now, in
  their own `0700` folders, including files that already existed.
- **Symlinks say what they are.** A symlinked folder used to appear as a plain attachment because nothing asked
  whether it was a link; a symlinked note showed its size from the link and its indexed-ness from whatever the link
  pointed at, which could be `/etc/passwd`. Jott still does not follow links out of your vault — that decision is
  yours to make deliberately, not by accident — but those rows now show `! refused · symlink` and a dangling link
  reports the path inside your folder rather than an absolute OS path.
- **A scan answer Jott cannot read no longer looks like success.** If note-processor's report is missing a field it
  needs, that used to be reported as "note-processor is not installed" — advice pointing at the wrong program — and in
  one shape, `{}`, the counters defaulted to zero and every note got marked indexed on the strength of nothing. It now
  says the answer was unreadable, names the missing field, marks nothing, and asks again next time something changes.
- **Search explains an empty meanings-only result.** With no vectors in a folder, a meanings-only search used to be
  answered with literally nothing — no hits, no reason — and Jott had to guess. np 0.6.0 answers that state with a
  sentence of its own, and Jott prints it instead of "Nothing matched".

## The engine in the box: note-processor 0.6.0

Protocol is still version 1, `initialize` still answers the same ten methods, and an index built by 0.5.1 opens in
0.6.0 with every note reported `unchanged` — no re-scan, no re-embed, nothing for you to do. Checked against the
published binaries rather than against the release page: hashes verified with `sha256sum -c`, `scripts/fetch-np` run
for real, and both versions driven over the same fixture vault. Two keys appear in a `scan` answer (`attachments`,
`attachment_examples`) and Jott ignores both; the floor stays at 0.5.1 because nothing here needs 0.6.0 and a person
whose np came from an older installer is not running something broken.

One visible side effect: np writes `INDEX.md`, and names containing `#`, `?`, `[`, `]`, `(`, `)`, `<`, `>` or a
newline are now percent-encoded in that catalogue — `roadmap#2.md` appears as `roadmap%232.md`. No note is renamed. If
you keep `INDEX.md` under git, expect one line per unusual name to change on the next index.

## Numbers for this release

- 380 Rust tests across 28 binaries, 498 Vitest tests, `svelte-check` clean, clippy clean with `-D warnings`.
- New gates: quit close-out, opening a vault that cannot be opened, single-instance wiring, rename threading, atomic
  whole-file rewrites, private file permissions, symlink rows, and `np_real_session` — a pre-release gate that drives a
  real note-processor through a whole session (open, save, index, search, close) instead of stopping at the handshake.
- Every fix above was reproduced first where a machine could observe it: red tests for the missing quit call, the
  swallowed registry refusal, the two spellings of one Windows path, the in-place `config.toml` rewrite, the
  world-readable draft, the symlink row that claimed to be an attachment, and the `{}` scan that marked everything
  indexed.

## What I know about and did not fix

- **A first-run race inside np.** Open a folder that has never been indexed and type in the search box before its
  first index finishes, and the reader child and the writer child can both try to create the same new database at the
  same moment; one answers `sqlite: table files already exists`. It reproduces on 0.5.1 as well — about one attempt in
  eight here — so it is not caused by this upgrade, and I did not want a retry heuristic landing in the same release as
  a version bump where nobody could tell which change caused a report. Filed upstream as note-processor#18, with
  jott#23 for what Jott would do if that stalls. The next keystroke clears it.
- **Two things I cannot see from this box.** The CSP is pinned as a string and the policy is checked by a test, but no
  webview runs here; if the window ever loads unstyled or blank with `Refused to apply inline style` in the console,
  that is the policy being too tight and it is a one-line fix. Same for whether a second launch raises your minimised
  window: the wiring is pinned, the raising needs eyes. Both are in §2b-ze of `docs/testing-on-windows.md`.

## Upgrading

Install over the top. Nothing moved, nothing needs re-indexing, and no setting changes meaning. If you were on 0.0.28
and the markdown styling looked right, take another look at the places sections 1, 3 and 4b of the testing guide point
at — several of this release's fixes are invisible until they aren't.
