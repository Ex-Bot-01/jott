<script lang="ts">
  // The vault tree (§9's left column). All the deciding is in `tree.ts` — which rows show, how deep they
  // sit, what the marker means — so this file only draws and reports clicks.

  import { MARKER_LEGEND, type Row } from './tree'

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
        <!-- Colour tells it at a glance; the shape tells it to a colour-blind eye, to forced-colours mode and to a
             screenshot in a bug report. A refusal keeps its `!` rather than going quietly amber: it is the one state
             where the colour is not enough. -->
        <span class="marker" data-tone={row.tone} title={row.title}>
          <span class="disc" aria-hidden="true"></span>
          <span class="glyph" aria-hidden="true">{row.marker}</span>
        </span>
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

{#if rows.some((row) => row.kind !== 'Folder')}
  <!-- Four dots, said once, in the same words the tooltips use (`MARKER_LEGEND` is built from `markerFor`, so the
       key and a row cannot disagree). A folder carries no marker, which is why an all-folders tree leaves this out
       rather than explaining dots that appear nowhere. -->
  <div class="legend" data-testid="marker-legend">
    {#each MARKER_LEGEND as entry (entry.tone)}
      <span class="key">
        <span class="marker" data-tone={entry.tone} aria-hidden="true">
          <span class="disc"></span>
          <span class="glyph">{entry.marker}</span>
        </span>
        {entry.word}
      </span>
    {/each}
  </div>
{/if}

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
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: var(--muted);
  }
  /* The disc, coloured from the five tones `tree.ts` can emit — and only those. A test reads that file's own type
     and refuses a colour here for a tone nothing produces, or a tone with no colour, either way a dot that means
     nothing. Hues are the status line's, so green never means one thing in the tree and another at the bottom. */
  .marker .disc {
    width: 0.5rem;
    height: 0.5rem;
    border-radius: 50%;
    background: transparent;
  }
  .marker[data-tone='embedded'] .disc {
    background: #2f9e44;
  }
  .marker[data-tone='queued'] .disc {
    /* Half-filled: indexed, no vectors yet. Two solid dots of different colours would be a colour-matching test,
       and this is the one pair a person has to tell apart at 8 px. */
    background: color-mix(in srgb, #0c8599 45%, transparent);
    box-shadow: inset 0 0 0 1.5px #0c8599;
  }
  .marker[data-tone='waiting'] .disc {
    /* Hollow: nothing is in np's index. Deliberately the same drawing as "unknown" on the model dot — both mean
       nobody has measured this yet. */
    background: transparent;
    box-shadow: inset 0 0 0 1.5px color-mix(in srgb, currentColor 55%, transparent);
  }
  .marker[data-tone='refused'] .disc {
    background: #d9832b;
  }
  :global(.dark) .marker[data-tone='refused'] .disc {
    background: #d9a441;
  }
  /* The second channel, normally hidden behind the disc. */
  .marker .glyph {
    display: none;
  }
  /* A refusal is the state worth noticing, so it keeps its shape as well as its colour — amber on a grey day is
     still amber, but `!` survives even where colour does not. */
  .marker[data-tone='refused'] .disc {
    display: none;
  }
  .marker[data-tone='refused'] .glyph {
    display: inline;
    color: #b8863b;
    font-weight: 700;
  }
  @media (prefers-color-scheme: dark) {
    .marker[data-tone='refused'] .glyph {
      color: #d9a441;
    }
  }
  /* Forced colours throws away every hue above, so the glyphs are the only thing left that says which state a row
     is in. This is the case a colour-only design forgets. */
  @media (forced-colors: active) {
    .marker .disc {
      display: none;
    }
    .marker .glyph {
      display: inline;
    }
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
  /* Small, muted, and out of the way until a person wants it — four lines of key would crowd out the notes. */
  .legend {
    display: flex;
    flex-wrap: wrap;
    gap: 0.15rem 0.75rem;
    padding: 0.4rem 0.7rem 0.5rem;
    margin-top: auto;
    border-top: 1px solid var(--line);
    font-size: 0.6875rem;
    color: var(--muted);
  }
  .legend .key {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    white-space: nowrap;
  }
  .legend .marker {
    width: auto;
  }
</style>
