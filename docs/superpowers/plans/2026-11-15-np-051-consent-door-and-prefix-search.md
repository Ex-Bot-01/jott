# np 0.5.1: consent door + prefix search — implementation plan

**Spec:** `docs/superpowers/specs/2026-11-15-np-051-consent-door-and-prefix-search-design.md`
**Branch:** `feat/m1-vault-editor` · **Release:** v0.0.15
**Mode:** native execution (no per-task subagents), TDD on every task, one fresh whole-branch reviewer at the end.

Ground rules that override anything below if they seem to conflict:

- Jott writes **only** `models.auto_fetch` in np's config. Everything else goes over the pipe.
- No number, sentence or count on screen unless np sent it. Absent optional data renders as unknown, never as zero.
- np's fault and note sentences are shown byte for byte.
- Tests about "np is absent" use `common::unreachable_np()`, never PATH.
- Rust gate: `PATH=$HOME/.cargo/bin:/usr/bin:/bin cargo test -p jott-core --no-fail-fast` — quote the **sum** of the
  `test result:` lines, never the last one (a doc-test with zero tests ends every run).
- Front-end gate is the `vitest` **exit code**, and `npx svelte-check` must read 0 errors.
- Shell work is only verified when the command prints `Checking jott-shell vX`.
- No `cargo fmt` on this branch.

---

### Task 1: np 0.5.1 becomes the floor and the bundled copy

**Files:** `crates/jott-core/src/np/resolve.rs`, `packaging/np-pin.json`, tests in `crates/jott-core/tests/np_resolve.rs`

**Interfaces:** unchanged signatures; `MIN_NP_VERSION: (u64,u64,u64) = (0, 5, 1)`, `NP_RELEASE_VERSION = "0.5.1"`.

- [ ] Step 1: extend the resolve tests — assert `min_version_text()` is `"0.5.1"`, that `asset_for(Windows)` and
      `asset_for(Linux)` name the 0.5.1 archives, and that a `(0, 5, 0)` np is now refused while `(0, 5, 1)` and
      `(0, 6, 0)` pass. Run one targeted test to see it fail first.
- [ ] Step 2: rewrite `packaging/np-pin.json` with the 0.5.1 archives and the hashes from that release's own
      `SHA256SUMS.txt`, verified here by `sha256sum -c` against the downloaded asset, not copied from a page:
      windows `c7fa67cd7fc7031eeaa54dd60cc4a4b562e3689d01dc738483ddbfe2d1f92d61`,
      linux `9b08f426bc2e679908ad43a8323b0df8e77ea0dd66596cb820cb925d9813a1e6`.
- [ ] Step 3: run `the_hint_names_the_np_the_installer_ships` and the whole resolve suite; update any prose in
      `resolve.rs` that still explains why 0.4.0 was the floor (it must name the new reasons: `embeddings.enable`,
      prefix search, truthful provenance).
- [ ] Step 4: commit — `feat: require and ship np 0.5.1`.

---

### Task 2: `embeddings.enable` and `embeddings.disable` as typed methods

**Files:** `crates/jott-core/src/np/methods.rs`, `crates/jott-core/src/np/outcomes.rs`, `tests/fake-np/np.py`,
`crates/jott-core/tests/np_methods.rs`

**Interfaces:**

```rust
pub struct EnableAnswer {           // np's answer, both doors
    pub changed: Vec<String>,       // "embeddings.provider", "embeddings.model" — possibly empty
    pub provider: String,           // "local-model" after enable, "none" after disable
    pub model: Option<String>,
    pub persisted_to: Option<PathBuf>,
    pub fetch_bytes: Option<u64>,   // np's `fetch.bytes_total`; None when it quoted no price
}
pub async fn embeddings_enable(client: &NpClient, model: &str) -> Result<EnableAnswer, NpFault>;
pub async fn embeddings_disable(client: &NpClient) -> Result<EnableAnswer, NpFault>;
```

- [ ] Step 1: failing tests in `np_methods.rs` against fake-np: enable quotes the measured shape (`changed` two keys,
      `fetch.bytes_total` 309855456, `persisted_to` a path); disable answers `changed: ["embeddings.provider"]`,
      `provider: "none"`, model kept; **a missing `persisted_to` stays `None`** (np writes the file even when it cannot
      name where — do not invent one); an unpinned-id fault arrives as `NpFault` with np's sentence intact.
- [ ] Step 2: fake-np learns `embeddings.enable` / `embeddings.disable` scenarios, including `-32602` for a param that
      is not `model` and `-32000` naming the ids the build pins — the two refusals measured on the real binary.
      Fake-np's `initialize` method list grows to the ten of 0.5.1, since that is what it claims to be.
