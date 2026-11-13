# Diagnostics drawer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show, live and inside the window, every request Jott makes to np plus the run's runtime, queue and vault state — without ever putting note text on screen.

**Architecture:** Core owns a fixed-size `Trace` ring beside the existing journal; the call sites that write journal lines push here too, and progress notifications arrive coalesced. `VaultSession::diagnostics()` answers a snapshot so the drawer is never blank when opened mid-run. The Tauri shell only forwards `diagnostics://event` and exposes the command; all logic sits in `jott-core` and pure TypeScript.

**Tech Stack:** Rust (tokio, serde), Svelte 5 runes, Vitest + jsdom.

**Spec:** `docs/superpowers/specs/2026-11-13-diagnostics-drawer-design.md`

## Global Constraints

- No note contents anywhere in this feature: every payload's key set is a closed list pinned by a test, the way
  `RequestRecord::KEYS` pins the journal's.
- np's own fault sentences travel byte for byte (§4); display may truncate with an ellipsis, the stored value never does.
- Never invent a number: an absent elapsed time, count or log size renders as nothing, not zero.
- Command surface lives in `crates/jott-core/src/ipc.rs` and is tested in `crates/jott-core/tests/ipc_surface.rs`; the shell stays thin and is verified with `PATH=/tmp/winres-stub:$PATH cargo check --target x86_64-pc-windows-msvc` from `apps/shell`.
- Frontend logic lives in pure TS modules with Vitest tests, not inside components.
- Run the Rust suite the way CI sees it before pushing: `PATH=$HOME/.cargo/bin:/usr/bin:/bin cargo test -p jott-core`.

## Review Focus

- A 300 MB model fetch makes np emit a progress line per chunk of bytes; the drawer must stay readable and the
  IPC quiet — expect one row per method that updates at most once a second, not one per notification.
- With no np installed at all, the drawer still opens and says `np: not found`; no field is guessed at to fill it.
- When the journal could not be opened (unwritable log folder), the runtime row says the log is unavailable
  instead of naming a file that does not exist.
- A fault sentence in another language or 2 KB long displays truncated with an ellipsis and copies whole.
- An unreadable folder leaves the vault row's counts still adding up to the note total, with the refusal counted
  once and no path beyond what the tree already shows.

---

### Task 1: The trace ring in core

**Files:**
- Modify: `crates/jott-core/src/diagnostics.rs`
- Test: `crates/jott-core/tests/diagnostics_journal.rs` (extend; same subject — what the diagnostics surface holds)

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces:
  - `pub struct TraceEvent { pub at_ms: u64, pub kind: TraceKind, pub method: Option<String>, pub outcome: Option<&'static str>, pub elapsed_ms: Option<u64>, pub detail: Option<String> }` serialized camelCase (`atMs`, `kind`, `method`, `outcome`, `elapsedMs`, `detail`), with `pub const KEYS: &'static [&'static str]`.
  - `pub enum TraceKind { Request, Progress, Connection, Consent, Queue }` serialized lowercase.
  - `pub struct Trace` with `Trace::new(capacity: usize)`, `push(&self, TraceEvent)`, `recent(&self) -> Vec<TraceEvent>` (oldest first), `subscribe(&self) -> tokio::sync::broadcast::Receiver<TraceEvent>`.
  - `impl Default for Trace` — capacity 200.

- [x] **Step 1: Write the failing tests** in `crates/jott-core/tests/diagnostics_journal.rs`: a ring keeps its last N events oldest-first; `subscribe` delivers what is pushed after subscribing and a lagged receiver does not panic; `TraceEvent::KEYS` equals exactly the serialized keys of one event (serialize an event with every field filled and compare `serde_json::Value::as_object().keys()`); no key in that list may be named `text`, `snippet`, `body`, `contents`, `query` or `path`.

- [x] **Step 2: Run to verify failure** — `cargo test -p jott-core --test diagnostics_journal` fails to compile on the missing types.

- [x] **Step 3: Implement** the types above in `diagnostics.rs`, beside `Journal`. The ring is a `Mutex<VecDeque<TraceEvent>>` plus a `broadcast::Sender`; `push` appends, pops the front past capacity, and ignores a send error when nobody is listening.

- [x] **Step 4: Run to verify pass** — same command, all green.

- [x] **Step 5: Commit** `git commit -m "feat: keep this run's backend activity in a ring beside the log"`

