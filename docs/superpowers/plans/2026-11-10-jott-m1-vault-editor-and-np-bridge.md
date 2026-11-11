# Jott M1 — vault, editor, and the `np` bridge

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Jott opens a folder of markdown notes, lets you write and save them, and quietly keeps `np`'s index current — indexing and embedding whatever changed, and telling the truth about what it did.

**Architecture:** A Tauri v2 app: Rust owns every file and process interaction (folder walk, note CRUD, one writer `np serve` child plus a lazily-spawned reader child), and the webview only asks for things and listens for events. Saving a note triggers a debounced `scan`, then `embed`, with `locked`/`offline` handled as states rather than failures.

**Tech Stack:** Rust 2021, `tauri` 2, `tokio`, `serde`/`serde_json`, `thiserror`, `sha2`, `toml_edit`, `notify`, `trash`; Svelte 5 + Vite + CodeMirror 6 (`@codemirror/lang-markdown`) for the window; Vitest for UI logic; GitHub Actions producing Windows and Linux artifacts.

**Spec:** [`docs/superpowers/specs/2026-11-10-np-note-editor-design.md`](../specs/2026-11-10-np-note-editor-design.md) — read §5 (backend facts), §7 (architecture), §9–§13 (window, search, ingest, links), §18 (measured answers; authoritative), §20 (ways of working).

## Global Constraints

- Backend floor: **np 0.3.1**, JSON-RPC protocol version **1**. Feature-detect from `initialize`'s `methods` and `features`; never assume a method exists.
- **Never modify `note-processor`.** Found a bug there: file an issue in that repo, work around it here, and if it blocks us, stop and report.
- Transport is stdio NDJSON. **One writer per child's stdin.** Match responses by `id`; a line with no `id` is a notification. `-32700` comes back with `"id": null`.
- **Searches never queue behind embeds**: embeds go to the writer connection, searches to the reader connection (spec §18.1: queued search waited 102.1 s; second-process search answered in 0.00 s).
- `scan` and `embed` take a **workspace path only** — no per-file target (spec §18.2).
- Never trigger the model download without consent. The switch is **`[models] auto_fetch` in np's config file**, not `NP_NO_AUTO_FETCH`, because env is frozen at spawn and config is re-read per request (spec §18.6, §18.10).
- **Write bytes exactly as read.** No CRLF normalisation, no trailing-newline "fixes" — hashes are SHA-256 over raw bytes (spec §18.8).
- Nothing is written outside the vault and the app data dir: `~/.config/jott/` on Linux, `%APPDATA%\jott\` on Windows. `np`'s `INDEX.md` stays visible in the tree and is never edited here.
- `locked`, `offline`, `vectors: false`, `reason` are **states**: they render in the status line, never as an error dialog. A `-32000` refusal shows np's own message verbatim.
- No duration in any user-facing string that was not measured on the machine showing it; estimates come from `np/progress` counters (spec §18.3).
- TDD: a failing test first, then the code. One commit per task, message prefixed `feat:`/`fix:`/`test:`/`ci:`/`docs:`.

## Review Focus

The spec is silent or thin on these, and each one is a way this app could lose a note or lie about the index. Every line gets a test in the task that owns the code.

- **A file changes on disk while it is open** (git checkout, another editor, a sync client): an untouched buffer reloads silently; a dirty buffer keeps your text and says `changed on disk`. → Task 8.
- **Windows specifics**: CRLF must survive a save unchanged; a path longer than 260 characters must still open (`\\?\` prefix), because np does this itself and we must not disagree with it. → Task 8.
- **np is missing, too old, or lacks a feature** (`local_model: false`, no `progress`): name the install command for that platform once, then stop — no silent retry loop, and no promise of semantic search from a build that cannot do it. → Task 2.
- **An embed is interrupted** (app closed, child killed, network gone mid-download): the work stays *owed* rather than becoming *done*, and the next launch re-queues it. The tree must not claim embedded what was never written. → Tasks 7 and 9.
- **Unreadable input in the vault**: a non-UTF-8 `.md`, a directory you cannot read, a 200 MB stray file. It appears as refused with np's reason (or ours), never vanishes from the tree. → Task 7.

---

## File Structure

```
src-tauri/                     Rust core — all file and process I/O lives here
  src/main.rs                  tauri builder, command registration, startup order
  src/ipc.rs                   the entire webview surface: #[tauri::command] fns + event names
  src/np/mod.rs                re-exports; NpError
  src/np/resolve.rs            where np comes from: bundled → NP_PATH → PATH
  src/np/frame.rs              NDJSON framing (one object per line, no embedded newlines)
  src/np/client.rs             one child: single stdin writer, reader loop, id matching, notifications stream
  src/np/methods.rs            typed methods + outcome types; state-vs-fault mapping
  src/np/pool.rs               writer connection + lazily spawned reader connection
  src/vault/store.rs           vaults.json (registered vaults, consent decisions)
  src/vault/walk.rs            folder walk → tree entries; hidden rules; generated-file marking
  src/vault/stamps.rs          per-file stamps: Indexed / Embedded / Waiting / Refused(reason)
  src/vault/files.rs           read/write bytes exactly, create, rename, move, delete to trash
  src/vault/watch.rs           external-change events → ReloadPolicy decision
  src/ingest/queue.rs          debounce → scan → embed; requeue on locked/offline with backoff
  src/ingest/consent.rs        read/write [models] auto_fetch preserving the rest of config.toml