- [ ] Step 3: implement; commit — `feat: ask np to record consent instead of pasting settings`.

---

### Task 3: consent goes through the door

**Files:** `crates/jott-core/src/ingest/queue.rs`, `crates/jott-core/src/ipc.rs`, `crates/jott-core/src/diagnostics.rs`
(if a new trace key is needed), tests in `ingest_queue.rs`, `ipc_surface.rs`, `diagnostics_journal.rs`

**Interfaces:** new event, serialised camelCase and pinned:

```rust
IngestEvent::ConsentRecorded {
    answer: String,                    // "granted" | "declined"
    #[serde(rename = "changed")] changed: Vec<String>,
    #[serde(rename = "providerSet")] provider_set: bool,   // np answered provider = "local-model"
    #[serde(rename = "configPath")] config_path: Option<PathBuf>,
    #[serde(rename = "fetchBytes")] fetch_bytes: Option<u64>,
},
```

- [ ] Step 1: failing tests. Grant with an np that accepts → one `consentRecorded` with np's keys, then
      `auto_fetch = true` in the file. Grant against an np that answers `-32000 no pinned model …` → a `refused` event
      quoting that sentence **and `auto_fetch` left at its old value**: enable first, write after, so a refusal changes
      nothing. Decline → `embeddings.disable` then `auto_fetch = false`, and one `consentRecorded` with
      `providerSet: false`. An np too old to have the method (`-32601`) must not look like a refusal of consent: keep
      writing `auto_fetch` (the 0.0.14 behaviour) and say which method was missing.
- [ ] Step 2: implement in `IngestQueue::set_consent` (it owns the writer connection); record one journal row per
      answer naming the method called, the keys np reported changing, and the destination — no values, no model ids;
      extend the pinned key sets and add the "no model id / no note text in this line" assertion the 0.0.14 review asked for.
- [ ] Step 3: full core suite; commit — `feat: consent is recorded by note-processor, one call per answer`.

---

### Task 4: prefix facts travel the whole wire

**Files:** `crates/jott-core/src/np/methods.rs`, `outcomes.rs`, `ipc.rs`, `src/lib/ipc.ts`,
tests in `np_methods.rs`, `ipc_surface.rs`, `src/lib/ipc.test.ts`

**Interfaces:**

```rust
pub async fn search(client: &NpClient, workspace: &Path, query: &str, limit: u8, mode: Mode, prefix: bool) -> …;
// SearchAnswer gains:  prefix_used: Option<bool>, prefix_terms: Vec<String>
// Hit gains:           matched_exactly: Option<bool>   // null = vector-only hit, never word-tested
```

`SearchResults` DTO gains `prefixUsed: Option<bool>`, `prefixTerms: Vec<String>`; each `HitDto` gains
`matchedExactly: Option<bool>`. TypeScript mirrors them as required `boolean | null` / `string[]` — Rust always
serialises the key, and an absent key would be a different fact from a null one.

- [ ] Step 1: failing tests — measured shapes first: whole-word search answers `prefix_used: false`, `prefix_terms: []`;
      with `prefix: true` the hit carries `matched_exactly: false` and `prefix_terms: ["grand"]`; a hit with no
      `matched_exactly` key stays `None`. Assert the request body actually sends `"prefix": true|false` (fake-np echoes
      params), because a param silently not sent is the whole feature missing while every test above still passes.
- [ ] Step 2: pin the DTO JSON exactly in `ipc_surface.rs`; extend the TS/Rust drift guard to `HitDto` as well as
      `SearchResults`.
- [ ] Step 3: thread `prefix` through `VaultSession::search` and the shell command; commit —
      `feat: np says which words it widened, and the window hears`.

---

### Task 5: the search box gets a remembered toggle

**Files:** new `src/lib/prefixSearch.ts` + `.test.ts`, `src/App.svelte`, `apps/shell/src/ipc.rs` (command arg),
`src/App.smoke.test.ts`

**Interfaces:**

```ts
export const PREFIX_KEY = 'jott.search.prefix'
export function loadPrefix(store: Pick<Storage, 'getItem'>): boolean   // anything but "1" is false
export function savePrefix(store: Pick<Storage, 'setItem'>, on: boolean): void
```

- [ ] Step 1: failing pure tests — default off when storage is empty; `"1"` reads true; a corrupt value reads false
      rather than throwing; saving `false` writes `"0"` (so a later version can tell "answered no" from "never asked").
