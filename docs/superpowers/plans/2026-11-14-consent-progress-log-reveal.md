# Consent Visibility and Download Progress Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make a consent answer visible (what Jott wrote, what np is still waiting for), show real download progress with a completion notice, and make "Open log folder" reveal the log file.

**Architecture:** np's own answers stay authoritative: a pure Rust function derives *what np's config is missing* from that config, an additive key on the `working` event carries np's member counters, and np's `fetched` record becomes modelled so "model ready" is something np said rather than something Jott inferred. The front end renders; it decides nothing about state.

**Tech Stack:** Rust (`jott-core`, Tauri shell), Svelte 5 + TypeScript + Vitest, `toml_edit`, `@tauri-apps/plugin-opener`, `@tauri-apps/plugin-notification`.

**Spec:** `docs/superpowers/specs/2026-11-14-consent-progress-log-reveal-design.md`

## Global Constraints

- Never write `embeddings.provider` or `embeddings.model` into np's config. The only key Jott writes is `[models] auto_fetch`, and the UI names that fact. (Spec "Rulings", np#7.)
- No Ollama anywhere: local inference in np is a statically linked llama.cpp, exposed as `provider = "local-model"`. Never suggest another provider or a server.
- Sizes, durations and counts on screen come from np (`pinned`, `bytes_total`, `fetched.members[].bytes`). Nothing invented; absent means unstated.
- np's fault/refusal sentences are shown byte for byte, never paraphrased into Jott's words.
- "No vectors" is a state, not a failure: tone `warn`, keyword search still works, retry stays available.
- Wire additions are optional keys with camelCase names; every serialised DTO/event key is pinned by a test in `crates/jott-core/tests/ipc_surface.rs`, and TypeScript unions/keys are compared against Rust source in `src/lib/ipc.test.ts`.
- Tests about np being absent use `common::unreachable_np()`, never PATH. Run the Rust suite as CI sees it: `PATH=$HOME/.cargo/bin:/usr/bin:/bin cargo test -p jott-core`.
- Front-end gate is Vitest's **exit code**, not the word "passed".
- No `cargo fmt` on this branch (formatting-only commit is planned between milestones).
- Copy style: short, plain, no emoji; a sentence a person can act on.

## File Structure

| File | Responsibility |
|---|---|
| `crates/jott-core/src/ingest/consent.rs` | Read np's config; derive which setting is missing (`SettingsGap`, `settings_gap`). |
| `crates/jott-core/src/ipc.rs` | `ConsentHelp` payload + `VaultSession::embeddings_help()`; new optional keys on `IngestEvent::Working`; model np's `fetched` record on embed reports. |
| `crates/jott-core/src/np/outcomes.rs` | `FetchedRecord` (id, bytes total, digest-verified) from np's embed document. |
| `apps/shell/src/ipc.rs` | Thin `embeddings_help` command; notification plugin init stays in `main.rs`. |
| `src/lib/statusLine.ts` | Status text plus a machine-readable `progress` for the bar. |
| `src/lib/StatusLine.svelte` | Draws determinate or indeterminate bar from `Status.progress`. |
| `src/lib/consentHelp.ts` | Pure: card sentences and copyable lines from `ConsentHelp`. |
| `src/App.svelte` | Renders the card, the persistent "model ready" banner, and asks for a notification when unfocused. |
| `src/lib/diagnostics.ts` / `Diagnostics.svelte` | `revealTarget()` returns the log **file**; reveal uses `revealItemInDir`. |

## Review Focus

Each line gets its test in the named task.

1. **np omits `bytes_total`** (size unknown) → an unquantified moving stripe plus "… MiB so far", never a bar parked at 0%. *(Task 4)*
2. **A model with several members** (`main`, `mmproj`) → "model part 1 of 2"; when np sends no `members_*`, no part wording is invented. *(Task 3, Task 4)*
3. **The user's own provider is something else** (`"none"` written deliberately, or a future spelling) → the card quotes np and advises nothing; it never tells someone to overwrite a choice. *(Task 1, Task 5)*
4. **A fetch that fails or goes offline mid-download** → the banner shows np's sentence and stays retryable; "model ready" appears only when np's report says it fetched. *(Task 5)*
5. **No notification permission, or a webview with no notifier** → the in-window banner still appears and nothing errors or dialogs. *(Task 6)*

