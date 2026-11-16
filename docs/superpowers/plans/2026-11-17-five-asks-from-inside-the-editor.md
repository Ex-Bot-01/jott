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
7. **Draw before colouring.** CodeMirror: `.cm-cursor { display: none }` until `.cm-focused`, and
   `.cm-content { caret-color: transparent !important }`. So the window records focus itself (`focusin`/`focusout` with
   `relatedTarget`) and forces the layer to draw; the native caret takes over only under `:not(.cm-focused)`, so exactly
   one caret is ever on screen. `min-height: 1em` catches a zero-height measurement.
8. **`--cm-cursor` does not exist.** A test reads `@codemirror/view/dist/index.js` and fails if it ever mentions that
   variable, so the 0.0.16 claim cannot return. Two releases of caret work coloured a variable nobody read; the comment
   that asserted it sent the next reader past the rule that was hiding the caret.

## Where the harness was more generous than reality

Two `vi.waitFor` waits were written `expect(el).not.toBeNull()` on `document.querySelector(...)?.closest('button')`.
Optional chaining yields `undefined`; `undefined` is not `null`; the wait succeeded on its first tick and awaited nothing,
then handed back `undefined` as a "found" element. Both now assert truthiness. A wait that cannot fail is not a wait.

## Gate record

306 Rust tests (np off `PATH`), clippy `-D warnings`, 367 front-end tests by exit code, `npm run build`, svelte-check
0/0, Windows-target `cargo check` printing `Checking jott-shell v0.0.18`. CI floor raised 295 → 300. Each change was red
before it was green, and the dot's rules were mutation-checked afterwards (answering `unknown` as `off`, pinning every
configured folder to `active`, deleting the `!important` caret fallback).

**Unfinished by construction:** this box has no screen. The caret work removes both known ways to lose it; whether that is
the one on the tester's display is answered by the devtools line in `docs/testing-on-windows.md` §2b-viii, not by me.