---

### Task 2: Feed the trace from where np is talked to

**Files:**
- Modify: `crates/jott-core/src/ingest/queue.rs` (the `record` / `record_fault` helpers, `set_journal`'s neighbour, progress handling in `run_pass`/`embed`, `requeue`, `set_consent`)
- Modify: `crates/jott-core/src/ipc.rs` (`VaultSession`: own `Arc<Trace>`, pass to queue, push search records and connection facts)
- Modify: `crates/jott-core/src/np/pool.rs` (connection open/close notifications are reported through a callback the session supplies — no new dependency from `np` onto `diagnostics`)
- Test: `crates/jott-core/tests/ingest_queue.rs`, `crates/jott-core/tests/ipc_surface.rs`

**Interfaces:**
- Consumes: Task 1's `Trace`, `TraceEvent`, `TraceKind`.
- Produces: `IngestQueue::set_trace(&self, Arc<Trace>)`; `VaultSession::trace(&self) -> Arc<Trace>`; `VaultSession::trace_events(&self) -> broadcast::Receiver<TraceEvent>`.

- [x] **Step 1: Write the failing test** in `ingest_queue.rs`: with a trace attached and a scenario that answers `scan` then refuses `embed` with a fault, `trace.recent()` contains a `Request` for `scan` with `outcome == Some("applied")` and elapsed set, and a `Request` for `embed` with `outcome == Some("fault")`, `method == Some("embed")`, and `detail` equal to np's sentence unchanged.

- [x] **Step 2: Run to verify failure** — `cargo test -p jott-core --test ingest_queue` fails: no trace attached.

- [x] **Step 3: Implement** `set_trace` and push a `Request` event wherever the queue already calls the journal (same elapsed, same outcome word), plus `Consent` on `set_consent` (`detail`: `granted` / `declined`) and `Queue` on `requeue` (`detail`: the reason word).

- [x] **Step 4: Write the failing test for coalescing** in `ingest_queue.rs`: a scenario with three `progress_counters` lines for `embed` across two phases yields at most one `Progress` row per phase change, and never more than one row per method per second (drive it with `backoff_floor`-style test knobs; assert counts, not timing precision).

- [x] **Step 5: Implement** the coalescing in the progress pump: keep `(method, phase, last_emitted_at)` per connection and emit on phase change or after 1 s.

- [x] **Step 6: Write the failing session test** in `ipc_surface.rs`: opening a session with an unreachable np pushes a `Connection` event naming the source kind; `search` through the session pushes a `Request` row for `search`; `trace_events()` delivers an event pushed after subscribing.

- [x] **Step 7: Implement** session ownership (`Arc<Trace>` field, `trace()`, `trace_events()`, push on search and on pool connect/fail) and run both test files green.

- [x] **Step 8: Commit** `git commit -m "feat: every np request, refusal and retry reaches the diagnostics trace"`

---

### Task 3: The snapshot command

**Files:**
- Modify: `crates/jott-core/src/ipc.rs` (`VaultSession::diagnostics`)
- Modify: `crates/jott-core/src/ingest/queue.rs` (shared queue snapshot updated by the worker)
- Modify: `crates/jott-core/src/np/pool.rs` (`connections() -> Connections`)
- Modify: `apps/shell/src/ipc.rs`, `apps/shell/src/main.rs` (thin command + event forwarding)
- Test: `crates/jott-core/tests/ipc_surface.rs`

**Interfaces:**
- Consumes: Task 1/2's `Trace`.
- Produces (all `rename_all = "camelCase"`, all optional fields `skip_serializing_if`):
  - `pub struct Diagnostics { pub runtime: RuntimeInfo, pub recent: Vec<TraceEvent>, pub queue: QueueState, pub vault: VaultCounts }` with `Diagnostics::KEYS`.
  - `pub struct RuntimeInfo { pub np_version: Option<String>, pub np_source: Option<String>, pub np_path: Option<String>, pub protocol: Option<u32>, pub writer: ConnectionState, pub reader: ConnectionState, pub data_dir: String, pub log_path: Option<String>, pub log_bytes: Option<u64>, pub consent: Consent, pub auto_fetch: Option<bool> }` with `RuntimeInfo::KEYS`.
  - `pub enum ConnectionState { Up, Idle, Down }` lowercase.
  - `pub struct QueueState { pub waiting: usize, pub held_for_consent: usize, pub backoff: Option<String>, pub retry_in_ms: Option<u64> }` with `QueueState::KEYS`.
  - `pub struct VaultCounts { pub notes: usize, pub embedded: usize, pub indexed: usize, pub waiting: usize, pub refused: usize, pub warnings: usize }` with `VaultCounts::KEYS`.
  - `NpPool::connections(&self) -> Connections { writer: ConnectionState, reader: ConnectionState }`.

- [x] **Step 1: Write the failing tests** in `ipc_surface.rs`: a session over the stand-in answers `diagnostics()` with `recent` holding the rows Task 2 pushed, `queue.waiting == 0` when idle, and `vault.notes` equal to the note files on disk; serialized keys of each DTO equal its `KEYS`; with np unreachable, `runtime.np_version` is `None` and nothing panics; an unreadable folder counts one warning and leaves `embedded + indexed + waiting + refused == notes`.

- [x] **Step 2: Run to verify failure** — `cargo test -p jott-core --test ipc_surface`.

- [x] **Step 3: Implement** the DTOs, `NpPool::connections` (writer slot alive → `Up`, closed → `Down`; reader never spawned → `Idle`), a `QueueState` kept in the queue's shared state and updated at each cycle/requeue/drain, log size from `Journal` (`log_bytes: None` when the journal is broken, and then `log_path: None` too), and `VaultCounts` from one `walk::walk`.

- [x] **Step 4: Run to verify pass**, plus `cargo clippy -p jott-core --all-targets -- -D warnings`.

- [x] **Step 5: Wire the shell**: `#[tauri::command] pub async fn diagnostics(app: State<'_, App>) -> Result<Diagnostics, IpcError>` registered in `main.rs`, and one forwarding task that subscribes to `VaultSession::trace_events()` on open and emits `diagnostics://event` — same pattern as the existing vault-changed forwarder.

- [x] **Step 6: Verify the shell type-checks** — `cd apps/shell && PATH=/tmp/winres-stub:$PATH cargo check --target x86_64-pc-windows-msvc` finishes without errors.

- [x] **Step 7: Commit** `git commit -m "feat: a snapshot of what the backend is doing, on request"`

---

### Task 4: The drawer

**Files:**
- Create: `src/lib/diagnostics.ts`, `src/lib/diagnostics.test.ts`, `src/lib/Diagnostics.svelte`
- Modify: `src/App.svelte`, `src/lib/About.svelte`, `src/App.smoke.test.ts`
- Test: `crates/jott-core/tests/ipc_surface.rs` (contract test against the TS mirror)

**Interfaces:**
- Consumes: Task 3's serialized DTOs and the `diagnostics://event` payload (`TraceEvent`).
- Produces: `src/lib/diagnostics.ts` exporting `Diagnostics`, `TraceEvent`, `RuntimeInfo`, `QueueState`, `VaultCounts` types plus `formatRow(event: TraceEvent): { time: string; label: string; detail: string }`, `runtimeLines(info: RuntimeInfo): string[]`, `queueLine(state: QueueState): string`, `vaultLine(counts: VaultCounts): string`, `diagnosticsText(d: Diagnostics): string` and `DIAGNOSTICS_KEYS: Record<string, string[]>`.

- [x] **Step 1: Write the failing tests** in `src/lib/diagnostics.test.ts`: `formatRow` renders `embed · applied · 1.9 s` for elapsed 1900 and omits the duration when it is null; a fault
row shows np's sentence truncated at 200 characters with an ellipsis while `diagnosticsText` copies it whole,
including a sentence with non-ASCII text that must not be cut mid-character; `runtimeLines` skips unknown fields instead of writing `unknown`/`0`; `vaultLine('')`-style empty counts produce `no notes yet`, not `0 notes: 0 embedded…`; `diagnosticsText` contains the np path and the log path and no key named in a leak list (`snippet`, `text`, `body`, `contents`).

- [x] **Step 2: Run to verify failure** — `npx vitest run src/lib/diagnostics.test.ts`.

- [x] **Step 3: Implement** `diagnostics.ts` (pure, no Tauri imports) and make the tests pass.

- [x] **Step 4: Write the failing mount test** in `App.smoke.test.ts`: stub the `diagnostics` command; pressing `Ctrl+Shift+D` opens the drawer showing the runtime line and one activity row; a `diagnostics://event` fired afterwards appends a row without re-requesting the snapshot; `Esc` or the shortcut closes it.

- [x] **Step 5: Implement** `Diagnostics.svelte` (sections: runtime, activity newest-first, queue, vault; `[copy everything]` uses the same clipboard-with-textarea-fallback pattern as About) and wire it in `App.svelte`: keyboard shortcut, a button in About labelled `Diagnostics`, subscription to `diagnostics://event` alongside the existing event listeners.

- [x] **Step 6: Add the contract test** in `ipc_surface.rs`: the serialized key sets of `Diagnostics`, `RuntimeInfo`, `QueueState`, `VaultCounts` equal `DIAGNOSTICS_KEYS` as written in `src/lib/diagnostics.ts` (read the file, same technique as `palette.test.ts`).

- [x] **Step 7: Run every gate** — `npx vitest run`, `npx svelte-check --tsconfig ./tsconfig.app.json`, `npm run build`, `PATH=$HOME/.cargo/bin:/usr/bin:/bin cargo test -p jott-core`, clippy `-D warnings`, and the Windows-target check.

- [x] **Step 8: Commit** `git commit -m "feat: a diagnostics drawer, opened where the note is"`

---

### Task 5: Ship it

- [x] Version bump in `apps/shell/Cargo.toml` and `apps/shell/tauri.conf.json`, `docs/release-notes.md` rewritten for the release, a section in `docs/testing-on-windows.md` telling the tester to open the drawer with `Ctrl+Shift+D` and what to send from it.
- [x] Merge to `main`, tag, push, watch CI to a published release, and quote the Windows MSI sha256 back.

## What differed from the plan, and why

Shipped as `v0.0.11`. Every gate the plan named ran green: 225 Rust tests with np off PATH, clippy
`-D warnings`, 96 frontend tests, `svelte-check` clean, `npm run build`, and the Windows-target type-check of
the shell.

1. **The event is `trace://row`, not `diagnostics://event`.** It carries one row, not a snapshot, and a name that
   says "event" while the payload says "one line of history" is how a later reader re-fetches the whole thing.
2. **The contract test lives in TypeScript, not in `ipc_surface.rs`** (Task 4 Step 6). The drift risk is on the
   TS side — Rust renames a field and its own tests still pass — so the test reads `ipc.rs` and `diagnostics.rs`
   with `node:fs`, exactly as `palette.test.ts` reads CSS. Rust still pins its own key sets, and the TS test also
   fails if the file it claims to read contains no `KEYS` at all, because a typo'd path would otherwise compare
   nothing while reporting success.
3. **`runtime.npVersion` is what the handshake answered**, even when np cannot be reached for a later request;
   the plan expected `None` there. A session only exists after a successful handshake, so `None` would mean
   "never asked", which does not happen in the app — reporting absence would be its own small lie. The test that
   was going to pin that now pins what is actually promised: counts and the remembered consent answer with np
   gone.
4. **`QueueState` answers a message instead of mirroring state.** Task 3 Step 3 sketched a copy of the queue's
   numbers kept in shared state and updated at each transition; the worker now replies to `Msg::Report` from its
   own bookkeeping. A second place that has to remember the same counts is a future disagreement, and the drawer
   can wait one hop on a channel for a number it is about to print.
5. **An np that answers with a JSON-RPC fault is *running*.** Only spawn and transport failures become
   `StartFailed` connection rows; otherwise the drawer would say "np could not start" over an np that started
   fine and simply refused the request — the exact confusion the feature exists to remove.
6. **`Journal::file()` was added rather than changing `path()`**, so About keeps its native-separator path while
   the drawer gets `None` when writing gave up. The log file is now created when the journal opens, so a path the
   window names is a path that exists; size comes from the filesystem, not a counter, so rotation reports truth.
7. **About gained the `Diagnostics` button** the plan asked for, and the window's key handler became async — which
   meant the `<svelte:window onkeydown=…>` binding had to wrap the call rather than name it.
8. **A process note worth keeping:** while adding tests to `ipc_surface.rs` I ran a scripted edit that silently
   deleted five of them, and the suite stayed green — 220 passing looked fine because 220 was what passed before
   the new ones existed. Clippy's unused-import warning is what gave it away. A green suite is not the gate; the
   *count* is, so the count gets quoted above.
