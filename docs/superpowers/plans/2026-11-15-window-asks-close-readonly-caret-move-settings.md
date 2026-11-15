# Plan: five window asks (close, catalogue read-only, caret, move-into-new-folder, settings)

Spec: `docs/superpowers/specs/2026-11-15-window-asks-close-readonly-caret-move-settings-design.md`

Executed natively in this session, one item at a time, each with its own commit and its own evidence. TDD
throughout: failing test first, then the change, then green — and a count quoted, never "the suite is green".

Gates after every item that touches Rust: `env PATH=/home/agent/.cargo/bin:/usr/bin:/bin cargo test -p jott-core
--no-fail-fast` (np off PATH), `cargo clippy -p jott-core --all-targets -- -D warnings`. After every item that
touches the front end: `npx vitest run` judged on **exit code** plus its count, `npm run build`,
`npx svelte-check --tsconfig ./tsconfig.app.json`. Windows-target check before release.

Order is by "smallest thing that stops hurting" so the tester can try each one as it lands.

## Item 1 — the caret, and telling CodeMirror it is dark

**Red**
- `src/lib/darkMode.test.ts`: `schemeOf(true) === 'dark'`; `watchScheme` calls back on a change through a fake
  `MediaQueryList`, and the returned unsubscribe stops the calls; no `matchMedia` at all → `'light'`, and
  `watchScheme` still returns a function it is safe to call.
- Smoke: with a dark media stub, the mounted editor's `.cm-editor` carries `cm-dark`; with a light stub it does
  not. (This is the observable half of "CM knows it is dark" — jsdom cannot show me colour.)
- `src/palette.test.ts`: `Editor.svelte` declares the caret inside an `EditorView.theme(…, { dark })` block using
  `var(--ink)`, keeps a `forced-colors` rule with `CanvasText`, and overrides the blink keyframes so the caret
  dims rather than switching off; `prefers-reduced-motion` stops the animation.

**Green**
- `src/lib/darkMode.ts` as specced (no `window` at import time).
- `Editor.svelte`: a compartment holding `jottTheme(scheme)` + `EditorView.darkTheme.of(dark)`; an effect swaps it
  when the scheme changes (never rebuilds the buffer); CSS keeps palette-derived rules, drops the specificity-race
  caret rule in favour of the theme, adds forced-colours and reduced-motion.
- Smoke harness grows a `matchMedia` stub — every command and browser API the window uses needs a stub, or Vitest
  reports passing tests and a non-zero exit.

## Item 2 — close the open note

**Red** — smoke: mounting with a note selected, clicking `close` empties the editor area and clears the selection
highlight; pressing `Escape` with focus on the body does the same; typing in the search box and pressing `Escape`
does **not**; `+ note`/tree click still select as before. A test that closing writes nothing: no `save_note` in
the command list, and a dirty buffer's mirror stays put ( reopening shows the unsaved notice ).

**Green** — one `closeNote()` in `App.svelte` clearing `selected`/`note`/dirty + focus back to the tree; a
`close` button next to rename/delete, `disabled={selected === null}`; `Escape` in the existing keydown handler
behind the same text-target stand-down rule the other shortcuts use.

## Item 3 — read `INDEX.md`, refuse to change it

**Red (Rust first)** — `vault_files.rs`: reading `INDEX.md` now returns bytes plus a "generated" verdict instead of
an error; saving it still fails with `NpGenerated`; a note named `INDEX.md` **inside a subfolder** is an ordinary
note on both counts. `ipc_surface.rs`: `NoteContent` gains `readonly`, true for the catalogue, false for a note —
key pinned in JSON. `src/lib/ipc.test.ts`: drift guard covers the new key.

**Red (front end)** — smoke: clicking the greyed `INDEX.md` row shows its text in the editor, the header says it is
note-processor's index, Ctrl+S sends no `save_note` and prints the read-only sentence; a normal note is unchanged.

**Green** — `read_note` answers with `readonly`; DTO + `ipc.ts` carry it; `Editor.svelte` gains `readonly`
(`EditorView.editable.of(false)`, save path refuses, notice in the header); App passes it through and keeps np's
sentence for a refused write.

## Item 4 — move into a folder that is not there yet

**Red** — `tree.ts` unit tests for a pure `newFolderMove(currentRel, name)` / validation helper: blank name refused,
a name that already exists as a folder resolves to the existing target rather than a second create, a path-escaping
name (`../x`) refused, a nested name (`a/b`) accepted as np/Jott would treat it. Smoke: with no folders in the
vault, the move dialog offers a name field and *create and move*; pressing it calls `create_folder` then
`move_entry` with the right pair and refreshes once; a failing create keeps the dialog open and shows the sentence.

**Green** — dialog gains the second control; both commands already exist in core, so this is front-end wiring plus
the pure helper.

## Item 5 — settings: how search matches, word beginnings, embeddings

First file the note-processor issue: `mode:"semantic"` with no vectors answers an empty list and omits `indexed`,
`keyword_used`, `semantic_used`, `reason` and `note`, which leaves any client to guess between "nothing matched"
and "nothing to match against". Jott's guard references that issue number in its sentence.

**Red (Rust)** — `ipc_surface.rs`: `search_vault` takes `mode` (`keyword|hybrid|semantic`), refuses an unknown label
naming what it accepted, and the recorded wire params show `"mode"` on every request; `SearchResults` unchanged.
`np_methods.rs`: reads the fake np's recorded params to prove `semantic` crossed the wire.

**Red (front end)** — `searchSettings.ts` unit tests: load/save of `{mode, prefix}` with corrupt/absent/hostile
storage falling back to defaults (`hybrid`, off); `meaningsOnlyAvailable(facts)` true only when np says this folder
has vectors; `emptySemanticSentence(...)` never says "nothing matched". Smoke: the panel opens from the menu and
`Ctrl+,`; choosing *words only* sends `mode:"keyword"` on the next search and is remembered across remount;
*meanings only* is disabled with its reason in a folder without vectors and does not become the saved preference;
the embeddings row calls `setConsent`, quotes np's answer, and no other command writes np's settings.

**Green** — mode threaded `ipc.ts` → shell command → `VaultSession::search` → `NpPool::search` →
`methods::search`; new `src/lib/searchSettings.ts` (replacing the single-key `prefixSearch.ts`, which keeps working
through a migration read of `jott.search.prefix`); new `Settings.svelte` with the three controls, reusing
`consentHelp.ts` wording for the embeddings section; results header gains the meanings-only sentence.

## Then, as usual

Docs (`docs/testing-on-windows.md` §2b-vi, `docs/release-notes.md` for 0.0.16), version bump in
`apps/shell/Cargo.toml` + `tauri.conf.json` + `Cargo.lock`, CI test floor raised to the measured count, whole-branch
review by a fresh reviewer, remediation, merge, tag, release, asset verification.