- [ ] Step 2: implement, then wire: a checkbox *match word beginnings* beside the search field, its state from
      `loadPrefix(localStorage)`, each search sending it, changing it re-running the current query, and the choice
      surviving a remount (smoke test: toggle, "relaunch", assert the second window's checkbox is checked and that its
      first search sent `prefix: true`).
- [ ] Step 3: commit — `feat: match word beginnings, when you ask for it`.

---

### Task 6: say what was widened, and mark the hits that came from it

**Files:** `src/lib/searchState.ts` + tests, `src/lib/highlight.ts` + tests, `src/App.svelte`, `src/lib/Editor.svelte`

**Interfaces:**

```ts
export function wideningSentence(results: SearchResults): string | null
// "also matched word beginnings (grand)" from prefixUsed/prefixTerms; null when nothing was widened.
export function hitSuffix(matchedExactly: boolean | null): string | null
// false → "start of a word"; true and null → null. Vector-only hits get no mark: they were never word-tested.
export function markTerms(text: string, query: string, prefix?: boolean): Segment[]   // prefix marks at word starts only
```

- [ ] Step 1: failing tests. `wideningSentence` names np's terms and says nothing when `prefixUsed` is false or
      unknown; `hitSuffix(false)` marks and `hitSuffix(null)` does not; highlight in prefix mode marks the start of
      *grandmother*, never marks `agrand`, and non-prefix mode is byte-identical to today (regression pins).
- [ ] Step 2: implement; result rows show the suffix, opening a hit scrolls using prefix-aware `firstMatch`; the
      results header prints np's widening sentence beside the count.
- [ ] Step 3: commit — `feat: a widened search admits it, word by word`.

---

### Task 7: offer the widening on an empty result, and let np's answer fill the card

**Files:** `src/lib/searchState.ts`, `src/lib/consentHelp.ts` + tests, `src/App.svelte`, `src/App.smoke.test.ts`

**Interfaces:**

```ts
export function suggestPrefix(results: SearchResults, query: string, prefixOn: boolean): boolean
// true only when: query non-empty, not already widening, np said indexed === true, and zero hits came back.
export interface ConsentFacts { help: ConsentHelp; refusal: string | null; outcome?: ConsentOutcome | null }
```

- [ ] Step 1: failing tests. `suggestPrefix` is false for an unindexed folder (np said nothing to widen yet), for a
      blank query, and when the toggle is already on. `cardSentences` with an outcome leads with np's own record —
      which keys it changed and into which file — and drops the paste-advice branch entirely; with a refusal and no
      outcome it still quotes np verbatim and says what Jott did **not** change; `configUnreadable` keeps its 0.0.14
      wording, because np will not rewrite a file it cannot read and that is still the true sentence.
- [ ] Step 2: implement in App — the empty state's *try matching word beginnings* button re-runs the same query with
      prefix on (and does not touch the remembered preference); `consentRecorded` feeds the card and clears a stale
      refusal; a grant that np refused leaves the guardrail sentence alone rather than claiming a change.
- [ ] Step 3: smoke tests for both, run the whole front-end suite by exit code; commit —
      `feat: an empty search offers the widening, and the card quotes np's own record`.

---

### Task 8: docs, version, gates, review, release

- [ ] Step 1: `docs/testing-on-windows.md` gains **§2b-v** — what the toggle does and why it is off by default, what
      the header sentence means, what the card says now that np writes its own settings (with the provenance line a
      tester should expect to see in `config.toml`), and that Jott still writes only `models.auto_fetch`.
- [ ] Step 2: rewrite `docs/release-notes.md` for 0.0.15 from the tester's side, closing #6 and #7 as *np opened the
      doors; Jott walked through them*, with the measured numbers quoted.
- [ ] Step 3: bump `apps/shell/Cargo.toml`, `apps/shell/tauri.conf.json` and the `jott-shell` entry in
      `apps/shell/Cargo.lock` to **0.0.15**; raise the CI Rust floor to just under the measured count and quote both.
- [ ] Step 4: full gates, each quoted with its number or exit code — core tests (summed), clippy `-D warnings`,
      `npm run test`, `npm run build`, svelte-check, and the Windows check printing `Checking jott-shell v0.0.15`
      after a `touch`.
- [ ] Step 5: whole-branch review against the merge-base, with a shell-capable reviewer or the diff pasted in; measure
      every behavioural premise against np 0.5.1 before acting on it.
- [ ] Step 6: `release: 0.0.15 — np writes its own settings, and search can start at the beginning of a word`; merge to
      main, tag, push, watch CI, then verify the published assets by downloading them and checking
      `sha256sum -c SHA256SUMS.txt`, that the bundled np answers `np 0.5.1`, and that the shipped binary carries the
      new command surface.