src/                           Svelte UI — renders state, never touches disk
  main.ts  App.svelte
  lib/Tree.svelte  lib/Editor.svelte  lib/StatusLine.svelte  lib/saveState.ts
tests/                         Rust integration tests (in src-tauri/tests/)
  fixtures/np-serve-transcript.ndjson     exists; verbatim np 0.3.1 stdout
  fake_np.rs + tests/fake-np/np.py        scripted stand-in for np, driven by a scenario file
.github/workflows/build.yml    windows-latest + ubuntu-latest artifacts on tag
```

---

### Task 1: Scaffold — Tauri v2 app that builds on Windows and Linux

**Files:**
- Create: `package.json`, `vite.config.ts`, `svelte.config.js`, `index.html`, `src/main.ts`, `src/App.svelte`
- Create: `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `src-tauri/build.rs`, `src-tauri/src/main.rs`, `src-tauri/capabilities/default.json`
- Create: `.github/workflows/build.yml`
- Test: `src-tauri/tests/scaffold.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: workspace where `npm run build` emits `dist/`, `cargo test --manifest-path src-tauri/Cargo.toml` runs, and crate name `jott` with lib target `jott_lib` (later tasks add modules under `src/`).

- [ ] **Step 1:** `npm create vite@latest . -- --template svelte-ts` in the repo root; set `"name": "jott"`; `npm i -D @tauri-apps/cli && npm i @tauri-apps/api`.
- [ ] **Step 2: Write the failing test** — `src-tauri/tests/scaffold.rs`:

```rust
#[test] fn app_builds_headless() { assert!(jott_lib::app_config().is_ok()); }
```

`app_config()` in a new `src-tauri/src/lib.rs` returns `Result<tauri::Config, String>` parsed from `tauri.conf.json`.
- [ ] **Step 3:** Run `cargo test --manifest-path src-tauri/Cargo.toml` — expect FAIL (crate missing `lib.rs` / unresolved import).
- [ ] **Step 4:** Implement the scaffold: `lib.rs` with `pub fn run()` and `pub fn app_config()`; `main.rs` calls `jott_lib::run()`; `tauri.conf.json` pins `productName: "Jott"`, `identifier: "dev.jott.app"`, `build.frontendDist: "../dist"`, `beforeBuildCommand: "npm run build"`, `beforeDevCommand: "npm run dev"`, `devUrl: "http://localhost:5173"`.
- [ ] **Step 5:** Run the test again — expect PASS. Then `npm run build` — expect `dist/` produced with no errors.
- [ ] **Step 6:** `.github/workflows/build.yml`: `on: push: tags: ["v*"]`, matrix `[windows-latest, ubuntu-latest]`, steps: checkout → node 22 → `npm ci && npm run build` → rust stable → `cargo tauri build` (via `tauri-apps/tauri-action@v0`) → `actions/upload-artifact@v4` with name `jott-{os}`. On Linux install the deps `libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev patchelf`.
- [ ] **Step 7:** Commit `feat: scaffold Jott as a Tauri v2 app with Windows and Linux builds`.
- [ ] **Step 8 — spike hand-off (throwaway, and labelled as one).** Tag `v0.0.0-spike`, let CI publish both artifacts, and ask for the one thing only their laptop can answer: does a window appear on Windows, and does a line written from Rust into the webview arrive *promptly* rather than in a lump? That is spec §15's first risk (WebView2 buffering), answered here for the cost of one download instead of after ten tasks.

> The window cannot be *seen* on this build box (`XDG_SESSION_TYPE=tty`). Green here means "compiles and its config parses"; it never means "works", and no task below gets to claim that.

### Task 2: Find np, shake hands, and say what is missing

**Files:**
- Create: `src-tauri/src/np/mod.rs`, `src-tauri/src/np/resolve.rs`, `src-tauri/src/np/methods.rs` (handshake types only)
- Test: `src-tauri/tests/np_resolve.rs`, `tests/fake-np/np.py`

**Interfaces:**
- Consumes: nothing.
- Produces:
  `pub enum NpSource { Bundled(PathBuf), Env(PathBuf), Path(PathBuf) }`
  `pub struct NpInfo { pub source: NpSource, pub path: PathBuf, pub version: String, pub protocol: u8, pub methods: Vec<String>, pub features: NpFeatures, pub data_dir: PathBuf, pub progress: bool }`
  `pub struct NpFeatures { pub local_model: bool, pub fetch: bool, pub vision: bool }`
  `pub fn resolve_np(app_dir: &Path) -> Result<NpSource, NpMissing>` and
  `pub enum NpMissing { NotFound, TooOld { found: String }, NoProtocol }` with `fn install_hint(&self, os: Os) -> String`.
  `pub fn has_feature(info: &NpInfo, m: &str) -> bool`.

- [ ] **Step 1: Failing test** — in a temp dir tree with an executable `bin/np`, `resolve_np` finds it via `NP_PATH` when set, otherwise via `PATH`, otherwise `Err(NpMissing::NotFound)`; and `install_hint(Os::Windows)` contains `np-0.3.1-x86_64-pc-windows-gnu-cross.zip` while the Linux hint contains `sha256sum -c SHA256SUMS.txt`.
- [ ] **Step 2:** Run `cargo test --manifest-path src-tauri/Cargo.toml np_resolve` — expect FAIL.
- [ ] **Step 3:** Implement `resolve.rs`. Order: `<app_dir>/np{,.exe}` → `$NP_PATH` (refused with `NpMissing::NotFound` plus its own hint if the path does not exist — a bad override must not silently fall through to PATH) → `PATH` lookup. Windows executable suffix handled by one helper, not sprinkled.
- [ ] **Step 4:** Add `tests/fake-np/np.py`: reads NDJSON from stdin; for `initialize` replies with the fixture's handshake line (np 0.3.1, all seven methods, `progress: true`) and echoes scenario-driven replies for other methods from a JSON file named by `FAKE_NP_SCENARIO`; exits 0 at EOF. Make it executable and note in its header that it exists so protocol behaviour is testable without the real binary.
- [ ] **Step 5:** Test the handshake against the fake np: `initialize` with `notifications: true` parses into `NpInfo`; a build reporting `"np": "0.2.3"` yields `TooOld { found: "0.2.3" }`; a reply missing `methods` yields `NoProtocol`. Floor is `0.3.1`, compared numerically per component, never by string.
- [ ] **Step 6:** Run tests — expect PASS. Commit `feat: resolve np and read its handshake`.

### Task 3: NDJSON client — one writer, id matching, notifications as a stream

**Files:**
- Create: `src-tauri/src/np/frame.rs`, `src-tauri/src/np/client.rs`
- Test: `src-tauri/tests/np_client.rs`

**Interfaces:**
- Consumes: `NpSource` (Task 2).
- Produces:
  `pub struct NpClient { .. }` with
  `pub async fn spawn(np: &Path) -> Result<NpClient, NpError>`
  `pub async fn request(&self, method: &str, params: Value) -> Result<Value, NpFault>`
  `pub fn notifications(&self) -> broadcast::Receiver<ProgressNote>`
  `pub async fn shutdown(self)` (sends `shutdown`, then closes stdin; EOF exit 0 is success)
  `pub struct NpFault { pub code: i32, pub message: String }` (`-32700 | -32600 | -32601 | -32602 | -32000 | -32603`)
  `pub struct ProgressNote { pub command: String, pub phase: String, pub request: Option<i64>, pub since_ms: u64, pub extra: Value }`

- [ ] **Step 1: Failing test — replay the fixture.** Feed `tests/fixtures/np-serve-transcript.ndjson` line by line into the reader over a pipe and assert: 22 lines parsed; every line with an `id` resolves to a response or fault; exactly the notification lines land on `notifications()`; the `-32700` fault parses with `request: None` rather than panicking.
- [ ] **Step 2:** Run `cargo test np_client` — expect FAIL.
- [ ] **Step 3:** Implement `frame.rs`: `encode(method, id, params) -> String` (one line; reject a serialized value containing `\n` before sending), `decode(&str) -> Frame` where `enum Frame { Response{i64, Value}, Fault(FaultFrame), Note(ProgressNote), Ignored }`. Blank lines are ignored, per np's documented behaviour.
- [ ] **Step 4:** Implement `client.rs`: spawn with `Stdio::piped()` for stdin/stdout and **stderr piped and drained by a task that discards** (an undrained stderr pipe slows np on Windows); one `mpsc` channel feeding the single stdin writer; an oneshot per request registered in a `Mutex<HashMap<i64, Oneshot>>`; reader loop dispatches by `id`, forwards anything without `id` to a `broadcast` channel. Requests are answered in order but notifications interleave anywhere — never assume "next line is my answer".
- [ ] **Step 5:** Test against fake np: two concurrent `request()` calls receive the right bodies (ids do not cross); a notification arriving mid-flight does not consume another request's response; `shutdown()` returns and the child exits 0.
- [ ] **Step 6:** Run tests — expect PASS. Commit `feat: speak NDJSON to np serve with one writer and id matching`.

### Task 4: Typed methods, and states that are not failures

**Files:**
- Create: `src-tauri/src/np/methods.rs` (extend), add `src-tauri/src/np/outcomes.rs`
- Test: `src-tauri/tests/np_methods.rs`

**Interfaces:**
- Consumes: `NpClient::request` (Task 3).
- Produces:
  `pub async fn initialize(&NpClient, workspace: &Path, notifications: bool) -> Result<NpInfo, NpError>`
  `pub async fn scan(&NpClient, workspace: &Path) -> Result<ScanOutcome, NpFault>`
  `pub async fn embed(&NpClient, workspace: &Path) -> Result<EmbedOutcome, NpFault>`
  `pub async fn search(&NpClient, workspace: &Path, query: &str, limit: u8, mode: Mode) -> Result<SearchAnswer, NpFault>`
  `pub async fn models_list(&NpClient, workspace: &Path) -> Result<Vec<ModelRow>, NpFault>`
  `pub enum ScanOutcome { Applied(ScanReport), Locked { waited_ms: u64 } }`
  `pub enum EmbedOutcome { Applied(EmbedReport), Locked { waited_ms: u64 }, Offline { reason: String }, Disabled { hint: String, model_id: String } }`
  `pub struct SearchAnswer { pub hits: Vec<Hit>, pub mode: Mode, pub vectors: bool, pub offline: bool, pub reason: Option<String> }`
  `pub struct Hit { pub path: String, pub snippet: Option<String>, pub kind: String, pub score: f64, pub bm25: Option<f64>, pub cosine: Option<f64> }`

- [ ] **Step 1: Failing test — every fixture line maps to the right variant.** Using scenario files that replay the captured documents: a scan result with `"locked": true, "waited_ms": 2000` → `ScanOutcome::Locked`, **not** an error; an embed result carrying `outcome.configuration-disabled` with its `hint` → `EmbedOutcome::Disabled`; a search result with `"vectors": false, "mode": "keyword"` keeps both fields; `-32601` and both `-32602` flavours map to `NpFault` with np's message preserved verbatim.
- [ ] **Step 2:** Run `cargo test np_methods` — expect FAIL.
- [ ] **Step 3:** Implement with serde structs mirroring np's documents exactly (unknown fields ignored, missing optional fields `None`, never defaulted to a plausible zero). `Mode` is `Hybrid | Semantic | Keyword` serialising to np's lowercase labels; `Hit` deliberately omits `title` and line numbers because np sends neither.
- [ ] **Step 4:** Test the two behaviours that bite: a hit whose `bm25` is `null` parses as `None` (a semantic-only hit must stay distinguishable from a zero score); a snippet containing `\n` survives one round trip through encode/decode without breaking framing.
- [ ] **Step 5:** Run tests — expect PASS. Commit `feat: typed np methods where locked and offline are states`.

### Task 5: Connection pool — writer now, reader only when search needs it

**Files:**
- Create: `src-tauri/src/np/pool.rs`
- Test: `src-tauri/tests/np_pool.rs`, extend `tests/fake-np/np.py` with a `sleep_ms` scenario action

**Interfaces:**
- Consumes: `NpClient`, `NpSource`.
- Produces:
  `pub struct NpPool { .. }` with `pub fn new(source: NpSource, info: NpInfo) -> NpPool`,
  `pub async fn writer(&self) -> Result<&NpClient, NpError>`,
  `pub async fn reader(&self) -> Result<&NpClient, NpError>` (spawns the second child on first call),
  `pub async fn embed(&self, workspace: &Path) -> Result<EmbedOutcome, NpFault>` (writer),
  `pub async fn scan(&self, workspace: &Path) -> Result<ScanOutcome, NpFault>` (writer),
  `pub async fn search(&self, workspace: &Path, q: &str, limit: u8) -> Result<SearchAnswer, NpFault>` (reader),
  `pub fn progress(&self) -> broadcast::Receiver<ProgressNote>`,
  `pub async fn close(self)` — shuts both children down.

- [ ] **Step 1: Failing test:** after construction and one `scan()`, exactly one fake-np process exists; after the first `search()`, two exist; a second `search()` does not spawn a third.
- [ ] **Step 2: Failing test — search must not wait for embed.** With a scenario where `embed` sleeps 1500 ms, start an embed, then call `search` at +50 ms and assert it returns in under 300 ms (spec §18.1 measured 102 s when this was done on one connection).
- [ ] **Step 3:** Run `cargo test np_pool` — expect FAIL.
- [ ] **Step 4:** Implement: the reader is created inside a `tokio::OnceCell`; writer and reader each own their child; `progress()` merges both children's notification streams into one broadcast (tag each note with which connection it came from, so an embed progress line can never be mistaken for a search one).
- [ ] **Step 5:** Test crash recovery: kill the fake reader child mid-session; the next `search()` respawns and answers; the writer is untouched. Commit `feat: keep embedding off the search path with two np connections`.

### Task 6: Vault registration and consent, in the app data dir

**Files:**
- Create: `src-tauri/src/vault/store.rs`
- Test: `src-tauri/tests/vault_store.rs`

**Interfaces:**
- Consumes: nothing.
- Produces:
  `pub struct VaultStore(PathBuf)` rooted at `app_data_dir()` (`~/.config/jott`, `%APPDATA%\jott`)
  `pub struct VaultEntry { pub path: PathBuf, pub last_opened: SystemTime, pub consent: Consent }`
  `pub enum Consent { Undecided, Granted, Declined }` (per vault, for the model download)
  `fn load(&self) -> Vec<VaultEntry>`, `fn register(&self, p: &Path) -> Result<(), StoreError>`, `fn set_consent(&self, p: &Path, c: Consent) -> Result<(), StoreError>`, `fn last_opened(&self) -> Option<PathBuf>`
  Persisted as `vaults.json`. Absolute paths live **only** here.

- [ ] **Step 1: Failing test:** registering a vault twice does not duplicate; `last_opened` returns the most recent; an unreadable/corrupt `vaults.json` is renamed to `vaults.json.corrupt-<unix>` and rebuilt empty rather than failing startup (a note app that cannot open is worse than one that forgot which folder was last).
- [ ] **Step 2:** Run `cargo test vault_store` — expect FAIL. Implement, then pass.
- [ ] **Step 3: Failing test for the data dir itself:** on Linux with `XDG_DATA_HOME` set, the store lands there; with it unset, `~/.local/share/jott`; and nothing is ever written inside the vault by this module (assert no new files under the vault path).
- [ ] **Step 4: Failing test for picking a folder that sits inside another vault** (spec §8.2): with `<vault>/INDEX.md` present, `register(<vault>/sub)` returns `RegisterWarning::NestedVault { parent }` — a warning the UI shows and the user may accept, never a silent acceptance and never a refusal.
- [ ] **Step 5:** Run tests — expect PASS. Commit `feat: register vaults, remember consent, warn on nested vaults`.

### Task 7: Folder walk, tree entries, and per-file stamps

**Files:**
- Create: `src-tauri/src/vault/walk.rs`, `src-tauri/src/vault/stamps.rs`
- Test: `src-tauri/tests/vault_walk.rs`, `tests/vault_stamps.rs`

**Interfaces:**
- Consumes: `VaultStore` (Task 6).
- Produces:
  `pub struct TreeEntry { pub rel: String, pub kind: EntryKind, pub size: u64, pub mtime: SystemTime, pub state: StampState }`
  `pub enum EntryKind { Folder, Note, Image, Attachment, Generated }` (Generated = `INDEX.md`/`INDEX-*.md`)
  `pub enum StampState { Embedded, Indexed, Waiting, Refused(String) }`
  `pub fn walk(root: &Path, stamps: &Stamps) -> Result<Vec<TreeEntry>, WalkError>`
  `pub struct Stamps(..)` with `fn load(vault: &Path) -> Result<Stamps, StoreError>`, `fn mark_indexed(&mut self, rel: &str, hash: &str)`, `fn mark_embedded(&mut self, rel: &str, hash: &str)`, `fn mark_refused(&mut self, rel: &str, reason: &str)`, `fn state_of(&self, rel: &str, current_hash: &str) -> StampState`, `fn save(&self) -> Result<(), StoreError>` persisted as `state.json` in the app data dir.
  Hashes are SHA-256 of file bytes — the same definition np uses (spec §18.8).

- [ ] **Step 1: Failing tests for the walk**, on a temp vault containing `notes/a.md`, `INDEX.md`, `img.png`, `data.pdf`, `.git/config`, `.hidden.md`, and a file with invalid UTF-8:
  dot-files and dot-folders are excluded (the root itself never is, matching np); `INDEX.md` and `INDEX-2a-z.md` come back as `Generated`; `.png`/`.jpg`/`.jpeg`/`.gif`/`.webp`/`.bmp` are `Image`, `.pdf` is `Attachment`; the invalid-UTF-8 file appears as a `Note` with `Refused("not-utf8")` and does **not** abort the walk; an unreadable subdirectory yields one `Refused("unreadable-dir")` entry, not an error.
- [ ] **Step 2:** Run `cargo test vault_walk` — expect FAIL. Implement (use `walkdir`, sort by `rel` bytes so order is stable across platforms), then pass.
- [ ] **Step 3: Failing tests for stamps:** unknown path + never seen → `Waiting`; hash equal to the stored indexed hash → `Indexed`; equal to stored embedded hash → `Embedded`; differing from both → `Waiting`; `Refused` survives a reload and is cleared by a later successful index.
- [ ] **Step 4:** Run `cargo test vault_stamps` — expect FAIL, then implement (a `BTreeMap<String, FileStamp>`; save writes atomically: temp file in the same directory + rename) and pass.
- [ ] **Step 5:** Commit `feat: walk the vault into a tree with honest per-file stamps`.

### Task 8: Byte-exact note I/O, renames, trash, and external changes

**Files:**
- Create: `src-tauri/src/vault/files.rs`, `src-tauri/src/vault/watch.rs`
- Test: `src-tauri/tests/vault_files.rs`, `tests/watch_policy.rs`

**Interfaces:**
- Consumes: `Stamps` (Task 7).
- Produces:
  `pub fn read_note(root: &Path, rel: &str) -> Result<Vec<u8>, FileError>`
  `pub fn write_note(root: &Path, rel: &str, bytes: &[u8]) -> Result<(), FileError>`
  `pub fn create_note(root: &Path, rel: &str) -> Result<(), FileError>` (refuses if it exists)
  `pub fn rename(root: &Path, from: &str, to: &str) -> Result<RenameReport, FileError>` with
  `pub struct RenameReport { pub incoming_links: Vec<LinkRef> }` and
  `pub struct LinkRef { pub src: String, pub line: u32, pub target: String }` — **detection only in M1**, no rewriting (Plan 2, spec §13); the type exists now so Plan 2 does not have to reshape this task
  `pub fn move_entry`, `pub fn delete_to_trash(root: &Path, rel: &str) -> Result<Trashed, FileError>` (`Trashed::Os | Trashed::AppDir(PathBuf)`)
  `pub enum ReloadPolicy { ReloadSilently, KeepAndWarn, NoChange }` and `pub fn decide(buffer_dirty: bool, disk_mtime: SystemTime, last_read: SystemTime, buffer_hash: &str, disk_hash: &str) -> ReloadPolicy`
  `pub struct PathGuard` — every entry point rejects a `rel` that escapes the root (`..`, absolute paths, symlink out).

- [ ] **Step 1: Failing tests for byte fidelity:** a file containing `line one\r\nline two\r\n` round-trips through `read_note`/`write_note` with an identical SHA-256; a file with no trailing newline keeps none; a UTF-8 BOM survives. These are the Windows guarantees (spec §18.8).
- [ ] **Step 2: Failing test for long paths:** `to_open_path` on Windows adds `\\?\` once and only for paths ≥ 260 chars; on Linux the path is untouched. Written as a pure string rule (`extended_length_prefix`) so it is tested on every platform instead of hiding behind an env switch inside a `cfg(windows)` branch. **Measured correction:** np 0.3.1 has no such rule to mirror — no `\\?\`, no `MAX_PATH` anywhere in its crates — so this is Jott's own, and the Windows pass (§19) still has to confirm a deep vault opens.
- [ ] **Step 3: Failing tests for containment:** `rel` values `../escape.md`, `/etc/passwd`, and a symlink inside the vault pointing outside are all refused with `FileError::EscapesRoot`.
- [ ] **Step 4:** Run `cargo test vault_files` — expect FAIL. Implement (read/write with `File::open`/`create` on raw bytes; `trash` for deletion, falling back to `<app_data>/trash/<unix>-<name>` and reporting which happened), then pass.
- [ ] **Step 5: Failing tests for `decide`:** identical hashes → `NoChange`; disk changed with a clean buffer → `ReloadSilently`; disk changed with a dirty buffer → `KeepAndWarn`. Wire a `notify` watcher on the vault in `watch.rs` that emits `vault://changed(rel)` events and does **not** react to our own writes within 200 ms of them.
- [ ] **Step 6:** Run tests — expect PASS. Commit `feat: byte-exact note IO, guarded paths, and external-change policy`.

