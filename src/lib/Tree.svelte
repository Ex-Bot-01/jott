<script lang="ts">
  // The vault tree (§9's left column). All the deciding is in `tree.ts` — which rows show, how deep they
  // sit, what the marker means — so this file only draws and reports clicks.

  import type { Row } from './tree'

  let {
    rows,
    open,
    selected,
    ontoggle,
    onselect,
    onmenu,
  }: {
    rows: Row[]
    /** Open folders. A `Set` the parent replaces, because Svelte 5 notices reassignment, not mutation. */
    open: Set<string>
    selected: string | null
    ontoggle: (rel: string) => void
    onselect: (rel: string) => void
    /** Right-click, on a row or on empty ground (`null`). The menu itself lives in the parent: it has to outlive
     *  the row it was opened from, and it needs the actions `tree.ts` decides. */
    onmenu?: (rel: string | null, x: number, y: number) => void
  } = $props()

  /** Folders are chosen by clicking the row; a note opens in the editor. Greyed rows are np's own files,
   * which have nothing to open here (§9). */
  function click(row: Row) {
    if (row.kind === 'Folder') ontoggle(row.rel)
    else if (row.kind !== 'Generated') onselect(row.rel)
  }
</script>

<ul
  class="tree"
  oncontextmenu={(event) => {
    // Empty ground: only creating is on offer there, which is `actionsFor(null)`'s business, not this file's.
    event.preventDefault()
    onmenu?.(null, event.clientX, event.clientY)
    return false
  }}
>
  {#each rows as row (row.rel)}
    <li>
      <button
        type="button"
        class:selected={selected === row.rel}
        class:greyed={row.greyed}
        class:folder={row.kind === 'Folder'}
        style:padding-inline-start={`${0.4 + row.depth * 0.85}rem`}
        title={row.title || row.rel}
        disabled={row.greyed}
        onclick={() => click(row)}
        oncontextmenu={(event) => {
          event.stopPropagation()
          event.preventDefault()
          onmenu?.(row.rel, event.clientX, event.clientY)
          return false
        }}
      >
        <span class="twisty" aria-hidden="true">
          {row.hasChildren ? (open.has(row.rel) ? '▾' : '▸') : ''}
        </span>
        {#if row.marker}
          <span class="marker" title={row.title}>{row.marker}</span>
        {:else}
          <span class="marker" aria-hidden="true"></span>
        {/if}
        <span class="name">{row.name}</span>
      </button>
    </li>
  {:else}
    <li class="empty">Nothing here yet.</li>
  {/each}
</ul>

<style>
  .tree {
    list-style: none;
    margin: 0;
    padding: 0.25rem 0;
    overflow-y: auto;
    font-size: 0.875rem;
  }
  .tree button {
    display: flex;
    align-items: center;
    gap: 0.3rem;
    width: 100%;
    padding: 0.12rem 0.4rem;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    text-align: start;
    cursor: default;
  }
  .tree button:hover:not(:disabled) {
    background: color-mix(in srgb, currentColor 8%, transparent);
  }
  .tree button.selected {
    background: color-mix(in srgb, currentColor 14%, transparent);
  }
  .tree button:disabled {
    /* np's own catalog files: visible so the folder is not mysterious, quiet so nobody edits them. */
    color: color-mix(in srgb, currentColor 45%, transparent);
    font-style: italic;
    cursor: default;
  }
  .twisty {
    width: 0.85rem;
    flex: none;
    color: color-mix(in srgb, currentColor 60%, transparent);
  }
  .marker {
    width: 0.9rem;
    flex: none;
    text-align: center;
    color: color-mix(in srgb, currentColor 70%, transparent);
  }
  .folder .name {
    font-weight: 600;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .empty {
    padding: 0.4rem 0.6rem;
    color: color-mix(in srgb, currentColor 55%, transparent);
  }
</style>
