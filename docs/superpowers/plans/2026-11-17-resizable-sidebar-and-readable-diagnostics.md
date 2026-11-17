# A sidebar you can size, and a diagnostics drawer that uses the window — implementation plan (v0.0.26)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let the person set the width of the sidebar and the height of the diagnostics drawer, and give that drawer the full width of the window it was always designed to have.

**Architecture:** One new pure module holds every number and both memories (`src/lib/layoutSize.ts`); `App.svelte` owns the sidebar handle because it owns the grid, `Diagnostics.svelte` owns its own height handle because nothing else sizes a drawer; `About.svelte` loses a click-bubbling bug found on the way in. No Rust changes, no new commands, nothing asked of note-processor.

**Tech Stack:** Svelte 5 runes, Vite, Vitest (jsdom), plain CSS in component `<style>` blocks plus `src/app.css`.

**Spec:** [`docs/superpowers/specs/2026-11-17-resizable-sidebar-and-diagnostics-design.md`](../specs/2026-11-17-resizable-sidebar-and-diagnostics-design.md) — read it with this plan; the numbers below come from it.

## Global Constraints

- Root font is `14px` (`src/app.css:19`), so **1 rem = 14 px**: `19rem` = 266 px, `26rem` = 364 px. All new limits are written in px, not rem, because they travel through JS.
- Logic that can be tested headlessly lives in a pure TS module with Vitest tests, **not** in a component (README ground rule). Clamp arithmetic and storage parsing are exactly that.
- No module touches `window` at import time (`src/lib/darkMode.ts:8` says why: a module that throws on import takes the whole suite down).
- Svelte 5 has **no event modifiers** — `onclick:self` does not exist. The About fix is an explicit `event.target === event.currentTarget` test in the handler.
- `jsdom` has no layout engine and no `PointerEvent`; tests assert arithmetic, attributes, inline styles, DOM order, and stored values — never a measured pixel. Where a rule cannot be seen by jsdom at all, a **source pin** is allowed and must be labelled as a claim about what somebody wrote, not about what a user sees (`docs/superpowers/plans/2026-11-17-five-asks-from-inside-the-editor.md` ruling 8).
- The pixels are verified in the browser harness (`npm run ui`, port 5199) with screenshots — see Task 6.
- Nothing in `crates/` or `apps/shell/` changes except the version number in Task 6. The drawer's privacy rule (no note contents, closed key set) is untouched: this plan changes layout and sizing only.
- Copy follows the window's voice: sentence case, tooltips say what will happen ("Drag to resize the list"), no icons where a word is clearer.
- Verification commands, all of which must be green before merge: `npm test`, `npm run build`, `npx svelte-check --tsconfig ./tsconfig.app.json --output human` (expect `0 errors`, `0 warnings`). Baseline at branch start: **410 Vitest tests passing**.

## Review Focus

The spec is a layout document, so the ways this breaks are about *sizes that arrive from elsewhere* and *input that isn't a clean drag*. Each line gets its test in the task named:

1. **A width remembered on a 27" monitor opening on a laptop** — the note pane must keep a readable column, and the too-wide preference must survive the clamp so the big monitor still gets it back. (Task 2)
2. **A pointer that leaves the handle mid-drag, or is released outside the window** — the column follows the cursor only while the button is down, and stops following on `pointerup`/`pointercancel`. (Task 2)
3. **Arrow keys pressed while focus is in the note or the search box** — resizing belongs to the handle alone; typing a sentence with arrow keys must never move the window's columns. (Task 2)
4. **A window short enough that the drawer's default height does not fit** — the default clamps into range rather than pushing the status line and the note off screen. (Task 4)
5. **A handle nobody can see without hovering** (high contrast, forced colours, a coarse pointer) — it keeps a visible edge and a word in its tooltip; colour alone carries nothing. (Task 2)

---

### Task 1: `layoutSize.ts` — every number and both memories

**Files:**
- Create: `src/lib/layoutSize.ts`
- Test: `src/lib/layoutSize.test.ts`

