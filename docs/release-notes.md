# Jott 0.0.18 — five asks from inside the editor

The reports this time did not come from the first five minutes. They came from someone who had notes open, a folder
indexed, and opinions — which is a different kind of feedback: *move that checkbox in there*, *why is there no cursor*,
*show me how close each match was*, *put a dot on the bar when the model is up*, *there is still no button to close this
file*. The backend inside is still **note-processor 0.5.1** on protocol 1; every change here is in the window, and one of
them needed nothing but reading a dependency.

## What you'll notice

- **The word-beginnings choice moved into Settings.** It was a checkbox welded to the search box, and it does not belong
  there: it changes what every query *means*, and it sat next to the place where you are composing one question. The
  settings panel already had it — the toolbar copy was a second door to one room. What stays beside the box is the
  one-off offer under an empty result, *try matching word beginnings*, because that one arrives exactly when widening is
  the thing that could change the answer, and it still does not touch your remembered choice.
- **The note pane can close itself.** "There is still no button to close a file from the editor" was true from where you
  were sitting: since 0.0.16 the button existed, in the *sidebar* toolbar between delete and the tree's other actions —
  nowhere near the page of text you were reading. The pane now names the note it is showing, marks np's own catalogue as
  `read-only`, and offers **close** at its right edge. Escape still works; with nothing open there is no button, rather
  than a greyed one to wonder about.
- **Every result carries the number that ranked it.** Measured against 0.5.1, a hit arrives as
  `{"score": 0.01639344262295082, "bm25": -0.51, "cosine": null, "keyword_rank": 1, "semantic_rank": null}`. `score` is a
  reciprocal-rank fusion of the keyword and meaning rankings — not a similarity, not a percentage. The row prints it as
  three significant figures (`0.0164`) and its tooltip says so: *"…Higher means nearer the top of its list; it is not a
  percentage."* Turning it into "87% similar" was the obvious move and would have been my arithmetic rather than np's
  measurement. `cosine` *is* a real similarity, and it is `null` whenever vectors did not run, so `meaning 0.82` appears
  only on rows where np actually compared meanings — never `meaning 0` on a keyword hit, which would describe a
  measurement nobody took. Nothing new crossed the wire: core has carried these three fields since 0.0.13 and spent none
  of them.
- **A status light that knows five things apart.** "Green when the model is active/downloaded/online" is three states in
  one sentence, measured in three places: np's `embeddings.provider` for *configured*, np's `models.list` for *the bytes
  are on this disk*, and Jott's own per-file markers for *this folder has vectors*. So there are five — plus the one that
  keeps happening in the field, **nobody answered**: np unreachable or its models directory unlistable is drawn as a
  hollow bead reading "note-processor did not answer", never as grey, because telling somebody with 296 MiB of weights on
  their disk that their embeddings are switched off is how they go and download them again. A provider Jott does not
  recognise (`openai`, somebody's own server) gets its own state: "on this disk" is a story about `local-model`, and np
  lists no rows for an endpoint, which would otherwise read as a download that never arrives. Each state carries a word —
  `meanings on`, `model ready`, `model not downloaded`, `embeddings off`, `model state unknown` — because a bead alone
  says nothing to a screen reader, a colour-blind tester, or a screenshot in a bug report. Clicking it opens Settings,
  the one place the state can be changed. It is asked once per opened folder over np's *reader* connection, so it cannot
  queue behind an embed in progress: when a download finishes, that is the call which learns about it.
- **The caret had to be drawn before it could be the right colour.** Third attempt, and the first that read
  CodeMirror's own stylesheet instead of guessing at it:

  ```css
  ".cm-cursor": { display: "none" }
  "&.cm-focused > .cm-scroller > .cm-cursorLayer .cm-cursor": { display: "block" }
  ".cm-content": { caretColor: "transparent !important" }
  ```

  Two independent ways to have no caret while typing, and neither is colour or width. If CodeMirror's own idea of focus
  disagrees with where the DOM focus is, its caret layer is `display: none` — and no `border-left` rescues an element
  that was never rendered; meanwhile the browser's real caret, which would have shown, is hidden behind an `!important`
  that our 0.0.17 `caret-color: var(--ink)` could not answer. So the window now tracks focus itself and draws the caret
  on that, and only when CodeMirror says it is *not* focused does the native caret take over in ink — one caret on
  screen either way, not two blinking at each other. A `min-height: 1em` catches a line measured zero pixels tall, which
  is an invisible caret with no explanation. **And the correction matters more than the fix:** 0.0.16 set
  `--cm-cursor: var(--ink)` and commented that CodeMirror takes its caret colour from that variable. It does not — the
  base theme hardcodes `borderLeft: "1.2px solid black"`. A test now reads the dependency and fails if it ever mentions
  `--cm-cursor`, so the claim cannot come back. Two releases of caret work coloured a variable nobody reads; a comment
  that names an internal API without checking it sent me straight past the rule hiding the caret.

## The one backend-shaped thing

The dot needed one new question answered in core rather than guessed in the webview: `embeddings_status`, which reads
np's settings file for the provider and asks `models.list` whether the model's rows are there — measured on 0.5.1 as
`pinned: [{ id, bytes_total: 309855456 }]` with `models: []` before a fetch, and a row under `models` after one. Its
`present` is `null` when np did not answer **or** when np's models directory would not list: `artifact_present` answers
"no" in that case because a consent gate must fail closed, and a status light has no business inheriting that caution as
a fact about somebody's disk. An answer is still exactly one writer's business — the dot reads, never writes.

## Also fixed

- `mebibytes` left `statusLine.ts` and became an export, so the dot and the download banner cannot quote two different
  sizes for one file.
- `notesEmbedded` joined the tree module's counts with its own test: an *indexed* note must not count towards vectors,
  or a green bead appears over a folder that cannot be searched by meaning at all.
- Two front-end waits were written `expect(el).not.toBeNull()` on `document.querySelector(...)?.closest('button')`.
  Optional chaining yields `undefined`, `undefined` is not `null`, so `vi.waitFor` succeeded on the first tick and waited
  for nothing. A wait that cannot fail is not a wait; both now assert truthiness.

## Numbers

- **306** Rust tests pass (`cargo test -p jott-core`, note-processor off `PATH`); CI prints the count and fails under its
  floor (**300**).
- **367** front-end tests pass (was 341 at 0.0.17), judged by Vitest's exit code rather than a grep of "passed".
- `svelte-check`: 0 errors, 0 warnings. Windows build target type-checks as `jott-shell v0.0.18`. Built stylesheet was
  grepped for both caret rules, since no browser on this box can compute them.

## If you test it

`docs/testing-on-windows.md` §2b-viii has the checks. Three worth doing first: search something and read the number
beside a row (and its tooltip — it should refuse to look like a percentage); find the dot in the bottom bar and check its
word matches what your folder really is; then type into a note and look for a caret. If the caret is *still* missing, the
tester guide has one line to paste into devtools (Ctrl+Shift+I) that will say which of the two bugs it is — this box has
no screen, and I would rather aim than ship a fourth guess.
