# Jott 0.0.15 — np writes its own settings, and search can start at the beginning of a word

Two requests Jott filed against note-processor have been fixed there, so this release is mostly Jott stopping doing
things it should never have had to do. The backend inside is now **np 0.5.1** (it was 0.4.0), and the wire protocol is
still version 1 — nothing about how the two programs talk changed, only what np is willing to answer.

## What you'll notice

- **There is a *match word beginnings* checkbox beside the search box, and it is off.** With it off, searching *grand*
  finds *grand* — exactly the search you have always had. Turn it on and *grand* finds *grandmother*, because
  note-processor now widens its own index query when a request asks it to
  ([#6](https://github.com/Ex-Bot-01/note-processor/issues/6)). It is off by default for the same reason np ships it
  off: in a vault of ticket numbers, code identifiers and half-remembered names, a term that quietly grows reports
  notes nobody asked about. Your choice is remembered per app and survives closing the window.
- **A widened search admits what it did.** The results header gains a line naming the words note-processor widened —
  *also matched word beginnings (grand)* — from np's own list rather than from Jott's guess at one, and any row that
  only matched by its first letters is marked *from the start of a word*. Rows found by meaning alone carry no mark:
  np never tested their words against your typing, and printing something about them would be a claim about a test
  nobody ran.
- **An empty search can offer the widening, without taking it.** In a folder np says is indexed, if whole words found
  nothing, you get *try matching word beginnings*. Pressing it re-runs that one query widened and deliberately does not
  move the checkbox: trying something once should not decide every search you make from now on. It appears only where
  pressing it can change an answer — not in a folder np has never indexed, not on an empty box, not when you are
  already widening.
- **Pressing "Allow the download" now ends with note-processor writing its own settings.** Since
  [#7](https://github.com/Ex-Bot-01/note-processor/issues/7) is fixed, Jott asks np over the pipe
  (`embeddings.enable`) instead of handing you two lines to paste. The card reports what np said it did — which keys it
  changed, into which file, and what the fetch will cost — with one line beside it about Jott's own part. If np refuses
  (an id this build cannot use, a config file it cannot parse), the refusal is quoted unchanged and the card says that
  Jott left `models.auto_fetch` alone. After a refused *yes* the download guardrail is exactly where it was, and the
  window says so rather than implying you had allowed something. Re-opening the same folder does not undo an answer
  either: the receipt is about np's one settings file, not about whichever vault was last listed.
- **"Stay keyword-only" revokes without erasing.** np switches its provider off and keeps which model you had chosen,
  so saying no twice does not make you pick again; the card repeats that, because "off" and "forgotten" are different
  states to a person reading the file later.

## Jott still writes one setting, and now there is a file to prove it

`models.auto_fetch`, that key only, in `%APPDATA%\note-processor\config.toml`. Everything else about embeddings is
written by note-processor itself, and it leaves its own provenance comment behind:

```toml
# note-processor set by the `embeddings.enable` method over `np serve` (np 0.5.1) at 2026-11-15T…
[embeddings]
provider = "local-model"
model = "embeddinggemma-2"

[models]
auto_fetch = true     # <- the one key Jott writes
```

`auto_fetch` stays Jott's because np does not touch it, which was measured rather than assumed: on 0.5.1, calling
`embeddings.enable` with `auto_fetch = false` in the file leaves the key where it is and `embed` still refuses to fetch
anything — no bytes move. Flip the key to `true` and the same request starts fetching 309,855,456 bytes (about 296 MiB)
for its built-in default model. A fresh np defaults that key to *yes*, so until you answer Jott holds it at `false`;
that is the whole guardrail, and it is the reason Jott still writes something at all.

The floor moved to 0.5.1 rather than 0.5.0 for one reason of its own: the provenance line above names *the method over
`np serve`* instead of a terminal command nobody ran
([#10](https://github.com/Ex-Bot-01/note-processor/issues/10)). Jott quotes that line as an audit trail, and a record
that invents a command is worse than no record.

## Measured, on the released 0.5.1 binaries

- `initialize` answers ten methods — the eight of 0.4.0 plus `embeddings.enable` and `embeddings.disable`. Protocol: 1.
- One note containing *grandmother*, *grandfather*, *kestrel*: `search "grand"` → no hits, `prefix_used: false`. The
  same request with `"prefix": true` → that note, `matched_exactly: false`, `prefix_terms: ["grand"]`. Same for
  `kestr`. So the widening is np's, and np says which words it widened.
- `embeddings.enable {"model":"embeddinggemma-2"}` answered
  `changed: ["embeddings.provider","embeddings.model"]`, `provider: "local-model"`,
  `fetch.bytes_total: 309_855_456`, and `persisted_to` naming the config file it had just written. An id this build does
  not pin is refused by name, naming the ids it will accept; a parameter that is not `model` is refused with `-32602`
  rather than answered `ok`; a config file np cannot parse is left byte-for-byte as it was and refused in words that
  name the line.
- `embeddings.disable {}` changed only `embeddings.provider`, to `"none"`, and kept the model.

## Not user-visible, but worth knowing

- **`prefix` is always sent, as a boolean.** np's default is off and Jott's toggle is off until asked; sending an
  omitted key would let a backend's default decide what somebody's search meant. Two tests read what actually crossed
  the wire rather than trusting the call site, because a parameter silently never sent is the whole feature missing
  while every other test still passes.
- **The consent answer comes back as data.** `set_consent` now returns which keys np moved, into which file, what the
  fetch costs, and one of four words about Jott's own key — `written`, `untouched`, `noConfigPath`, `failed`. "Jott
  wrote it" and "Jott could not write it" are different instructions to a person, so the wire has to tell them apart.
  The keys are pinned twice: exactly in Rust, and against `src/lib/ipc.ts` in the front-end suite.
- **Prefix marking requires a word start.** In prefix mode *grand* lights up inside *grandmother* and stays dark inside
  *agrand*. Whole-word marking is unchanged from 0.0.13 — it still marks the first occurrence, letters anywhere,
  because np's BM25 answers `kumquat` for `kumquats` and a highlight that disagrees with the row it opened is worse than
  a slightly generous one. That difference is pinned by tests on both sides rather than left to memory.
- **The editor marks with the same flag as the list.** A note opened from a widened row is marked with prefix rules;
  flipping the toggle re-marks the open note, because the words that matched genuinely changed and leftovers would
  describe a search you are no longer running.
- **The stand-in np answers like 0.5.1 by default**, including both consent doors and the `-32602` refusal for a
  parameter it does not take, and `initialize` in the captured handshake fixture was re-recorded from the real binary
  rather than edited by hand — a fixture that keeps saying "0.4.0" tests a refusal nobody ships.
- **An *answer* has exactly one writer.** Two places in Jott can touch `models.auto_fetch`, and they are different
  jobs: opening a folder whose answer is still undecided holds it at `false` (it never raises it), and recording your
  answer writes what the answer means. What used to exist was a third: the ingest queue wrote it whenever an answer
  arrived, and so did the consent path — unconditionally. A *yes* that note-processor refused therefore ended with
  `auto_fetch = true` in someone's file while the card said Jott had left it alone — two writers, two answers to the
  same question, one of them wrong. The queue now only remembers the answer; `consent::answer_consent` writes, and
  writes our key only when np opened its own door. A
  *no* still gets its `false` even with no np to ask, because lowering a guardrail needs nothing from a process that
  failed to start.
- **np's sentences stay out of the log file, and into the drawer.** The journal line for a consent answer names the
  method called (`embeddings.enable` / `embeddings.disable`) and what became of Jott's key — from the receipt, never
  from what the button meant; it used to say "wrote models.auto_fetch = false anyway" for every refusal, including the
  ones where the write itself had failed. np's own words are quoted on the card and in the drawer row instead, because
  they name model ids and the log is read by somebody else years later.
- **Answering consent now spawns np**, which surfaced a harness hazard: `FAKE_NP_SCENARIO` is process-global, so a test
  in the same binary could leave it pointing at a deleted tempdir and hand the next one a Python traceback. The fix was
  ownership (each test that talks to np configures its own stand-in), not clearing the environment somewhere else.

Tests: **295** Rust, **231** front-end. The CI job prints whether np was on `PATH` (several tests are about np being
absent and pass for the wrong reason on a machine that has it) and fails if the Rust count drops below its floor.

## What to look at, if you tested 0.0.14

1. Search a stem you know lives only inside longer words, get nothing, press **try matching word beginnings**. The
   header should name the widened word and the row should be marked *from the start of a word*.
2. Close Jott and reopen it: the checkbox should still be on. Turn it off and search again — the widened rows should go
   away, not sit there unlabelled.
3. Say no to embeddings in a fresh folder, then **Set up embeddings → Allow**. The card should read as what np did —
   keys, file, size — with no lines left to copy.
4. Put `provider = "local-model"` in `%APPDATA%\note-processor\config.toml` with no `model` line and run an embed: the
   card should quote np's sentence about needing a model, and say what Jott did **not** change.
5. Say no to embeddings in a folder whose settings np will not touch, then quit and reopen Jott and search. The window
   should still be keyword-only: a decline you gave once must not have to be given again because the answer happened to
   arrive while np was unreachable.
6. After allowing a download, open that `config.toml`. You should be able to tell from the file alone which lines np
   wrote and which single line came from Jott. If you cannot, that is a bug worth reporting.

Previous releases are listed at <https://github.com/Ex-Bot-01/jott/releases>. Docs:
[testing on Windows](https://github.com/Ex-Bot-01/jott/blob/main/docs/testing-on-windows.md) ·
[building from source](https://github.com/Ex-Bot-01/jott/blob/main/docs/building-on-windows.md).
