# Five asks from inside the editor (v0.0.18)

Asked on 2026-11-17 by the Windows tester, after using 0.0.17 with real notes rather than a first run. Recorded here as
a plan because every previous release has one, and because two of these had a wrong premise worth writing down before
someone repeats it. Executed natively in this session (no per-task subagents), TDD throughout, one fresh whole-branch
review at the end.

## What was asked, and what turned out to be true

| ask | what the code actually said | shipped as |
| --- | --- | --- |
| move "match word beginnings" into settings | it was in the toolbar *and* already in Settings — two doors to one room | toolbar copy removed; the empty-result one-off offer stays, and still does not write the preference (`e2a8bb4`) |
| a button to close a file from the editor | one existed since 0.0.16 — **in the sidebar toolbar**, not near the note | note pane gets a header: file name, `read-only`, **close** (`e2a8bb4`) |
| similarity scores beside results | core has carried `score`/`bm25`/`cosine` since 0.0.13 and displayed none of them | np's fused rank printed at 3 significant figures + `meaning 0.82` where `cosine` exists (`07115ca`) |
| a green dot when the model is active/downloaded/online | three states, measured in three places, plus a fourth nobody asked about: nobody answered | five states + word + click → Settings; new core `embeddings_status` (`be2c7c3`) |
| the cursor still looks like no cursor | my 0.0.16 fix coloured a CSS variable **that CodeMirror does not have** | focus tracked by the window; caret drawn on that; native caret as the unfocused fallback (`139540d`) |

## Measurements taken first (np 0.5.1, JSON-RPC over stdio)

```json
{"path":"b.md","score":0.01639344262295082,"bm25":-0.5108266237659908,
 "cosine":null,"keyword_rank":1,"semantic_rank":null,"matched_exactly":true}
```

- `score` is reciprocal-rank fusion, not similarity. Displaying it as a percentage would be my arithmetic. Three
  significant digits: two decimals print every keyword hit as `0.02`; rounding `0.000123` to `0` says "not at all" about
  a note that matched.
- `cosine` is `null` whenever vectors did not run (measured on a keyword-only folder). So the meaning figure appears only
  where np sent one; `meaning 0` would describe a measurement nobody took.
- `models.list` with `[embeddings] provider/model` set and nothing fetched:
  `pinned: [{ id: "embeddinggemma-2", bytes_total: 309855456 }]`, `models: []`. A row appears under `models` once the
  weights are recorded. So *configured*, *downloaded* and *used by this folder* are three questions with three sources —
  the third from Jott's own per-file markers, not from np.
- `models.list` called with `{}` is refused `-32602`; it wants `{"workspace": ...}`, which is what core already sends.

## Rulings

1. **No invented similarity.** Print np's number and say in the tooltip that it is a rank, not a percentage.
2. **Absence is a state, not "off".** `present: null` (np unreachable, or models directory unlistable) draws a hollow
   bead reading *note-processor did not answer*. `artifact_present()` answers "no" for an unreadable directory because a
   consent gate must fail closed; a light does not inherit that caution as a fact.
3. **A provider Jott does not know is its own state.** `"on this disk"` is a story about `local-model`; np lists no rows
   for somebody else's endpoint, which would otherwise read as a download that never arrives.