---

### Task 1: What np is waiting for, derived in Rust

**Files:**
- Modify: `crates/jott-core/src/ingest/consent.rs`
- Test: `crates/jott-core/tests/np_consent.rs`

**Interfaces:**
- Consumes: an np config path (`Option<&Path>`), and the model id Jott quoted in the offer.
- Produces:
  ```rust
  /// Which setting stands between "consent given" and vectors existing, read from np's own config.
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub enum SettingsGap {
      /// No `[embeddings] provider` at all — np reads that as `provider = "none"` and embeds nothing.
      ProviderMissing,
      /// Provider is `local-model`, but no model is named, so np faults rather than guesses.
      ModelMissing,
      /// Someone chose a provider deliberately. Jott has no advice about it (§ separation ruling).
      ProviderOther { provider: String },
      /// Nothing missing: consent and configuration agree.
      Ready,
  }

  pub fn settings_gap(np_config: Option<&Path>, quoted_model: Option<&str>) -> SettingsGap;

  /// The lines np needs added, in TOML order, for a gap Jott is allowed to describe. Empty for
  /// `ProviderOther` and `Ready` — no advice about a choice somebody made by hand.
  pub fn gap_lines(gap: &SettingsGap, quoted_model: Option<&str>) -> Vec<String>;
  ```
  `gap_lines` for `ProviderMissing` yields `["[embeddings]", "provider = \"local-model\"", "model = \"<id>\""]`, using the quoted id when there is one and np's built-in default id (`crate::ingest::consent::builtin_default_model().id`) when there is not. For `ModelMissing` it yields only the `model = "<id>"` line.

- [ ] **Step 1: Write the failing tests** in `np_consent.rs`, one per measured config shape (all four were measured against np 0.4.0 and are recorded in the spec):

```rust
#[test]
fn a_config_with_no_embeddings_section_is_missing_the_provider() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "[models]\nauto_fetch = true\n").unwrap();
    assert_eq!(settings_gap(Some(&path), Some("embeddinggemma-2")), SettingsGap::ProviderMissing);
    assert_eq!(
        gap_lines(&SettingsGap::ProviderMissing, Some("embeddinggemma-2")),
        vec!["[embeddings]", "provider = \"local-model\"", "model = \"embeddinggemma-2\""]
    );
}

#[test]
fn a_missing_config_is_the_same_gap_as_an_empty_one() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(settings_gap(Some(&dir.path().join("absent.toml")), None), SettingsGap::ProviderMissing);
    assert_eq!(settings_gap(None, None), SettingsGap::ProviderMissing, "no config path is still no provider");
    // No quoted model: the line names np's own built-in default rather than inventing a placeholder.
    assert!(gap_lines(&SettingsGap::ProviderMissing, None).iter().any(|l| l.contains("embeddinggemma-2")));
}

#[test]
fn a_provider_without_a_model_names_the_model_line_only() {
    /* config: `[embeddings]\nprovider = "local-model"\n` → ModelMissing; lines are just `model = "…"` */
}

#[test]
fn a_provider_someone_else_chose_gets_no_advice() {
    /* provider = "none" and provider = "something-new" both → ProviderOther { provider },
       and gap_lines(...) is empty in both cases. */
}

#[test]
fn provider_and_model_together_leave_nothing_to_say() {
    /* `[embeddings]\nprovider = "local-model"\nmodel = "bge-m3"\n` → Ready, lines empty */
}
```

- [ ] **Step 2: Run them and watch them fail** — `cargo test -p jott-core --test np_consent` (expect: unresolved items `SettingsGap`, `settings_gap`).
- [ ] **Step 3: Implement** `SettingsGap`, `settings_gap`, `gap_lines` in `consent.rs`. Reuse the existing `embedding_model_named(path)` helper for the model; read the provider with the same `load(path)?` document path used by `auto_fetch_allowed`, and treat an unreadable file as `ProviderMissing` — np would refuse it the same way, and a card that says "unreadable" instead of naming lines is Task 2's problem to phrase.
- [ ] **Step 4: Run the tests** — expect all pass.
- [ ] **Step 5: Commit** — `git add crates/jott-core/src/ingest/consent.rs crates/jott-core/tests/np_consent.rs` with message `feat: say which np setting stands between consent and vectors`.

