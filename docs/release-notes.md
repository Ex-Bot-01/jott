# Jott 0.0.14 — consent you can see, progress you can watch

Three reports came back from testing 0.0.13. One was about a sentence nobody could act on, one about a search that
missed *grandma* when you typed *grand*, and one where a button opened the wrong window. This release answers the two
that belong to Jott, and files the third where it belongs — which is not in Jott.

## What you'll notice

- **Pressing "Allow the download" now ends in something you can use.** Before, it ended with
  `keyword-only: embeddings.provider is not set` — note-processor's shrug, presented as if it were an answer to your
  click. Now, when embedding still cannot run, the window shows a card: what your click recorded, note-processor's own
  refusal quoted word for word, and the exact lines its settings file is missing, as text you can copy. A **Try again**
  button re-runs the pass once you have edited them.
- **Jott writes one setting, and says which.** Your answer becomes `models.auto_fetch` in note-processor's
  `config.toml`, that key only. Jott does not choose your embedding engine or model, because nothing over the protocol
  lets it ask — see [#7](https://github.com/Ex-Bot-01/note-processor/issues/7) below. Until you answer, the key reads
  `false`: a fresh note-processor fetches a ~300 MB model without asking, and that is not Jott's decision to make.
- **A download has a bar that moves on real bytes.** While a model arrives, the bottom line counts np's own byte
  counters and adds *model part 1 of 2* when np fetches more than one file. When np announces no total you get movement
  without a percentage. A bar parked at 0% reads as a stalled download, so Jott would rather show nothing than lie.
- **"Model ready" is said out loud.** When a model finishes, the window keeps a line: which model, how many bytes np
  fetched, and that search now matches meanings as well as words. If the window is in the background — which is how
  most people will spend those few minutes — you also get one OS notification; Jott asks for permission at that moment,
  not on first launch, and if you say no, the banner says it instead. Nothing appears when np fetched nothing.
- **Diagnostics → Show log file opens Explorer with `jott.jsonl` selected.** It used to be *Open log folder*, a thing
  the shell is deliberately not permitted to do; the refusal was caught and its fallback showed a small box of copied
  diagnostics text. That is how "open the folder" turned into a textarea. If a file manager ever refuses again, the
  line names the path and offers to copy it.

## Two things Jott asked note-processor for instead of working around

**[#6 — prefix matching, so *grand* can reach *grandma*.]** Measured against np 0.4.0 with one note containing
*grandma*, *grandmother*, *kestrel*: `grandma` and `grandmother` hit, `grand`, `grandm` and `kestr` do not. Once the
folder is embedded, those fragments hit again — by meaning, `semantic_used: true`. The keyword index uses FTS5 phrase
matching and quotes each token, so this is a deliberate design decision in note-processor, not two lines of Jott code.
The wrong fix would be a second search inside the window that re-scans your notes and merges its hits with np's: it
would rank differently from every other search, run on folders np has already indexed, and be invisible in the drawer.

**[#7 — no way to turn consent into embeddings.]** Jott records `auto_fetch = true` and note-processor still embeds
nothing: measured, `embed` then answers `embedded: 0`, `model: null`, `fetched: null`. Setting
`embeddings.provider = "local-model"` by hand faults with *"needs embeddings.model to name an installed model; none is
set"*. Only when both provider and model are in the file does np fetch 309,855,456 bytes and embed. There is no `serve`
method and no CLI command that writes those two keys, so Jott shows you the lines instead of writing settings it was
not given permission to own.

## Not user-visible, but worth knowing

- The card's facts come from core: one command reads np's config and answers which setting is short
  (`provider missing` / `model missing` / `an engine Jott does not know` / `ready`). The window only puts sentences
  under it — so the same verdict cannot be worded two ways, and a provider Jott has never heard of gets no advice at
  all rather than a guess.
- Nothing in Jott talks to Ollama, and nothing does now: local inference in note-processor is `local-model`, which is
  statically linked llama.cpp with a pinned model.
- The front-end test harness had been quietly degrading: tests appended since 0.0.13 landed *outside* their `describe`
  block, so the shared scenario state stopped being reset between them. One new test failed for exactly that reason
  and is what surfaced it.

Tests: **263** Rust, **167** front-end. The CI job prints whether np was on `PATH` (several tests are about np being
absent, and pass for the wrong reason on a machine that has it) and fails if the Rust count drops below its floor.

## What to look at, if you tested 0.0.13

1. Say **no** to embeddings first, then use *Set up embeddings* under the tree: you should get a card naming
   `models.auto_fetch`, not a state token.
2. Say **yes** in a folder with notes: either the model starts downloading with a bar that moves, or you get the card
   telling you which two lines to add — and *Try again* picks it up after you add them.
3. Alt-tab out during a download: expect one notification when it finishes (the first time, Jott asks whether it may).
4. **Ctrl+Shift+D → Show log file**: Explorer should open with `jott.jsonl` selected.
5. Search for a fragment of a word you know is there: it will still miss. That is [#6](https://github.com/Ex-Bot-01/note-processor/issues/6), and the results header now says which half of search ran.

Previous releases are listed at <https://github.com/Ex-Bot-01/jott/releases>. Docs:
[testing on Windows](https://github.com/Ex-Bot-01/jott/blob/main/docs/testing-on-windows.md) ·
[building from source](https://github.com/Ex-Bot-01/jott/blob/main/docs/building-on-windows.md).
