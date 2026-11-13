# Diagnostics drawer — design

Approved by the person who asked for it on 2026-11-13, in the shape sketched to them: a drawer in the main
window, not a second OS window.

## Intent and why

Today, "what is the backend doing?" means opening `%APPDATA%\jott\logs\jott.jsonl` in another program while the
thing you are asking about is happening on screen. Every np request is already recorded there, the status line
already speaks np's states, and About already copies a diagnostics block — so this is not new information, it is
the same information where the person is looking. It exists because debugging a tester's machine over chat costs
both of us an hour per round trip.

## Goals

- See, live, every request Jott made to np: method, elapsed, outcome, and np's own sentence on a fault.
- See the run's shape without opening a file: which np binary, protocol and features, which connections are
  alive, what the queue is holding, how the notes divide by stamp state, where the log is and how big it is.
- Copy all of it in one press, the way About does.
- Keep the rule that made the journal usable: **no note contents**, enforced by a test on the key set rather than
  by care.

## Non-goals

- No second Tauri window (its own webview, capability and shell code cannot be tested from this box).
- No log viewer for rotated files; the drawer shows the run, and points at the file for the runs before it.
- No new np calls. Everything here is what Jott already knows or already asked.

## Anti-goals

- A screen that leaks someone's note text into a screenshot sent to me.
- A panel that costs the app speed: the trace is a fixed-size ring, and progress lines are coalesced rather than
  forwarded one per np notification (a fetch emits one per chunk of bytes).

## Constraints

- The journal's privacy rule (§11/Task 11) extends to this screen: closed key set, pinned by tests.
- np's *answers* stay out of the drawer by default — a `search` response carries note excerpts. Showing raw
  answers is a later, explicit, opt-in feature; not in this design.
- Frontend logic that can be tested headlessly lives in pure TS modules with Vitest tests, not in a component.
- The command surface lives in `jott-core` (`crates/jott-core/src/ipc.rs`), because this box cannot compile the
  Tauri shell; the shell stays thin and is verified by Windows-target type-checking.

## Approaches considered

- **Drawer in the main window** *(chosen)* — no new window plumbing, testable with the existing jsdom smoke test,
  stays open beside the note it describes.
- **Second OS window** — nicer for a second monitor; costs untestable shell surface and a capability entry, and
  the drawer can be widened later without changing what is in it.
- **Devtools only** (`Ctrl+Shift+I`, already shipped) — free, but it shows console noise and JS internals, not
  "np said the workspace is locked for 40 s".

## Decided shape

```
┌ DIAGNOSTICS ─────────────────────────── np 0.3.1 · bundled ┐
│ runtime   np: D:\Jott\np\np.exe · protocol 1 · writer up,  │
│           reader idle · log 412 KB · consent: undecided    │
│ activity  19:04:22 embed   applied  1.9 s  chunks 7/7      │
│           19:04:20 scan    applied   41 ms added 3         │
│           19:03:58 search  fault -32000 workspace locked   │
│ queue     2 waiting · 0 held for consent · retry in 5 s    │
│ vault     148 notes: 141 embedded · 6 indexed · 1 refused  │
└───────────────────────── [copy everything] [open log] ────┘
```

- Core owns a `Trace`: a fixed-size ring of events plus a broadcast so the window can follow them live. The same
  call sites that write journal lines push here, and progress lines arrive coalesced (per method: on phase
  change, or once a second).
- `VaultSession::diagnostics()` answers a snapshot — runtime facts, recent trace, queue state, vault counts — so
  opening the drawer mid-run is not a blank pane.
- The shell forwards `diagnostics://event` and exposes the command; it decides nothing.

## Left to the builder

Exact wording of rows beyond the values themselves; how the sections are laid out in CSS; whether the shortcut
also closes the drawer (it does).
