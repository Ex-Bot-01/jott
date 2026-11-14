<script lang="ts">
  // The window: tree on the left, note in the middle, one line about the index along the bottom (§9).
  // Everything factual here came from a command or an event; the decisions live in the tested modules
  // (`tree.ts`, `saveState.ts`, `statusLine.ts`) and in `jott-core`.

  import { onMount } from 'svelte'
  import About from './lib/About.svelte'
  import Diagnostics from './lib/Diagnostics.svelte'
  import Tree from './lib/Tree.svelte'
  import Editor from './lib/Editor.svelte'
  import StatusLine from './lib/StatusLine.svelte'
  import { actionsFor, flattenTree, moveTargets, moveTo, notesAwaitingEmbedding, notesAwaitingIndex, targetFolder } from './lib/tree'
  import type { TreeAction } from './lib/tree'
  import { bareKeyFor, isTypingTarget, shortcutFor } from './lib/shortcuts'
  import { emptySearchSentence, resultsHeadline, searchCaveat, searchIsCurrent } from './lib/searchState'
  import { markTerms } from './lib/highlight'
  import { consentSentence, statusFor, type Status } from './lib/statusLine'
  import type { SavePhase } from './lib/saveState'
  import * as ipc from './lib/ipc'
  import type { DiagnosticsSnapshot, TraceEvent } from './lib/diagnostics'
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
  /** Indexed but never embedded: search finds these notes' words and not their meanings. */
  let awaitingEmbed = $derived(listing ? notesAwaitingEmbedding(listing.nodes) : 0)
  /** What np would need to set embeddings up — its model, its folder, the size when anybody measured one.
      `null` means either "nothing to offer" or "not asked yet for this vault", and `offerAskedFor` says which. */
  let embeddingsOffer = $state<ipc.ConsentOffer | null>(null)
  let offerAskedFor: string | null = null
  let showAbout = $state(false)
  let aboutLoading = $state(false)

  /** The diagnostics drawer. Its rows accumulate from launch rather than from the moment it opens: the question
      that makes somebody press Ctrl+Shift+D is always about something that already happened. */
  let showDiagnostics = $state(false)
  let snapshot = $state<DiagnosticsSnapshot | null>(null)
  let traceRows = $state<TraceEvent[]>([])

  let searchQuery = $state('')
  /** The query that produced the hit this note was opened from. Kept apart from `searchQuery` on purpose: the
   * box keeps being typed in, and marks should follow the hit that was clicked, not every keystroke after it. */
  let hitQuery = $state('')
  let results = $state<SearchResults | null>(null)
  let searching = $state(false)
  let searchTimer: ReturnType<typeof setTimeout> | undefined

  /** `folder` is where a new note or folder lands; `rel` is what is being moved. */
  let dialog = $state<{ kind: 'new' | 'rename' | 'folder' | 'move'; value: string; folder?: string; rel?: string } | null>(null)
  let confirmDelete = $state<string | null>(null)
  /** The row menu: where it was opened, and for what. `null` means empty ground — only creating is on offer there. */
  let menu = $state<{ rel: string | null; x: number; y: number } | null>(null)
  /** Bumped to ask the editor to write its buffer now (Ctrl+S with focus outside it). */
  let saveSignal = $state(0)
  let searchBox = $state<HTMLInputElement | null>(null)

  const rows = $derived(flattenTree(listing?.nodes ?? [], openFolders))
  /** A folder is "open" only while its parent is; `flattenTree` handles the hiding, this holds the wish. */
  const vaultName = $derived(listing ? basename(listing.vault) : null)

  function actionLabel(action: TreeAction): string {
    switch (action) {
      case 'new-note':
        return 'New note here'
      case 'new-folder':
        return 'New folder here'
      case 'rename':
        return 'Rename  (F2)'
      case 'move':
        return 'Move to…'
      case 'delete':
        return 'Move to trash  (Delete)'
    }
  }

  /** The folders the registry remembers, minus the one already open: a recent that is current is noise. */
  const recents = $derived((registry?.recent ?? []).filter((folder) => folder !== listing?.vault))

  async function openRecent(folder: string) {
    if (!folder) return
    const next = await call(() => ipc.openVault(folder))
    if (next) listing = next
  }

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

  /** One pass fires several of these events, and each wants the tree again. A re-list is a walk of the whole
      folder — every note hashed that has not moved since it was last read — so three events in half a second
      used to mean three walks of the same unchanged bytes. The last one is the one worth having. */
  let relistTimer: ReturnType<typeof setTimeout> | undefined

  function scheduleRelist() {
    clearTimeout(relistTimer)
    relistTimer = setTimeout(() => void refresh(), 150)
  }

  async function refresh() {
    const next = await call(ipc.listVault)
    listing = next
    void askOffer()
  }

  /** Once per opened vault: is there still a download to talk about? Asked here rather than assumed from the
      tree, because "not embedded" has two causes with opposite answers — the bytes are on disk and embedding
      is merely owed work, or nobody has ever agreed to fetch them. Only np knows which (§18.6). */
  async function askOffer() {
    const vault = listing?.vault ?? null
    // Nothing awaiting vectors, nothing to offer. This is also what keeps a folder that merely *opened* from
    // asking np a question — and asking means starting the second connection, which is ~8 MB parked for a
    // dialog nobody is being shown (0.0.8's ruling on lazy connections, applied to my own new call site).
    if (!vault || offerAskedFor === vault || awaitingEmbed === 0) return
    offerAskedFor = vault
    embeddingsOffer = await call(ipc.embeddingsOffer)
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

  async function openNote(rel: string, fromQuery = '') {
    const content = await call(() => ipc.readNote(rel))
    if (!content) return
    note = content
    selected = rel
    // The marks follow the hit that was clicked. Clicking a row in the tree clears them, even though the
    // search box still holds what was typed — those are different notes with different reasons to be open.
    hitQuery = fromQuery
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

  /** Both versions and which np file this run picked. Fetched once, whoever asks: the drawer needs the Jott
      version for "copy details" too, and opening it should not be a second round trip to the same facts. */
  async function loadAbout(): Promise<void> {
    if (about || aboutLoading) return
    aboutLoading = true
    about = await call(() => ipc.appInfo())
    aboutLoading = false
  }

  async function openAbout() {
    showAbout = true
    await loadAbout()
  }

  async function openDiagnostics() {
    showDiagnostics = true
    void loadAbout()
    // Asked again every time it opens. The queue and the counts are facts about right now, and a drawer stuck on
    // the numbers from ten minutes ago is precisely how a debugging screen loses its credibility.
    snapshot = await call(ipc.diagnostics)
  }

  /** The window's whole keyboard, decided by `shortcuts.ts` so one table says what every binding does. */
  async function onWindowKey(event: KeyboardEvent) {
    const shortcut = shortcutFor(event)
    if (shortcut !== null) {
      // Ctrl+S inside the editor is CodeMirror's own binding and it already prevented this event; asking again
      // would save a second time, which is at best a wasted round trip to np.
      if (shortcut === 'save') {
        // Ask before preventing: the editor's own binding has already taken the key when it has focus, and a second
        // save for one keystroke is its own bug.
        const alreadyHandled = event.defaultPrevented
        event.preventDefault()
        if (!alreadyHandled) saveSignal += 1
        return
      }
      if (shortcut === 'close-drawer') {
        menu = null
        return
      }
      event.preventDefault()
      if (shortcut === 'diagnostics') {
        // Ctrl+Shift+D, because "show me what the backend did" should not need a menu tour — and because the
        // alternative somebody reaches for otherwise is the log file, in a text editor, at 11pm.
        if (showDiagnostics) showDiagnostics = false
        else await openDiagnostics()
        return
      }
      if (shortcut === 'search') {
        searchBox?.focus()
        searchBox?.select()
        return
      }
      runAction(shortcut === 'new-note' ? 'new-note' : shortcut === 'new-folder' ? 'new-folder' : 'rename')
      return
    }

    // F2 and Delete belong to the selected row — and stop belonging to it the moment focus is somewhere someone
    // is typing, where those keys mean "type this".
    const bare = bareKeyFor(event)
    if (bare !== null && selected !== null && !isTypingTarget(event.target)) {
      event.preventDefault()
      runAction(bare)
    }
  }

  /** One place turns an intention into a dialog or a command, whether it came from the menu, a button or a key. */
  function runAction(action: TreeAction, on?: string | null) {
    // Which row the intention is about: the one the menu was opened on, else the selected one. Read before `menu`
    // is cleared, or every action would silently fall back to the root of the vault.
    const row = rowFor(on === undefined ? menu?.rel ?? null : on)
    menu = null
    switch (action) {
      case 'new-note':
        dialog = { kind: 'new', value: '', folder: targetFolder(row) }
        return
      case 'new-folder':
        dialog = { kind: 'folder', value: '', folder: targetFolder(row) }
        return
      case 'rename':
        if (selected === null) return
        dialog = { kind: 'rename', value: selected }
        return
      case 'move':
        if (selected === null) return
        dialog = { kind: 'move', value: '', rel: selected }
        return
      case 'delete':
        if (selected === null) return
        confirmDelete = selected
        return
    }
  }

  /** The row a menu was opened on, or the selected one when the action came from a key. */
  function rowFor(rel: string | null) {
    const which = rel ?? selected
    return which === null ? null : rows.find((candidate) => candidate.rel === which) ?? null
  }

  function openMenu(rel: string | null, x: number, y: number) {
    if (rel !== null) selected = rel
    menu = { rel, x, y }
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

  /** Counts up with every request so a late answer can be recognised as late. */
  let searchSeq = 0

  async function runSearch() {
    const query = searchQuery.trim()
    if (!query) {
      results = null
      searching = false
      return
    }
    const seq = ++searchSeq
    searching = true
    const answer = await call(() => ipc.searchVault(query, 12))
    // Typing "abc" fires three requests and np is free to answer them in any order. Without this guard the slowest
    // one wins, which reads as the search box ignoring what you typed last.
    if (!searchIsCurrent(seq, searchSeq)) return
    results = answer
    searching = false
  }

  function scheduleSearch() {
    clearTimeout(searchTimer)
    // 250 ms of quiet before asking np: each keystroke would otherwise start a search nobody finishes.
    searchTimer = setTimeout(() => void runSearch(), 250)
  }

  /** Where a dialog's new thing goes: the folder it was asked for, spelled as a `rel` prefix. */
  function prefixed(dialog: { folder?: string }, name: string): string {
    return `${dialog.folder ?? ''}${name}`
  }

  async function createFolder(rel: string) {
    const made = await call(() => ipc.createFolder(rel))
    dialog = null
    if (made !== null) await refresh()
  }

  async function moveEntry(from: string, to: string) {
    const moved = await call(() => ipc.moveEntry(from, to))
    dialog = null
    if (moved !== null) {
      await refresh()
      if (selected === from) selected = to
    }
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
    embeddingsOffer = null
    offerAskedFor = null
    await call(() => ipc.setConsent(granted))
  }

  /** The same question, asked again later. "Stay keyword-only" answers the moment, not the year, and the way
      back should not be editing a TOML file — so the offer sits under the tree until it is taken up, and
      pressing it says exactly what it will do before it does it. */
  async function setUpEmbeddings() {
    const offer = embeddingsOffer
    if (!offer) return
    const sentence = consentSentence(offer)
    if (!(await ipc.askConfirm(sentence, 'Set up semantic search'))) return
    embeddingsOffer = null
    offerAskedFor = null
    await call(() => ipc.setConsent(true))
    // "Yes" means go, not "yes, whenever the next save happens": the vault goes through the pass now.
    await call(ipc.ingestAll)
    notice = 'Downloading the model and embedding your notes. The line at the bottom keeps count.'
    await refresh()
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
      if ('indexed' in event || 'embedded' in event || 'refused' in event || 'disabled' in event) scheduleRelist()
    })
    // Bounded: a long run produces thousands of rows, and core's ring holds two hundred anyway. The drawer shows
    // the newest ones, so dropping the oldest loses nothing that was still on screen.
    void ipc.onTraceRow((row) => {
      traceRows = [...traceRows.slice(-399), row]
    })

    void ipc.onVaultChanged((change) => void onVaultChanged(change.rel))
    void ipc.onNpMissing((payload) => (npMissing = payload))
  })

