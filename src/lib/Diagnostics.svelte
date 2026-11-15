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
</script>

<!-- Escape is routed once, by App (`escapeAction`). This drawer used to listen on `window` itself, which meant one
     press closed the drawer *and* whatever `escapeAction` decided was next. -->

<section class="drawer" aria-label="Jott diagnostics" tabindex="-1" bind:this={drawer}>
  <header>
    <h2>What Jott is doing</h2>
    <p class="promise">This is what the backend asked and answered. Nothing you have typed appears here.</p>
    <button type="button" class="close" onclick={onClose} title="Close (Esc)">✕</button>
  </header>

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

  {#if revealFailed && logFile}
    <!-- The file manager said no. Nothing new to show, so the line says where to look and offers the path. -->
    <p class="quiet reveal-fallback">
      The file manager would not open. The log is at
      <code>{logFile}</code>
      <button type="button" onclick={() => void copyLogPath()}>Copy path</button>
    </p>
  {/if}

  {#if manual}
    <textarea bind:this={clipBox} readonly rows="6" aria-label="Diagnostics text to copy">{snapshot ? copyText(snapshot, jottVersion) : ''}</textarea>
  {/if}
</section>

<style>
  .drawer {
    border-top: 1px solid color-mix(in srgb, var(--ink) 22%, transparent);
    background: color-mix(in srgb, var(--paper) 92%, var(--ink));
    color: var(--ink);
    padding: 0.6rem 0.9rem 0.8rem;
    max-height: 45vh;
    overflow: auto;
    font-size: 0.82rem;
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

  .facts dd {
    margin: 0;
    word-break: break-all;
  }

  code {
    font-family: ui-monospace, 'Cascadia Mono', Consolas, monospace;
    font-size: 0.78rem;
  }

  .activity {
    list-style: none;
    margin: 0 0 0.6rem;
    padding: 0;
    max-height: 22vh;
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
