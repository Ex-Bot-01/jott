<script lang="ts">
  // The window: tree on the left, note in the middle, one line about the index along the bottom (§9).
  // Everything factual here came from a command or an event; the decisions live in the tested modules
  // (`tree.ts`, `saveState.ts`, `statusLine.ts`) and in `jott-core`.

  import { onMount } from 'svelte'
  import About from './lib/About.svelte'
  import Tree from './lib/Tree.svelte'
  import Editor from './lib/Editor.svelte'
  import StatusLine from './lib/StatusLine.svelte'
  import { flattenTree, notesAwaitingIndex } from './lib/tree'
  import { statusFor, type Status } from './lib/statusLine'
  import type { SavePhase } from './lib/saveState'
  import * as ipc from './lib/ipc'
  import type { AboutInfo, IngestEvent, NoteContent, RegistryInfo, SearchResults, VaultListing } from './lib/ipc'

  let registry = $state<RegistryInfo | null>(null)
  let listing = $state<VaultListing | null>(null)
  /** Open folders, for this run of the app (§9). A `Set` that gets replaced rather than mutated. */
  let openFolders = $state<Set<string>>(new Set())
  let selected = $state<string | null>(null)
  let note = $state<NoteContent | null>(null)
  /** Bumped to put text into the editor from outside: a silent reload, or an accepted mirror (§14). */
  let external = $state<{ text: string; token: number } | null>(null)
  let externalToken = 0
  /** "Keep typing" was pressed for this note, so its conflict banner stands down. */
  let keptRel = $state<string | null>(null)
  /** The editor's own phase, reported rather than guessed at here (§14). */
  let editorPhase = $state<SavePhase>('clean')

  let status = $state<Status | null>(null)
  let consent = $state<Extract<IngestEvent, { needsConsent: unknown }>['needsConsent'] | null>(null)
  let npMissing = $state<ipc.NpMissingPayload | null>(null)
  /** One line of what went wrong, in the command's own words (§4). */
  let problem = $state<string | null>(null)
  /** And one line of what just happened, for actions with no other visible effect. */
  let notice = $state<string | null>(null)
  /** The About panel: both versions and which np file this run picked. Fetched on first open, not at
      startup — it may ask np a question, and the window should not wait on that to paint (§13). */
  let about = $state<AboutInfo | null>(null)
  /** Notes that have never been indexed. Opening a folder runs no pass — indexing writes into the vault
      (`INDEX.md`), so it stays something you ask for once, not something that happens to a folder you were
      only looking at (§11). This is what makes "I opened it and nothing happened" sayable. */
  let awaitingIndex = $derived(listing ? notesAwaitingIndex(listing.nodes) : 0)
  let showAbout = $state(false)
  let aboutLoading = $state(false)

  let searchQuery = $state('')
  let results = $state<SearchResults | null>(null)
  let searching = $state(false)
  let searchTimer: ReturnType<typeof setTimeout> | undefined

  let dialog = $state<{ kind: 'new' | 'rename'; value: string } | null>(null)
  let confirmDelete = $state<string | null>(null)

  const rows = $derived(flattenTree(listing?.nodes ?? [], openFolders))
  /** A folder is "open" only while its parent is; `flattenTree` handles the hiding, this holds the wish. */
  const vaultName = $derived(listing ? basename(listing.vault) : null)

  function basename(path: string): string {
    const trimmed = path.replace(/[\\/]+$/, '')
    return trimmed.slice(Math.max(trimmed.lastIndexOf('/'), trimmed.lastIndexOf('\\')) + 1) || trimmed
  }

  async function call<T>(what: () => Promise<T>): Promise<T | null> {
    try {
      problem = null
      return await what()
    } catch (error) {
      // Tauri hands back the serialized `IpcError`; its `message` is already a sentence for a person.
      const message = typeof error === 'object' && error !== null && 'message' in error ? String((error as { message: unknown }).message) : String(error)
      problem = message
      return null
    }
  }

  async function refresh() {
    const next = await call(ipc.listVault)
    listing = next
  }

  async function openVault(path: string) {
    const next = await call(() => ipc.openVault(path))
    if (next) {
      listing = next
      openFolders = new Set()
      selected = null
      note = null
    }
  }

  async function openNote(rel: string) {
    const content = await call(() => ipc.readNote(rel))
    if (!content) return
    note = content
    selected = rel
    keptRel = null
  }

  function toggleFolder(rel: string) {
    const next = new Set(openFolders)
    if (next.has(rel)) next.delete(rel)
    else next.add(rel)
    openFolders = next
  }

  /**
   * A write that came from the editor. Deliberately not wrapped in `call`: the editor has to see a failed
   * save to keep its own state honest, and the sentence travels back with the rejection (§4).
   */
  async function saveThroughCommand(rel: string, text: string) {
    const result = await ipc.saveNote(rel, text)
    // The answer's hash is of the bytes that landed, so the buffer can stop calling itself unsaved.
    if (selected === rel && note) note = { ...note, diskHash: result.hash, diskMtimeMs: result.mtimeMs }
    return result
  }

  async function openAbout() {
    showAbout = true
    if (about) return
    aboutLoading = true
    about = await call(() => ipc.appInfo())
    aboutLoading = false
  }

  async function pickFolder() {
    // `null` is a cancelled dialog, not a failure: whatever was on screen stays as it was.
    const next = await call(ipc.pickVault)
    if (!next) return
    listing = next
    openFolders = new Set()
    selected = null
    note = null
  }

  /** §18.13's two answers. Both take the offer off screen; only *discard* touches the mirror file, since
   * using it means the next save overwrites it with the same bytes anyway. */
  function useMirror() {
    if (!note?.mirrored) return
    external = { text: note.mirrored.text, token: ++externalToken }
    note = withoutMirror(note)
  }

  function discardMirror() {
    if (!note?.mirrored) return
    void ipc.discardMirror(note.rel)
    note = withoutMirror(note)
  }

  function withoutMirror(content: NoteContent): NoteContent {
    return {
      rel: content.rel,
      text: content.text,
      hash: content.hash,
      diskHash: content.diskHash,
      diskMtimeMs: content.diskMtimeMs,
      mirrored: undefined,
    }
  }

  function reportPhase(phase: SavePhase) {
    editorPhase = phase
  }

  async function onVaultChanged(rel: string) {
    if (rel === selected) {
      const fresh = await call(() => ipc.readNote(rel))
      if (!fresh) return
      if (editorPhase === 'clean' || editorPhase === 'saved') {
        // §14: nothing of ours is at risk, so the file simply wins and says nothing about it.
        note = fresh
        external = { text: fresh.text, token: ++externalToken }
        keptRel = null
      } else {
        // Something is unsaved. Update what the disk says and let the editor's own phase raise the banner.
        note = { ...(note as NoteContent), diskHash: fresh.diskHash, diskMtimeMs: fresh.diskMtimeMs }
      }
      return
    }
    await refresh()
  }

  async function runSearch() {
    const query = searchQuery.trim()
    if (!query) {
      results = null
      return
    }
    searching = true
    const answer = await call(() => ipc.searchVault(query, 12))
    results = answer
    searching = false
  }

  function scheduleSearch() {
    clearTimeout(searchTimer)
    // 250 ms of quiet before asking np: each keystroke would otherwise start a search nobody finishes.
    searchTimer = setTimeout(() => void runSearch(), 250)
  }

  async function createNote(rel: string) {
    const created = await call(() => ipc.createNote(rel))
    dialog = null
    if (created) {
      await refresh()
      await openNote(rel)
    }
  }

  async function renameEntry(to: string) {
    const from = selected
    dialog = null
    if (!from) return
    const outcome = await call(() => ipc.renameEntry(from, to))
    if (!outcome) return
    notice = outcome.incomingLinks.length
      ? `renamed · ${outcome.incomingLinks.length} note${outcome.incomingLinks.length === 1 ? '' : 's'} still refer${outcome.incomingLinks.length === 1 ? 's' : ''} to the old name`
      : 'renamed'
    selected = to
    await refresh()
  }

  async function deleteEntry(rel: string) {
    const said = await call(() => ipc.deleteEntry(rel))
    confirmDelete = null
    if (said) {
      notice = said
      if (selected === rel) {
        selected = null
        note = null
      }
      await refresh()
    }
  }

  async function answerConsent(granted: boolean) {
    consent = null
    await call(() => ipc.setConsent(granted))
  }

  onMount(async () => {
    registry = await call(ipc.registryInfo)
    // §9: the window comes back to the folder it had. A vault that has gone missing is not an error worth
    // shouting about at startup — the empty state asks for a folder instead.
    const last = registry?.recent[0]
    if (last) {
      // A vault that has gone missing is not worth a shout at startup; the empty state asks instead.
      const reopened = await ipc.openVault(last).catch(() => null)
      if (reopened) listing = reopened
    } else {
      await refresh()
    }

    void ipc.onIngestEvent((event) => {
      if ('needsConsent' in event) consent = event.needsConsent
      status = statusFor(event)
      // A finished pass changes every marker in the tree, and the tree is a snapshot of the stamps as they
      // were when it was listed. Without this re-list the status line says "completed" over dots that still
      // say "waiting for the index" — which is exactly what a tester reported and reasonably called confusing.
      if ('indexed' in event || 'embedded' in event || 'refused' in event || 'disabled' in event) void refresh()
    })
    void ipc.onVaultChanged((change) => void onVaultChanged(change.rel))
    void ipc.onNpMissing((payload) => (npMissing = payload))
  })