**Interfaces:**
- Consumes: nothing.
- Produces:
  ```ts
  export const SIDE_MIN = 266
  export const SIDE_MAX_CAP = 560
  export const SIDE_FRACTION = 0.45
  export const SIDE_DEFAULT = 364
  export const DRAWER_MIN = 180
  export const DRAWER_FRACTION = 0.7
  export const DRAWER_DEFAULT = 320
  export const SIDE_KEY = 'jott.layout.sidePx'
  export const DRAWER_KEY = 'jott.layout.drawerPx'

  export interface Layout { sidePx: number; drawerPx: number }
  export function sideMax(viewportW: number): number
  export function clampSide(px: number, viewportW: number): number
  export function sideFromPointer(clientX: number, viewportW: number): number
  export function drawerMax(viewportH: number): number
  export function clampDrawer(px: number, viewportH: number): number
  export function drawerFromPointer(clientY: number, viewportH: number): number
  export function loadLayout(store: Store | null | undefined, viewportW: number, viewportH: number): Layout
  export function saveSide(store: WritableStore | null | undefined, px: number): void
  export function saveDrawer(store: WritableStore | null | undefined, px: number): void
  ```
  `Store` / `WritableStore` are the same two-method structural types `src/lib/searchSettings.ts:34-38` uses, so a test hands in a `Map`.

- [ ] **Step 1: Write the failing tests** in `src/lib/layoutSize.test.ts`, one `describe` for the arithmetic and one for storage. The assertions, exactly:

  ```ts
  expect(sideMax(1920)).toBe(560)                      // capped, not 45 % of a huge screen
  expect(sideMax(1200)).toBe(540)
  expect(sideMax(820)).toBe(369)
  expect(sideMax(400)).toBe(SIDE_MIN)                  // the floor wins; the pane still gets 134 px
  expect(clampSide(100, 1200)).toBe(SIDE_MIN)
  expect(clampSide(4000, 1200)).toBe(540)
  expect(clampSide(Number.NaN, 1200)).toBe(SIDE_DEFAULT) // a number nobody chose reads as the default
  expect(sideFromPointer(-40, 1200)).toBe(SIDE_MIN)
  expect(sideFromPointer(500, 1200)).toBe(500)
  expect(drawerMax(768)).toBe(537)
  expect(drawerMax(400)).toBe(200)                     // h - 200 keeps the note pane alive
  expect(clampDrawer(10, 768)).toBe(180)
  expect(clampDrawer(5000, 768)).toBe(537)
  expect(drawerFromPointer(400, 768)).toBe(368)        // 768 - 400: the edge went up, the drawer got taller
  ```

  Storage: `loadLayout(null, 1200, 768)` → `{ sidePx: 364, drawerPx: 320 }`; a store holding `'512'`/`'240'` reads `{ 512, 240 }`; a store holding `'900'` at `viewportW = 820` reads `369`, and the same store at `1920` reads `560`; each of `'512px'`, `''`, `'-4'`, `'3.5'`, `'yes'` reads as the default; `saveSide(map, 412)` writes `'412'`; a store whose `getItem`/`setItem` throw neither throws nor changes the returned defaults.

- [ ] **Step 2: Run them to verify they fail** — `npx vitest run src/lib/layoutSize.test.ts` → cannot resolve `./layoutSize`.
- [ ] **Step 3: Implement the module.** Rules the tests do not already state: `sideMax = Math.max(SIDE_MIN, Math.min(SIDE_MAX_CAP, Math.floor(viewportW * SIDE_FRACTION)))`; `drawerMax = Math.max(DRAWER_MIN, Math.min(Math.floor(viewportH * DRAWER_FRACTION), viewportH - 200))`, so the note pane always keeps 200 px; both clamps replace a non-finite input with their default first; `loadLayout` accepts a stored value only when `/^\d+$/` matches and then runs it through the clamp for the viewport it was given. Copy `searchSettings.ts`'s comment style and say, at the top of the file, why the floors are 266 (labels stop wrapping) and why the ceiling is proportional rather than fixed (a saved width must not crush a smaller window).
- [ ] **Step 4: Run the suite** — `npx vitest run src/lib/layoutSize.test.ts` → all pass.
- [ ] **Step 5: Commit**

  ```bash
  git add src/lib/layoutSize.ts src/lib/layoutSize.test.ts
  git commit -m "feat: one place decides how wide a sidebar this window can hold"
  ```