### Task 9: The ingest queue — save changes bytes, so index then embed

**Files:**
- Create: `src-tauri/src/ingest/queue.rs`, `src-tauri/src/ingest/consent.rs`
- Test: `src-tauri/tests/ingest_queue.rs`, `tests/np_consent.rs`

**Interfaces:**
- Consumes: `NpPool` (Task 5), `Stamps` (Task 7). The registry's `VaultStore::set_consent` (Task 6) is
  written by the shell when it calls `IngestQueue::set_consent`: `spawn`'s signature has no room for the
  registry, and a queue that reached into app state on its own would be two owners of one decision.
- Produces:
  `pub struct IngestQueue` with `pub fn spawn(pool: Arc<NpPool>, stamps: Arc<Mutex<Stamps>>, vault: PathBuf) -> IngestQueue`,
  `pub fn note_saved(&self, rel: &str)`, `pub fn flush_now(&self)`, `pub fn set_consent(&self, consent: Consent)`, `pub async fn drain(self)`
  `pub enum IngestEvent { Working { phase: String, done: u64, total: Option<u64> }, Indexed(ScanReport), Embedded(EmbedReport), Behind { reason: StateReason, retry_in_ms: u64 }, NeedsConsent { model_id: Option<String>, bytes: Option<u64>, data_dir: PathBuf }, Refused { message: String }, Disabled { key: Option<String>, model_id: Option<String>, hint: Option<String> } }` (emitted as `ingest://event`)

  Three shapes differ from the brief, each for a reason measured against np 0.3.1: `Arc<NpPool>`,
  because `drain` has to force-kill a child that is still busy at its cap (§11) and a pool moved into
  the task leaves nobody able to do that; optional `model_id`/`bytes`, because nothing in 0.3.1 reports
  an artifact size (`models.list` rows carry none, measured) and np's config may name no model at all —
  §4 forbids inventing either to fill a dialog; and the `Disabled` arm, because np's configuration state
  carries no sentence for `Refused.message` unless it sent a `hint`, and filing a settled configuration
  under "refused" would make a setting look like a failure.
  `pub enum StateReason { Locked { waited_ms: u64 }, Offline }`
  `pub fn set_auto_fetch(config_path: &Path, allow: bool) -> Result<(), ConsentError>` (via `toml_edit`; preserves comments and unknown keys), `pub fn auto_fetch_allowed(config_path: &Path) -> bool`.

