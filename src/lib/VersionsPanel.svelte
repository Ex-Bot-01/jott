<script lang="ts">
  // The versions of the open note: pick one, see what a restore would change, put it back. A restore is itself a new
  // version, so it can be undone from this same list. Nothing is restored over typing that has not been saved.

  import type { Version } from './ipc'
  import { lineDiff, whenText } from './history'

  let {
    rel,
    versions,
    current,
    restorable,
    busy = false,
    error = null,
    loadText,
    onrestore,
    onClose,
  }: {
    rel: string
    /** Newest first; `null` while loading. */
    versions: Version[] | null
    /** The note's text as the editor holds it now. */
    current: string
    /** False while the editor holds unsaved typing. */
    restorable: boolean
    busy?: boolean
    error?: string | null
    loadText: (version: Version) => Promise<string | null>
    onrestore: (version: Version) => void
    onClose: () => void
  } = $props()

  let chosen = $state<Version | null>(null)
  let chosenText = $state<string | null>(null)
  let loading = $state(false)
  const now = Date.now()
  const diff = $derived(chosenText === null ? [] : lineDiff(chosenText, current))
  const identical = $derived(chosenText !== null && chosenText === current)

  async function choose(version: Version) {
    chosen = version
    chosenText = null
    loading = true
    const text = await loadText(version)
    if (chosen === version) {
      chosenText = text
      loading = false
    }
  }

  let panel = $state<HTMLElement | undefined>()
  $effect(() => {
    panel?.focus()
  })
</script>

<div class="scrim" role="presentation" onclick={onClose}></div>

<div class="versions" role="dialog" aria-modal="true" aria-label="Versions of {rel}" data-testid="versions" tabindex="-1" bind:this={panel}>
  <header>
    <h2>Versions of <code>{rel}</code></h2>
    <button type="button" data-testid="close-versions" onclick={onClose}>Close</button>
  </header>

  {#if error}<p class="warn" data-testid="versions-error">{error}</p>{/if}

  <div class="body">
    <ol class="list">
      {#if versions === null}
        <li class="quiet">Reading the history…</li>
      {:else if versions.length === 0}
        <li class="quiet" data-testid="versions-empty">No versions of this note yet. One is saved once you pause typing.</li>
      {:else}
        {#each versions as version (version.id)}
          <li>
            <button
              type="button"
              class:chosen={chosen?.id === version.id}
              data-testid="version-row"
              onclick={() => void choose(version)}
              title={version.message}
            >
              <span>{whenText(version.at * 1000, now)}</span>
              <span class="quiet">{version.message}{version.rel !== rel ? ` · then ${version.rel}` : ''}</span>
            </button>
          </li>
        {/each}
      {/if}
    </ol>

    <div class="preview">
      {#if chosen === null}
        <p class="quiet">Choose a version to see what restoring it would change.</p>
      {:else if loading}
        <p class="quiet">Reading that version…</p>
      {:else if chosenText === null}
        <p class="quiet">That version cannot be shown.</p>
      {:else}
        {#if identical}
          <p class="quiet" data-testid="versions-identical">This version says exactly what the note says now.</p>
        {:else}
          <p class="quiet">Green lines come back with a restore; red lines are in the note now and would go.</p>
          <pre data-testid="versions-diff">{#each diff as line}<span class={line.kind}>{line.kind === 'removed' ? '+ ' : line.kind === 'added' ? '− ' : '  '}{line.text}
</span>{/each}</pre>
        {/if}
        <div class="buttons">
          <button
            type="button"
            data-testid="version-restore"
            disabled={busy || identical || !restorable}
            onclick={() => chosen && onrestore(chosen)}
          >
            {busy ? 'Restoring…' : 'Restore this version'}
          </button>
          {#if !restorable}<span class="quiet">Save your changes first — a restore must not overwrite them.</span>{/if}
        </div>
      {/if}
    </div>
  </div>
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    background: color-mix(in srgb, var(--ink) 28%, transparent);
    z-index: 40;
  }

  .versions {
    position: fixed;
    inset: 6vh auto auto 50%;
    transform: translateX(-50%);
    width: min(56rem, 94vw);
    height: 80vh;
    display: flex;
    flex-direction: column;
    background: var(--paper);
    color: var(--ink);
    border: 1px solid var(--line);
    border-radius: 6px;
    box-shadow: 0 8px 28px rgb(0 0 0 / 28%);
    padding: 0.9rem 1rem 1rem;
    z-index: 41;
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.6rem;
  }

  h2 {
    margin: 0;
    font-size: 1rem;
  }

  .body {
    display: grid;
    grid-template-columns: minmax(12rem, 18rem) 1fr;
    gap: 0.8rem;
    min-height: 0;
    flex: 1;
    margin-top: 0.6rem;
  }

  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    overflow: auto;
    border-right: 1px solid var(--line);
  }

  .list button {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    width: 100%;
    text-align: left;
    padding: 0.35rem 0.5rem;
    background: none;
    border: none;
    border-radius: 4px;
    color: inherit;
    cursor: pointer;
  }

  .list button.chosen {
    background: color-mix(in srgb, var(--ink) 10%, transparent);
  }

  .preview {
    overflow: auto;
    min-width: 0;
  }

  pre {
    font-size: 0.82rem;
    margin: 0.3rem 0;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  /* `removed` is in the version only — a restore brings it back; `added` is in the note now only — a restore drops it. */
  .removed {
    background: color-mix(in srgb, #15803d 16%, transparent);
  }

  .added {
    background: color-mix(in srgb, #b91c1c 16%, transparent);
  }

  .quiet {
    font-size: 0.84rem;
    opacity: 0.85;
  }

  .warn {
    color: var(--warn-ink, #b45309);
  }

  .buttons {
    display: flex;
    gap: 0.6rem;
    align-items: center;
    flex-wrap: wrap;
    margin-top: 0.5rem;
  }
</style>