4. **The dot reads; it never writes.** An answer still has exactly one writer (v0.0.15's ruling).
5. **Once per opened folder, over the reader connection.** Unlike the consent offer, which stays lazy, a light must be
   able to say "on" as well as "not bought yet" — and on the writer it would queue behind an embed and sit amber through
   the whole download.
6. **A colour carries nothing alone.** Every state ships a word (`meanings on`, `model ready`, `model not downloaded`,
   `embeddings off`, `model state unknown`, `another provider`) in the aria-label, plus a forced-colours border.
7. **The caret belongs to the browser here, and this window was hiding it.** `drawSelection()` — the only extension that
   creates `.cm-cursor` elements — is not installed, so every rule written in two releases (the ink `border-left`, the
   blink keyframes, `--cm-cursor`, and a fallback added earlier in this branch) selected elements that do not exist. The
   real cause was ours: `Editor.svelte` set `'.cm-content': { caretColor: 'transparent' }` since c6665c7 (0.0.16), under a
   comment asserting CodeMirror hides the native caret itself — it does not, `hideNativeSelection` ships inside
   `drawSelection()` too. So: name the caret in ink in the theme (the only layer that can decide it, since CM injects
   after the bundle), delete the dead rules rather than keep them "in case", give forced-colours mode `CanvasText`, and pin
   the invariant that would have caught this in 0.0.16 — *styling `.cm-cursor` requires installing `drawSelection()`*, both
   halves checked so neither can arrive alone.
8. **A claim about a dependency gets checked against the dependency.** `--cm-cursor` does not exist in CodeMirror; a test
   now reads `@codemirror/view/dist/index.js` and fails if it ever mentions that variable. The lesson is not the variable,
   it is the comment: an unverified assertion about internal machinery sent two releases past the line that was hiding the
   caret, and the pins written to "prove" the fix were regexes on source text pointing at elements never created — they
   stayed green while the bug shipped. Where jsdom cannot compute styles (it reports `rgb(0, 0, 0)` for `.cm-content` no
   matter what is injected), a source pin is a claim about what somebody wrote, not what a user sees, and it must be sold
   as that.

## Where the harness was more generous than reality

Two `vi.waitFor` waits were written `expect(el).not.toBeNull()` on `document.querySelector(...)?.closest('button')`.
Optional chaining yields `undefined`; `undefined` is not `null`; the wait succeeded on its first tick and awaited nothing,
then handed back `undefined` as a "found" element. Both now assert truthiness. A wait that cannot fail is not a wait.

## What the whole-branch review found

Blocked on Ruling 7: the caret claim was false as shipped. Also accepted and fixed:

- **An unreadable np settings file drawn as "embeddings off"** — the exact flattening `SettingsGap::ConfigUnreadable`
  refuses on the consent card. `configUnreadable` now travels, the dot says the file could not be read, and no `models.list`
  is asked for a file nobody can parse.
- **Four causes of `unknown`, one sentence.** np answers an unlistable models directory with a *result* carrying
  `models_dir_error`; saying "did not answer" about a process that answered — and named what it could not do — is wrong in
  an actionable way. `modelsDirError` now quotes np's own sentence.
- **A price attached to the wrong model**: `price_of(id).or_else(only_price)` can only produce a number when the named
  model has no pin and exactly one *other* model does. The fallback is gone; a named-but-unpinned model gets `null`.
- **A late `models.list` answer overwriting the folder on screen** (A configured, slow because its child must start; B
  unconfigured, answers at once). Guarded by capturing the folder before the await, the same shape as search's guard since
  0.0.12; and `modelFacts` returns nothing when no folder is open, so a closed or trashed vault stops being described by
  old facts.
- **Cost of the dot disclosed rather than glossed**: it does wake np's second child at folder-open, a deliberate departure
  from 0.0.12's lazy-connections ruling, now pinned (one ask per folder; none before a folder is open; none per keystroke;
  exactly one extra after a pass finishes) and stated in the release notes.
- **Stub honesty**: `embeddings_status` answered with no session while core refuses — so deleting App's guard left a
  first-run window showing "embeddings off" with the suite green. Fixed, plus the missing `beforeEach` reset and a type
  that accepted `number` where core sends a boolean.
- **Fixtures that could not occur**: scores of `1.1`/`1.4` never exercised the rounding the module exists for; they now
  hold measured values. And "not a percentage" lived only in a `title`, so keyboard and screen-reader users got a bare
  number — `rowLabel` puts the explanation in the row's accessible name.

**The two theatre tests, because this is the reusable part.** The race test passed with its guard deleted, twice: first
because both answers were delayed equally (so the "late" one was still the earlier one), then because the stub read its
switch *inside* the timer, so the delayed reply carried the new folder's facts and there was no stale answer to drop. A
race test is only evidence once it has been shown red without the guard — and a fake that reads its scenario when it
answers rather than when it is asked silently defangs every timing test written with it.

## Gate record

Before the review: 306 Rust / 367 front-end. After remediation: **309 Rust** tests (np off `PATH`) exit 0, clippy
`-D warnings` exit 0, **373 front-end** tests exit 0 (one deleted with its subject — the focus-tracking class the caret
finding made pointless), `npm run build` exit 0, svelte-check 0/0, Windows-target `cargo check` printing
`Checking jott-shell v0.0.18`. CI floor 295 → 300. Every change was red before it was green, and mutation-checked after:
answering `unknown` as `off`, pinning configured folders to `active`, deleting the stale-answer guard, deleting the
no-vault guard, restoring `only_price()`, and restoring `caretColor: 'transparent'` each turn their own tests red.

**Unfinished by construction:** this box has no screen. The caret cause found in review is ours and is removed; whether
anything else stands between the tester and a visible caret is answered by the devtools line in
`docs/testing-on-windows.md` §2b-viii check 6, not by me.