---

### Task 2: A drag handle between the tree and the note

**Files:**
- Modify: `src/App.svelte` — script (state + handlers), markup (`</aside>` → `<section class="middle">`, around line 1345), `<style>` (`.window`, `.side`, `.middle`, new `.splitter`)
- Test: `src/App.smoke.test.ts` (add to the existing `describe('the window mounts')`)

**Interfaces:**
- Consumes: Task 1's `loadLayout`, `saveSide`, `clampSide`, `sideFromPointer`, `sideMax`, `SIDE_MIN`, `SIDE_DEFAULT`, `SIDE_KEY`.
- Produces: a `<main class="window">` whose first grid track is `var(--side, 364px)` and which carries `style="--side: NNNpx"`; a handle at `[data-testid="sidebar-splitter"]` with `role="separator"`, `aria-orientation="vertical"`, `tabindex="0"`, live `aria-valuenow/min/max`; the storage key `jott.layout.sidePx`.

- [ ] **Step 1: Write the failing tests.** Four `it(...)` blocks, using the existing `start()` helper and `localStorage.clear()` in `beforeEach` (already there):
  - *"the sidebar width is yours to set"* — mount; the handle exists with `aria-valuenow === '364'`; `keydown ArrowRight` **on the handle element** → `aria-valuenow === '380'`, `document.querySelector('main.window')!.getAttribute('style')` contains `--side: 380px`, and `localStorage.getItem('jott.layout.sidePx') === '380'`; then `Shift+ArrowLeft` → `332`; `Home` → `364`.
  - *"a remembered width is clamped to the window it landed in"* — `localStorage.setItem(SIDE_KEY, '900')` before mount; jsdom's `innerWidth` is 1024 → handle reads `aria-valuenow === '460'` and the inline style says `--side: 460px`, while the stored value is **still `'900'`** (a clamp for this window must not overwrite a preference).
  - *"dragging the handle moves the column, and stops when you let go"* — dispatch `pointerdown` (`new MouseEvent('pointerdown', { clientX: 420, bubbles: true })`) on the handle, then `pointermove` at `clientX: 520`, then `pointerup`; expect stored `'520'`. A further `pointermove` at `clientX: 700` after release leaves it at `'520'`.
  - *"arrows belong to the note, not to the handle"* — with focus on `input[type="search"]`, dispatch `keydown ArrowRight` on `window`; width stays `364`.