</script>

<svelte:window
  onkeydown={(event) => void onWindowKey(event)}
  onclick={() => (menu = null)}
/>

<main class="window">
  <aside class="side">
    <header class="vault">
      {#if vaultName}
        <span class="vault-name" title={listing?.vault}>{vaultName}</span>
        <button type="button" onclick={() => void pickFolder()}>Open…</button>
        {#if recents.length > 0}
          <!-- The registry has kept a list of folders since Task 6 and showed none of it. Reopening yesterday's
               notes was a dialog tour; now it is a click. -->
          <select
            class="recents"
            aria-label="Recent folders"
            onchange={(event) => void openRecent(event.currentTarget.value)}
            value=""
          >
            <option value="" disabled>recent…</option>
            {#each recents as folder (folder)}
              <option value={folder}>{basename(folder)}</option>
            {/each}
          </select>
        {/if}
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
        bind:this={searchBox}
        type="search"
        placeholder="Search this vault (Ctrl+F)"
        bind:value={searchQuery}
        oninput={scheduleSearch}
        aria-label="Search the vault"
      />
      <span class="tool-buttons">
        <button type="button" title="New note (Ctrl+N)" onclick={() => runAction('new-note', null)}>+ note</button>
        <button type="button" title="New folder (Ctrl+Shift+N)" onclick={() => runAction('new-folder', null)}>
          + folder
        </button>
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
      {@const caveat = searchCaveat(results, results.hits.length)}
      <ul class="results">
        {#if results.hits.length === 0}
          <!-- An empty list is two different facts and only one of them is about the words. np 0.4.0 answers
               `indexed` and settles it (note-processor#5); behind that, the sentence comes from what the tree's own
               markers know — never from silence. -->
          <li class="none">
            {emptySearchSentence(results, searchQuery.trim(), awaitingIndex)}
            {#if results.offline}(np is offline — keyword search only){/if}
          </li>
        {/if}
        {#each results.hits as hit (hit.path)}
          <li>
            <button type="button" onclick={() => void openNote(hit.path, searchQuery.trim())}>
              <span class="hit-path">{hit.path}</span>
              <span class="hit-snippet">
                {#each markTerms(hit.snippet ?? '', searchQuery) as part, index}
                  {#if part.matched}<mark>{part.text}</mark>{:else}{part.text}{/if}
                {/each}
              </span>
            </button>
          </li>
        {/each}
        {#if resultsHeadline(results.hits.length)}
          <!-- A count, not np's `mode` word: 0.3.1 answers "hybrid" for searches where only keywords ran. What
               actually ran goes on its own line, in `searchCaveat`'s words or np's own sentence. -->
          <li class="mode">
            {resultsHeadline(results.hits.length)}{searching ? ' · searching…' : ''}
            {#if caveat}<span class="caveat">· {caveat}</span>{/if}
          </li>
        {:else}
          {#if caveat}<li class="mode quiet">{caveat}</li>{/if}
          {#if searching}<li class="mode quiet">searching…</li>{/if}
        {/if}
      </ul>
    {:else}
      <Tree
        {rows}
        open={openFolders}
        selected={selected}
        ontoggle={toggleFolder}
        onselect={(rel) => void openNote(rel)}
        onmenu={openMenu}
      />
    {/if}

    {#if listing && awaitingIndex > 0 && !selected}
      <div class="first-pass">
        <span>
          {awaitingIndex === 1 ? '1 note is' : `${awaitingIndex} notes are`} not indexed yet, so search will
          find nothing.
        </span>
        <button type="button" onclick={() => void ipc.ingestAll()}>Index now</button>
      </div>
    {:else if listing && awaitingEmbed > 0 && !consent && embeddingsOffer}
      <div class="first-pass" data-testid="embeddings-offer">
        <span>
          {awaitingEmbed === 1 ? '1 note is' : `${awaitingEmbed} notes are`} indexed but not embedded, so
          search finds their words and not their meanings.
        </span>
        <button type="button" onclick={() => void setUpEmbeddings()}>Set up embeddings</button>
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

  {#if menu !== null}
    <!-- A real menu, on the row that was clicked, offering only what `actionsFor` says can be done to it. -->
    <div class="row-menu" style:left="{menu.x}px" style:top="{menu.y}px" role="menu" tabindex="-1">
      {#each actionsFor(menu.rel === null ? null : rows.find((row) => row.rel === menu?.rel) ?? null) as action (action)}
        <button
          type="button"
          role="menuitem"
          onclick={() => runAction(action, menu?.rel ?? null)}
        >
          {actionLabel(action)}
        </button>
      {:else}
        <span class="quiet">Nothing to do here.</span>
      {/each}
    </div>
  {/if}

  <section class="middle">
    {#if dialog}
      <form
        class="ask"
        onsubmit={(event) => {
          event.preventDefault()
          const value = dialog?.value.trim() ?? ''
          if (!value) return
          if (dialog?.kind === 'new') void createNote(prefixed(dialog, value))
          else if (dialog?.kind === 'folder') void createFolder(prefixed(dialog, value))
          else if (dialog?.kind === 'move' && dialog.rel) void moveEntry(dialog.rel, moveTo(dialog.rel, value))
          else void renameEntry(value)
        }}
      >
        {#if dialog.kind === 'move'}
          <label for="rel">Move {dialog.rel} into:</label>
          <select id="rel" bind:value={dialog.value}>
            {#each moveTargets(rows, dialog.rel ?? '') as folder (folder)}
              <option value={folder}>{folder === '' ? 'the vault root' : folder}</option>
            {/each}
          </select>
        {:else}
          <label for="rel">
            {dialog.kind === 'new'
              ? `New note${dialog.folder ? ` in ${dialog.folder}` : ', in a path from the vault root'}:`
              : dialog.kind === 'folder'
                ? `New folder${dialog.folder ? ` in ${dialog.folder}` : ':'}`
                : 'Rename to:'}
          </label>
          <input id="rel" bind:value={dialog.value} spellcheck="false" autocomplete="off" />
        {/if}
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
      saveSignal={saveSignal}
      highlight={hitQuery}
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

  {#if showDiagnostics}
    <Diagnostics
      snapshot={snapshot}
      live={traceRows}
      jottVersion={about?.jottVersion ?? 'unknown'}
      onClose={() => (showDiagnostics = false)}
    />
  {/if}

  {#if showAbout}
    <About
      info={about}
      loading={aboutLoading}
      onDiagnostics={() => void openDiagnostics()}
      onClose={() => (showAbout = false)}
    />
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
  .recents {
    max-width: 8rem;
    font: inherit;
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
  .hit-snippet mark {
    /* Same ruler as the editor's marks: a tint of the ink, so it reads on light and dark paper alike. */
    background: color-mix(in srgb, currentColor 24%, transparent);
    color: inherit;
    border-radius: 2px;
  }
  .results .none,
  .results .mode.quiet,
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
  .row-menu {
    position: fixed;
    z-index: 40;
    display: flex;
    flex-direction: column;
    min-width: 11rem;
    padding: 0.25rem;
    border: 1px solid color-mix(in srgb, currentColor 25%, transparent);
    border-radius: 6px;
    background: var(--paper);
    color: var(--ink);
    box-shadow: 0 6px 18px rgb(0 0 0 / 18%);
    font-size: 0.85rem;
  }
  .row-menu button {
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    text-align: start;
    padding: 0.3rem 0.45rem;
    border-radius: 4px;
    cursor: default;
  }
  .row-menu button:hover {
    background: color-mix(in srgb, currentColor 12%, transparent);
  }
  .row-menu .quiet {
    padding: 0.3rem 0.45rem;
    opacity: 0.6;
  }
</style>