- [ ] **Step 1: Failing test — the happy path.** With fake np recording calls: `note_saved("a.md")` then a 1.2 s wait produces exactly one `scan` followed by one `embed`; ten `note_saved` calls inside the debounce window produce **one** scan+embed pair (np takes a workspace only, so batching is the only honest shape — spec §18.2).
- [ ] **Step 2: Failing test — no work when nothing changed.** A `note_saved` for a file whose hash equals the stored embedded hash produces no np calls at all.
- [ ] **Step 3: Failing tests — states requeue rather than fail.** Scenario `locked` on scan: `IngestEvent::Behind { reason: Locked { .. }, retry_in_ms }` is emitted, stamps stay `Waiting`, and a retry happens (backoff 2 s → 5 s → 15 s, capped at 60 s). Scenario `offline` on embed: `Behind { Offline }`, stamps keep the file as `Indexed` — not `Embedded`. A `-32000` refusal emits `Refused` with np's message unchanged.
- [ ] **Step 4: Failing tests for consent.** With `models_list` reporting no artifact and consent `Undecided`: no `embed` is sent, `NeedsConsent` is emitted once per vault per session. On `Declined`: `set_auto_fetch(path, false)` is written and subsequent saves still index (keyword search must keep working). On `Granted`: `set_auto_fetch(path, true)` then `embed`. If `config.toml` already exists with comments and other keys, the diff touches only `[models] auto_fetch`.
- [ ] **Step 5:** Run `cargo test ingest_queue np_consent` — expect FAIL. Implement (`tokio::time::timeout`-style debounce on an mpsc; a `VecDeque` of pending rels drained in save order; stamps updated from reports, never optimistically), then pass.
- [ ] **Step 6: Failing→passing test for shutdown:** `drain()` finishes an in-flight scan+embed and saves stamps; if the child is still busy after 5 s it is shut down anyway and the file stays `Waiting` (spec §11: closing never blocks on embedding).
- [ ] **Step 7:** Commit `feat: ingest on save with consent-gated embedding and state-aware retries`.