---

### Task 2: The card's payload, on the wire

**Files:**
- Modify: `crates/jott-core/src/ipc.rs`
- Modify: `apps/shell/src/ipc.rs` (command + `generate_handler!` list)
- Test: `crates/jott-core/tests/ipc_surface.rs`

**Interfaces:**
- Consumes: `SettingsGap`, `gap_lines`, `auto_fetch_allowed`, `np_config_path()` from `ingest::consent`.
- Produces:
  ```rust
  /// What the consent card tells the window: what Jott recorded, what np still needs, and where.
  #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
  #[serde(rename_all = "camelCase")]
  pub struct ConsentHelp {
      /// `"providerMissing" | "modelMissing" | "providerOther" | "ready"` — the `SettingsGap` spelled for the wire.
      pub gap: String,
      /// The provider np's config names today, when it names one. A setting name, never note content.
      pub provider: Option<String>,
      pub model_id: Option<String>,
      /// The TOML lines that would close the gap; empty when it is not Jott's place to say.
      pub lines_to_add: Vec<String>,
      /// np's config file the lines belong in.
      pub config_path: Option<PathBuf>,
      /// Keys Jott itself has written, spelled like `models.auto_fetch`, so the card can own up to them.
      pub jott_wrote: Vec<String>,
  }

  impl VaultSession {
      pub fn embeddings_help(&self) -> ConsentHelp;
  }
  ```
  and the shell command `pub fn embeddings_help(app: State<'_, App>) -> Result<ConsentHelp, IpcError>`.

- [ ] **Step 1: Write the failing tests** in `ipc_surface.rs`:

```rust
#[tokio::test]
async fn the_card_names_the_provider_line_and_owns_up_to_auto_fetch() {
    // np config with `[models] auto_fetch = true` and no `[embeddings]`, as Jott leaves it after "Allow".
    let help = session.embeddings_help().await;
    assert_eq!(help.gap, "providerMissing");
    assert_eq!(help.provider, None);
    assert_eq!(help.lines_to_add, vec!["[embeddings]", "provider = \"local-model\"", "model = \"embeddinggemma-2\""]);
    assert_eq!(help.jott_wrote, vec!["models.auto_fetch".to_string()], "the card says what Jott touched");
}

#[test]
fn consent_help_keys_are_pinned() {
    assert_eq!(
        serde_json::to_value(ConsentHelp { /* every field filled */ }).expect("serialises"),
        json!({ "gap": "ready", "provider": "local-model", "modelId": "embeddinggemma-2",
                "linesToAdd": [], "configPath": "/home/you/.config/note-processor/config.toml",
                "jottWrote": ["models.auto_fetch"] })
    );
}

#[tokio::test]
async fn a_provider_of_someones_own_gets_no_lines_from_jott() { /* provider = "none" → gap "providerOther", lines empty */ }
```

