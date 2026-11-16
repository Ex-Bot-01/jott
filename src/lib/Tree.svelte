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
    onselectfolder,
    selectedFolder = null,
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
    /** Which folder the toolbar will make its next thing inside, if any. Kept apart from `selected` because a note and
     *  a destination are two different claims about the same click. */
    onselectfolder?: (rel: string | null) => void
    selectedFolder?: string | null
    onmenu?: (rel: string | null, x: number, y: number) => void
  } = $props()

  /** A folder opens or closes; everything else opens in the editor — including np's own catalogue, which used to
   * swallow the click. Refusing to *look* was never the point of refusing to *write*: a tester who wondered what
   * the index said got silence, and silence reads as a broken file. Catalogue rows stay greyed (they are np's, not
   * yours) and open read-only; `readonly` on the file is what enforces that, not a disabled row. */
  function click(row: Row) {
    if (row.kind === 'Folder') {
      // One click means both things a folder row is asked for: it becomes the destination for **+ note** / **+ folder**,
      // and it opens or closes. That is what one click has always meant in a Windows file tree; here it meant only the
      // second, so "I cannot select the folder I want to create inside" was true of the window as built.
      onselectfolder?.(row.rel)
      ontoggle(row.rel)
    } else {
      // A note is not a destination. Leaving a folder selected while a note is open would make the toolbar's tooltip
      // and its behaviour disagree about where the next file goes.
      onselectfolder?.(null)
      onselect(row.rel)
    }
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
        data-kind={row.kind}
        class:selected={selected === row.rel || selectedFolder === row.rel}
        class:greyed={row.greyed}
        class:folder={row.kind === 'Folder'}
        style:padding-inline-start={`${0.4 + row.depth * 0.85}rem`}
        title={row.title || row.rel}
        aria-expanded={row.kind === 'Folder' ? row.hasChildren && open.has(row.rel) : undefined}
        onclick={() => click(row)}
        oncontextmenu={(event) => {
          event.stopPropagation()
          event.preventDefault()
          onmenu?.(row.rel, event.clientX, event.clientY)
          return false
        }}
      >
        <!-- A folder carries a mark at every age. It used to get one only once it had children, which meant the folder
             you had just made was bold text and nothing else — indistinguishable from a note, and read as "nothing
             happened". `·` is a folder with nothing in it yet: the column never jumps, and closing an empty folder does
             not pretend there was something to close. -->
        <span class="twisty" class:faint={row.kind === 'Folder' && !row.hasChildren} aria-hidden="true">
          {row.kind === 'Folder' ? (row.hasChildren ? (open.has(row.rel) ? '▾' : '▸') : '·') : ''}
        </span>
        {#if row.marker}
          <span class="marker" title={row.title}>{row.marker}</span>
        {:else}
          <span class="marker" aria-hidden="true"></span>
        {/if}
        <span class="name">{row.name}</span>
        {#if row.kind === 'Folder' && !row.hasChildren}
          <!-- Says the quiet thing out loud: it is empty, not missing. -->
          <span class="empty-hint" data-testid="folder-empty">(empty)</span>
        {/if}
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
    width: calc(100% - 0.5rem);
    margin-inline: 0.25rem;
    padding: 0.16rem 0.45rem;
    border: 0;
    border-radius: 6px;
    background: none;
    color: inherit;
    font: inherit;
    text-align: start;
    cursor: default;
  }
  .tree button:hover:not(:disabled):not(.greyed) {
    background: var(--hover);
  }
  /* Which row is open has to survive a grey-on-grey display and a screenshot, so the tint is not the only signal:
     an ink bar on the reading edge says it again in a shape no colour setting can flatten. */
  .tree button.selected {
    background: var(--selected);
    box-shadow: inset 2px 0 0 var(--ink);
  }
  /* np's own catalog files: visible so the folder is not mysterious, quiet so nobody edits them.
   *
   * This used to hang off `:disabled`, which was an attribute doing two jobs — unclickable *and* dimmed. Opening the
   * catalogue for reading dropped the first job, and the styling went with it silently: the class was still emitted,
   * no rule matched it, and three sentences in the docs kept promising a greyed row. A class needs its own rule. */
  .tree button.greyed {
    color: var(--faint);
    font-style: italic;
  }

  /* Still clickable — it opens to be read — but quieter about it than a row that will take your typing. */
  .tree button.greyed:hover {
    background: color-mix(in srgb, currentColor 4%, transparent);
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
  .twisty.faint {
    /* The dot for an empty folder is a place-holder, not a claim about contents. */
    color: color-mix(in srgb, currentColor 45%, transparent);
  }
  .empty-hint {
    margin-inline-start: 0.35rem;
    color: color-mix(in srgb, currentColor 55%, transparent);
    font-style: italic;
  }
  /* `min-width:0` so a name that genuinely does not fit gets its ellipsis inside the row instead of pushing the row
     open; the trailing padding because an italic name lost its last letter to the box — `INDEX.md` came out as
     `INDEX.ma`, since an oblique glyph's ink reaches past its advance and `overflow:hidden` cut it there. A file name
     is the one string in this window nobody may have to guess. */
  .name {
    flex: 0 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    padding-inline-end: 0.3rem;
  }
  .empty {
    padding: 0.5rem 0.7rem;
    color: var(--muted);
  }
</style>