### Task 10: The window — tree, editor, status line

**Files:**
- Create: `src/ipc.rs`, `src/App.svelte`, `src/lib/Tree.svelte`, `src/lib/Editor.svelte`, `src/lib/StatusLine.svelte`, `src/lib/saveState.ts`
- Test: `src/lib/saveState.test.ts` (Vitest), `src-tauri/tests/ipc_surface.rs`

**Interfaces:**
- Consumes: everything above.
- Produces (Tauri commands, camelCase on the JS side): `listVault()`, `openVault(path)`, `pickVault()` (native dialog), `readNote(rel) -> { text, hash }`, `saveNote(rel, text)`, `createNote(rel)`, `renameNote(from, to)`, `deleteEntry(rel)`, `searchVault(query, limit)`, `ingestAll()`, `setConsent(granted)`; events `vault://changed`, `ingest://event`, `np://missing`.
  `pub fn saveState(input: { dirty: boolean; diskChanged: boolean; saving: boolean; savedHash?: string; currentHash: string }): SavePhase` where `SavePhase = 'clean' | 'dirty' | 'saving' | 'saved' | 'conflict'`.

- [ ] **Step 1: Failing Vitest tests for `saveState`:** unmodified buffer + Ctrl+S → `clean` (no write, no ingest); typing → `dirty`; save in flight → `saving`; hash match → `saved`; disk changed while dirty → `conflict`. This is the one place "did the content change?" is decided (spec §11).
- [ ] **Step 2:** Run `npm run test -- run` — expect FAIL. Implement `saveState.ts`, pass.
- [ ] **Step 3: Failing Rust test for the IPC surface:** `readNote` refuses an escaping `rel` with the same `EscapesRoot` error as Task 8 (the webview is not a trusted caller); `saveNote` returns the new hash so the front end can compute `saved`; `searchVault` routes to the pool's reader connection — assert by scenario that calling it during a sleeping embed returns promptly.
- [ ] **Step 4:** Implement `ipc.rs` commands and event bridging (each `IngestEvent` becomes an `ingest://event` payload; `NpMissing` becomes `np://missing` with `install_hint`).
- [ ] **Step 5:** `Tree.svelte`: entries with their stamp marker (`●` waiting, `·` embedded, `!` refused with the reason in the title attribute), `Generated` entries greyed with a "generated by np" tooltip, dot-folders absent, folders collapsible with the open/closed set kept per session.
- [ ] **Step 6:** `Editor.svelte`: CodeMirror 6 with markdown highlighting, line wrap on, no minimap; Ctrl+S saves; autosave at 2 s idle plus on blur and unload; an unsaved-buffer mirror at `<app_data>/buffers/<hash-of-rel>`, replayed at launch only when the buffer is newer than the file (spec §18.13). Reload-vs-keep comes from Task 8's `decide`, never from component-local logic.
- [ ] **Step 7:** `StatusLine.svelte`: one line from `ingest://event` — `indexed 1.2 s ago`, `embedding 3/7 chunks`, `index behind · retrying in 5 s`, `keyword-only: model not installed [set it up]` — plus `[embed]` calling `ingestAll()`, and the consent prompt rendered once from `NeedsConsent` using the model id, byte count and `data_dir` carried on that event.
- [ ] **Step 8:** Run `npm run test -- run` and `cargo test --manifest-path src-tauri/Cargo.toml` — expect PASS. Commit `feat: tree, editor and status line wired to the ingest queue`.
- [ ] **Step 9 — second hand-off:** publish artifacts and ask them to look at what only a desktop reveals: markdown tinting and cursor behaviour under WebView2, and whether the buffer mirror ever resurrects text they deleted on purpose (spec §15's second risk).

### Task 11: Hand-off build for Windows

**Files:**
- Create: `docs/testing-on-windows.md`, `src-tauri/src/diagnostics.rs`
- Modify: `.github/workflows/build.yml`

**Interfaces:**
- Consumes: Tasks 1–10.
- Produces: a downloadable Windows artifact plus the log Jott writes where they can find it.

- [ ] **Step 1:** `diagnostics.rs`: JSONL to `<app_data>/logs/jott.jsonl`, one record per np request (`method`, `workspace`, `elapsed_ms`, outcome variant, and np's own code/message on a fault), rotating at 2 MiB keeping two files. Failing test first: a fault record contains np's message verbatim and never the vault's file contents.
- [ ] **Step 2:** `build.yml`: keep artifacts for 30 days; name them `jott-windows-x64.msi` and `jott-linux-x64.deb`/`.AppImage`; add `np --version` to the build log so a release proves which backend it was tested against.
- [ ] **Step 3:** `docs/testing-on-windows.md`: what to install (release asset + `np` per [`INSTALL.md`](../../INSTALL.md) §2 Windows), the five things to try (first run, save→embed timing, rename, a file changed by another editor, closing mid-embed), and exactly which file to send back (`%APPDATA%\jott\logs\jott.jsonl`).
- [ ] **Step 4:** Tag `v0.0.1`, confirm both artifacts appear, and hand the link over as the third hand-off: by now the spike answered "does a window work on Windows" (Task 1) and "does the editor behave" (Task 10); this one answers the question np's own author could not — **write-lock contention on Windows** — by running `np scan <vault>` in a terminal while Jott is mid-embed, and checking that Jott reports `index behind · retrying` rather than an error or a frozen window.
- [ ] **Step 5:** Commit `ci: publish Windows and Linux artifacts with a log worth reading`.

---

## Not in this plan

Deferred on purpose, each needing its own plan when it comes: the ⌘K search overlay and pinned-results panel (spec §10); wikilink completion, unresolved-target marks and rename link-rewriting (§13 — Plan 2, and it depends on our own link index, which is why `RenameReport.incoming_links` exists as a stub here); backlinks (waiting on upstream issue #2); the installer/signing story (§20); macOS.
