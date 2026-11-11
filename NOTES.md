# Project notes — decisions and ground rules

Running log while we design the note-processor frontend. Everything here is either **said** (by me, my
human partner) or **proposed** (by the assistant, with a reason, correctable). Design doc comes later;
this file is the raw record so nothing gets quietly invented later.

## Ground rules

| | |
|---|---|
| **Bugs** | Found a bug → open a GitHub issue. np's own bugs go to `Ex-Bot-01/note-processor`; frontend bugs go to the new frontend repo once it exists. Repro steps + observed vs expected, in the same blunt style the backend repo uses. |
| **Audience** | Single-user personal tool now. No installer, no onboarding polish — but architecture must not block packaging later. |
| **Packaging-ready from day one (proposed)** | np binary resolved as *bundled resource → `NP_PATH` → PATH*; app state in the OS data dir; nothing written outside the vault + that data dir; no absolute paths persisted in shared files. |

## What it is

A desktop markdown note editor for a folder on disk, with `np` (note-processor) as its index and search
backend. Windows and Linux first; macOS later (np has no macOS asset — source build only).

## Decisions said

1. First run asks where the vault lives: pick or create a folder, remember it.
2. Left side: tree of notes. Middle: editor, "like a coding editor, slim modern simple".
3. Ingest exists in two actions: this note, and everything not yet ingested.
4. Search is included and uses the backend.
5. Quiet auto-index on save; embeddings also automatic — every save re-embeds. Saving happens on user
   action and when the app closes.
6. Link support while typing (autocomplete) — option 2. Backlinks pane later.
7. Search = **Ctrl+K overlay**, results with snippet + evidence, keyboard-first; a result can be pinned
   into a right-hand panel that holds it beside the open note.
8. Right-hand side: index/ingest state, and pinned search results.

## Proposed (mine, correctable)

- **Ingest timing.** Debounce ~1 s after typing stops → `scan`; embed shortly after, so a burst of ten
  saves embeds once. One long-lived `np serve` for the whole app session (measured: embed is 0.6 s only
  while the model stays loaded).
- **One consent prompt, ever.** The first time embedding needs the 295 MiB model, ask; afterwards stay
  silent. `locked` and `offline` render as states ("index behind — retry"), never as errors, because np
  reports them as data.
- **Ingest state per file** comes from the app's own record of each file's hash at last successful scan
  (np answers in aggregates: `added 3, unchanged 12`), stored in the app data dir keyed by vault path —
  so the vault itself stays clean apart from np's own `INDEX.md`.
- **Autocomplete offers both shapes**: bare note name, and folder-qualified path; shows the folder
  automatically when a basename is ambiguous, because that is exactly when np refuses to guess.

## Measured on this machine (see INSTALL.md §5)

`scan` 12–31 ms · `embed` one edited note 0.6 s · first `np setup` 13 s incl. 295 MiB download ·
`np serve` ~115 MB RSS during vector work · artifacts are **per vault** (second vault = second copy).

## Filed on `Ex-Bot-01/note-processor`

