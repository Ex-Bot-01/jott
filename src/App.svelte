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
  import {
  actionsFor,
  flattenTree,
  moveTargets,
  moveTo,
  notesAwaitingEmbedding,
  notesAwaitingIndex,
  planMoveInto,
  targetFolder,
} from './lib/tree'
  import type { TreeAction } from './lib/tree'
  import { bareKeyFor, escapeAction, isTypingTarget, shortcutFor } from './lib/shortcuts'
  import {
    emptySearchSentence,
    hitSuffix,
    resultsHeadline,
    searchCaveat,
    searchIsCurrent,
    suggestPrefix,
    wideningSentence,
  } from './lib/searchState'
  import { markTerms } from './lib/highlight'
  import { consentSentence, modelReadySentence, statusFor, type Status } from './lib/statusLine'
  import { cardClipboard, cardSentences, NP_ISSUE } from './lib/consentHelp'
  import type { ConsentOutcome } from './lib/ipc'
  import { openUrl } from '@tauri-apps/plugin-opener'
  import { getCurrentWindow } from '@tauri-apps/api/window'
  import { isPermissionGranted, requestPermission, sendNotification } from '@tauri-apps/plugin-notification'
  import { notifyBody, shouldNotify, type Permission } from './lib/notify'
  import type { SavePhase } from './lib/saveState'
  import * as ipc from './lib/ipc'
  import { loadPrefix, savePrefix } from './lib/prefixSearch'
  import type { DiagnosticsSnapshot, TraceEvent } from './lib/diagnostics'
  import type { AboutInfo, ConsentHelp, FetchedRecord, IngestEvent, NoteContent, RegistryInfo, SearchResults, VaultListing } from './lib/ipc'

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
  /** What np's settings actually say, read after a pass that could not embed. `null` means "not asked yet". */
  let consentHelp = $state<ConsentHelp | null>(null)
  /** The last sentence np sent as a refusal, kept word for word for the card to quote (§4). */
  let lastRefusal = $state<string | null>(null)
  /** What note-processor answered to the button this window pressed (0.5.x). It outranks the file's state in the card,
   *  because "np wrote these two settings ten seconds ago" is a truer sentence than a re-read of the file — and it is
   *  the only version that can admit a refusal. */
  let consentOutcome = $state<ConsentOutcome | null>(null)
  /** A model that this run's embed pass downloaded. The announcement outlives the status line and goes when the
      next pass supersedes it or the person moves on to another note. */
  let modelReady = $state<FetchedRecord | null>(null)
  let cardClipFailed = $state(false)
  /** Somebody answered "don't notify". Held for the run: a refusal is not a question to repeat. */
  let notifyRefused = $state(false)
  let showAbout = $state(false)
  let aboutLoading = $state(false)

  /** The diagnostics drawer. Its rows accumulate from launch rather than from the moment it opens: the question
      that makes somebody press Ctrl+Shift+D is always about something that already happened. */
  let showDiagnostics = $state(false)
  let snapshot = $state<DiagnosticsSnapshot | null>(null)
  let traceRows = $state<TraceEvent[]>([])

  let searchQuery = $state('')
  /** Word-beginning matching (note-processor#6), remembered between runs and off until asked: widening a query
   * changes what it means, so it is a choice somebody makes once about their own notes, not a default. */
  let prefixOn = $state(loadPrefix(typeof localStorage === 'undefined' ? null : localStorage))
  /** One search with word beginnings matched, asked for from an empty answer. Deliberately not the remembered
   *  preference: pressing "try this" once should not silently decide every future search. */
  let prefixOnce = $state(false)
  /** The query that produced the hit this note was opened from. Kept apart from `searchQuery` on purpose: the
   * box keeps being typed in, and marks should follow the hit that was clicked, not every keystroke after it. */
  let hitQuery = $state('')
  let results = $state<SearchResults | null>(null)
  let searching = $state(false)
  let searchTimer: ReturnType<typeof setTimeout> | undefined

  /** `folder` is where a new note or folder lands; `rel` is what is being moved. */
  let dialog = $state<{
    kind: 'new' | 'rename' | 'folder' | 'move'
    value: string
    folder?: string
    rel?: string
    /** The move dialog's second half: a destination that does not exist yet. */
    newFolder?: string
  } | null>(null)
  /** A refusal about the dialog itself, shown inside it: "that name walks out of the folder" is not a backend
   *  failure and must not look like one — and it is useless if the dialog has already closed. */
  let dialogSaid = $state<string | null>(null)
  let confirmDelete = $state<string | null>(null)
  /** The row menu: where it was opened, and for what. `null` means empty ground — only creating is on offer there. */
  let menu = $state<{ rel: string | null; x: number; y: number } | null>(null)
  /** Bumped to ask the editor to write its buffer now (Ctrl+S with focus outside it). */
  let saveSignal = $state(0)
  let searchBox = $state<HTMLInputElement | null>(null)
  /** The tree, so closing a note has somewhere to put the focus. Focusable with -1 rather than a tab stop: it is a
   *  destination for the keyboard, not another thing to tab through on the way to a note. */
  let treeHost = $state<HTMLElement | undefined>()

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

  /** Adopt a freshly listed folder, and drop everything that was only ever true of the previous one.
   *
   *  "model ready", the consent card's facts and np's last refusal are statements about *this* folder's index.
   *  Carried across they let the window describe yesterday's download as a property of the folder someone just
   *  opened — which is the same class of claim this release exists to stop making. */
  function adopt(next: VaultListing) {
    const changedFolder = listing !== null && listing.vault !== next.vault
    listing = next
    // A model was downloaded for one folder's index, and a search widened itself in one folder's results; carried
    // across they let the window describe yesterday's news as a property of the folder someone just opened — the same
    // class of claim v0.0.14 exists to stop making.
    if (!changedFolder) return
    modelReady = null
    consentHelp = null
    lastRefusal = null
    consentOutcome = null
    cardClipFailed = false
    prefixOnce = false
  }

  async function openRecent(folder: string) {
    if (!folder) return
    const next = await call(() => ipc.openVault(folder))
    if (next) adopt(next)
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
      adopt(next)
      openFolders = new Set()
      selected = null
      note = null
    }
  }

  /** The card's sentences. Empty means np is ready to embed, and the card stays out of the way — emptiness is
      decided in `consentHelp.ts`, where it is tested, rather than in markup. */
  const cardFacts = $derived(consentHelp ? { help: consentHelp, refusal: lastRefusal, outcome: consentOutcome } : null)
  const cardLines = $derived(cardFacts ? cardSentences(cardFacts) : [])
  const cardText = $derived(cardFacts ? cardClipboard(cardFacts) : '')

  async function readConsentHelp() {
    consentHelp = await call(ipc.embeddingsHelp)
  }

  async function copyCardSettings() {
    try {
      await navigator.clipboard.writeText(cardText)
      cardClipFailed = false
    } catch {
      // The webview can refuse a clipboard write. The text is worth having either way, so it goes on screen
      // selected rather than being lost.
      cardClipFailed = true
    }
  }

  /** The banner is up; this is only for the window nobody is looking at.
   *
   * Permission is asked for once, here, at the moment it is first useful — not at startup, where a dialog would
   * stand between a person and their notes. Anything that goes wrong (no notifier, a denied permission, a webview
   * without the plugin) leaves the banner as the announcement, which was always the primary one (§4).
   */
  async function announceModelReady(fetched: FetchedRecord) {
    if (notifyRefused) return
    try {
      let permission: Permission = (await isPermissionGranted()) ? 'granted' : 'not-determined'
      if (permission === 'not-determined') {
        permission = (await requestPermission()) === 'granted' ? 'granted' : 'denied'
        // Remembered, because the second ask would be the same question with a worse answer: somebody who said no to
        // notifications should not be asked again by every model that finishes downloading.
        if (permission === 'denied') notifyRefused = true
      }
      if (!shouldNotify(await getCurrentWindow().isFocused(), permission)) return
      sendNotification({ title: 'Jott', body: notifyBody(fetched.id) })
    } catch {
      // No notifier, or the webview refused to answer. The banner stays; nothing else changes.
    }
  }

  /** "Try again" after editing np's settings in another window: re-run the pass, then read the settings again. */
  async function retryEmbeddings() {
    await call(ipc.ingestAll)
    await readConsentHelp()
    await refresh()
  }

  async function openNote(rel: string, fromQuery = '') {
    const content = await call(() => ipc.readNote(rel))
    if (!content) return
    note = content
    selected = rel
    // The announcement did its job once the person moved on to working again (§4: say it, then get out of the way).
    modelReady = null
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
      if (shortcut === 'save' && note?.readonly) {
        // The refusal is core's to make and it does make it — but a keystroke that appears to do nothing is worse
        // than one that says why, and "why" here is one sentence about whose file this is.
        event.preventDefault()
        notice = `${note.rel} is note-processor's own index; it cannot be edited in Jott.`
        return
      }
      if (shortcut === 'save') {
        // Ask before preventing: the editor's own binding has already taken the key when it has focus, and a second
        // save for one keystroke is its own bug.
        const alreadyHandled = event.defaultPrevented
        event.preventDefault()
        if (!alreadyHandled) saveSignal += 1
        return
      }
      if (shortcut === 'close-drawer') {
        // One press, one thing dismissed, topmost first — the order lives in `escapeAction`, where it is tested.
        // Closing a note while a menu still covers the vault reads as throwing work away to hide an overlay.
        const step = escapeAction({
          menuOpen: menu !== null,
          drawerOpen: showDiagnostics,
          noteOpen: selected !== null,
          typing: isTypingTarget(event.target),
        })
        if (step === 'menu') menu = null
        else if (step === 'drawer') showDiagnostics = false
        else if (step === 'note') closeNote()
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
        dialog = { kind: 'move', value: '', rel: selected, newFolder: '' }
        dialogSaid = null
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
    adopt(next)
    openFolders = new Set()
    selected = null
    note = null
  }

  /** Stop looking at this note. It writes nothing and forgets nothing: an unsaved buffer stays mirrored in core
   *  (§18.13), so closing a window is not a decision about somebody's typing — it is only putting it out of sight.
   *
   *  One function because the button and Escape must not be able to disagree about what "close" means. */
  function closeNote() {
    if (selected === null) return
    selected = null
    note = null
    // The marks came from a search hit; with no note open there is nothing for them to be on.
    hitQuery = ''
    // Focus goes back where the next intention is most likely to come from, rather than to nothing at all — which is
    // also what makes Escape feel like a way out rather than a dead end.
    treeHost?.focus()
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
    const answer = await call(() => ipc.searchVault(query, 12, prefixOn || prefixOnce))
    // Typing "abc" fires three requests and np is free to answer them in any order. Without this guard the slowest
    // one wins, which reads as the search box ignoring what you typed last.
    if (!searchIsCurrent(seq, searchSeq)) return
    results = answer
    searching = false
  }

  /** The "match word beginnings" toggle. Saving and re-running are both immediate: someone flipping it wants to see
   * what changes about *this* query, and a 250 ms debounce would read as the click having done nothing. */
  function togglePrefix(event: Event) {
    prefixOn = (event.currentTarget as HTMLInputElement).checked
    savePrefix(typeof localStorage === 'undefined' ? null : localStorage, prefixOn)
    // Back to the box afterwards: shortcuts stand down while focus is in any input, so a person who flips this and
    // then presses Ctrl+S would otherwise find the save key ignored.
    searchBox?.focus()
    if (searchQuery.trim()) void runSearch()
  }

  /** A keystroke means a new question, and a new question starts from the remembered setting rather than from a
   *  one-off widening asked for against the last one. */
  function onQueryInput() {
    prefixOnce = false
    scheduleSearch()
  }

  /** The empty answer's offer: run *this* query again with word beginnings matched, once. It does not write the
   *  preference — that is what the toggle beside the box is for. */
  function widenThisSearch() {
    prefixOnce = true
    void runSearch()
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
    // A failed move leaves the dialog up. Closing it would say "done" about something that did not happen, and the
    // note is still where it was — `problem` already carries the sentence about why.
    if (moved === null) return
    dialog = null
    await refresh()
    if (selected === from) selected = to
  }

  /** Move this note into a folder, making the folder first when somebody named one that is not there yet.
   *
   *  The vault may have no folders at all — which is how the dialog read as "only an empty field" — and `create_folder`
   *  already makes parents, so a nested name needs nothing special. Half a plan is the failure to design for: if the
   *  folder could not be made, no move is sent and the dialog stays open with the name still in it. */
  async function createAndMove(rel: string, typed: string) {
    const plan = planMoveInto(rel, typed, moveTargets(rows, rel))
    if (plan.kind === 'refuse') {
      dialogSaid = plan.sentence
      return
    }
    if (plan.kind === 'create-then-move') {
      const made = await call(() => ipc.createFolder(plan.folder.replace(/\/$/, '')))
      if (made === null) return
    }
    const moved = await call(() => ipc.moveEntry(rel, plan.to))
    if (moved === null) return
    dialog = null
    dialogSaid = null
    await refresh()
    if (selected === rel) selected = plan.to
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
    // np's answer goes straight onto the card, and a stale refusal from before the button is cleared beside it: two
    // reports about one question, one of them ten minutes old, read as a window that disagrees with itself.
    const receipt = await call(() => ipc.setConsent(granted))
    if (receipt) {
      consentOutcome = receipt
      lastRefusal = null
    }
    void readConsentHelp()
  }

  /** The same question, asked again later. "Stay keyword-only" answers the moment, not the year, and the way
      back should not be editing a TOML file — so the offer sits under the tree until it is taken up, and
      pressing it says exactly what it will do before it does it. */
  async function setUpEmbeddings() {
    const offer = embeddingsOffer
    if (!offer) return
    const sentence = consentSentence(offer)
    if (!(await ipc.askConfirm(sentence, 'Set up semantic search'))) return
    // The same answer as the offer's own Allow button — one code path, so both leave np's receipt on the card and
    // neither can forget to re-read the settings beside it.
    await answerConsent(true)
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
      if (reopened) adopt(reopened)
    } else {
      await refresh()
    }

    void ipc.onIngestEvent((event) => {
      if ('needsConsent' in event) consent = event.needsConsent
      // Either answer can mean "embedding cannot run": np answers a configuration it dislikes with
      // `configuration-disabled`, and one it cannot parse or complete with a fault that arrives here as `refused`
      // (measured against 0.4.0 for a provider named without a model). Both are worth reading the settings over —
      // and `cardSentences` answers nothing when the settings are fine, so an unrelated refusal in a healthy folder
      // still raises no card.
      if ('refused' in event) {
        // Both facts stand: what np answered the button, and what it says now. `cardSentences` prints the receipt and
        // then this sentence — dropping the receipt put the paste-advice back over a file np had just written.
        lastRefusal = event.refused.message
        void readConsentHelp()
      }
      if ('disabled' in event) void readConsentHelp()
      if ('embedded' in event) {
        const fetched = event.embedded.fetched
        if (fetched) {
          modelReady = fetched
          lastRefusal = null
          void announceModelReady(fetched)
        } else {
          // A pass that downloaded nothing supersedes the announcement: it is about a finished download, not a
          // permanent property of the vault.
          modelReady = null
        }
      }
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
        oninput={onQueryInput}
        aria-label="Search the vault"
      />
      <label
        class="prefix-toggle"
        title="Also match word beginnings: “grand” finds grandmother. note-processor says which words it widened."
      >
        <input type="checkbox" checked={prefixOn} onchange={togglePrefix} aria-label="Match word beginnings" />
        match word beginnings
      </label>
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
        <!-- A word, not a ×: an icon button in a toolbar of words reads as decoration, and "close what?" is exactly
             the question a tester had to ask out loud. -->
        <button
          type="button"
          data-testid="close-note"
          title="Stop looking at this note (Escape). Nothing is written by closing it."
          disabled={selected === null}
          onclick={closeNote}
        >
          close
        </button>
      </span>
    </div>

    {#if results}
      {@const caveat = searchCaveat(results, results.hits.length)}
      {@const widened = wideningSentence(results)}
      <ul class="results">
        {#if results.hits.length === 0}
          <!-- An empty list is two different facts and only one of them is about the words. np 0.4.0 answers
               `indexed` and settles it (note-processor#5); behind that, the sentence comes from what the tree's own
               markers know — never from silence. -->
          <li class="none">
            {emptySearchSentence(results, searchQuery.trim(), awaitingIndex)}
            {#if results.offline}(np is offline — keyword search only){/if}
            {#if suggestPrefix(results, searchQuery.trim(), prefixOn || prefixOnce)}
              <!-- np says this folder is indexed and the whole-word search found nothing, so widening is the one
                   thing that can change the answer. Offered, never done silently: a widened search is a different
                   question than the one that was typed (§4). -->
              <button type="button" data-testid="widen-offer" onclick={widenThisSearch}>try matching word beginnings</button>
            {/if}
          </li>
        {/if}
        {#each results.hits as hit (hit.path)}
          <li>
            <button type="button" onclick={() => void openNote(hit.path, searchQuery.trim())}>
              <span class="hit-path">{hit.path}</span>
              <span class="hit-snippet">
                {#each markTerms(hit.snippet ?? '', searchQuery, prefixOn) as part, index}
                  {#if part.matched}<mark>{part.text}</mark>{:else}{part.text}{/if}
                {/each}
              </span>
              {#if hitSuffix(hit.matchedExactly)}
                <!-- np reached this note by a word beginning, so the row says which kind of match it is. Only rows np
                     marked `matched_exactly: false` get one — a meaning-only hit was never word-tested (§4). -->
                <span class="hit-kind">{hitSuffix(hit.matchedExactly)}</span>
              {/if}
            </button>
          </li>
        {/each}
        {#if resultsHeadline(results.hits.length)}
          <!-- A count, not np's `mode` word: 0.3.1 answers "hybrid" for searches where only keywords ran. What
               actually ran goes on its own line, in `searchCaveat`'s words or np's own sentence. -->
          <li class="mode">
            {resultsHeadline(results.hits.length)}{searching ? ' · searching…' : ''}
            {#if widened}<span class="caveat">· {widened}</span>{/if}
            {#if caveat}<span class="caveat">· {caveat}</span>{/if}
          </li>
        {:else}
          {#if caveat}<li class="mode quiet">{caveat}</li>{/if}
          {#if searching}<li class="mode quiet">searching…</li>{/if}
        {/if}
      </ul>
    {:else}
      <div class="tree-host" bind:this={treeHost} tabindex="-1">
        <Tree
          {rows}
          open={openFolders}
          selected={selected}
          ontoggle={toggleFolder}
          onselect={(rel) => void openNote(rel)}
          onmenu={openMenu}
        />
      </div>
    {/if}

    {#if modelReady}
      <div class="first-pass" data-testid="model-ready">
        <span>{modelReadySentence(modelReady)}</span>
        <button type="button" onclick={() => (modelReady = null)}>Dismiss</button>
      </div>
    {:else if cardLines.length > 0 && listing}
      <!-- Consent was answered and note-processor still cannot embed. The old window said
           `keyword-only: embeddings.provider is not set` and left it there; this says what Jott recorded, which
           settings are missing, and where to say so (§4). -->
      <div class="first-pass consent-card" data-testid="consent-card">
        <div class="card-lines">
          {#each cardLines as line (line)}
            <p>{line}</p>
          {/each}
        </div>
        {#if consentHelp && consentHelp.linesToAdd.length > 0 && !consentOutcome}
          <!-- The lines np needs, as copyable text rather than as a sentence to retype (§4). Shown only while nobody
               has answered: since np 0.5.x a press writes those keys itself, and paste-instructions beside a file that
               was just written for you are two contradictory instructions on one screen. -->
          <pre class="settings">{consentHelp.linesToAdd.join('\n')}</pre>
        {/if}
        <span class="buttons">
          <button type="button" onclick={() => void copyCardSettings()}>
            {consentOutcome ? 'Copy what happened' : 'Copy these settings'}
          </button>
          <button type="button" data-testid="consent-retry" onclick={() => void retryEmbeddings()}>Try again</button>
          <button type="button" onclick={() => void openUrl(NP_ISSUE).catch(() => undefined)}>
            Read the note-processor request
          </button>
        </span>
        {#if cardClipFailed}
          <!-- The clipboard refused. The text still has to leave this window somehow. -->
          <textarea readonly rows="6" aria-label="Settings to copy" onfocus={(e) => e.currentTarget.select()}>{cardText}</textarea>
        {/if}
        {#if !consentOutcome}
          <!-- The request to note-processor is worth naming only while somebody still has to act by hand. Once np
               answered over the pipe, this window has no unfinished business pointing there. -->
          <span class="hint">{NP_ISSUE}</span>
        {/if}
      </div>
    {:else if listing && awaitingIndex > 0 && !selected}
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
          <!-- A vault with no folders used to offer an empty list and nothing else. The destination can be a folder
               that does not exist yet: name it, and this makes it and moves the note in — one intention, two commands. -->
          <span class="new-folder">
            <label for="newfolder" class="quiet">…or a new folder:</label>
            <input
              id="newfolder"
              type="text"
              data-testid="new-folder-name"
              bind:value={dialog.newFolder}
              placeholder="ideas"
              spellcheck="false"
              autocomplete="off"
            />
            <button
              type="button"
              data-testid="create-and-move"
              onclick={() => dialog?.rel && void createAndMove(dialog.rel, dialog.newFolder ?? '')}
            >
              create and move
            </button>
          </span>
          {#if dialogSaid !== null}<p class="warn" role="alert">{dialogSaid}</p>{/if}
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
      highlightPrefix={prefixOn}
      readonly={note?.readonly ?? false}
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
  .prefix-toggle {
    display: inline-flex;
    align-items: center;
    gap: 0.25rem;
    font-size: 0.8rem;
    white-space: nowrap;
    user-select: none;
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
  .hit-kind {
    font-size: 0.75rem;
    opacity: 0.75;
    white-space: nowrap;
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
  /* The consent card is taller than the one-line notices around it: sentences, then the settings as text. */
  .consent-card {
    display: grid;
    gap: 0.4rem;
    align-items: start;
  }
  .card-lines p {
    margin: 0 0 0.25rem;
  }
  .settings {
    margin: 0;
    padding: 0.35rem 0.5rem;
    font-family: var(--mono, ui-monospace, monospace);
    font-size: 0.75rem;
    white-space: pre-wrap;
    background: color-mix(in srgb, currentColor 7%, transparent);
    border: 1px solid color-mix(in srgb, currentColor 15%, transparent);
    /* Selectable by hand even when the clipboard refuses a write. */
    user-select: text;
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
