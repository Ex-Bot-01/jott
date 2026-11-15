# np 0.5.1: consent through np's own door, and search that can match a word's beginning

Date: 2026-11-15 · Decided by the user as scope "C, your call" after measuring 0.5.1 below.
Supersedes nothing in §18/§20 of the note-editor spec; it closes the two gaps that release 0.0.14 had to work around.

## Why

Jott 0.0.14 tells someone to paste `provider = "local-model"` into a file, because np had no door for it
([note-processor#7](https://github.com/Ex-Bot-01/note-processor/issues/7)), and it cannot find *grandma* when you type
*grand* ([#6](https://github.com/Ex-Bot-01/note-processor/issues/6)). np 0.5.0 opened both doors; 0.5.1 made the audit
line in `config.toml` truthful (#10) and turned the Windows nightly green (#9). Keeping Jott on 0.4.0 would keep two
workarounds alive that the backend no longer needs.

## What was measured, against the released Linux binary (2026-11-15)

Both assets verified with `sha256sum -c SHA256SUMS.txt` before running anything.

- `initialize` → `np: "0.5.1"`, `protocol: 1`, **ten** methods: the eight of 0.4.0 plus `embeddings.enable`,
  `embeddings.disable`. A client that ignores them is unaffected, so this is an additive protocol change.
- Prefix search. One note containing *grandmother*, *grandfather*, *kestrel*, scanned:

  | request | answer |
  | --- | --- |
  | `search "grand"` | `hits: []`, `prefix_used: false`, `prefix_terms: []` |
  | `search "grand", prefix: true` | `hits: [{path: a.md, matched_exactly: false}]`, `prefix_used: true`, `prefix_terms: ["grand"]` |
  | `search "grand*"` | widens the same way (`prefix_used: true`) — the CLI syntax works over the pipe too |

  The default is unchanged: no `*` and no `prefix` means whole words, exactly as in 0.4.0. Widening is opt-in because
  a vault holds identifiers, and np says so in its release notes. Jott inherits that default.
- Consent door. `embeddings.enable {model:"embeddinggemma-2"}` →
  `{changed: ["embeddings.provider","embeddings.model"], fetch: {bytes_total: 309855456, members: ["main"]},
  provider: "local-model", model: "embeddinggemma-2", persisted_to: "<path>/config.toml"}`, and the file gains

  ```toml
  # note-processor set by the `embeddings.enable` method over `np serve` (np 0.5.1) at 2026-11-15T17:40:41Z
  [embeddings]
  provider = "local-model"
  model = "embeddinggemma-2"
  ```

- Refusals are loud, and Jott shows them unchanged: an unknown parameter (`{"modle": …}`) → `-32602 params need a
  "model" this build pins …`; an id this build does not pin → `-32000 config io: no pinned model "no-such-model":
  this build can name "embeddinggemma-2" — enable one of those, or install bytes you already hold with np models
  install`.
- `embeddings.disable {}` → `{changed: ["embeddings.provider"], provider: "none", model: "embeddinggemma-2"}`. It
  revokes the download and **keeps** which model was chosen.
- `models.auto_fetch` is still Jott's key and nobody else's. With `auto_fetch = false` and both embeddings keys
  written, `embed` answers `configuration-disabled {id: "embeddinggemma-2", key: "embeddings.model", hint: "fetch it
  with np models fetch …"}` and moves nothing (data dir stayed 164 KB). With `auto_fetch = true` it began fetching —
  160 MB in the first twelve seconds. There is no method for this key; `models.list` only reports an in-flight fetch.
- `models.list` is unchanged in shape (`pinned[].{id,bytes_total,members}`) and gains a `fetching[]` array carrying
  `bytes_done`/`bytes_total`. Jott does not read `fetching` in this release: the status line already gets its bytes
  from the progress stream of the connection that is doing the work.

## Decisions

1. **Floor and bundle move to 0.5.1.** `MIN_NP_VERSION = (0, 5, 1)`, `NP_RELEASE_VERSION = "0.4.0" → "0.5.1"`,
   `packaging/np-pin.json` re-hashed from the release's own `SHA256SUMS.txt`. The existing test that makes the install
   hint and the installer pin agree keeps its teeth; it just has new numbers to check.
2. **Consent is recorded by np, over the pipe.** "Allow the download" calls `embeddings.enable <model>`; "stay
   keyword-only" calls `embeddings.disable`. Jott still writes exactly one key — `models.auto_fetch` — and says so.
   Order and failure: on *grant*, enable first, and write `auto_fetch = true` only if np accepted, so a refusal leaves
   the file as it was rather than half-consented. On *decline*, disable first and then write `false`: both directions
   fail toward fewer bytes moving.
3. **The card reports what np did.** New ingest event `consentRecorded` carries np's own answer (`changed`,
   `provider`, `model`, `persistedTo`, fetched byte total). The card leads with that — "note-processor wrote
   embeddings.provider, embeddings.model to <file>" — and stops reciting lines to paste. It keeps its advice only for
   the states where nothing was written: np's refusal quoted verbatim, and a `config.toml` that will not parse (np
   refuses to rewrite a file whose meaning it cannot check). The gap machinery stays, because those states still exist.
4. **Prefix search is opt-in, remembered per app, default off.** A checkbox beside the search field: *match word
   beginnings*. Off by default because widening `np-0.5` or a person's filename reports notes they never asked about —
   np's reasoning, adopted. The choice persists across launches and is not per-folder: a person's search habit is one
   habit.
5. **Every widening fact comes from np.** The request sends `"prefix": bool`. The header says *also matched word
   beginnings (grand)* from `prefix_used`/`prefix_terms`, never from re-reading the query. A hit that only a prefix
   reached is marked, from `matched_exactly: false`; `null` (a vector-only hit) gets no mark, because it was never in
   a whole-word test at all.
6. **Highlighting widens the same way the index did.** In prefix mode a term marks at word starts (`grand` marks the
   start of *grandmother*) and nowhere else — not mid-word, so `agrand` stays unmarked. Non-prefix mode is unchanged.
7. **An empty result can offer the widening, not perform it.** When an indexed folder returns nothing for a whole-word
   search, the empty state offers *try matching word beginnings*; pressing it re-runs with prefix on. Silently
   widening would make the count in the header mean something other than what was asked.
8. **Audit trail.** The consent row in the journal names which method Jott called and which keys np reported changing
   — no values beyond `true`/`false` for `auto_fetch`, no model ids, no note text. The closed key sets stay closed;
   new fields are added to `KEYS` deliberately and pinned on both sides, or the build fails.

## Non-goals

- No `models.list.fetching` polling, no resume of a partial download, no progress from anything but the connection
  doing the work.
- No `np models fetch`/`install` surface: those are terminal commands for someone who already has bytes.
- No auto-widening heuristics (per-query guessing, "did you mean"). The toggle says what it does; np decides the rest.
- No change to what Jott may write in np's config: still `models.auto_fetch`, and nothing else.

## What "done" looks like

A tester who presses *Allow the download* sees their own `config.toml` quoted back at them by np, with a provenance
line naming `embeddings.enable` over `np serve`. A tester who types `grand` in a folder containing *grandmother* gets
nothing until they tick *match word beginnings*, and then gets a result that says it was widened and which word did
it. About → the np line reads **0.5.1**.
