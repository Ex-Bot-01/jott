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
  import { openPath } from '@tauri-apps/plugin-opener'
  import {
    clip,
    connectionLabel,
    consentSentence,
    copyText,
    countsSentence,
    folderOf,
    formatAgo,
    formatBytes,
    mergeRows,
    newestFirst,
    queueSentence,
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
  const logFolder = $derived(folderOf(runtime?.logPath ?? null))

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
    const folder = logFolder
    if (!folder) return
    // A refused open is not worth a dialog: the path is on screen and selectable either way.
    void openPath(folder).catch(() => (manual = true))
  }
</script>

<svelte:window onkeydown={(event) => event.key === 'Escape' && onClose()} />

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
    <button type="button" onclick={reveal} disabled={logFolder === null}>Open log folder</button>
    <span class="quiet">Ctrl+Shift+D closes this</span>
  </footer>

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

  .down {
    color: color-mix(in srgb, var(--ink) 70%, #b3261e);
  }
</style>