| # | Title | Why |
|---|---|---|
| [1](https://github.com/Ex-Bot-01/note-processor/issues/1) | docs: keyword search demands every word of the query | Verified in code (`match_expression`, AND-joined quoted phrases) — **deliberate behaviour, undocumented to users**. Design consequence for us: the overlay must show which half answered (`vectors`, `reason`) so a weak answer explains itself. |
| [2](https://github.com/Ex-Bot-01/note-processor/issues/2) | feat(serve): nothing answers "who links here" | The `links` table already holds `(src, dst, kind, target, line)`; no method exposes it. Until it does, **the app keeps its own in-memory link index** from the folder walk — it also feeds autocomplete and rename rewriting, so we need it regardless. |

## Decisions said (round 2)

9. Rename/move: ask each time, with **update all** or **review one by one**; ambiguous basename matches
   never get guessed, they go on the one-by-one list.
10. The app keeps its own link graph while walking files — agreed, np's is not queryable (issue 2).
11. No notes exist yet. Notes will live inside **the workspace folder** itself, no separate store.

## Proposed (round 2, mine)

- External edit: reload silently if the buffer is untouched; otherwise keep my text and show "changed on disk".
- np's generated `INDEX.md` shows greyed-out as generated; `.git`/`.obsidian`/dot-folders stay out of the tree.
- Images and attachments appear in the tree (np indexes PNG/JPEG), non-text files open outside the editor.

12. **One search, one box.** The status-line "⌘K" is a button that opens the overlay, not a second input.
13. Matched keywords highlight in the result row **and** in the opened note; Enter or a click opens the
    hit in the editor. Semantic-only hits (cosine, `bm25: null`) get no highlight and say "by meaning" —
    absence of a mark is information, not a missing feature.
14. Scroll-to-match is computed by the app on open: an np search hit carries a snippet, **not a line**,
    so we locate the first matched token in the file text ourselves.

## Stack and design doc

15. **Stack picked: A — Tauri v2** (Rust core + system webview, CodeMirror 6 UI). Electron rejected on
    weight, native toolkits rejected because the editor would have to be hand-built, HTTP shim rejected
    because it re-adds the port/daemon np removes on purpose. Considered and rejected: linking `np-core`
    as a library — we spawn `np serve` so users can upgrade np without rebuilding us.
16. Design doc written: `docs/superpowers/specs/2026-11-10-np-note-editor-design.md`. Builder check
    dispatched before it goes to my human partner for approval.

## Builder check — what came back, and how it was settled

A fresh reviewer read the design doc and returned 10 ranked questions plus 5 smaller ones. Only three
needed my human partner (design doc §19); the rest were backend facts, so I measured or read them rather
than guessing. New facts, evidence in `docs/superpowers/specs/2026-11-10-np-note-editor-design.md` §18:

- Search queued behind a full-vault embed waited **102.1 s** (no cancellation). From a second process:
  keyword search **0.00 s**, and a `scan` committed mid-embed → design now uses a **writer connection plus
  a lazy reader connection**; searches never queue behind embeddings.
- `scan`/`embed` take **only a workspace** — no per-note target, so "ingest this note" means "run the
  vault's owed work, which includes this note".
- Embed throughput ≈ **1.9 chunks/s** (200 notes in 106 s) → first embed of ~4 000 notes ≈ **35 minutes**.
  Per-save re-embed of one note stays 0.6–2 s.
- Captured a verbatim protocol fixture: `tests/fixtures/np-serve-transcript.ndjson`, its README reading
  each line group, and `scripts/capture-fixture.py` which regenerates it from scratch.
- Per-file refusal reasons live on np's diagnostics channel (`[log] file`), not in the scan report; image
  refusals *are* per-file in the embed report. Snippets do not escape `[ ]`, so highlights come from our
  own token matches, never from parsing the snippet.
- Workspace is a per-request param (never config); paths are `<data_dir>/workspaces/<sha256(abs)[0..16]>`;
  config re-read per request ⇒ consent changes need no respawn, and the switch is `[models] auto_fetch`.
- Link resolution ladder recorded exactly; hash is SHA-256 over **raw bytes** → on Windows we write bytes
  as read, or CRLF normalisation makes the whole vault look changed forever.
- This dev box has **no display server** (`XDG_SESSION_TYPE=tty`): Tauri windows build here but cannot be
  seen here. Visual verification and the spike's two risks belong to a machine with a desktop.

## Ways of working (said — these are rules, not suggestions)

| Rule | What it means in practice |
|---|---|
| **Jott is purely a frontend.** | Never edit `note-processor`. Bug found → file an issue there → keep going. If it **blocks** us, stop and tell them; they fix backend issues themselves. |
| **Windows is the primary target.** | Windows behaviour is designed for first, not retrofitted. Linux stays supported (it is where the code gets built). |
| **Device-in-the-loop checkpoints.** | Test-driven and spec-driven work here; whenever a build passes its features and needs a real machine, **tell them** — they install it on their Windows laptop and send back logs. |
| **Repo: `Jott`, private now, MIT later.** | Name from "Jotting". Nothing published yet; license and visibility change later as a deliberate step. |

## Embed policy (said, replacing my debounce proposal)

After a save — by the user or automatic — **if the file's content changed**, embed it. Same for a new file
that gets saved. No long quiet window, no "batch of twelve saves embeds once" cleverness: one changed
file, one embed pass. `scan` still precedes `embed`, because embedding reads the index rather than the vault.

## On my 35-minute figure (their correction)

They doubt it — "this model runs on a phone". Fair challenge, and my own evidence is weaker than I made it
look: **one unreplicated run**, in a container on shared CPUs, over synthetic uniform notes. np's own
design note sets its bar at "a full first pass fits in one evening" for ~2k real notes and warns that a
single machine's tokens/sec "is an anecdote". So the number is not dropped into the design as a promise.
What replaces it: Jott derives any time estimate from **np's live progress counters** (`chunks_done`,
`chunks_total`, `since_ms`) on the machine actually running, and never states a duration we have not
measured there.