- [ ] **Step 2: Run them to verify they fail** — `npx vitest run src/App.smoke.test.ts -t "yours to set"` → handle not found.
- [ ] **Step 3: Implement in `src/App.svelte`.**
  - Script: `const store = typeof localStorage === 'undefined' ? null : localStorage` (the pattern already used for `storedSettings`), `let sidePx = $state(loadLayout(store, innerWidth, innerHeight).sidePx)`, and a `let draggingSide = $state(false)` for the cursor/selection class.
  - Markup, between `</aside>` and `<section class="middle">`:

    ```svelte
    <div
      class="splitter"
      role="separator"
      aria-orientation="vertical"
      tabindex="0"
      aria-label="Sidebar width"
      aria-valuemin={SIDE_MIN}
      aria-valuemax={sideMax(innerWidth)}
      aria-valuenow={sidePx}
      title="Drag to resize the list · double-click or Home for the default width"
      data-testid="sidebar-splitter"
    >
    ```
    with `onpointerdown` / `onpointermove` / `onpointerup` / `onpointercancel` / `ondblclick` / `onkeydown` handlers. `pointerdown` calls `setPointerCapture?.(event.pointerId)` **with the optional call** (jsdom has no capture; a real webview does), sets `draggingSide`, and updates from `sideFromPointer(event.clientX, innerWidth)`; `pointermove` updates only while `draggingSide`; `pointerup`/`pointercancel` clear it and `saveSide(store, sidePx)`; `dblclick` and `Home` set `SIDE_DEFAULT` and save; arrows move by 16 px, or 48 px with Shift, through `clampSide`, saving each stop (a person who nudges and closes the window should not lose it) and calling `preventDefault()` so the page does not scroll.
  - Resize: one `addEventListener('resize', …)` in `onMount` that sets `sidePx = clampSide(sidePx, innerWidth)` **without saving**, removed in `onDestroy`. Say why in a comment: the remembered preference belongs to the desk, the clamp to this window.
  - Styles: `.window { grid-template-columns: var(--side, 364px) 5px 1fr; }` (three tracks — an unplaced third child would otherwise fall to a new row), `.middle { grid-column: 3 }`, `:global(.status)` and the drawer span `1 / -1` as before. `.splitter { cursor: ew-resize; border: 0; background: transparent; }`, tinted on `:hover` and `:focus-visible` with `color-mix(in srgb, var(--ink) 28%, transparent)`; `.window.dragging { user-select: none; cursor: ew-resize; }`. Keep `.side`'s `border-right` as the visible divider, and keep the `19rem`/`26rem` story in a comment where the numbers now live.
- [ ] **Step 4: Add the Review Focus pins.** Forced colours is the one thing jsdom cannot show: add a source pin that reads `src/App.svelte` as text and asserts the stylesheet contains an `@media (forced-colors: active)` block naming `.splitter` with `border-color: CanvasText`. Label the test as a claim about what the stylesheet says, not about what a screen shows.
- [ ] **Step 5: Run the suite** — `npx vitest run src/App.smoke.test.ts` → all pass, and `npm test` still green at its new count.
- [ ] **Step 6: Commit**

  ```bash
  git add src/App.svelte src/App.smoke.test.ts
  git commit -m "feat: the sidebar takes the width you give it, and remembers it"
  ```

---

### Task 3: Give the drawer the window's width, and the status line its floor back

**Files:**
- Modify: `src/App.svelte` (markup order: move the `{#if showDiagnostics}` block above `<StatusLine …>`; `.window` rows), `src/lib/Diagnostics.svelte` (`<style>` `.drawer`, root element `data-testid`)
- Test: `src/App.smoke.test.ts`

**Interfaces:**
- Consumes: nothing new.
- Produces: a drawer root carrying `data-testid="diagnostics-drawer"` that is a grid child spanning every column and placed **before** `.status` in the DOM.

