# Jott improvement sweep — inspection findings and the plan that acts on them

Date: 2026-11-13. Build inspected: `v0.0.11` (`90e4468`, tag `v0.0.11`).

The ask was "go through every part, including the logging, and write down what to improve". This document is
that inspection plus the ordered plan. Nothing here is speculative: each finding names the file and the line of
reasoning, and everything I could measure, I measured — against the real `np` 0.3.1 binary where a claim about
the backend was involved.

## What was read

Every module in the product, not a sample:

| Area | Files | Verdict |
|---|---|---|
| np transport | `np/frame.rs`, `client.rs`, `handshake.rs`, `pool.rs`, `methods.rs`, `outcomes.rs`, `resolve.rs` | Careful and honest about states. One real hang risk (§A1). |
| Vault files | `vault/files.rs`, `walk.rs`, `stamps.rs`, `buffers.rs`, `watch.rs`, `store.rs` | Byte fidelity and path containment are solid. Two behavioural bugs (§A5, §A6) and one doc/code mismatch (§A7). |
| Ingest | `ingest/queue.rs`, `consent.rs` | The best-documented part of the app. One stuck-state bug (§A5). |
| Command surface | `ipc.rs` | Correct, but its error vocabulary lies in one common case (§A3), and it sends np a path spelling np does not normalise (§A4). |
| Diagnostics | `diagnostics.rs`, `about.rs` | Journal + trace are sound. Coverage gap: the log knows nothing about file work or about which run wrote it (§B). |
| Shell | `apps/shell/src/{main,ipc}.rs`, capabilities, `tauri.conf.json` | Thin as intended. Missing window-state persistence; CSP off (declined to change, §D). |
| Front end | `App.svelte`, `lib/*`, styles | The tested-modules pattern is working. Two races (§C11, §C12) and a large usability hole: two backend commands nobody can reach (§C13). |
| Build/CI | workflow, scripts, pins | Good. No formatting gate; the test job does not prove np was absent (§D15). |

Two of the findings are np's, not ours, and are filed rather than worked around:
[note-processor#4](https://github.com/Ex-Bot-01/note-processor/issues/4) (a trailing path separator makes `scan`
re-add a whole vault — measured: `added: 1`, then `added: 1` again for `/ws/`, then `unchanged: 1`) and
[note-processor#5](https://github.com/Ex-Bot-01/note-processor/issues/5) (`search` on a never-scanned folder
answers exactly like "no matches", and says `mode: hybrid` while only the keyword half ran).

---

## A. Backend bugs

**A1 — a wedged np hangs the window forever.** `NpClient::request` awaits its answer with no deadline
(`crates/jott-core/src/np/client.rs:203`). stdout staying open while np never answers is not hypothetical: an
embed that stalls on a download, or a scan blocked on a Windows file lock, leaves the search overlay spinning and
the ingest worker's `cycle()` never returning. Today only quitting escapes, via the drain cap.
*Fix:* a deadline per class of call — `initialize`/`models.list`/`search` short (they are supposed to answer in
milliseconds; §18.1 measured search at milliseconds when it is healthy), `scan`/`embed` long but finite, and a
shutdown ask already has its own grace. A timeout becomes a fault with np's stderr tail attached, so the drawer
and the log both show it. Test: the stand-in gets a `never_answers` scenario; the request must come back faulty
rather than hanging, and the trace must carry the row.

**A2 — a poisoned mutex can take the app down through its own diagnostics.** The journal's rule is "a diagnostic
that can stop a save is worse than no diagnostic" (`diagnostics.rs:69`), yet `Journal::record`, `broken()`, and
the session's `journal.lock().unwrap()` / `stamps.lock().unwrap()` / `watch_task.lock().unwrap()` panic the
moment any thread panics while holding one. One panicked worker would turn into a panic on every later save.
*Fix:* recover from poisoning everywhere in the diagnostics and stamp paths
(`lock().unwrap_or_else(|p| p.into_inner())`), keeping the panic's original site as the bug it is instead of
adding a second failure on top of it. Test: poison the journal's mutex deliberately, then assert a record still
lands and nothing panics.

**A3 — `NpGone` is returned for faults that prove np is alive.** `VaultSession::search` maps *every* JSON-RPC
fault to `ErrorKind::NpGone` (`ipc.rs:657`). Measured against real np 0.3.1: a missing folder answers `-32000`
"workspace … is not a directory that exists", an old build answers `-32601` naming its methods, bad params answer
`-32602`. All three mean "np answered and refused", which is the opposite of "np is gone" — and this is the same
class of mislabelling that cost us the v0.0.6 release note.
*Fix:* split the kinds — transport/startup failures stay `npGone`; a fault from np becomes `npRefused` carrying
np's sentence; `-32601` additionally hints at "Jott needs np X or newer". Front end gains the branch and the
wording, with the ipc contract test updated on both sides.

