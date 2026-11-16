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
- **A status light that knows six things apart.** "Green when the model is active/downloaded/online" is three states in
  one sentence, measured in three places: np's `embeddings.provider` for *configured*, np's `models.list` for *the bytes
  are on this disk*, and Jott's own per-file markers for *this folder has vectors*. So there are six — plus the one that
  keeps happening in the field, **nobody could say**: drawn as a hollow bead, never as grey, because telling somebody with
  296 MiB of weights on their disk that their embeddings are switched off is how they go and download them again. Four
  things produce it and the dot names which: np did not answer; its settings file will not parse (which is *not*
  "embeddings off" — advice nobody could follow); np is running but could not list its models directory, in which case the
  dot quotes np's own sentence; or np named no model for this folder. A provider Jott does not
  recognise (`openai`, somebody's own server) gets its own state: "on this disk" is a story about `local-model`, and np
  lists no rows for an endpoint, which would otherwise read as a download that never arrives. Each state carries a word —
  `meanings on`, `model ready`, `model not downloaded`, `embeddings off`, `model state unknown`, `another provider` —
  because a bead alone
  says nothing to a screen reader, a colour-blind tester, or a screenshot in a bug report. Clicking it opens Settings,
  the one place the state can be changed. It is asked once per opened folder over np's *reader* connection, so it cannot
  queue behind an embed in progress: when a download finishes, that is the call which learns about it.
- **The caret was being hidden by Jott, not by CodeMirror — and the release that "fixed" it is what hid it.** Third
  attempt at "the cursor looks like no cursor"; this one read the dependency instead of the comments. Two things came
  back:

  ```js
  function drawSelection(config = {}) {      // @codemirror/view
      return [selectionConfig.of(config), cursorLayer, selectionLayer, hideNativeSelection, …];
  }
  ```

  `cursorLayer` is the only thing that creates `.cm-cursor` elements, and **Jott never installs `drawSelection()`** — so
  every caret rule this project has written in two releases (the ink `border-left`, the `--cm-cursor` variable, the blink
  keyframes, and the fallback added earlier in this very branch) selected elements that do not exist. The caret this
  window has always had is the browser's own, and `Editor.svelte` was hiding it with
  `'.cm-content': { caretColor: 'transparent' }`, added by 0.0.16 under a comment asserting CodeMirror hides the native
  caret itself. It does not — that hiding ships inside `drawSelection()` too. **The bug reported twice was introduced by
  the release that claimed to fix it.**

  Now the theme names the caret in ink (the only place that can decide it: CodeMirror injects its style modules after the
  bundled stylesheet, so a global rule cannot outvote it), nothing anywhere makes a caret transparent, and Windows
  high-contrast mode gets `CanvasText`. The dead rules are deleted rather than kept "in case", and a test now enforces the
  invariant that would have caught this in 0.0.16: *if you style `.cm-cursor`, you must have installed
  `drawSelection()`* — both halves checked, so neither can arrive alone. What is **not** claimed: nobody has seen your
  screen. The bundle greps clean and the rules are pinned, but jsdom cannot resolve either stylesheet (it reports
  `rgb(0, 0, 0)` for `.cm-content` no matter what is injected), so the last word is yours — §2b-viii item 6 has one line
  to paste into devtools if a caret is still missing.

## The one backend-shaped thing

The dot needed one new question answered in core rather than guessed in the webview: `embeddings_status`, which reads
np's settings file for the provider and asks `models.list` whether the model's rows are there — measured on 0.5.1 as
`pinned: [{ id, bytes_total: 309855456 }]` with `models: []` before a fetch, and a row under `models` after one. Its
Its answer also carries `configUnreadable` and `modelsDirError`, because "no provider" and "nobody could read the file
that would name one" need opposite advice.

**What it costs, said plainly:** asking once per opened folder means np's *second* child process starts when you open a
folder whose settings name a provider — roughly the 8 MB that connection has been kept lazy to avoid since 0.0.12. That is
a deliberate departure, argued in the plan record and pinned by tests: a status light must be able to say "on" as well as
"not bought yet", and it goes over the reader so it cannot queue behind an embed (measured in `np/pool.rs`: 102 s of
queueing on one connection against 0.00 s from a second process). What has *not* changed is that nothing else about loading
a folder talks to np — no ask per keystroke, none per re-list, and exactly one extra after a pass finishes, because a
finished pass is what puts weights into np's models directory.

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

## What the review found

A fresh whole-branch reviewer read the dependency rather than my notes and blocked the release on the caret (above). It
also found the unreadable-settings flattening, a late `models.list` answer overwriting the folder on screen, a price being
quoted for a model other than the one named, and — most usefully — three tests that passed with the bug put back. Two of
them were my race test, which was harmless twice over: both answers were delayed equally, and the stub read its switch
*inside* the timer, so the "late" reply carried the new folder's facts and there was no stale answer to drop. Both are
fixed, and each guard is now shown red without its test. The third was a stub that answered `embeddings_status` where core
refuses: with it, deleting App's no-folder guard left a first-run window displaying "embeddings off" and the suite green.

## Numbers

- **309** Rust tests pass (`cargo test -p jott-core`, note-processor off `PATH`); CI prints the count and fails under its
  floor (**300**).
- **373** front-end tests pass (was 341 at 0.0.17), judged by Vitest's exit code rather than a grep of "passed". One test
  was deleted along with its subject — the focus-tracking class the caret finding made pointless — and a lower count is
  reported rather than hidden.
- `svelte-check`: 0 errors, 0 warnings. Windows build target type-checks as `jott-shell v0.0.18`. Built stylesheet was
  grepped for both caret rules, since no browser on this box can compute them.

## If you test it

`docs/testing-on-windows.md` §2b-viii has the checks. Three worth doing first: search something and read the number
beside a row (and its tooltip — it should refuse to look like a percentage); find the dot in the bottom bar and check its
word matches what your folder really is; then type into a note and look for a caret. If the caret is *still* missing, the
tester guide has one line to paste into devtools (Ctrl+Shift+I) that will say which of the two bugs it is — this box has
no screen, and I would rather aim than ship a fourth guess.
