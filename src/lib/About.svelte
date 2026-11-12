<script lang="ts">
  // The About panel: which Jott, which np, and where that np file actually is.
  //
  // It exists because a colleague reported "np is too old" and the app, which knew both the version it had
  // found and the file it had run, said neither. Every row here answers a question that would otherwise come
  // back as a message asking them to describe their screen.

  import { openUrl } from '@tauri-apps/plugin-opener'
  import type { AboutInfo } from './ipc'

  let { info, loading = false, onClose }: { info: AboutInfo | null; loading?: boolean; onClose: () => void } = $props()

  let clip = $state<HTMLTextAreaElement | null>(null)
  /** The webview's clipboard can be unavailable (a plain-HTTP origin is not a secure context). Then the text
      is shown selected instead, which is slower and cannot fail. */
  let manual = $state(false)

  const text = $derived(info?.diagnostics ?? '')

  async function copy() {
    if (!info) return
    try {
      await navigator.clipboard.writeText(text)
      manual = false
    } catch {
      manual = true
      clip?.select()
    }
  }

  function external(event: MouseEvent, url: string) {
    // A link in a webview either navigates the app away or does nothing; both are worse than the browser.
    event.preventDefault()
    void openUrl(url).catch(() => (manual = true))
  }
</script>

<svelte:window onkeydown={(event) => event.key === 'Escape' && onClose()} />

<div class="scrim" role="presentation" onclick={onClose}>
  <div class="panel" role="dialog" aria-label="About Jott" tabindex="-1">
    <header>
      <h2>Jott</h2>
      <button type="button" class="close" onclick={onClose} title="Close (Esc)">✕</button>
    </header>

    {#if loading && !info}
      <p class="quiet">Asking what is installed…</p>
    {:else if info}
      <dl>
        <dt>Jott</dt>
        <dd>
          {info.jottVersion}
          <a href={info.releasesUrl} onclick={(event) => external(event, info.releasesUrl)}>releases</a>
        </dd>

        {#if info.np.found}
          <dt>np</dt>
          <dd>{info.np.version}</dd>

          <dt>np comes from</dt>
          <dd>
            <span class="source">{info.np.source}</span>
            <code title="The np Jott actually runs — this is the line worth quoting">{info.np.path}</code>
          </dd>
        {:else}
          <dt>np</dt>
          <dd class="problem">
            {info.np.problem ?? 'not found'}
            {#if info.np.hint}<span class="hint">{info.np.hint}</span>{/if}
          </dd>
        {/if}

        <dt>Data folder</dt>
        <dd><code title="Registry, mirrored buffers of unsaved text, and Jott's own trash">{info.dataDir}</code></dd>

        <dt>Log file</dt>
        <dd><code title="One line per np request; never the contents of a note">{info.logPath}</code></dd>
      </dl>

      <footer>
        <button type="button" onclick={() => void copy()}>Copy details</button>
        <a href={info.repoUrl} onclick={(event) => external(event, info.repoUrl)}>Repository</a>
        <a href={info.docsUrl} onclick={(event) => external(event, info.docsUrl)}>Testing guide</a>
      </footer>

      {#if manual}
        <textarea class="clip" bind:this={clip} readonly value={text} rows={5}></textarea>
        <p class="quiet">Copying was refused here — select the text above and copy it yourself.</p>
      {/if}
    {:else}
      <p class="problem">Jott could not read its own details. The log file may say why.</p>
    {/if}
  </div>
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    background: color-mix(in srgb, currentColor 28%, transparent);
    display: grid;
    place-items: center;
    z-index: 10;
  }
  .panel {
    width: min(34rem, calc(100vw - 3rem));
    max-height: min(80vh, 34rem);
    overflow: auto;
    background: var(--paper);
    color: var(--ink);
    border: 1px solid color-mix(in srgb, currentColor 25%, transparent);
    border-radius: 6px;
    padding: 0.9rem 1rem 1rem;
  }
  header {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 1rem;
  }
  h2 {
    margin: 0 0 0.6rem;
    font-size: 1rem;
  }
  .close {
    border: none;
    background: none;
    color: inherit;
    font: inherit;
    cursor: default;
    padding: 0 0.2rem;
  }
  dl {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    gap: 0.3rem 0.9rem;
    margin: 0;
    font-size: 0.875rem;
  }
  dt {
    color: color-mix(in srgb, currentColor 62%, transparent);
  }
  dd {
    margin: 0;
    min-width: 0;
  }
  code {
    display: block;
    font-family: ui-monospace, 'Cascadia Mono', Consolas, monospace;
    font-size: 0.8125rem;
    overflow-wrap: anywhere;
  }
  .source {
    color: color-mix(in srgb, currentColor 62%, transparent);
    font-size: 0.8125rem;
  }
  .problem {
    color: #b8863b;
  }
  .hint {
    display: block;
    color: color-mix(in srgb, currentColor 70%, transparent);
    font-size: 0.8125rem;
  }
  footer {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    margin-top: 0.9rem;
    font-size: 0.875rem;
  }
  footer button,
  footer a {
    color: inherit;
  }
  footer button {
    border: 1px solid color-mix(in srgb, currentColor 25%, transparent);
    background: none;
    font: inherit;
    padding: 0.15rem 0.55rem;
    cursor: default;
  }
  a {
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  .quiet {
    color: color-mix(in srgb, currentColor 62%, transparent);
    font-size: 0.8125rem;
  }
  .clip {
    width: 100%;
    margin-top: 0.5rem;
    font-family: ui-monospace, 'Cascadia Mono', Consolas, monospace;
    font-size: 0.75rem;
    background: none;
    color: inherit;
    border: 1px solid color-mix(in srgb, currentColor 20%, transparent);
  }
</style>