**A4 — we send np a path spelling it does not normalise.** Jott stores and keys its own state through
`store::to_abs` (which strips trailing separators) but hands `pool.scan/embed/search` the raw picked path.
Because of np#4, one picker that returns `D:\Notes\` and later a registry entry reading `D:\Notes` costs a full
re-add of the vault. We cannot fix np from here and should not wait for it.
*Fix:* normalise once at the boundary — every workspace value sent to np goes through `to_abs`. Test: open a
session on a path with a trailing separator and assert, from the stand-in's recorded request params, that np was
asked about the canonical form.

**A5 — the retry ladder sticks at 60 s.** `state.attempt` is reset only in the embed-applied arm
(`queue.rs:757`). A pass that indexes fine and then hits `configuration-disabled` or a refusal never resets it, so
the next genuinely locked scan waits an hour's worth of patience for a two-second problem.
*Fix:* reset on a committed scan (the index is not behind any more), independent of what embed did. Test: lock →
requeue → succeed → assert the next `Behind` quotes 2 s, not 60 s.

**A6 — every indexing pass costs an extra full walk.** np writes `INDEX.md` into the vault; our watcher reports
it; `onVaultChanged` with a rel that is not the open note calls `refresh()` (`App.svelte:247`) — a whole re-walk,
immediately followed by the ingest event's own coalesced re-list. On a big vault that is a second walk per pass,
and bursts of them during a first index.
*Fix:* two halves. Rust: `watch::should_report` ignores np's root catalogues (`INDEX.md`, `INDEX-*.md`) — they are
never editable and never shown as editable, so there is nothing on screen for that event to update. Front end:
route watcher-driven re-lists through the same 150 ms coalescing the ingest events already use, so a sync client
touching 500 files costs one walk instead of 500.

**A7 — `delete_to_trash` throws away the reason it claims to keep.** The comment says the OS refusal "goes into
the returned path's face rather than being swallowed" (`files.rs:478`); the code does `let _ = os_refusal;`. The
window then says "moved to Jott's own folder" with no hint of why not the Recycle Bin — and a user on a machine
with a disabled Recycle Bin is exactly the reader who needs that sentence.
*Fix:* carry the reason in `Trashed::AppDir` and in the command's sentence, and put a line in the log (§B).

---

## B. Logging — what the log does not know today

The journal holds one line per **np request** and nothing else (`RequestRecord`, seven keys). Consequences
measured against what testers have actually reported:

- "My note vanished." Nothing in the log says a file was written, moved, trashed, or that a mirrored unsaved
  buffer was replayed. The one question with an answer in someone's head has no answer in the file.
- "It worked yesterday." Lines carry `at_ms` but no run boundary and no version, so a log spanning an upgrade
  cannot be read as two different runs of two different builds against two different np files.
- The docs promise "seven possible keys and nothing else" (`docs/testing-on-windows.md:157`), which is true today
  and will be false the moment this section is implemented — so the doc changes with it, not after.

**B8 — a run header line.** Written once at startup, its own closed key set: Jott version, np version, protocol,
the np binary's path and how it was found (bundled / `$NP_PATH` / `$PATH`), OS, and the state root Jott is using.
That last one matters: `VaultSession::open` silently falls back to a temp folder when the app data dir is not
writable (`ipc.rs:305`), which today means someone's unsaved-buffer mirrors land in a place nobody told them
about and nothing says so.

**B9 — local-event lines.** A second record type for the file work around np: save written (bytes, rel), write
failed (sentence), external change raised as a conflict, mirror replayed or discarded, delete destination with its
reason, consent answered, "Index now" pressed, watcher unavailable, state root is a fallback. Same rules as the
request records: closed key set, pinned by tests, and **no note contents** — `rel` is allowed because "which file"
is the question being asked, and that decision gets written down next to the key list rather than left implied.

**B10 — the drawer names the state folder too**, not only np's data dir, and says when it is a fallback. One row,
and one sentence in "copy details".

---

## C. What a person using the app feels

**C11 — search can answer out of order.** Typing fast fires overlapping searches (`App.svelte:257-269`) and any
late answer can overwrite a newer one, because nothing says which request a result belongs to. The editor already
learned this lesson (the generation counter in `Editor.svelte:160`); the search box has not. Also from np#5: an
empty result in a folder that has never been indexed reads as "search is broken" — we know the folder's state, so
we should say *"this folder is not indexed yet"* instead of *no results*, and stop showing np's `mode` word when
it says `hybrid` while only keywords ran.

**C12 — watcher bursts re-walk the vault once per event** (see A6; the front-end half lives here).