- [ ] **Step 2: Run** `cargo test -p jott-core --test ipc_surface` and confirm the new tests fail to compile.
- [ ] **Step 3: Implement** `ConsentHelp` + `VaultSession::embeddings_help()` in `ipc.rs`, mapping `SettingsGap` to the wire words, and `jott_wrote` from `auto_fetch_allowed(path)` (a readable file whose value is `true` → `["models.auto_fetch"]`; `false` → the same key with the card wording handled by Task 5; unreadable → empty). Add the shell command and register it.
- [ ] **Step 4: Run** the test binary again, then `cargo clippy -p jott-core --all-targets -- -D warnings`.
- [ ] **Step 5: Record it in the journal.** When consent is answered, append a file-work line (`LocalRecord { event: "consent", … }`) whose detail names the keys Jott wrote and the destination path — never their values. Add the failing test to `crates/jott-core/tests/diagnostics_journal.rs` first: answer consent on a session with a journal, read the lines back, assert one line has `event == "consent"` and a `detail` containing `models.auto_fetch`, and that no line contains the model id string (the privacy rule is about keys and destinations, not settings values).
- [ ] **Step 6: Add the TypeScript side**: `ConsentHelp` interface in `src/lib/ipc.ts`, plus `embeddingsHelp = () => invoke<ConsentHelp>('embeddings_help', {})`; extend the existing key-comparison test in `src/lib/ipc.test.ts` with `interfaceKeys(ipcTs, 'ConsentHelp')` vs `rustFields(ipcRust, 'ConsentHelp')`.
- [ ] **Step 7: Run** `npx vitest run src/lib/ipc.test.ts` (expect pass), then prove it bites by deleting one field from the interface and seeing red; restore.
- [ ] **Step 8: Commit** — `feat: the consent card gets its facts from core, not from markup`.

---

### Task 3: np's member counters reach the window

**Files:**
- Modify: `crates/jott-core/src/ingest/queue.rs` (`IngestEvent::Working`, progress mapping near the `phase → counter keys` match)
- Modify: `src/lib/ipc.ts` (`working` payload type)
- Test: `crates/jott-core/tests/ipc_surface.rs` (event key pinning), `crates/jott-core/tests/ingest_queue.rs` (a fetch with members produces them)

**Interfaces:**
- Consumes: np's `np/progress` line for `phase: "fetching"`, which carries `bytes_done`, `bytes_total`, `member`, `members_done`, `members_total`.
- Produces:
  ```rust
  Working {
      phase: String,
      /// Phase-appropriate counters, as today: bytes while fetching, chunks while embedding/writing.
      done: u64,
      total: Option<u64>,
      /// Which of several model files is arriving: `members_done + 1` and `members_total`. Both `None`
      /// when np did not send them — a single-member build sends none, and "part 1 of 1" would be noise.
      #[serde(rename = "memberIndex")] member_index: Option<u64>,
      #[serde(rename = "memberCount")] member_count: Option<u64>,
  }
  ```

- [ ] **Step 1: Write the failing tests.** In `ipc_surface.rs`, extend the pinned `working` assertions (keep the existing exact-JSON test passing by filling the new fields with `None`), and add:

```rust
#[tokio::test]
async fn a_multi_member_fetch_says_which_part_is_arriving() {
    // fake-np: progress_counters for "embed" with np's own keys, members_total 2.
    let events = collect(&mut rx, Duration::from_millis(900)).await;
    let working = events.iter().find_map(|e| match e { IngestEvent::Working { phase, member_index, member_count, .. } if phase == "fetching" => Some((*member_index, *member_count)), _ => None });
    assert_eq!(working, Some((Some(1), Some(2))), "np's `members_done: 0` of 2 is part 1 of 2");
}

#[tokio::test]
async fn a_single_member_fetch_says_nothing_about_parts() {
    // Same scenario without `members_*`: both fields stay None rather than becoming 1-of-1.
}
```

- [ ] **Step 2: Run** `cargo test -p jott-core --test ipc_surface --test ingest_queue` and confirm failure.
- [ ] **Step 3: Implement** the two fields, deriving `member_index = members_done + 1` only when np sent both keys (and `members_total > 1`; a total of 1 says nothing the absence does not). Update every `IngestEvent::Working` construction site.
- [ ] **Step 4: Run** both test binaries; then add `memberIndex: number | null; memberCount: number | null` to the `working` variant in `src/lib/ipc.ts` and run `npx vitest run`.
- [ ] **Step 5: Commit** — `feat: the window learns a model has more than one file`.

---

### Task 4: A bar that moves only when np says so

