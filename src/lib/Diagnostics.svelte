<script lang="ts">
  // The diagnostics drawer: what Jott asked np, what np said back, and where the files are.
  //
  // It is a drawer at the bottom of this window rather than a second window, because the thing being debugged is
  // usually happening in the note that would be hidden behind a new one — and because a second webview means a
  // second set of capabilities, permissions and ways to fail.
  //
  // Nothing here shows the contents of a note. The rows are the backend's own records (`TraceEvent`), whose key
  // set is closed and pinned by tests on both sides; "what did np send back" means its status and its error
  // sentence, never a file's text.

  import { onDestroy } from 'svelte'
  import { revealItemInDir } from '@tauri-apps/plugin-opener'
  import {
    DRAWER_DEFAULT,
    DRAWER_MIN,
    clampDrawer,
    drawerHeightFromDrag,
    drawerMax,
    loadLayout,
    saveDrawer,
  } from './layoutSize'
  import {
    clip,
    connectionLabel,
    consentSentence,
    copyText,
    countsSentence,
    formatAgo,
    formatBytes,
    mergeRows,
    newestFirst,
    queueSentence,
    revealTarget,
    rowDetail,
    rowLabel,
    type DiagnosticsSnapshot,
    type TraceEvent,
  } from './diagnostics'

  let {
    snapshot,
    live = [],
    jottVersion,
    onClose,
  }: {
    snapshot: DiagnosticsSnapshot | null
    live?: TraceEvent[]
    jottVersion: string
    onClose: () => void
  } = $props()

  /** The rows age while the drawer is open — "just now" that never becomes "2 min ago" is its own small lie. */
  let now = $state(Date.now())
  const tick = setInterval(() => (now = Date.now()), 5_000)
  onDestroy(() => clearInterval(tick))

  let drawer = $state<HTMLElement | null>(null)
  let clipBox = $state<HTMLTextAreaElement | null>(null)
  /** The webview can refuse a clipboard write (an insecure origin, a locked-down policy). Then the text is shown
      selected instead: slower, and unable to fail. */
  let manual = $state(false)

  const runtime = $derived(snapshot?.runtime ?? null)
  const rows = $derived(newestFirst(mergeRows(snapshot?.recent ?? [], live)))
  /** The file to reveal, or `null` when the journal never opened. */
  const logFile = $derived(revealTarget(runtime))
  /** Set when the file manager refused. The path is already on screen above, so the fallback points at it rather
      than opening another box of text — which is exactly what this button used to do by accident. */
  let revealFailed = $state(false)

  $effect(() => {
    drawer?.focus()
  })

  async function copy() {
    if (!snapshot) return
    const text = copyText(snapshot, jottVersion)
    try {
      await navigator.clipboard.writeText(text)
      manual = false
    } catch {
      manual = true
      clipBox?.select()
    }
  }

  function reveal() {
    const file = logFile
    if (!file) return
    // `revealItemInDir` asks Explorer (or the desktop's equivalent) to open the folder *with this file selected*.
    // The old call was `openPath(folder)`, which the capability set does not grant: the promise rejected, and the
    // rejection handler showed the clipboard box — a small window of text where a file manager should have been.
    revealFailed = false
    void revealItemInDir(file).catch(() => (revealFailed = true))
  }

  async function copyLogPath() {
    const file = logFile
    if (!file) return
    try {
      await navigator.clipboard.writeText(file)
    } catch {
      // The path stays on screen as text either way; a second failure needs no second message.
    }
  }

  /** How tall the drawer is — read when it opens, so a drawer opened in a window that has been dragged to another
   *  monitor is clamped to the one it is in. The limits and the memory are `layoutSize.ts`'s; what lives here is the
   *  drag, the keys, and spending the height on rows rather than on an empty box. */
  const store = typeof localStorage === 'undefined' ? null : localStorage
  let heightPx = $state(loadLayout(store, innerWidth, innerHeight).drawerPx)
  let dragging = $state(false)
  /** Where the current drag started. The height is a delta from here rather than "everything below the pointer":
   *  the status strip lives under this drawer, so that reading overshoots and the first press jumps the edge away
   *  from the cursor. See `drawerHeightFromDrag`. */
  let grab = $state<{ y: number; h: number } | null>(null)
  /** The viewport in state, because `innerHeight` is not reactive: read directly, `aria-valuemax` would go on naming
   *  the ceiling of the window this drawer was opened in after somebody shrinks it. */
  let viewport = $state({ w: innerWidth, h: innerHeight })

  function nudgeHeight(event: KeyboardEvent) {
    const step = event.shiftKey ? 48 : 16
    let next: number
    switch (event.key) {
      case 'ArrowUp':
        // Up makes it taller. The handle is the drawer's top edge, and people read "drag up, more of it" — the naive
        // `clientY` reading gets this backwards, so the keys get it wrong in the same direction on purpose.
        next = heightPx + step
        break
      case 'ArrowDown':
        next = heightPx - step
        break
      case 'Home':
        next = DRAWER_DEFAULT
        break
      default:
        return
    }
    event.preventDefault()
    heightPx = clampDrawer(next, viewport.h)
    saveDrawer(store, heightPx)
  }

  function startDrag(event: PointerEvent) {
    // The primary button only: a right-press means "show me the menu", and treating it as a grab both resized the
    // drawer and hijacked the gesture whose meaning is leave-it-alone.
    if (event.button !== 0) return
    try {
      ;(event.currentTarget as HTMLElement).setPointerCapture?.(event.pointerId)
    } catch {
      // A drag that cannot capture still drags; it just stops tracking once the pointer leaves the strip.
    }
    dragging = true
    grab = { y: event.clientY, h: heightPx }
  }

  function moveDrag(event: PointerEvent) {
    if (!dragging) return
    // No button held means the release was never seen — capture refused, or the button came up somewhere else. End
    // here rather than leave a handle that resizes on hover and a window stuck in `user-select: none`.
    if (event.buttons === 0) {
      endDrag()
      return
    }
    if (grab === null) return
    heightPx = drawerHeightFromDrag(grab.y, grab.h, event.clientY, viewport.h)
  }

  /** Release, or a cancelled drag. Nothing is written mid-drag: a drag in progress is not yet a decision; and a
   *  cancelled one still keeps the height it reached, because that is where the pointer stopped. */
  function endDrag() {
    if (!dragging) return
    dragging = false
    grab = null
    saveDrawer(store, heightPx)
  }

  /** A shorter window shortens the drawer *for now*, exactly as it narrows the sidebar: the remembered height belongs
   *  to the desk, the clamp to this window. Without this, undocking with the drawer open leaves the note pane
   *  collapsed and pushes the status strip off the bottom edge. */
  const onWindowResize = () => {
    viewport = { w: innerWidth, h: innerHeight }
    heightPx = clampDrawer(heightPx, viewport.h)
  }
  if (typeof addEventListener === 'function') addEventListener('resize', onWindowResize)
  onDestroy(() => {
    if (typeof removeEventListener === 'function') removeEventListener('resize', onWindowResize)
  })