**C13 — two finished features are unreachable.** `ipc.createFolder` and `ipc.moveEntry` exist, are wired as
Tauri commands, and have Rust tests. Nothing in the window calls them. So today you cannot make a folder, and you
cannot move a note between folders — only rename it where it stands. There is also no Ctrl+S: people press it, and
autosave quietly covering them is not the same as answering the key.
*Fix:* a context menu on tree rows (New note here, New folder, Rename, Move to…, Delete) plus buttons in the
sidebar header for the common two, and shortcuts: Ctrl+N new note, Ctrl+Shift+N new folder, Ctrl+R rename, Ctrl+F
focus search, Ctrl+S save now. The menu's deciding logic goes in `tree.ts` where it is testable; the component
only draws what it is told.

**C14 — the window does not remember itself.** Size and position reset to 1200×760 every launch, and the title
says "Jott" whatever folder is open. Both are one plugin and one call. Recents exist in the registry and are
shown nowhere — a small folder menu in the sidebar turns "reopen yesterday's notes" from a dialog tour into a
click.

---

## D. Declined, with reasons

- **Content-Security-Policy** is `null` (`tauri.conf.json`). Tightening it is defensible for a local app and could
  not be verified on this box, where no webview runs: a wrong CSP ships a blank window, which is the one failure
  mode we have already paid for once. Left as-is; documented as an accepted decision instead of an oversight.
- **Atomic save via temp-file-and-replace.** Rejected again for the same reason §18.8 gave: a plain temp file gets
  indexed by np, a dotted one confuses the watcher, and cross-volume replace is not atomic anyway. The mirrored
  buffer (§18.13) remains the recovery path, and A6/B9 make it more visible rather than replacing it.
- **Rewriting links on rename.** Detection only in M1 by design; rewriting needs np's resolver (§13, Plan 2).
- **Pruning stamps, mirrors and the digest cache.** All three grow slowly with files ever seen, none is unbounded
  in a way that hurts at any size we have measured, and every pruning rule risks forgetting something somebody
  will ask about. Recorded as known growth, not fixed blind.

---

## The plan, in order

Each item gets a failing test first, then the change, then the whole suite with np off `PATH`
(`PATH=$HOME/.cargo/bin:/usr/bin:/bin cargo test -p jott-core`), clippy under `-D warnings`, the front-end suite,
`svelte-check`, and the Windows-target type-check of the shell. Counts get quoted in the commit, not just "green"
— a scripted edit once deleted five tests while the suite stayed green, and only the count noticed.

| # | Work | Where | Proof |
|---|---|---|---|
| 1 | A1 request deadlines | `np/client.rs`, fake-np `never_answers` | new test in `np_client.rs`; trace row asserted |
| 2 | A2 poison-tolerant diagnostics | `diagnostics.rs`, `ipc.rs` | poisoned-mutex test in `diagnostics_journal.rs` |
| 3 | A3 honest error kinds | `ipc.rs`, `ipc.ts`, `App.svelte` | contract test on kinds; smoke test on wording |
| 4 | A4 canonical workspace to np | `ipc.rs`, `np/pool.rs` | stand-in records the params it was sent |
| 5 | A5 ladder reset on committed scan | `ingest/queue.rs` | lock → succeed → next wait is 2 s |
| 6 | A6 watcher: ignore np catalogues, coalesce re-lists | `vault/watch.rs`, `App.svelte` | `watch_policy.rs`; smoke test counts re-lists |
| 7 | A7 keep the trash refusal reason | `vault/files.rs`, `ipc.rs` | sentence contains the reason |
| 8 | B8 run header record | `diagnostics.rs`, shell startup | keys pinned; written once per run |
| 9 | B9 local-event records + privacy test extended | `diagnostics.rs`, `ipc.rs` | forbidden-key test covers both kinds |
| 10 | B10 drawer shows state root | `ipc.rs`, `diagnostics.ts/.svelte`, About text | TS contract test still passes; new row asserted in smoke test |
| 11 | C11 search generation guard + not-indexed sentence | `App.svelte`, `tree.ts`/`statusLine.ts` | unit tests for the pure parts; smoke test for the race |
| 12 | C13 tree actions and shortcuts | `tree.ts`, `Tree.svelte`, `App.svelte`, `Editor.svelte` | menu logic unit-tested; smoke test drives a new folder and a move |
| 13 | C14 window state + title + recents menu | shell plugin, `App.svelte` | Windows-target type-check; smoke test for the menu |
| 14 | D15 gates: `cargo fmt --check`, np-absent evidence in CI, doc-comment and indentation cleanup | workflow, `ipc.rs` | CI run itself |
| 15 | Docs: release notes, testing guide (the "seven keys" sentence), ledger | `docs/` | read-through against behaviour |

Releases: items 1–7 ship as **v0.0.12** (behaviour people can hit today); 8–13 as **v0.0.13** once they are in,
so a tester who reports "fixed in 0.0.12" means something checkable.