**Files:**
- Modify: `crates/jott-core/src/ipc.rs` (model np's `fetched` record on embed reports) and `src/lib/ipc.ts` (`FetchedRecord`)
- Modify: `src/lib/statusLine.ts`, `src/lib/StatusLine.svelte`
- Test: `crates/jott-core/tests/np_methods.rs` (parsing), `src/lib/statusLine.test.ts`, `src/App.smoke.test.ts`

**Interfaces:**
- Consumes: np's embed document (`fetched: { id, members: [{ name, bytes, digest-verified }], origin }`) and the `working` event from Task 3.
- Produces:
  ```rust
  /// np's own record of a model it fetched during this run. Bytes are np's numbers.
  #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
  #[serde(rename_all = "camelCase")]
  pub struct FetchedRecord { pub id: String, pub bytes_total: Option<u64>, pub digest_verified: Option<bool> }
  // EmbedReport gains: pub fetched: Option<FetchedRecord>,   // null/absent on np → None
  ```
  ```ts
  export interface Progress { done: number; total: number | null; memberIndex: number | null; memberCount: number | null }
  export interface Status { text: string; tone: 'neutral' | 'busy' | 'warn'; action?: 'consent'; progress?: Progress }
  /** `model ready · embeddinggemma-2 · 310 MiB · keyword and meaning search both on now` */
  export function modelReadySentence(fetched: FetchedRecord): string
  ```

- [ ] **Step 1: Write the failing Rust test** in `np_methods.rs`: an embed fixture whose document carries np's real `fetched` object (copy the measured shape: `{"id":"embeddinggemma-2","members":[{"bytes":309855456,"digest-verified":true,"name":"main"}],"origin":"compiled-in-pin"}`) asserts `report.fetched == Some(FetchedRecord { id, bytes_total: Some(309_855_456), digest_verified: Some(true) })`, and the existing fixture with `"fetched": null` asserts `None`.
- [ ] **Step 2: Run** `cargo test -p jott-core --test np_methods` (expect failure), then implement `FetchedRecord` + the `EmbedReport` field. `bytes_total` is the sum of member bytes when every member names one, `None` otherwise — a partial total must not be presented as the whole download.
- [ ] **Step 3: Run** it again (expect pass).
- [ ] **Step 4: Write the failing front-end tests** in `statusLine.test.ts`:

```ts
it('turns a fetch into a measured bar', () => {
  const s = statusFor({ working: { phase: 'fetching', done: 159 * 1024 * 1024, total: 310 * 1024 * 1024, memberIndex: 1, memberCount: 2 } })!
  expect(s.progress).toEqual({ done: 159 * 1024 * 1024, total: 310 * 1024 * 1024, memberIndex: 1, memberCount: 2 })
  expect(s.text).toContain('model part 1 of 2')
})

it('says nothing about parts when np named none', () => {
  const s = statusFor({ working: { phase: 'fetching', done: 1024, total: null, memberIndex: null, memberCount: null } })!
  expect(s.text).not.toContain('part')
  expect(s.progress?.total).toBeNull()   // Review Focus 1: unknown size stays unknown
})

it('announces the model in np’s numbers', () => {
  expect(modelReadySentence({ id: 'embeddinggemma-2', bytesTotal: 309855456, digestVerified: true }))
    .toBe('model ready · embeddinggemma-2 · 295 MiB · keyword and meaning search both on now')
})

it('omits a size np did not total up', () => {
  expect(modelReadySentence({ id: 'm', bytesTotal: null, digestVerified: null })).toBe(
    'model ready · m · keyword and meaning search both on now',
  )
})
```

- [ ] **Step 5: Run** `npx vitest run src/lib/statusLine.test.ts` (expect failures), then implement `Progress` on `Status`, the member suffix, and `modelReadySentence`. MiB is 1024·1024 and stays integer.
- [ ] **Step 6: Draw it** in `StatusLine.svelte`: a `<progress>` when `progress?.total` is a number (value/max), and a CSS-striped `.bar-indeterminate` div when it is `null`; both get `aria-label` from the status text. No animation that advances on its own.
- [ ] **Step 7: Prove it end to end** in `App.smoke.test.ts`: drive fake-np with `progress_counters` lines carrying `bytes_done`/`bytes_total`/`members_*`, assert a `progress` element appears with a value at the named percentage, then answer the embed with np's `fetched` object and assert the banner text from Task 5 (skip that assertion if Task 5 has not landed; keep this assertion to the bar).
- [ ] **Step 8: Run** `npx vitest run` (exit code 0) and `npm run build`; commit — `feat: a download bar that only moves on bytes np reported`.

---

### Task 5: The consent card, and "model ready" that stays

**Files:**
- Modify: `src/lib/consentHelp.ts` (new), `src/App.svelte`, `src/lib/statusLine.ts` (banner text reuse)
- Test: `src/lib/consentHelp.test.ts` (new), `src/App.smoke.test.ts`

**Interfaces:**
- Consumes: `ipc.embeddingsHelp()` → `ConsentHelp`; `IngestEvent.embedded.fetched` from Task 4.
- Produces:
  ```ts
  /** One paragraph per gap, in Jott's voice, plus what to copy. Never advice for someone else's provider. */
  export function cardSentences(help: ConsentHelp): string[]
  /** Text the Copy button puts on the clipboard: the lines np needs, or the path when there are no lines. */
  export function cardClipboard(help: ConsentHelp): string
  ```

- [ ] **Step 1: Write the failing `consentHelp` tests**, covering: `providerMissing` yields a sentence naming what Jott wrote (`models.auto_fetch`) and the file it wrote it to, np's requirement, and a link to note-processor#7; `modelMissing` names only the model line; `providerOther` yields exactly one sentence quoting np with **no** instruction to change anything and no clipboard lines (Review Focus 3); `ready` yields no card.
- [ ] **Step 2: Run** `npx vitest run src/lib/consentHelp.test.ts` (expect "cardSentences is not a function"), then implement the module.
- [ ] **Step 3: Render it in `App.svelte`** as a card that replaces the embeddings-offer block when a consent answer is on record and `gap !== 'ready'`: sentences, a `<code>` block of `linesToAdd`, **Copy lines**, **Show np config file** (via `revealItemInDir` from `@tauri-apps/plugin-opener`, which `opener:default` already grants — Task 7 uses the same call for the log), **Try again** (calls the existing `ingestAll`), and a link to `https://github.com/Ex-Bot-01/note-processor/issues/7`. Add a smoke test: after answering *Allow*, with fake-np reporting `disabled { key: "embeddings.provider" }`, the card is on screen containing `auto_fetch` and `provider = "local-model"`.
- [ ] **Step 4: Write the failing banner tests** in `App.smoke.test.ts`: when an embed event arrives with `fetched: { id: 'embeddinggemma-2', bytesTotal: 309855456, digestVerified: true }`, a `[data-testid="model-ready"]` element shows `model ready`; it is still there after a later unrelated event (a re-list) and disappears when the person opens a note or clicks it (their next action). When an embed event carries `fetched: null`, no banner appears.
- [ ] **Step 5: Implement** the banner in `App.svelte` (state cleared by the next user action, not by a timer), with tone `neutral` and a dismiss affordance.
- [ ] **Step 6: Review Focus 4:** add a test that an embed event whose run went offline (`offline: true`, `fetched: null`) leaves the "offline — keyword search only" wording in place and shows no banner.
- [ ] **Step 7: Run** `npx vitest run` (exit code 0), `npx svelte-check --tsconfig ./tsconfig.app.json` (0 errors), commit — `feat: consent you can see through, and a download that says it finished`.

---

### Task 6: A notification for the person who alt-tabbed

**Files:**
- Modify: `package.json` (+ `@tauri-apps/plugin-notification`), `apps/shell/Cargo.toml`, `apps/shell/src/main.rs`, `apps/shell/capabilities/default.json`
- Modify: `src/App.svelte`
- Test: `src/lib/notify.test.ts` (new)

**Interfaces:**
- Consumes: the banner trigger from Task 5.
- Produces:
  ```ts
  /** Notify only when nobody is looking: no focus, or a webview that cannot notify at all. */
  export function shouldNotify(focused: boolean, permission: 'granted' | 'denied' | 'not-determined'): boolean
  /** The OS line — short, no size promises beyond np's numbers. */
  export function notifyBody(modelId: string): string
  ```

- [ ] **Step 1: Write the failing tests** for both functions: `shouldNotify(true, 'granted')` is false; `shouldNotify(false, 'denied')` is false; `shouldNotify(false, 'not-determined')` is false until permission is granted (permission request stays in the component); `shouldNotify(false, 'granted')` is true.
- [ ] **Step 2: Run** `npx vitest run src/lib/notify.test.ts` (expect failure), implement, pass.
- [ ] **Step 3: Wire it**: add the npm package and `tauri-plugin-notification = "2"`, init in `main.rs` next to the other plugins, add `"notification:default"` to capabilities; in `App.svelte`, on the model-ready transition ask for permission once if `not-determined`, then notify when `shouldNotify(await getCurrentWindow().isFocused(), permission)` says so — wrapped so a rejection leaves only the banner (Review Focus 5).
- [ ] **Step 4: Verify the shell still type-checks for Windows**: from `apps/shell`, `PATH=/tmp/winres-stub:$PATH cargo check --target x86_64-pc-windows-msvc` (expect success), and `npm run build`.
- [ ] **Step 5: Commit** — `feat: tell me when the model is ready, but only when I am not looking`.

---

### Task 7: Open the folder with the log selected

**Files:**
- Modify: `src/lib/diagnostics.ts`, `src/lib/Diagnostics.svelte`
- Test: `src/lib/diagnostics.test.ts`, `src/App.smoke.test.ts`

**Interfaces:**
- Consumes: `RuntimeInfo.logPath`.
- Produces:
  ```ts
  /** The thing to reveal: the log file itself, so the file manager selects it. `null` when there is no log. */
  export function revealTarget(runtime: RuntimeInfo | null): string | null
  ```

- [ ] **Step 1: Write the failing tests**: `revealTarget` returns the log path unchanged (not its folder), and `null` when `logPath` is `null` or the runtime snapshot is missing.
- [ ] **Step 2: Run** `npx vitest run src/lib/diagnostics.test.ts` (expect failure), implement, pass.
- [ ] **Step 3: Replace** `openPath(folder)` with `revealItemInDir(target)` in `Diagnostics.svelte`; rename the button to **Show log file**; keep it disabled with a tooltip when `revealTarget(...)` is `null`. Change the failure fallback from the clipboard textarea to a line naming the path with a **Copy path** button, and leave the existing clipboard-failure textarea for the *Copy details* path only.
- [ ] **Step 4: Smoke test**: stub `@tauri-apps/plugin-opener` so `revealItemInDir` records its argument; click **Show log file**; assert it was called with the full `jott.jsonl` path, and that no call happens when `logPath` is null.
- [ ] **Step 5: Run** `npx vitest run` (exit code 0) and commit — `fix: Show log file reveals the file, not a box of text`.

---

### Task 8: Say it in the docs, ship it as 0.0.14

**Files:**
- Modify: `docs/testing-on-windows.md`, `docs/release-notes.md`, `apps/shell/Cargo.toml`, `apps/shell/tauri.conf.json`, `apps/shell/Cargo.lock`
- Test: the full gate set

- [ ] **Step 1:** Add a §2b-iv to `docs/testing-on-windows.md`: what the consent card now says, that Jott writes only `models.auto_fetch` and why (np#7), where the bar appears, what "model ready" means, and that **Show log file** opens Explorer with the file selected.
- [ ] **Step 2:** Rewrite `docs/release-notes.md` for 0.0.14 from the user's point of view, naming np#6 and np#7 as the two things Jott asked for instead of working around.
- [ ] **Step 3:** Bump to `0.0.14` in the three version places; if the Rust passing-test count moved, raise the floor in `.github/workflows/build.yml` to just under it and quote both numbers in the commit message.
- [ ] **Step 4: Full gates**, quoting counts: `PATH=$HOME/.cargo/bin:/usr/bin:/bin cargo test -p jott-core --no-fail-fast`; `cargo clippy -p jott-core --all-targets -- -D warnings`; `npm run test` (exit code); `npm run build`; `npx svelte-check --tsconfig ./tsconfig.app.json`; the Windows type-check from Task 6 Step 4.
- [ ] **Step 5: Commit** — `release: 0.0.14 — consent you can see, progress you can watch`.
