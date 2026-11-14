# Jott 0.0.13 — built against np 0.4.0

The backend inside this installer is now **np 0.4.0**. That is the release where the two bugs filed against it last
month got fixed, and one more thing that Jott had been guessing at became a number np hands over.

- [#4](https://github.com/Ex-Bot-01/note-processor/issues/4) — a workspace was keyed by the exact path string, so
  `D:\Notes\` and `D:\Notes` could look like two vaults and re-index one folder twice. Measured against 0.4.0 here:
  scanning the same folder with and without a trailing separator now answers `unchanged`, not `added`.
- [#5](https://github.com/Ex-Bot-01/note-processor/issues/5) — a search could not tell *"this folder was never
  indexed"* from *"nothing matched"*, and answered `mode: hybrid` for searches where only keywords ran. It now
  answers `indexed`, `keyword_used`, `semantic_used`, and a sentence of its own (`note`) when something notable
  happened.
- [#3](https://github.com/Ex-Bot-01/note-processor/issues/3) — `models.list` now carries `pinned`: the model this np
  build would fetch, with its byte size taken from the table compiled into that binary.

Jott's floor moved to **0.4.0**. If you set `NP_PATH` at an older np you will be told so, with the exact asset name
and hash check for your platform; unsetting it gets you the copy in the installer. Jott still *reads* an older
answer — every 0.4.0 key is optional on the wire — but a backend that cannot answer those questions is not one to
keep pointing people at.

## What that changes on screen

- **An empty search says which kind of empty it is, from np's own answer.** A folder nobody has indexed gets *"This
  folder is not indexed yet … use Index now"*; a folder that was searched and did not match gets *"Nothing matched
  `q`"*. Until now the first of those was Jott's inference from the tree's markers — which is still the fallback
  when np does not say, because silence is not an answer.
- **The results list admits what it skipped.** When only the keyword half ran, the header adds
  *words only · meanings not searched — this folder has no embeddings yet*. The `mode` word used to promise a hybrid
  search that never happened; these two flags are the facts behind it.
- **np's own sentence appears when it sends one** — kept byte for byte, not paraphrased into Jott's words.
- **The download prompt quotes np's number.** The consent dialog said *"a model"* with no size because np 0.3.1
  reported none anywhere (that gap is what issue #3 was filed for). It now says the size np itself would fetch —
  309,855,456 bytes for the built-in text model — and if a future build quotes a vision model at 865 MB, the prompt
  will say 865 MB rather than a number Jott compiled in last autumn. Jott's own pin is only used when np names
  nothing at all.

## One protocol change worth knowing about

np 0.4.0 stopped sending progress notifications to clients that did not ask: support is advertised by the
`progress` key, and only on a connection whose `initialize` sent `"notifications": true`. That is right for a
script reading one line per request — and it would have silently muted Jott's status bar, because the child started
at startup is the same child that does the indexing. Jott asks now; measured against the released binary, a 400-note
scan sends five progress lines with the flag and zero without it.

## Not user-visible, but it caught two things

- The np stand-in used in tests emulates 0.4.0 by default — including that opt-in rule — so a client that forgot to
  ask can no longer pass 250 green tests.
- The front-end suite could fail on an unhandled rejection with every test passing: the stub had no `mirror_buffer`,
  and the editor mirrors a dirty buffer on a timer that can fire after a test ends. Every command the window can
  legitimately send is stubbed now.

Tests: **249** Rust, **138** front-end. The CI job prints whether np was on `PATH` (some tests are about np being
absent) and fails if the Rust count drops below its floor.

## What to look at, if you tested 0.0.12

1. About → the np line should read **0.4.0**.
2. Search in a folder you have never indexed: it should say so, not *"Nothing matched"*. Then *Index now* and search
   again: the second empty answer should be *"Nothing matched"*.
3. Search for something that only matches by word, in a folder without embeddings: expect the *words only* note.
4. If you set `NP_PATH` at an np you built months ago, Jott should refuse it by name and version — and work with the
   variable unset.

Previous releases are listed at <https://github.com/Ex-Bot-01/jott/releases>. Docs:
[testing on Windows](https://github.com/Ex-Bot-01/jott/blob/main/docs/testing-on-windows.md) ·
[building from source](https://github.com/Ex-Bot-01/jott/blob/main/docs/building-on-windows.md).