</script>

<main class="window">
  <aside class="side">
    <header class="vault">
      {#if vaultName}
        <span class="vault-name" title={listing?.vault}>{vaultName}</span>
        <button type="button" onclick={() => void pickFolder()}>Open…</button>
      {:else}
        <button type="button" onclick={() => void pickFolder()}>Open a folder</button>
      {/if}
      <button
        type="button"
        class="about"
        aria-label="About Jott"
        title="Versions, which np this is running, and where the files are"
        onclick={() => void openAbout()}
      >
        About
      </button>
    </header>

    <div class="tools">
      <input
        type="search"
        placeholder="Search this vault"
        bind:value={searchQuery}
        oninput={scheduleSearch}
        aria-label="Search the vault"
      />
      <span class="tool-buttons">
        <button type="button" title="New note" onclick={() => (dialog = { kind: 'new', value: '' })}>+ note</button>
        <button
          type="button"
          title="Rename this note"
          disabled={selected === null}
          onclick={() => selected && (dialog = { kind: 'rename', value: selected })}
        >
          rename
        </button>
        <button type="button" title="Move this note to the trash" disabled={selected === null} onclick={() => (confirmDelete = selected)}>
          delete
        </button>
      </span>
    </div>

    {#if results}
      <ul class="results">
        {#each results.hits as hit (hit.path)}
          <li>
            <button type="button" onclick={() => void openNote(hit.path)}>
              <span class="hit-path">{hit.path}</span>
              <span class="hit-snippet">{hit.snippet ?? ''}</span>
            </button>
          </li>
        {:else}
          <li class="none">
            Nothing matched.
            {#if results.offline}(offline — keyword search only){/if}
          </li>
        {/each}
        <li class="mode">{results.mode}{searching ? ' · searching…' : ''}</li>
      </ul>
    {:else}
      <Tree {rows} open={openFolders} selected={selected} ontoggle={toggleFolder} onselect={(rel) => void openNote(rel)} />
    {/if}

    {#if listing && awaitingIndex > 0 && !selected}
      <div class="first-pass">
        <span>
          {awaitingIndex === 1 ? '1 note is' : `${awaitingIndex} notes are`} not indexed yet, so search will
          find nothing.
        </span>
        <button type="button" onclick={() => void ipc.ingestAll()}>Index now</button>
      </div>
    {/if}

    <div class="warnings">
      {#if npMissing}
        <p class="np">
          {npMissing.message}
          <span class="hint">{npMissing.installHint}</span>
        </p>
      {/if}
      {#each listing?.warnings ?? [] as warning (warning)}
        <!-- Sentences from the backend, shown as written (§4). -->
        <p class="warn">{warning}</p>
      {/each}
      {#if problem}
        <p class="warn">{problem}</p>
      {/if}
      {#if notice}
        <p class="note-said">
          {notice}
          <button type="button" onclick={() => (notice = null)}>ok</button>
        </p>
      {/if}
    </div>
  </aside>

  <section class="middle">
    {#if dialog}
      <form
        class="ask"
        onsubmit={(event) => {
          event.preventDefault()
          const value = dialog?.value.trim() ?? ''
          if (!value) return
          if (dialog?.kind === 'new') void createNote(value)
          else void renameEntry(value)
        }}
      >
        <label for="rel">{dialog.kind === 'new' ? 'New note, in a path from the vault root:' : 'Rename to:'}</label>
        <input id="rel" bind:value={dialog.value} spellcheck="false" autocomplete="off" />
        <span>
          <button type="submit">OK</button>
          <button type="button" onclick={() => (dialog = null)}>Cancel</button>
        </span>
      </form>
    {/if}

    {#if confirmDelete}
      <div class="ask">
        <span>Move <code>{confirmDelete}</code> to the trash? np will drop it from the index on the next pass.</span>
        <span>
          <button type="button" onclick={() => confirmDelete && void deleteEntry(confirmDelete)}>Trash it</button>
          <button type="button" onclick={() => (confirmDelete = null)}>Keep it</button>
        </span>
      </div>
    {/if}

    <Editor
      rel={selected}
      {note}
      {external}
      kept={keptRel === selected}
      onphase={reportPhase}
      onsave={saveThroughCommand}
      onmirror={(rel, text) => void ipc.mirrorBuffer(rel, text)}
      onusemirror={useMirror}
      ondiscardmirror={discardMirror}
      onreload={() => {
        if (!selected) return
        const rel = selected
        void (async () => {
          const fresh = await call(() => ipc.readNote(rel))
          if (!fresh) return
          note = fresh
          external = { text: fresh.text, token: ++externalToken }
          keptRel = null
        })()
      }}
      onkeep={() => (keptRel = selected)}
    />
  </section>

  <StatusLine {status} consent={consent} onembed={() => void ipc.ingestAll()} onconsent={(granted) => void answerConsent(granted)} />

  {#if showAbout}
    <About info={about} loading={aboutLoading} onClose={() => (showAbout = false)} />
  {/if}
</main>

<style>
  .window {
    display: grid;
    grid-template-columns: minmax(14rem, 22rem) 1fr;
    grid-template-rows: 1fr auto;
    height: 100vh;
    font: 14px/1.5 ui-sans-serif, system-ui, 'Segoe UI', sans-serif;
  }
  .side {
    grid-row: 1;
    display: flex;
    flex-direction: column;
    min-height: 0;
    border-right: 1px solid color-mix(in srgb, currentColor 18%, transparent);
  }
  .middle {
    grid-column: 2;
    grid-row: 1;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  :global(.status) {
    grid-column: 1 / -1;
  }
  .vault {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.4rem;
    padding: 0.4rem 0.5rem;
  }
  /* The one thing in the header that is not about the vault, so it reads as a different kind of control. */
  .about {
    border: none;
    background: none;
    color: color-mix(in srgb, currentColor 62%, transparent);
    font: inherit;
    font-size: 0.8125rem;
    padding: 0.1rem 0.3rem;
    cursor: default;
  }
  .vault-name {
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .tools {
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
    padding: 0 0.5rem 0.4rem;
  }
  .tools input {
    width: 100%;
    font: inherit;
    padding: 0.2rem 0.35rem;
    border: 1px solid color-mix(in srgb, currentColor 25%, transparent);
    background: none;
    color: inherit;
  }
  .tool-buttons {
    display: flex;
    gap: 0.3rem;
  }
  button {
    border: 1px solid color-mix(in srgb, currentColor 25%, transparent);
    background: none;
    color: inherit;
    font: inherit;
    padding: 0.08rem 0.45rem;
    cursor: default;
  }
  button:hover:not(:disabled) {
    background: color-mix(in srgb, currentColor 10%, transparent);
  }
  button:disabled {
    opacity: 0.45;
  }
  .results {
    list-style: none;
    margin: 0;
    padding: 0;
    overflow-y: auto;
    font-size: 0.8125rem;
  }
  .results li button {
    display: block;
    width: 100%;
    border: 0;
    text-align: start;
    padding: 0.3rem 0.5rem;
    line-height: 1.35;
  }
  .hit-path {
    display: block;
    font-size: 0.75rem;
    color: color-mix(in srgb, currentColor 60%, transparent);
  }
  .hit-snippet {
    display: block;
    white-space: normal;
  }
  .results .none,
  .results .mode {
    padding: 0.35rem 0.5rem;
    color: color-mix(in srgb, currentColor 60%, transparent);
  }
  .first-pass {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.35rem 0.6rem;
    font-size: 0.8125rem;
    border-top: 1px solid color-mix(in srgb, currentColor 12%, transparent);
  }
  .first-pass button {
    border: 1px solid color-mix(in srgb, currentColor 25%, transparent);
    background: none;
    color: inherit;
    font: inherit;
    padding: 0.05rem 0.45rem;
    cursor: default;
  }
  .warnings {
    margin-top: auto;
    padding: 0.35rem 0.5rem;
    font-size: 0.75rem;
  }
  .warnings p {
    margin: 0.2rem 0;
  }
  .warnings .hint {
    display: block;
    color: color-mix(in srgb, currentColor 60%, transparent);
  }
  .warn,
  .np {
    color: #b8863b;
  }
  :global(.dark) .warn,
  :global(.dark) .np {
    color: #d9a441;
  }
  .ask {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex-wrap: wrap;
    padding: 0.4rem 0.75rem;
    background: color-mix(in srgb, currentColor 8%, transparent);
    font-size: 0.8125rem;
  }
  .ask input {
    flex: 1;
    min-width: 10rem;
    font: inherit;
    padding: 0.15rem 0.35rem;
    border: 1px solid color-mix(in srgb, currentColor 25%, transparent);
    background: none;
    color: inherit;
  }
</style>