- [ ] **Step 1: Write the failing test.** Open the drawer with `Ctrl+Shift+D` (the existing test shows how), then assert two things jsdom *can* see: `[data-testid="diagnostics-drawer"]` exists, and `document.querySelector('.status')!.compareDocumentPosition(drawer) & Node.DOCUMENT_POSITION_PRECEDING` is truthy — the status strip comes after the drawer, so it is the last row. Add the source pin in the same test: `Diagnostics.svelte`'s stylesheet contains `grid-column: 1 / -1` on `.drawer`, and no longer contains `max-height: 45vh` or a `max-height` inside `.activity`.
- [ ] **Step 2: Run it to verify it fails** — the drawer is found but precedes nothing; status still comes first.
- [ ] **Step 3: Implement.** Move the `{#if showDiagnostics}<Diagnostics … />{/if}` block above `<StatusLine>` in `src/App.svelte`; set `.window { grid-template-rows: 1fr auto auto; }`; in `Diagnostics.svelte` add `grid-column: 1 / -1;` to `.drawer` (scoped styles reach the component's own root, which is the grid child) and delete `max-height: 45vh` plus the `.activity { max-height: 22vh }` line — Task 4 replaces them with a height that is set on purpose. Add `data-testid="diagnostics-drawer"` to the `<section>`.
- [ ] **Step 4: Run the suite** — `npx vitest run src/App.smoke.test.ts` → pass; existing Escape-order tests (`escapeAction`) must still pass unchanged, since only DOM order moved.
- [ ] **Step 5: Commit**

  ```bash
  git add src/App.svelte src/lib/Diagnostics.svelte src/App.smoke.test.ts
  git commit -m "fix: the diagnostics drawer finally gets the window's width"
  ```

---

### Task 4: A drawer height you set, that spends itself on activity rows

**Files:**
- Modify: `src/lib/Diagnostics.svelte` (script: state + handlers; markup: handle + `.body` wrapper; `<style>`)
- Test: `src/App.smoke.test.ts`

**Interfaces:**
- Consumes: Task 1's `loadLayout`, `saveDrawer`, `clampDrawer`, `drawerFromPointer`, `DRAWER_MIN`, `DRAWER_DEFAULT`.
- Produces: a drawer whose root has an inline `height: NNNpx`, a handle at `[data-testid="drawer-splitter"]` (`role="separator"`, `aria-orientation="horizontal"`), and the storage key `jott.layout.drawerPx`.

- [ ] **Step 1: Write the failing tests.** With the drawer open: the handle exists; `keydown ArrowUp` on it → root inline style contains `height: 336px` (default 320 + 16) and `localStorage['jott.layout.drawerPx'] === '336'`; `Shift+ArrowDown` → `288`; `Home` → `320`. Then a second test: store `'900'`, mount with jsdom's `innerHeight = 768` → root reads `height: 537px`; and one where `innerHeight` is redefined to `400` for the mount (`Object.defineProperty(window, 'innerHeight', { value: 400, configurable: true })`, restored afterwards) → the remembered height lands on `200`, not on 320 or 900.
- [ ] **Step 2: Run them to verify they fail** — no handle exists yet.
- [ ] **Step 3: Implement.** `let heightPx = $state(loadLayout(store, innerWidth, innerHeight).drawerPx)`; the root gets `style:height="{heightPx}px"` and `display: flex; flex-direction: column; min-height: 0; overflow: hidden`. New first child: the horizontal handle (same keyboard/drag shape as Task 2's, with `drawerFromPointer(event.clientY, innerHeight)` — dragging the top edge up makes the drawer taller), `cursor: ns-resize`, `10px` tall so it is grabbable without being scenery. Then `header`, a `.body` (`flex: 1 1 auto; min-height: 0; overflow: auto`) wrapping the `<dl class="facts">` and the `<ol class="activity">`, and the existing `footer` — the cap that used to sit on `.activity` went in Task 3, so give it nothing but `min-height: 0` and let it grow.
- [ ] **Step 4: Use the width.** In the same `<style>`: at `@media (min-width: 900px)` make `.body` a two-column grid — `grid-template-columns: minmax(0, 1fr) minmax(0, 1.25fr); gap: 0.9rem; align-items: start` — so the runtime facts sit left and the activity list right, which is what the new width buys. Below that it stays one column. Keep `.facts dd { word-break: break-all }` only for the narrow case: at full width `overflow-wrap: anywhere` reads better on a Windows path, and neither may split a path mid-word when there is room.
- [ ] **Step 5: Run the suite** — `npx vitest run src/App.smoke.test.ts` → pass.
- [ ] **Step 6: Commit**

  ```bash
  git add src/lib/Diagnostics.svelte src/App.smoke.test.ts
  git commit -m "feat: drag the diagnostics drawer to the height you need"
  ```

---

### Task 5: About stops dismissing itself, and hands over to the drawer

**Files:**
- Modify: `src/lib/About.svelte` (scrim handler), `src/App.svelte` (`openDiagnostics`)
- Test: `src/App.smoke.test.ts` (update one existing test, add one)

**Interfaces:**
- Consumes: nothing new.
- Produces: `openDiagnostics()` closes About; About's scrim dismisses only on a click that landed on the scrim itself.

- [ ] **Step 1: Write the failing tests.** In the existing *"answers "which np is this running" without leaving the window"* test, after pressing **Diagnostics** add `expect(document.querySelector('[role="dialog"][aria-label="About Jott"]')).toBeNull()` — one thing on screen at a time. New test *"clicking inside About keeps About open"*: open About, click the panel's own body (`.panel`) and then **Copy details**; after each, `[role="dialog"][aria-label="About Jott"]` is still in the document; a click dispatched on `.scrim` itself does close it.
- [ ] **Step 2: Run them to verify they fail** — today every click inside the panel bubbles to the scrim and closes it.
- [ ] **Step 3: Implement.** In `src/lib/About.svelte`, replace `onclick={onClose}` on `.scrim` with a handler that calls `onClose()` only when `event.target === event.currentTarget`, with a one-line comment naming Svelte 5's missing `|self`. In `src/App.svelte`'s `openDiagnostics()`, set `showAbout = false` before fetching, and say why: the drawer is in the window's flow, so a modal left open would sit over it behind a dimming scrim — which is exactly what the person pressing **Diagnostics** was trying to look at.
- [ ] **Step 4: Run the full suite and both gates** — `npm test`, `npm run build`, `npx svelte-check --tsconfig ./tsconfig.app.json --output human`.
- [ ] **Step 5: Commit**

  ```bash
  git add src/lib/About.svelte src/App.svelte src/App.smoke.test.ts
  git commit -m "fix: a click inside About stays in About, and Diagnostics takes over"
  ```

---

### Task 6: Look at it, write it down, ship the number

**Files:**
- Modify: `docs/release-notes.md`, `docs/testing-on-windows.md`, `apps/shell/Cargo.toml`, `apps/shell/tauri.conf.json`

**Interfaces:**
- Consumes: Tasks 1–5 as shipped.
- Produces: a release note, a tester walkthrough with checks that can be run without a developer, version `0.0.26`.

- [ ] **Step 1: Run the harness and look at it.** `npm run ui`, then Firefox on `:99` (recipe in `/home/agent/desktop_use.md`; keyboard goes through `xdotool`, not the computer-use tools). Capture four states to `/tmp` and read each image: default window, sidebar dragged wide and then narrow, drawer open at default height and dragged tall, and dark mode (`prefers-color-scheme`). The checks are visual, so state what is seen rather than what was written: no label wraps or clips at the 266 px floor; the note pane keeps a readable column at every sidebar width down to 820 px window width; a Windows-style path in the drawer's `log` row breaks at a separator or not at all, never mid-word; dragging the drawer taller shows **more activity rows** rather than scrolling a capped list; the status strip is the last row of the window.
- [ ] **Step 2: Fix what the pictures show** before writing about it. If a rule needs changing, change it and re-shoot — a release note describing a layout nobody looked at is how this repo got a 364 px diagnostics drawer in the first place.
- [ ] **Step 3: Write the release note** at the top of `docs/release-notes.md` as `# Jott 0.0.26 — …`, in that file's voice: what was wrong, why it was wrong (the grid track that fixed the sidebar; the auto-placed drawer that never got column 2), and the numbers a person can quote — 266 px floor, 45 %/560 px ceiling, 180…70 % drawer.
- [ ] **Step 4: Write the tester walkthrough** as the next `## 2b-…` section of `docs/testing-on-windows.md`: drag the sidebar and reopen Jott; press `Ctrl+Shift+D`, drag the drawer taller, read a path whole; press **About → Diagnostics** and see one panel rather than two; `Escape` closes the drawer then the note.
- [ ] **Step 5: Bump the version to `0.0.26`** in `apps/shell/Cargo.toml` and `apps/shell/tauri.conf.json`, matching how 0.0.25 was cut.
- [ ] **Step 6: Final gate, then commit** — `npm test && npm run build && npx svelte-check --tsconfig ./tsconfig.app.json --output human`; report the passing count and the svelte-check line rather than asserting success.

  ```bash
  git add docs/release-notes.md docs/testing-on-windows.md apps/shell/Cargo.toml apps/shell/tauri.conf.json
  git commit -m "docs: 0.0.26 — a window that takes the shape you give it"
  ```