</script>

<!-- Escape is routed once, by App (`escapeAction`). This drawer used to listen on `window` itself, which meant one
     press closed the drawer *and* whatever `escapeAction` decided was next. -->

<section
  class="drawer"
  data-testid="diagnostics-drawer"
  aria-label="Jott diagnostics"
  tabindex="-1"
  style="height: {heightPx}px"
  class:dragging
  bind:this={drawer}
>
  <!-- The same split-bar pattern as the sidebar's, turned through a quarter turn: this strip *is* the drawer's top
       edge, so dragging it up makes the drawer taller. See `App.svelte` for why the a11y warnings are silenced rather
       than the role changed. -->
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    class="splitter"
    role="separator"
    aria-orientation="horizontal"
    tabindex="0"
    aria-label="Diagnostics height"
    aria-valuemin={DRAWER_MIN}
    aria-valuemax={drawerMax(viewport.h)}
    aria-valuenow={heightPx}
    title="Drag to resize · double-click or Home for the default height"
    data-testid="drawer-splitter"
    onpointerdown={startDrag}
    onpointermove={moveDrag}
    onpointerup={endDrag}
    onpointercancel={endDrag}
    ondblclick={() => {
      heightPx = DRAWER_DEFAULT
      saveDrawer(store, heightPx)
    }}
    onkeydown={nudgeHeight}
  ></div>

  <header>
    <h2>What Jott is doing</h2>
    <p class="promise">This is what the backend asked and answered. Nothing you have typed appears here.</p>
    <button type="button" class="close" onclick={onClose} title="Close (Esc)">✕</button>
  </header>

  <!-- Everything below the header scrolls together, and this is the only scroller in the drawer: a second one inside a
       height somebody chose by hand is how you end up with two scrollbars and no more visible rows. -->
  <div class="body">
  {#if snapshot && runtime}
    <dl class="facts">
      <dt>np</dt>
      <dd>
        {runtime.npVersion ?? 'version unknown'}
        {#if runtime.protocol !== null}<span class="quiet">(protocol {runtime.protocol})</span>{/if}
        {#if runtime.npPath}<code title="The np file this run actually runs">{runtime.npPath}</code>{/if}
      </dd>

      <dt>connections</dt>
      <dd>
        <span class:down={runtime.writer === 'down'}>writer {connectionLabel(runtime.writer)}</span>
        ·
        <span class:down={runtime.reader === 'down'}>reader {connectionLabel(runtime.reader)}</span>
      </dd>

      <dt>embeddings</dt>
      <dd>
        {consentSentence(runtime.consent)}
        {#if runtime.autoFetch !== null}
          <span class="quiet">· np may fetch a model on its own: {runtime.autoFetch ? 'yes' : 'no'}</span>
        {/if}
      </dd>

      <dt>np data folder</dt>
      <dd><code>{runtime.dataDir || 'unknown'}</code></dd>

      <dt>Jott's files</dt>
      <dd>
        <code>{runtime.stateDir || 'unknown'}</code>
        {#if runtime.stateFallback}
          <!-- Unsaved drafts are mirrored here, so a folder nobody chose is worth saying out loud. -->
          <span class="warn">temporary — the app data folder was not writable</span>
        {/if}
      </dd>

      <dt>log</dt>
      <dd>
        {#if runtime.logPath}
          <code>{runtime.logPath}</code>
          {#if runtime.logBytes !== null}<span class="quiet">({formatBytes(runtime.logBytes)})</span>{/if}
        {:else}
          <span class="quiet">none opened this run</span>
        {/if}
      </dd>

      <dt>queue</dt>
      <dd>{queueSentence(snapshot.queue)}</dd>

      <dt>this folder</dt>
      <dd>{countsSentence(snapshot.vault)}</dd>
    </dl>
  {:else}
    <p class="quiet">Asking the backend what it has been doing…</p>
  {/if}

  <ol class="activity" aria-label="Backend activity this run">
    {#each rows as row (row.atMs + row.kind + rowLabel(row) + (row.detail ?? ''))}
      <li>
        <span class="when">{formatAgo(row.atMs, now)}</span>
        <strong>{rowLabel(row)}</strong>
        {#if rowDetail(row)}
          <!-- The full sentence is the `title` and in "copy details"; only the row gets shortened (§4). -->
          <span class="detail" title={rowDetail(row) ?? undefined}>{clip(rowDetail(row) ?? '')}</span>
        {/if}
      </li>
    {:else}
      <li class="quiet">Nothing has been asked of np yet.</li>
    {/each}
  </ol>

  {#if revealFailed && logFile}
    <!-- The file manager said no. Nothing new to show, so the line says where to look and offers the path. Both this
         and the clipboard fallback below live *inside* `.body`, because that is the only box in here that scrolls:
         left after the footer they sat in a clipped flex column, unreachable at a short drawer height — which is
         precisely when the text they carry is most wanted. -->
    <p class="quiet reveal-fallback">
      The file manager would not open. The log is at
      <code>{logFile}</code>
      <button type="button" onclick={() => void copyLogPath()}>Copy path</button>
    </p>
  {/if}

  {#if manual}
    <textarea bind:this={clipBox} readonly rows="6" aria-label="Diagnostics text to copy">{snapshot ? copyText(snapshot, jottVersion) : ''}</textarea>
  {/if}
  </div>

  <footer>
    <button type="button" onclick={() => void copy()}>Copy details</button>
    <button
      type="button"
      data-testid="show-log"
      onclick={reveal}
      disabled={logFile === null}
      title={logFile ?? 'No log file was opened this run'}>Show log file</button>
    <span class="quiet">Ctrl+Shift+D closes this</span>
  </footer>

</section>

<style>
  .drawer {
    /* Every column, and it has to be said out loud: this element is a child of the window's grid, so without this it
       is auto-placed into the *sidebar's* column — which is what made the whole screen about 364 px wide, paths and
       all, with empty space beside it. Its height used to be capped at 45 vh here; it is now a number the person
       drags (Task 4), so nothing in this rule limits what somebody asked for. */
    grid-column: 1 / -1;
    border-top: 1px solid color-mix(in srgb, var(--ink) 22%, transparent);
    background: color-mix(in srgb, var(--paper) 92%, var(--ink));
    color: var(--ink);
    /* No top padding: the handle is the first thing in here and it belongs flush against the edge, because that line
       is what a pointer aims at. The space this used to provide is the handle's own bottom margin now. */
    padding: 0 0.9rem 0.8rem;
    /* A column whose height is the person's, so exactly one thing inside it scrolls (`.body`) and the header and the
       buttons stay put while the rows move. */
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
    font-size: 0.82rem;
  }

  /* `app.css` gives every focused element a 2 px ink outline, which is right for anything a keyboard can reach. This
     element cannot be reached: `tabindex="-1"` means it is only ever focused by our own code, on open, so the ring
     communicated "this is where your keys go" by drawing a black line the full width of the window over the screen
     somebody had just opened to read. The keys land here either way — Escape is routed once, by the window. */
  .drawer:focus-visible {
    outline: none;
  }

  /* The handle, which is the drawer's top edge rather than a bar below it — the pointer should land where the border
     you are looking at is. It reaches past the padding so there is no dead strip either side of it. */
  .splitter {
    /* 12 px, flush with the border, and reaching past the side padding so there is no dead corner either end of it. */
    height: 12px;
    margin: 0 -0.9rem 0.45rem;
    border: 0;
    background: transparent;
    cursor: ns-resize;
    flex: 0 0 auto;
    /* See the sidebar's handle: without this a finger drag is a page pan, and WebView2 answers it with
       `pointercancel` instead of moves. */
    touch-action: none;
  }

  .splitter:hover,
  .splitter:focus-visible {
    background: color-mix(in srgb, var(--ink) 28%, transparent);
  }

  .drawer.dragging {
    user-select: none;
    cursor: ns-resize;
  }

  @media (forced-colors: active) {
    /* Same reasoning as the sidebar's handle: a tint is a colour, and this mode throws colours away. */
    .splitter {
      border: 1px solid;
      border-color: CanvasText;
      background: Canvas;
    }
  }

  /* The one scroller. `min-height: 0` is what lets it shrink inside a flex column — without it the drawer grows past
     its own height instead of scrolling, which is how a resizable panel ends up with nothing at the bottom. */
  .body {
    flex: 1 1 auto;
    min-height: 0;
    overflow: auto;
  }

  /* Wide enough that two columns both get readable lines: the runtime facts on the left, where the paths live, and
     the activity list on the right where they scroll past. This is what the window's width was worth having — below
     it, one column, because a path broken into three lines tells you less than the row you cannot see. */
  @media (min-width: 900px) {
    .body {
      display: grid;
      grid-template-columns: minmax(0, 1fr) minmax(0, 1.25fr);
      gap: 0.9rem;
      align-items: start;
    }

    .facts {
      margin-top: 0;
    }

    .activity {
      margin-bottom: 0;
    }
  }

  header {
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: baseline;
    gap: 0.6rem;
  }

  h2 {
    margin: 0;
    font-size: 0.95rem;
  }

  .promise {
    margin: 0;
    font-size: 0.75rem;
  }

  .close {
    align-self: center;
  }

  .facts {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 0.15rem 0.8rem;
    margin: 0.7rem 0 0.6rem;
  }

  .facts dt {
    font-weight: 600;
    opacity: 0.72;
  }

  /* `anywhere`, not `break-all`: a Windows path should break at a separator when it has to and not at all when it
     does not. `break-all` broke every long path mid-word, which is how `…\Jott\np\np.exe` arrived as three lines. */
  .facts dd {
    margin: 0;
    overflow-wrap: anywhere;
  }

  code {
    font-family: ui-monospace, 'Cascadia Mono', Consolas, monospace;
    font-size: 0.78rem;
  }

  .activity {
    /* This list used to carry its own cap, which meant a taller drawer bought a taller empty box instead of more
       rows — the exact thing the drag exists to fix. Nothing limits it now but the drawer it sits in. */
    list-style: none;
    margin: 0 0 0.6rem;
    padding: 0;
    min-height: 0;
    overflow: auto;
    border-top: 1px solid color-mix(in srgb, var(--ink) 14%, transparent);
  }

  .activity li {
    display: flex;
    gap: 0.5rem;
    align-items: baseline;
    padding: 0.12rem 0;
    border-bottom: 1px solid color-mix(in srgb, var(--ink) 8%, transparent);
  }

  .when {
    min-width: 5.5rem;
    opacity: 0.6;
    font-variant-numeric: tabular-nums;
  }

  .detail {
    opacity: 0.82;
    overflow-wrap: anywhere;
  }

  footer {
    display: flex;
    gap: 0.5rem;
    align-items: center;
  }

  .quiet {
    opacity: 0.66;
  }

  /* A state folder that is a temp fallback is worth more weight than a footnote: unsaved drafts live there. */
  .warn {
    display: block;
    color: var(--danger, inherit);
    font-size: 0.85em;
  }

  .down {
    color: color-mix(in srgb, var(--ink) 70%, #b3261e);
  }
</style>
