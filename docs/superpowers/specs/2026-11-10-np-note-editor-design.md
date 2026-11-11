# Jott — a note editor for `np` (design)

*Jott, from "jotting"*: private repository now, MIT and public later as a deliberate step (§20).

**Status:** for approval.
**Date:** 2026-11-10.
**Builder note:** a reviewer reads this before it goes to the person who asked for it; questions are
sorted into *answered*, *minor*, and *theirs* before implementation starts.

---

## 1. Intent, and the why

There is a working local note index called `np` ([`Ex-Bot-01/note-processor`](https://github.com/Ex-Bot-01/note-processor)).
It indexes a folder of markdown, resolves the links between notes, embeds text and figures with a small
local model, and answers hybrid keyword+semantic queries. It has no face: it is a CLI and a stdio
protocol. Anyone who wants its answers has to open a terminal.

This project is that face: a desktop note editor for **one person** — the person who wrote `np` — where
writing a note quietly keeps the index current, and finding something is one keystroke and a phrase.

The reason it exists at all is daily use, not demonstration. It is judged by whether it stays out of the
way while notes accumulate, and by whether the index can ever be trusted to be true. Packaging, installers
and onboarding are deliberately later; the architecture must not block them.

## 2. Goals

| Goal | What "done" looks like |
|---|---|
| Write markdown in a folder on disk | Files are the truth. No database of notes, no import/export step, no app-only format. |
| Keep `np` current without being asked | Any save whose content actually changed re-indexes within ~1 s and then embeds that file, with no dialog and no button press — for edited notes and newly created ones alike. |
| Find things by word *and* by sense | One overlay, results with the matched words marked, Enter opens the note at the match. |
| Make `[[wikilinks]]` pleasant | Completion while typing, the folder shown exactly when the name is ambiguous, incoming links rewritten on rename/move after asking. |
| Be honest about the index | The user can always tell which files are in the index, which are waiting, and whether meaning or mere words produced a hit. |
| Stay portable enough to package later | Windows and Linux work today; macOS is buildable-in-principle; an installer later changes configuration, not architecture. |

## 3. Non-goals

- Sync, collaboration, mobile, cloud of any kind.
- WYSIWYG editing, live preview rendering, or a "reading view". The editor edits markdown text.
- Plugin system, theming marketplace, keymap customization beyond what the UI framework gives free.
- Backlinks pane (decided: later, once there are enough notes to need it; upstream issue #2 is its enabler).
- Editing anything `np` owns — `INDEX.md` and its shards are generated and read-only here.
- Multiple vaults in one window. One folder per app session; a switcher may come later (np supports several workspaces on one connection, so this is additive).
- An installer, code signing, auto-update. Architecture must not block them (§14).

## 4. Anti-goals — what would make this a failure even if it runs

- **A silent stale index.** A search that answers from yesterday's index while looking current. Every degraded state must be visible in the chrome.
- **A quiet edit to a file the user did not touch.** Rewriting links in other notes always asks first.
- **A surprise download.** The embedding model is 295 MiB; it is asked for once, in words, before byte one.
- **An error that names no fix.** `np` already refuses in sentences that say what to do; the app passes those through instead of showing "something went wrong".
- **Losing typed text** to a reload, a save conflict, or a crash.

## 5. Constraints — facts about the backend, measured here unless attributed otherwise

All measurements are from this host (Linux x86_64, 16 CPUs) against `np 0.3.1`, and are recorded with
their commands in [`INSTALL.md`](../../INSTALL.md).

| Fact | Consequence for us |
|---|---|
| Transport is **stdio JSON-RPC 2.0 / NDJSON**, one request per line. No port, no auth, no daemon. | The app spawns and owns a child process. There is nothing to connect to. |
| `np serve` handles **one request at a time**, in order; progress notifications interleave anywhere. Measured: a `search` queued behind a full-vault `embed` on the same connection waited **102.1 s**; a keyword search from a *second process* during that same embed answered in **0.00 s**, and a second-process `scan` committed in 60 ms mid-embed (§18.1). | One writer thread to stdin, one reader loop, requests matched by `id`. **Never queue a search behind an embed** — hence the two connections in §7. |
| Methods: `initialize`, `search`, `scan`, `embed`, `models.list`, `status.get`, `shutdown`. **No file-writing method, no listing method, no watch mode, no link-graph query.** | The app owns all file I/O and the tree; the app decides when to re-index; the app keeps its own link index (upstream [#2](https://github.com/Ex-Bot-01/note-processor/issues/2)). |
| Progress is **opt-in per connection** (`initialize` with `notifications: true`); phases `locking`, `resolving`, `indexing`, `fetching`, `embedding`, `writing`; every request ends `completed` **or** `refused`. | The progress bar is driven by these lines and nothing else. |
| `locked`, `offline`, `reason`, `vectors: false` arrive as **data in a result**, never as an error object; `locked` still ends `completed`. | They render as states, not failures. A greyed-out button on `locked` would report failure where nothing failed. |
| A search hit has **no `title`, no line number**; keys are `path, snippet, kind, score, bm25, cosine, keyword_rank, semantic_rank, linked_from, linked_notes, source_note`; `snippet` may contain newlines and marks terms with `[ ]`. | Result rows render the snippet's brackets as highlights; scroll-to-match is computed by the app (§10). |
| Keyword search is an **AND of every token** ([upstream #1](https://github.com/Ex-Bot-01/note-processor/issues/1)); hybrid masks it once vectors exist. | The overlay says which half answered, so an empty answer explains itself. |
| Measured: `scan` 12–31 ms (small vault), ~2.6 s at 4 000 notes; `embed` of one edited note **0.58–0.67 s** with a warm server; first `np setup` 13 s including the 295 MiB download at ~24 MB/s; `np serve` RSS 10 MB idle / **115 MB** during vector work. | One long-lived child for the whole session — the 0.6 s figure depends on the model staying loaded. Embed-on-save is affordable. |
| Model artifacts are **per workspace**: `<data_dir>/workspaces/<key>.models/embeddinggemma-2`. | A second vault means a second 295 MiB download, asked for again. |
| `provider = "local-model"` without `model = "<id>"` is refused as a config error. | We never write a partial `[embeddings]` block (§12). |
| Released 0.3.1 lacks `last_scan` in `status.get`; repo `main` has it (commit `ac31ce9`). | The app keeps its own last-scan stamp and **prefers `last_scan` when present**. Nothing may require it. |
| Windows asset is a mingw-w64 cross-build, **unsigned**; write-lock behaviour unmeasured on Windows; **no macOS asset at all**. | First-run must survive "np not found" with instructions, and the Windows path gets real testing early (§15). |

## 6. Approaches considered, and the pick

| | Shape | Trades off | Verdict |
|---|---|---|---|
| **A. Tauri v2** — Rust core + system webview UI | Rust spawns `np serve`, does FS walking, link index, JSON-RPC; window renders HTML/CSS with CodeMirror 6. | Constant Rust↔JS boundary; WebView2 on Windows, WebKitGTK on Linux instead of one guaranteed renderer. | **Chosen.** ~8–15 MB installed, no runtime shipped, installer later from the same tool, and np's protocol types live in the same language as np. Keeps the word "slim" true. |
| B. Electron — Node core + Chromium | Same split, bundled Chromium. | ~200 MB installed, 150–250 MB RAM for the shell before np's 115 MB; identical rendering everywhere and the most mature packaging. | Rejected: buys reliability with weight, against the stated priority. |
| C. Native toolkit (Slint / egui / AvalonEdit) | No webview anywhere. | The text editor — selection, IME, wrapping, decorations, completion popup — is hand-built. Months of work on the one component the app *is*. | Rejected: the editor is the product. |
| D. Web app + local HTTP shim | Browser UI, small server bridging to np. | Adds a listening port, a daemon, auth and port conflicts — four things np removes on purpose by using pipes. | Rejected outright. |

**Also considered and rejected: linking `np-core` as a Rust library instead of spawning `np serve`.**
Tempting (same language, no protocol), but `np` ships as a binary users install or download, its release
cadence is independent of ours, and the stdio protocol exists precisely so an app does not have to pin a
source revision. Spawning keeps us upgradeable without a rebuild.

UI stack inside A is the builder's call; the current intent is **Svelte + Vite + CodeMirror 6**, plain CSS
(no Tailwind), dark-first with a light theme, and `@tauri-apps/api` for IPC.

## 7. Architecture

```
┌───────────────────────────── app window (webview) ─────────────────────────────┐
│ Tree · Editor (CodeMirror 6) · Overlay search · Right panel · Status line       │
└───────────────▲───────────────────────────────────────────────▲────────────────┘
                │ typed IPC commands / events                   │
┌───────────────┴───────────────────────────────────────────────┴────────────────┐
│ Rust core (Tauri)                                                              │
│  · VaultService   folder walk, file CRUD, external-change watch                │
│  · LinkIndex      in-memory [[wikilinks]] + [md](links) graph, per-file hashes │
│  · NpBridge       one child process, one stdin writer, one reader loop         │
│  · IngestQueue    debounce → scan → embed; consent gate; state store           │
│  · AppState       app data dir: vault registration, ingest stamps, settings    │
└───────┬──────────────────────────────────────────┬─────────────────────────────┘
        │ read/write .md, images                   │ stdin/stdout NDJSON (+ stderr drained)
   <vault folder on disk>                    np serve (child, one per session)
```

Rules that hold it together:

- **Rust owns all file and process I/O.** The webview never touches a path or a pipe; it asks for things.
  This is what makes a second front-end (CLI mode, another window) possible later.
- **Two np connections, not one** (§18.1 forced this): a **writer** for `scan`/`embed`, and a **reader** spawned lazily for `search`. Idle `np serve` is ~10 MB; the reader only pays the ~115 MB model map when a vector search actually runs. Never spawn-per-command, never more than one writer (two writers only contend).
- **Exactly one writer to the child's stdin** (a task fed by a channel), never two threads interleaving bytes.
- **stderr is drained and discarded** unless diagnostics are requested — an undrained pipe slows np on Windows.
- The app's own state lives in the OS data dir, keyed by vault path; nothing in the vault except `np`'s own `INDEX.md`.

## 8. First run

1. Window opens with one panel: where should notes live? Native folder picker, plus "create new folder".
2. Creating or picking a non-empty folder is fine — it just gets indexed. Picking inside an existing vault's subfolder is allowed and warned about only if the parent already looks like a vault (contains `INDEX.md` with np's fingerprint header).
3. The choice is stored in the app data dir; subsequent launches open straight into the vault.
4. Then it checks np: bundled resource → `NP_PATH` env override → `PATH`. Found: `initialize` with `notifications: true`, record `np` version, `data_dir`, `features`, `methods`. Not found: a screen that says exactly what is missing, the platform's install command for the release asset, and where to put it — never a silent retry loop.
5. Embedding consent (§11) is asked on first *embed*, not at install time.

## 9. The window

```
┌──────────────────────────────────────────────────────────────────────┐
│ ~/Notes                                    Search ⌘K       ● embedded │
├─────────────────────┬─────────────────────────────┬──────────────────┤
│ ▾ projects        ＋│ # Q3 review                 │ (pinned results) │
│   ▸ 2026            │ plain markdown, here        │                  │
│   q2-review.md ●    │ [[q2-rev┃]] → completion    │                  │
│ • timeline.md       │                             │                  │
│ ▾ inbox           ＋│                             │                  │
│ INDEX.md  (np)      │              saved 14:35    │                  │
├─────────────────────┴──────────────────────┴──────┴──────────────────┤
│ indexed 1.2 s ago · embed waiting on 2 notes          [retry] [embed]│
└──────────────────────────────────────────────────────────────────────┘
```

- **Left:** the tree, from our own folder walk — folders and `.md` files, images/attachments visible, dot-files and dot-folders hidden, `INDEX.md*` greyed with a "generated by np" tooltip. Per-file marker: clean · waiting to embed · never indexed · refused (named reason). Folders get `＋` on hover for new note / new folder; context menu for rename, move, delete.
- **Middle:** CodeMirror 6, markdown syntax tinting, line wrap on, no minimap, autosave indicator in the corner. The only persistent UI chrome in the app.
- **Right:** pinned search results (§10). Empty by default and takes no width until something is pinned.
- **Bottom / status line:** what the index is doing, in one line — `indexed 1.2 s ago`, `embedding 412/1890 chunks`, `index behind · retrying`, `keyword-only: model not installed` — plus a manual `[embed]` escape hatch.
- **Top-right:** `Search ⌘K` is a *button* that opens the overlay. There is exactly one search input in the app and it lives in the overlay.

## 10. Search

- Overlay on Ctrl+K (and clicking the button). Vault view is replaced, not dimmed; Esc returns to the editor with focus restored.
- Content search runs on the **reader connection**, so it answers while an embed is running. If the reader is busy, the overlay shows the fuzzy path/title results and one honest line — `content search is waiting for the embedding job to finish` — rather than a spinner over an empty list.
- One field. As you type, debounced ~150 ms: a **fuzzy pass over paths and titles from our own index** answers instantly for "which file was that", and an `np search` (mode `hybrid`) fills in below it for content. The two groups are labelled, never merged — different engines, different meanings of "score".
- Row rendering: path, then the snippet with np's `[brackets]` turned into highlights; on the right, evidence — `bm25 −3.1`, `cos .82`, or **`by meaning`** when `bm25` is null (a hit only the vector list held gets no highlight, because inventing one would be a lie).
- Keys: ↑↓ move, Enter opens, `p` pins into the right panel, Esc closes. Clicking a row is the same as Enter; clicking a pinned row opens it too.
- Opening scrolls to the first occurrence of a matched token (computed by the app from file text — np returns no line) and tints every occurrence for a few seconds.
- Empty result says which half answered: `no note holds all of these words · try fewer` (the AND behaviour of upstream #1), or `keyword-only — the embedding model is not installed  [set it up]` when `vectors: false`.

## 11. Ingest pipeline

**Trigger:** a save whose content changed. Manual save, autosave, and the save that happens when the app closes all take the same path; a write that leaves the bytes identical (Ctrl+S on an unmodified buffer) triggers nothing, because np would answer "nothing owed" and we would spend a round trip saying so. A newly created file is just another changed file.

**Sequence, per vault:**

1. Debounce **~1 s** after typing stops → `scan` (np's own hashes tell it what moved; our stamps are for display).
2. Update per-file stamps from the scan result: hash recorded as indexed; anything whose current hash differs is *waiting*.
3. **Embed that file's owed work immediately after the scan** — one changed file, one embed pass, no quiet window and no batching cleverness. The queue drains in save order; a `locked` or `offline` result re-queues with backoff rather than dropping the work.
4. `embed` progress lines drive the status line (`embedding 3/7 chunks`, `fetching 83 MB / 295 MiB`) and, when there is enough to embed, an estimate computed from **np's own counters on this machine** — never a duration baked into our strings.

Closing the app never blocks on embedding: it flushes what it can and the rest is owed at next launch, because the vault is files and nothing is lost.

**Consent gate, asked once per vault:** if `embed` would need the model (the artifact is absent — decided from `models.list`, not by trying), show: *"Semantic search and embedding need the local model: 295 MiB download to `<data_dir>`. [Download] [Not now]"*. "Not now" writes `[models] auto_fetch = false` in np's config — **the file, not `NP_NO_AUTO_FETCH`**, because env is frozen at spawn and the child is spawned before consent exists; np re-reads config per request (§18.6), so flipping it needs no respawn. Keyword search and indexing keep working fully, and `[set it up]` stays available on the status line.

**Scale of the first run (measured, §18.3):** a full embed moves ~1.9 chunks/s — 200 short notes took 106 s. A vault of a few thousand notes is a **~35-minute background job**, not a blink. It therefore gets its own progress line with byte/chunk counts from `np/progress`, survives a window close (the next launch resumes what is owed), and the consent prompt says roughly how long it will take on this vault rather than promising a moment.

**States, never errors:** `locked` → `index behind · retrying (n)` with backoff, no error dialog; `offline` → `offline — keyword search only`; a refusal (`-32000`) shows np's own message verbatim plus what the app did in response. `completed` with `locked: true` must not look like a failure.

**Manual ingest actions, kept because they were asked for:** on a note — "ingest this"; in the tree header — "ingest everything waiting". Both run the same queue rather than a separate path, so there is one definition of "ingested".

## 12. Settings the app writes

App data dir — `$XDG_DATA_HOME/jott`, or `~/.local/share/jott` when that is unset, on Linux; `%APPDATA%\jott\` on Windows. Each platform's own per-user **app-data** root: where an installed app belongs, what the bundler expects, and where the Tauri shell puts its own files.

> Amended 2026-11-11 while building M1. This line said `~/.config/jott/` on Linux. Everything in the table below is state rather than configuration — a registry of folders, ingest stamps, mirrored unsaved buffers, a trash fallback, logs — and one app should not be split across two roots when the platform already names one for it.

| File | Contents |
|---|---|
| `vaults.json` | registered vault paths, last opened, per-vault consent decision. Paths stored here are the only absolute paths the app keeps, and they are never shared between machines. |
| `state.sqlite` *(or JSON, builder's call below budget)* | per file: rel path, hash at last successful scan, hash at last successful embed, last state + reason, timestamps. This is what makes "waiting" answerable, since np reports aggregates only. |
| np config | the app writes `$XDG_CONFIG_HOME/note-processor/config.toml` **only** as a whole-file change with both keys together (`provider` **and** `model`), and never overwrites a file it did not write without showing the diff first. |

## 13. Editor and links

- **Completion on `[[`:** from our in-memory index of every note path and basename, case-insensitively like np; when two notes share a basename, the folder is part of the suggestion automatically — that is precisely the case where np refuses to guess. `[text](` and `![](` complete relative paths including images (np indexes PNG/JPEG).
- **Unresolved targets** get a subtle mark in the editor, with "create this note" on click, using our own link index (which carries line numbers, so the mark is exact). Not from np: `scan` reports unresolved *counts*, not places.
- **Ctrl+click follows** a resolvable link; on an ambiguous one it shows the candidates instead of picking.
- **Rename / move:** detected by us, reported to np as `moved` by content hash (vectors survive). Then:

```
moved  notes/q3-review.md → archive/2026/q2-review.md     (np: moved, vectors kept)
  5 links to update · 2 ambiguous ([[review.md]] matches 2 notes)
  [ Update all 5 ]   [ Review one by one ]   [ Skip ]
```

  Rewriting edits only real link targets, preserves `[[Note|alias]]` aliases and the bracket syntax, and never touches a note that merely mentions the filename in prose. Ambiguous matches are **never** guessed — they go on the one-by-one list or stay broken visibly.
- **External change:** file modified on disk while open → reload silently if the buffer is unmodified; otherwise keep the user's text and show `changed on disk` with reload/overwrite choices. Never auto-overwrite a dirty buffer.
- **Delete** moves to OS trash where available, else a `.trash` inside the app data dir, and says which it did.

## 14. Constraints carried forward for the later installer

Chosen now because they cost nothing today and are what packaging needs: np is resolved as *bundled resource → `NP_PATH` → PATH*; all app state sits in the OS data dir; no absolute path is written to any file that leaves this machine; every user-visible string is in one module; window size, theme and layout persist per vault. The Tauri bundler's MSI/NSIS and deb/AppImage targets are expected to be configuration, not surgery — the first packaging run is its own later task.

## 15. Verification stance

- **Spike first (throwaway, no tests, kept labeled as a spike):** one window that spawns `np serve`, sends `initialize` with notifications on, presses a button and moves a progress bar through a real `embed`. It answers the two risks this design cannot settle on paper: does WebView2 on Windows stream `np/progress` lines promptly rather than buffering them, and do CodeMirror decorations survive WebKitGTK on Linux. Both are cheap to discover in an afternoon and expensive to discover in week three.
- **Windows gets real testing early**, not at the end: np's own write-lock behaviour on Windows is unmeasured by its author, and our `locked` handling depends on it.
- Rust unit tests for the JSON-RPC framing (including a notification interleaved mid-response-stream, a `locked` result that must render as a state, and an oversized line), the link parser, and the stamp store. UI tests only where behaviour matters: debounce timing, consent gate, rename rewriting against fixture notes.
- Golden fixtures for link rewriting: alias preservation, ambiguous basenames, images, links inside code fences (which must **not** be rewritten).

## 16. Left to the builder

Anything not named here: exact crate choices (`tauri`, `tokio`, `notify`, `serde_json` are the obvious set), the store format under a few hundred KB, how the fuzzy path filter is implemented, font stack and colour tokens, window geometry defaults, whether pinned results cap at N. None of these change what gets built.

## 17. Known unknowns, tracked

| Unknown | What it would change |
|---|---|
| Upstream #2 (`links.list`) — will np expose the link graph? | Nothing now (we keep our own index regardless); it would let the backlinks pane read one source of truth instead of a second parser. |
| Upstream #1 — will keyword AND semantics change or get documented? | Only the wording of the empty-result message. |
| `last_scan` reaching a release | Then we drop our own scan stamp in favour of np's. |
| Behaviour of a long-lived `np serve` under many hours and thousands of embeds (RSS growth) | If it grows without bound, restart the child after N embeds — an app-side policy, no redesign. |
| macOS | No np asset exists; source build only. Out of scope until someone with a Mac wants it. |

## 19. The three questions, answered

1. **Long first embeds** — the 35-minute figure is treated as an artifact of this container (§18.3), so no
   special "long job" flow: after the one-time model consent, a changed save embeds and the status line
   reports progress from np's live counters.
2. **Where it runs** — **Windows is the primary target**, on the user's own laptop; this build box has no
   desktop session (§18.14), so every visual or platform-risk check is a hand-off, not something I can
   observe here. The loop is in §20.
3. **Name and visibility** — **Jott**, private repository now, MIT-licensed and public later by choice.

## 20. Ways of working (rules from the person this is for)

| Rule | In practice |
|---|---|
| **Jott is purely a frontend.** Nothing in `note-processor` gets edited by us — not even to fix an obvious bug. Found one: open an issue there, keep building. If it **blocks** progress, stop and say so; the backend owner fixes backend problems. | Upstream issues #1 and #2 are already filed and left exactly there: filed. |
| **Windows first.** | Windows paths are designed in, not retrofitted — byte-exact writes (§18.8), WebView2 streaming (§15), unsigned-build reality (§5). |
| **Test-driven and spec-driven**, with a device in the loop. | Features get a failing test first; when a build passes its features and needs a real Windows machine, I say so plainly and hand over what to install and which log to send back. Nothing is claimed as working on hardware I could not run it on. |
| **Private now, MIT later.** | No publishing step is implied by any of this work; when it happens it also settles the notice question for anything Jott ships beside itself. |

## 18. Verified against np 0.3.1 — answers to the builder check

Every line below was measured on this host or read out of np's source; where it contradicts an earlier
section, **this section is authoritative**. The raw protocol evidence is in
[`tests/fixtures/np-serve-transcript.ndjson`](../../../tests/fixtures/np-serve-transcript.ndjson)
(with [`tests/fixtures/README.md`](../../../tests/fixtures/README.md) explaining each line group).

1. **Concurrency.** One request at a time per connection, no cancellation: a search queued behind a
   full-vault embed waited **102.1 s**. From a second process, keyword search answered in **0.00 s**
   during that same embed and a `scan` committed in 60 ms — np does not hold the write lock across the
   whole embedding run. Hence: writer connection + lazy reader connection (§7), searches never queued
   behind embeds, and a `locked` result treated as brief contention with backoff rather than a fault.
2. **No per-note targeting.** `np scan` and `np embed` take a workspace and nothing else. So "ingest this
   note" is honestly *run the vault's owed work, which includes this note*, and the queue has one shape:
   aggregate job, per-file stamps for display only. `embed` reads the index, so `scan` always precedes it.
3. **Throughput — one anecdote, not a spec.** 200 synthetic notes of ~140 words = 200 chunks embedded in
   106 s on this container (≈1.9 chunks/s); a single edited note re-embeds in 0.6–2 s. Unreplicated, shared
   CPUs, uniform text: the extrapolation to "~35 minutes for 4 000 notes" is **not** a design input and the
   app must never repeat it to the user. np's own bar for the model choice is "a full first pass fits in one
   evening", with the same warning that one machine's tokens/sec proves nothing about another's. What the
   number does support: per-save embedding of one file costs about a second, which is what §11 builds on.
   Any duration shown in Jott is derived from `np/progress` counters at runtime.
4. **Protocol shapes.** Captured verbatim in the fixture: `initialize` with `notifications`, `scan`
   bookends, a keyword hit, an `embed` refused by configuration that is still a **result** ending
   `completed` (with a human-readable `hint` we can show as-is), `-32601`, two flavours of `-32602`, and
   `-32700` with `id: null`. A JSON-RPC error and a `phase:"refused"` notification carry the same code.
5. **Per-file honesty (§9) has two sources.** `scan.skipped` is paths only — no reason; reasons are
   diagnostics with rule tokens (`identity-taken`, `duplicate-rows`), so per-file "why" comes from np's
   `[log] file` JSONL, whose records carry `request` and `workspace`. The embed report **does** name
   per-file image refusals (`image_refusals: [(path, reason)]`), while over-budget text chunks are a
   count. When diagnostics are off, the marker reads *refused* with no reason rather than inventing one.
6. **Workspace identity is a request param**, never config: `initialize`/`scan`/`embed`/`search` each take
   `workspace`, so first run does not edit np's config to say which folder is open. Per-workspace paths are
   `<data_dir>/workspaces/<sha256(abs)[0..16]>.sqlite` and `.models/` (`np-core/src/config.rs:853`), which
   is how we check artifact presence before promising a download. Config is re-read **per request**
   (`load_config()` inside each handler), so consent changes need no respawn; env does not.
7. **Link grammar, exactly as np resolves it** (`np-core/src/links.rs`): the target is stripped of
   `|alias`, `#heading` and `^block`, trimmed, then placed by (1) exact `rel`, (2) case-insensitive `rel`
   when exactly one path owns that fold, (3) unique basename — indexed both with and without its extension.
   More than one candidate at any step **never resolves**, including two paths differing only in case,
   which path-qualifying does not rescue. Attachments recognised as linked-to-but-unparsed: `png jpg
   jpeg gif webp bmp pdf`; only PNG/JPEG are ever embedded, and not in released binaries
   (`features.vision = false`). Our resolver implements that ladder and no other, with np's own ambiguity
   cases as tests — otherwise we mark good links broken, which is §4's forbidden lie in a different coat.
8. **Moves are np's discovery, not ours to report.** Hashing is SHA-256 over **raw bytes**
   (`np-core/src/hash.rs`) and `scan` classifies `Moved { from }` itself. So the rename dialog may state
   "np: moved, vectors kept" only when that scan reported it for that path pair — otherwise it says
   nothing of the kind. Corollary for Windows: **write bytes exactly as read**; CRLF normalising every save
   would make the whole vault look changed and re-embed it forever.
9. **Locking.** Embed does not hold the write lock for its whole run (§18.1), so `locked` is brief and
   rare; one writer connection means we never contend with ourselves, a second app window on the same
   vault gets `locked` and retries, and a terminal `np scan` the user runs by hand is expected behaviour,
   not an error to fight.
10. **Consent mechanics.** `[models] auto_fetch` (config file) is the switch that survives a lazily
    spawned child; `NP_NO_AUTO_FETCH=1` stays as the belt for a run we did not configure. `[Download]`
    means write the key and re-issue `embed`. Figures need a second, larger artifact only in a `vision`
    build, which the release binaries are not — so "295 MiB" is true today, and §8's handshake reads
    `features.vision` before ever promising image search.
11. **Snippets are not parseable.** np marks matches with `[` `]` and does **not** escape literal brackets,
    while markdown notes are full of `- [ ]`. So the snippet renders as text and we highlight our own token
    matches; np's brackets stay visible as source characters. Empty-result provenance needs no inference:
    the result document carries `mode`, `vectors`, `offline`, `reason`, and `limit` caps the list (no
    pagination exists).
12. **Version policy** (builder's call, recorded): develop against released **0.3.1**, feature-detect from
    `initialize`'s `methods`/`features`, and degrade — no `progress` ⇒ status line says only "working…";
    no `models.list` ⇒ the consent gate falls back to the artifact path check from §18.6; `last_scan`
    (repo `main` only) is preferred when present and never required.
13. **Crash safety** (builder's call): autosave at ~2 s idle plus on blur and close, and the last unsaved
    buffer for each open file mirrored in the app data dir, replayed at next launch if it is newer than the
    file on disk. No full journal — that is the cheapest thing that satisfies "never lose typed text".
14. **Where the GUI is actually run** (open, §19): this development box has no display server
    (`XDG_SESSION_TYPE=tty`), so Tauri windows can be *built* here but never *seen*. Visual verification,
    and the two spike risks in §15, belong to a machine with a desktop.
