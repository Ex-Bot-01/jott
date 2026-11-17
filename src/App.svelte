<script lang="ts">
  // The window: tree on the left, note in the middle, one line about the index along the bottom (§9).
  // Everything factual here came from a command or an event; the decisions live in the tested modules
  // (`tree.ts`, `saveState.ts`, `statusLine.ts`) and in `jott-core`.

  import { onMount } from 'svelte'
  import About from './lib/About.svelte'
  import Settings from './lib/Settings.svelte'
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
  notesEmbedded,
  planMoveInto,
  targetFolder,
  toolbarTarget,
} from './lib/tree'
  import type { TreeAction } from './lib/tree'
  import { bareKeyFor, escapeAction, isTypingTarget, shortcutFor } from './lib/shortcuts'
  import {
    emptySearchSentence,
    hitSuffix,
    resultsHeadline,
    searchCaveat,
    searchIsCurrent,
    semanticEmptySentence,
    suggestPrefix,
    wideningSentence,
  } from './lib/searchState'
  import { markTerms } from './lib/highlight'
  import { figuresNote, rowFigures, rowLabel, scoreTitleFor } from './lib/scoreLabel'
  import { consentSentence, modelReadySentence, statusFor, type Status } from './lib/statusLine'
  import { cardSentences } from './lib/consentHelp'
  import { createBlocked } from './lib/createHere'
  import { formatsSentence, opensAsText, planNoteName } from './lib/noteName'
  import type { ConsentOutcome } from './lib/ipc'
  import type { ModelFacts } from './lib/modelStatus'
  import { downloadSentence, initialWatch, readMarker, type WatchState } from './lib/downloadWatch'
  import { openUrl } from '@tauri-apps/plugin-opener'
  import { getCurrentWindow } from '@tauri-apps/api/window'
  import { isPermissionGranted, requestPermission, sendNotification } from '@tauri-apps/plugin-notification'
  import { notifyBody, shouldNotify, type Permission } from './lib/notify'
  import type { SavePhase } from './lib/saveState'
  import * as ipc from './lib/ipc'
  import { loadSettings, saveMode, savePrefix, type SearchMode } from './lib/searchSettings'
  import type { DiagnosticsSnapshot, TraceEvent } from './lib/diagnostics'
  import type { AboutInfo, ConsentHelp, FetchedRecord, IngestEvent, NoteContent, RegistryInfo, SearchResults, VaultListing } from './lib/ipc'

  let registry = $state<RegistryInfo | null>(null)
  let listing = $state<VaultListing | null>(null)
  /** Open folders, for this run of the app (§9). A `Set` that gets replaced rather than mutated. */
  let openFolders = $state<Set<string>>(new Set())
  let selected = $state<string | null>(null)
/** The folder the toolbar will make its next note or folder inside, as the tree spells it (no trailing slash). */
  let selectedFolder = $state<string | null>(null)
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
/** Notes against everything else in this folder. note-processor parses only `.md`, so a vault of `notes.txt` and
    `scan.pdf` is a folder whose files Jott can list but cannot search — said once, from the rows already walked. */
  // A destination is a promise about where the next file goes, so it is only true while the walk still shows the folder
  // it names. Trash or rename that folder and core would happily re-create it on the next **+ note** — it makes parents
  // — so the tooltip would be promising to undo what was just done.
  let destination = $derived(
    toolbarTarget(
      selectedFolder !== null &&
        listing?.nodes.some((n) => n.kind === 'Folder' && (n.rel === selectedFolder || `${n.rel}/` === selectedFolder))
          ? selectedFolder
          : null,
    ),
  )
  let formatsNotice = $derived(
    listing
      ? formatsSentence({
          notes: listing.nodes.filter((n) => n.kind === 'Note').length,
          others: listing.nodes.filter((n) => n.kind === 'Attachment' || n.kind === 'Image').length,
        })
      : null,
  )
  /** What np would need to set embeddings up — its model, its folder, the size when anybody measured one.
      `null` means either "nothing to offer" or "not asked yet for this vault", and `offerAskedFor` says which. */
  let embeddingsOffer = $state<ipc.ConsentOffer | null>(null)
  let offerAskedFor: string | null = null
  /** What np said about the model behind this folder: configured, on disk, in use here. `null` means nobody has been
      asked yet for this folder, which draws no dot at all — an absence on screen rather than a grey one that reads as
      "switched off". */
  let modelStatus = $state<ipc.ModelStatus | null>(null)
  let modelAskedFor: string | null = null
  /** The download banner's two pieces of state. `downloadShown` is what is on screen *now*; `downloadWatch` is what
      the last poll saw, so the next reading can be compared with it. Both live in `lib/downloadWatch.ts`'s rule
      rather than in markup, because "is this marker live?" is a question with a testable answer. */
  let downloadShown = $state<ipc.Downloading | null>(null)
  let downloadWatch: WatchState = initialWatch

  /** The dot's facts: np's answers about the model, plus how many notes in *this* folder carry vectors. Both halves are
      needed — "the weights are on my disk" and "this folder can be searched by meaning" are different sentences, and
      only the second explains an empty semantic search. Each key is named rather than spread: a proxied `$state` object
      hands back `undefined` for a key nobody set, and `undefined` must not reach a dot that reads `null` as "off". */
  const modelFacts = $derived<ModelFacts | null>(
    // `listing === null` means no folder is open — which is not a folder whose np is quiet. Answering from the old
    // status here would keep describing a vault that has been closed or trashed.
    modelStatus === null || listing === null
      ? null
      : {
          provider: modelStatus.provider ?? null,
          modelId: modelStatus.modelId ?? null,
          present: modelStatus.present ?? null,
          bytesTotal: modelStatus.bytesTotal ?? null,
          configUnreadable: modelStatus.configUnreadable ?? false,
          modelsDirError: modelStatus.modelsDirError ?? null,
          embeddedNotes: listing ? notesEmbedded(listing.nodes) : 0,
        },
  )
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
  /** The two remembered search choices, read once at startup: which engine answers, and whether word beginnings
   * count (note-processor#6). Both change what a query *means*, so each is decided once and expected to stay; both
   * default to note-processor's own defaults rather than to an opinion of ours. */
  const storedSettings = loadSettings(typeof localStorage === 'undefined' ? null : localStorage)
  let prefixOn = $state(storedSettings.prefix)
  let searchMode = $state<SearchMode>(storedSettings.mode)
  /** The settings panel: the tester's "a settings menu to show this, so it isn't hidden". */
  let showSettings = $state(false)
  /** While an answer to the embeddings question is in flight, both of its buttons are down. Two presses, two
   *  `embeddings.enable` calls, and np answers them in whichever order it likes. */
  let consentBusy = $state(false)
  /** True while np's settings are being read for the panel. */
  let embeddingsLoading = $state(false)
  /** One search with word beginnings matched, asked for from an empty answer. Deliberately not the remembered
   *  preference: pressing "try this" once should not silently decide every future search. */
  let prefixOnce = $state(false)
  /** Which folder the open buffer belongs to, captured when the note is opened. Core stores unsaved buffers per
   *  folder, and a teardown flush can arrive after the window has moved on to another one — filing A's text under
   *  B's name would give B an "unsaved changes" notice about words never typed into it. So a write that no longer
   *  matches the folder on screen is dropped rather than guessed about. */
  let bufferVault = $state<string | null>(null)
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
    /** Why this cannot be done yet, in words. Set when a note or folder is asked for with no folder open: the dialog
     *  stays up with what was typed, and submitting sends nothing rather than collecting a refusal nobody sees. */
    blocked?: string | null
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
  /** Whether what is selected belongs to note-processor rather than to the person. The row menu already refuses these
   *  (`actionsFor` answers nothing); the toolbar has to agree with it, or the window offers two opposite answers
   *  about whether a file is touchable — and core's refusal, though correct, arrives only after the click. */
  const selectedIsGenerated = $derived(rows.find((row) => row.rel === selected)?.greyed ?? false)

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
    if (changedFolder) {
      // A model was downloaded for one folder's index, and a search widened itself in one folder's results; carried
      // across they let the window describe yesterday's news as a property of the folder someone just opened — the
      // same class of claim v0.0.14 exists to stop making. Otherwise this is a re-list of the folder already on
      // screen, and its news stays.
      modelReady = null
      consentHelp = null
      lastRefusal = null
      consentOutcome = null
      prefixOnce = false
      // The offer named one folder's model and one folder's waiting count. Carried across, the banner beside a second
      // folder quotes a `models.list` that was asked about the first.
      embeddingsOffer = null
      // The search answer on screen is note-processor's report about the folder it was asked in — `vectors`,
      // `indexed`, which words it widened. Carried across, the settings panel warns "this folder has no vectors yet"
      // about a folder Jott has never searched, and the results pane lists files that are not in this vault. The open
      // note goes with them: a `rel` means nothing in another folder, and leaving a dirty buffer attached after core
      // switched sessions is how one folder's typing ends up written into another's file.
      results = null
      searchQuery = ''
      hitQuery = ''
      selected = null
      selectedFolder = null
      note = null
      bufferVault = null
      // What np said about its model belongs to the folder that was asked about. Carrying `present: true` across into a
      // folder nobody has queried yet would light a green bead over a vault with no vectors at all.
      modelStatus = null
      modelAskedFor = null
    }

    // Every path that installs a listing comes through here, so this is where the embeddings offer is asked. It used
    // to be asked only by `refresh()`, which meant a folder that arrived by being *opened* — at startup or through the
    // picker — never asked about, and the banner that tells someone their notes are indexed but cannot be searched by
    // meaning had no way to appear until some later re-list happened to come along.
    void askOffer()
    void askModel()
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
    void askModel()
  }

  /** Once per opened folder: is the model configured, and are its bytes on this disk? Unlike the offer, this is asked
      even when nothing is awaiting embedding — a status light has to be able to say "on" as well as "not bought yet".
      It reads np's settings file plus `models.list` over the pool's *reader*, so it cannot queue behind an embed in
      progress: when a download finishes, this is the call that learns about it. */
  async function askModel() {
    const vault = listing?.vault ?? null
    if (!vault || modelAskedFor === vault) return
    modelAskedFor = vault
    await readModel(vault)
  }

  /** Ask np about the model again, past the once-per-folder guard, and update the banner from what it says. While a
      fetch marker exists this runs every 1.5 s: one local pipe round trip to the reader child — no new process, no
      workspace walk — and it is what keeps the bar honest when a paint drops the progress events. */
  async function pollModel() {
    const vault = listing?.vault ?? null
    if (!vault) return
    await readModel(vault)
  }

  async function readModel(vault: string) {
    const answer = await call(ipc.embeddingsStatus)
    // The folder changed while np was answering — and `models.list` goes to the reader connection, which may have had to
    // start that child first, so "while" can be seconds. A late answer about the previous folder would otherwise land on
    // top of the new folder's own (search has the same guard for the same reason, since 0.0.12), and because
    // `modelAskedFor` already names the new folder nothing would re-ask until an event happened to come along.
    if (listing?.vault !== vault) return
    modelStatus = answer
    const reading = readMarker(downloadWatch, answer?.downloading ?? null)
    downloadWatch = reading.state
    // Only ever np's own figures, from the reading that earned them: a banner about bytes that stopped moving would
    // describe a download nobody is running (`lib/downloadWatch.ts` explains why a marker alone proves nothing).
    downloadShown = reading.show ? (answer?.downloading ?? null) : null
  }

  // Deliberately keyed on "is there a marker", not on the marker object: an object would re-create the timer on every
  // poll and turn a steady 1.5 s rhythm into a race with itself.
  $effect(() => {
    const running = (modelStatus?.downloading ?? null) !== null
    if (!running) return
    const timer = setInterval(() => void pollModel(), 1500)
    return () => clearInterval(timer)
  })

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
  /** Whether the card may offer *Allow the download*. Two gaps say no: a file np cannot parse (nothing can be written
      until somebody edits it by hand) and an engine Jott does not recognise — pressing Allow there would overwrite a
      choice this window has no business touching. */
  const cardCanAllow = $derived(
    consentHelp !== null && consentHelp.gap !== 'providerOther' && consentHelp.gap !== 'configUnreadable',
  )

  const cardFacts = $derived(consentHelp ? { help: consentHelp, refusal: lastRefusal, outcome: consentOutcome } : null)
  const cardLines = $derived(cardFacts ? cardSentences(cardFacts) : [])

  async function readConsentHelp() {
    // The panel distinguishes "still reading" from "read, and there is nothing to say": the two looked identical, and
    // the panel printed the first one forever — including after a failed read.
    embeddingsLoading = true
    try {
      consentHelp = await call(ipc.embeddingsHelp)
    } finally {
      embeddingsLoading = false
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
    // One place decides this. The tree row click was a second writer and could not cover the paths that do not go
    // through it — opening a hit from the search results kept a folder as the destination while a note from anywhere
    // was on screen, which is exactly the tooltip-versus-button disagreement.
    selectedFolder = null
    bufferVault = listing?.vault ?? null
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
  /** Whether a write from the editor still belongs to the folder on screen (§18.13's per-folder mirror, §9's
   *  "write it where it lives"). Opening another folder clears the selection, so this is a second line of defence
   *  rather than the only one — and the ordering that makes it necessary is real: `open_vault` switches core's
   *  session before the new listing arrives, so an editor destroyed by that switch would otherwise file its text in
   *  the wrong folder. */
  function bufferStillInPlace(): boolean {
    return bufferVault === null || listing === null || bufferVault === listing.vault
  }

  async function saveThroughCommand(rel: string, text: string) {
    // No folder check is needed here because switching folders destroys the editor (`adopt` clears the selection),
    // and a destroyed view cannot reach this function — `saveNow` returns as soon as it has no view. The mirror path
    // below does need one: it fires from teardown, which is exactly the moment the session has already moved on.
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
          settingsOpen: showSettings,
          aboutOpen: showAbout,
          // Both kinds of asking count: a rename dialog and the trash confirmation are dismissed the same way.
          dialogOpen: dialog !== null || confirmDelete !== null,
          drawerOpen: showDiagnostics,
          noteOpen: selected !== null,
          typing: isTypingTarget(event.target),
        })
        if (step === 'menu') menu = null
        else if (step === 'settings') showSettings = false
        else if (step === 'about') showAbout = false
        else if (step === 'dialog') {
          // The dialog's own refusal, not the note's: cancelling "Move to…" must leave what was open behind it open.
          dialog = null
          confirmDelete = null
          dialogSaid = null
        } else if (step === 'drawer') showDiagnostics = false
        else if (step === 'note') closeNote()
        return
      }
      event.preventDefault()
      if (shortcut === 'settings') {
        // Ctrl+comma, the key every desktop app already uses for this. The panel is where the search choices live, and
        // a person looking for them should not have to find the checkbox beside the search box first.
        if (showSettings) showSettings = false
        else await openSettings()
        return
      }
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
      if (shortcut === 'new-note') createInToolbar('new')
      else if (shortcut === 'new-folder') createInToolbar('folder')
      else runAction('rename')
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
    // Whatever was refused in the dialog before this one stays out of it. A sentence from an older intention, printed
    // above a field nobody has typed into yet, is a stale report beside a fresh question — the same class of carry-over
    // this window already prevents three times over in `adopt`.
    dialogSaid = null
    // Which row the intention is about: the one the menu was opened on, else the selected one. Read before `menu`
    // is cleared, or every action would silently fall back to the root of the vault.
    const row = rowFor(on === undefined ? menu?.rel ?? null : on)
    menu = null
    switch (action) {
      case 'new-note':
        dialog = { kind: 'new', value: '', folder: targetFolder(row), blocked: createBlocked(listing !== null) }
        return
      case 'new-folder':
        dialog = { kind: 'folder', value: '', folder: targetFolder(row), blocked: createBlocked(listing !== null) }
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
    // A dialog that was waiting on this folder can go on: the name typed is kept, so picking finishes the intention
    // instead of starting it over.
    if (dialog) dialog = { ...dialog, blocked: null }
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
    selectedFolder = null
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
    const answer = await call(() => ipc.searchVault(query, 12, prefixOn || prefixOnce, searchMode))
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

  /** The panel's *how search matches* choice. Saved and re-run at once, for the same reason as the toggle: someone
   *  changing a search setting wants to see what it does to the query in the box, not to the next one they type. */
  function chooseMode(mode: SearchMode) {
    searchMode = mode
    saveMode(typeof localStorage === 'undefined' ? null : localStorage, mode)
    // A one-off widening belongs to the answer it was offered against; changing engine is a new question.
    prefixOnce = false
    if (searchQuery.trim()) void runSearch()
  }

  /** The same choice made from the panel rather than the box: one writer per key, one re-run either way. */
  function choosePrefix(on: boolean) {
    prefixOn = on
    savePrefix(typeof localStorage === 'undefined' ? null : localStorage, on)
    if (searchQuery.trim()) void runSearch()
  }

  async function openSettings() {
    showSettings = true
    // Read what note-processor has recorded when the panel opens rather than trusting an older card: this is the
    // place somebody comes to find out what is switched on, and the file may have been edited since we last looked.
    void readConsentHelp()
  }

  /** The panel's two permission buttons. The same code path as the consent card's own button — one answer, one
   *  writer — so both places show np's receipt rather than a hope about what the click did. */
  async function setEmbeddings(granted: boolean) {
    // The same path as the card's buttons, including its in-flight guard.
    await answerConsent(granted)
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
    if (made === null) return // the dialog stays up: a folder that was not made is not a thing to close on
    dialog = null
    await refresh()
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
    let created: string | null = null
    if (plan.kind === 'create-then-move') {
      const made = await call(() => ipc.createFolder(plan.folder.replace(/\/$/, '')))
      if (made === null) return
      created = plan.folder
    }
    const moved = await call(() => ipc.moveEntry(rel, plan.to))
    if (moved === null) {
      // Two commands, and the first one worked. np's refusal lands in the status line wherever it goes, but a folder
      // now exists in their vault that nobody asked for on its own — so the dialog says what it did before it says
      // what went wrong, rather than leaving an unexplained directory behind.
      if (created !== null) {
        dialogSaid = `Created ${created.replace(/\/$/, '')}, but the move was refused. The empty folder is still there.`
      }
      return
    }
    dialog = null
    dialogSaid = null
    await refresh()
    if (selected === rel) selected = plan.to
  }

  /** Make the note the person asked for, under the name that will actually be a note. */
  /** The toolbar's two create buttons. `runAction` stays the row-menu path, which takes its target from the row the menu
   *  was opened on; this one takes it from the folder that is selected, because there is no row to ask. */
  function createInToolbar(kind: 'new' | 'folder') {
    // The row menu's path clears the menu inside `runAction`; this one is reached by button and by shortcut alike, and
    // a menu left open would float over the dialog it just opened.
    menu = null
    dialogSaid = null
    dialog = { kind, value: '', folder: destination.folder, blocked: createBlocked(listing !== null) }
  }

  async function createNote(typed: string, folder: string) {
    const plan = planNoteName(typed, folder)
    if (plan.rel === null) {
      // Refused in words, dialog up, name kept. Renaming it behind their back is how a note ends up called `untitled2`.
      dialogSaid = plan.said ?? 'That name will not work.'
      return
    }
    // Held in a local because narrowing does not follow a property into a closure: `() => ipc.createNote(plan.rel)`
    // would be asking for a `string | null`.
    const rel = plan.rel
    const created = await call(() => ipc.createNote(rel))
    if (created === null) return
    dialog = null
    if (plan.said !== null) notice = plan.said
    await refresh()
    // An empty `photo.png` opened in an editable buffer is an invitation to write prose into a file Jott's own walk
    // labels Image and note-processor never parses. The notice above already says what was made; there is nothing to
    // edit here.
    if (opensAsText(rel)) await openNote(rel)
  }

  async function renameEntry(to: string) {
    const from = selected
    if (!from) {
      dialog = null
      return
    }
    const outcome = await call(() => ipc.renameEntry(from, to))
    // A refused rename — the name is taken, the target exists — keeps the dialog and the typed name. Closing it would
    // say "done" about something that did not happen, and the person would start over from nothing.
    if (!outcome) return
    dialog = null
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
    // Two doors ask the same question — the status-line card and the settings panel — and a second press while np is
    // still answering would send a second `embeddings.enable` whose answer can arrive in either order. The guard
    // lives here, on the one path both doors share, rather than in either button's markup.
    if (consentBusy) return
    consentBusy = true
    try {
      await recordConsentAnswer(granted)
    } finally {
      consentBusy = false
    }
  }

  async function recordConsentAnswer(granted: boolean) {
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
    // "Allow the download" moves np's provider setting, which is half of what the dot reports. Asking again is how the
    // light agrees with the button that was just pressed, instead of waiting for a re-list nobody triggered.
    modelAskedFor = null
    void askModel()

    // A yes means *go*, not "yes, whenever the next save happens". This mattered more in 0.0.23 than before: with the
    // receipt card gone the strip is the only feedback a first download gets, and recording settings starts no
    // transfer — so the click produced np's `ok` and a screen that looked untouched. Walking the folder now is what
    // makes np begin fetching, which is what puts bytes on the bar. A decline starts nothing: there is nothing to fetch.
    if (granted && receipt) {
      await call(ipc.ingestAll)
      // The walk has begun; np's marker exists or is about to. One more read, because the strip shows on a *moving*
      // count and the first reading of a fresh marker is enough to start drawing it.
      modelAskedFor = null
      await askModel()
    }
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
    // `answerConsent` runs the pass itself now (see the comment there), so this door does not start a second one.
    await answerConsent(true)
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
      // Nothing to come back to, so nothing to ask about. Asking `list_vault` anyway used to be how a first run began:
      // core answers "no vault is open" — true, and not an error worth showing at all — and that sentence appeared in
      // the sidebar before anyone had done anything. The empty state asks for a folder; that is the message.
      listing = null
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
      if ('working' in event && event.working.phase === 'fetching') {
        // np says bytes are arriving, which means its fetch marker exists — and that marker is what the strip reads.
        // Spending one cheap request on the reader connection here is the difference between a bar appearing within a
        // second of the first fetching line and waiting for some unrelated reason to re-ask.
        modelAskedFor = null
        void pollModel()
      }
      status = statusFor(event)
      // A finished pass changes every marker in the tree, and the tree is a snapshot of the stamps as they
      // were when it was listed. Without this re-list the status line says "completed" over dots that still
      // say "waiting for the index" — which is exactly what a tester reported and reasonably called confusing.
      if ('indexed' in event || 'embedded' in event || 'refused' in event || 'disabled' in event) {
        // The dot reads np's `models.list`, and a pass is exactly what changes what sits in that directory. Asking
        // again is one cheap request; the alternative is a light that stays amber through a finished download.
        modelAskedFor = null
        scheduleRelist()
      }
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
        aria-label="Settings"
        title="How search matches, and whether embeddings may download (Ctrl+,)"
        onclick={() => void openSettings()}
      >
        Settings
      </button>
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

    {#if problem}
      <!-- Refusals go where the eyes already are: above the tools, beside whatever was just pressed. At v0.0.16 this
           sentence existed and did render — below the tree in the sidebar, under whatever the walk had warned about —
           which on a first run meant beneath an empty list, and on every run meant somewhere other than the dialog
           that had just closed itself. -->
      <p class="warn" role="alert" data-testid="global-problem">{problem}</p>
    {/if}

    <div class="tools">
      <input
        bind:this={searchBox}
        type="search"
        placeholder="Search this vault (Ctrl+F)"
        bind:value={searchQuery}
        oninput={onQueryInput}
        aria-label="Search the vault"
      />
      <span class="tool-buttons">
        <!-- The destination is in the tooltip because it used to be a secret: both of these always meant the vault root,
             whatever had just been clicked. -->
        <button
          type="button"
          title="New note in {destination.label} (Ctrl+N)"
          data-testid="toolbar-new-note"
          onclick={() => createInToolbar('new')}
        >
          + note
        </button>
        <button
          type="button"
          title="New folder in {destination.label} (Ctrl+Shift+N)"
          data-testid="toolbar-new-folder"
          onclick={() => createInToolbar('folder')}
        >
          + folder
        </button>
        <button
          type="button"
          title="Rename this note"
          disabled={selected === null || selectedIsGenerated}
          onclick={() => selected && (dialog = { kind: 'rename', value: selected })}
        >
          rename
        </button>
        <button
          type="button"
          title="Move this note to the trash"
          disabled={selected === null || selectedIsGenerated}
          onclick={() => (confirmDelete = selected)}
        >
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
            {searchMode === 'semantic'
              ? semanticEmptySentence(results, searchQuery.trim())
              : emptySearchSentence(results, searchQuery.trim(), awaitingIndex)}
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
        {@const figures = rowFigures(hit)}
          <li>
            <button
              type="button"
              aria-label={rowLabel({ path: hit.path, snippet: hit.snippet, score: hit.score, cosine: hit.cosine })}
              onclick={() => void openNote(hit.path, searchQuery.trim())}
            >
              <span class="hit-path">{hit.path}</span>
              <span class="hit-snippet">
                {#each markTerms(hit.snippet ?? '', searchQuery, prefixOn) as part, index}
                  {#if part.matched}<mark>{part.text}</mark>{:else}{part.text}{/if}
                {/each}
              </span>
              <span class="hit-numbers">
                <!-- note-processor's own figure, unconverted. The tooltip is where it says what the number is, because
                     every search UI is tempted to turn a ranking into "87% similar" and this one was asked for as a
                     "similarity score". -->
                {#if figures.rank !== null}
                  <!-- The tooltip names *which* measurement this is, because np's `score` field holds three different
                       ones depending on which half of search ran — a fused rank, the cosine, or negated bm25. -->
                  <span class="hit-score" data-testid="hit-score" title={`${scoreTitleFor(hit)}${figuresNote(hit)}`}>
                    {figures.rank}
                  </span>
                {/if}
                {#if figures.meaning !== null}
                  <!-- Only rows where vectors actually ran get a meaning figure; np answers `cosine: null` otherwise,
                       and "meaning 0" would describe a measurement nobody took. In meanings-only search this *is* the
                       row's score, so it stands alone rather than appearing twice (`scoreLabel.ts`). -->
                  <span class="hit-meaning" data-testid="hit-meaning" title={`${scoreTitleFor(hit)}${figuresNote(hit)}`}>
                    {figures.meaning}
                  </span>
                {/if}
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
          selectedFolder={selectedFolder}
          onselectfolder={(rel) => (selectedFolder = rel)}
          onmenu={openMenu}
        />
      </div>
      {#if formatsNotice}
        <!-- The sentence a folder of `.txt` and `.pdf` files deserves the first time it is opened, from the rows the walk
             already produced. Without it the only "answer" is a search that finds nothing in a folder full of writing. -->
        <p class="quiet formats" data-testid="formats-notice">{formatsNotice}</p>
      {/if}
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
        <!-- One button, because one decision is on the table. What used to sit here — a block of TOML to paste, Copy,
             Try again, a link into note-processor's issue tracker and the bare URL beneath it — was instructions for a
             manual job that one press has done through the pipe since 0.5.x (note-processor#7). A tester counted it in
             a screenshot and asked what any of it was for an ordinary user; the honest answer was: nothing. The detail
             is one click away in Settings, where somebody who wants it is already looking for it. -->
        <span class="buttons">
          {#if cardCanAllow}
            <button
              type="button"
              class="primary"
              data-testid="consent-allow"
              disabled={consentBusy}
              onclick={() => void setEmbeddings(true)}
            >
              Allow the download
            </button>
          {:else if consentHelp?.gap === 'configUnreadable'}
            <!-- The one state a button cannot fix: np will not rewrite a file it cannot parse. After editing it in
                 another window, this is how the window finds out. -->
            <button type="button" data-testid="consent-retry" onclick={() => void retryEmbeddings()}>Try again</button>
          {/if}
        </span>
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

      {#if notice}
        <p class="note-said" data-testid="notice">
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

    {#if downloadShown}
      <!-- The tester asked, twice, for something they could not overlook: a 295 MB download had run with no bar,
           no words, nothing. So this is a strip across the whole note pane, in bold, with np's own byte counts —
           not another word in the corner that competes with five other facts. -->
      <div class="download" role="status" data-testid="download-banner">
        <span class="words">{downloadSentence(downloadShown)}</span>
        {#if downloadShown.bytesTotal !== null}
          <progress
            data-testid="download-bar"
            value={downloadShown.bytesDone}
            max={downloadShown.bytesTotal}
          ></progress>
        {:else}
          <!-- np did not announce a size, so nothing pretends to know one: a bar at zero would be a measurement
               nobody took. A moving stripe says "bytes are arriving" and no more. -->
          <span class="stripe" role="progressbar" aria-label="downloading the model"></span>
        {/if}
      </div>
    {/if}
    {#if dialog}
      <form
        class="ask"
        onsubmit={(event) => {
          event.preventDefault()
          const value = dialog?.value.trim() ?? ''
          if (!value) return
          // Nowhere to put it: keep the name, keep the dialog, send nothing. The sentence beside the field is the
          // whole answer, and a command sent here would only be refused somewhere the person cannot see.
          if (dialog?.blocked) return
          if (dialog?.kind === 'new') void createNote(value, dialog.folder ?? '')
          else if (dialog?.kind === 'folder') void createFolder(prefixed(dialog, value))
          else if (dialog?.kind === 'move' && dialog.rel) void moveEntry(dialog.rel, moveTo(dialog.rel, value))
          else void renameEntry(value)
        }}
      >
        {#if dialog.blocked}
          <!-- Said here, in the thing the person is looking at, rather than in a status area that only exists once a
               folder is open. The button is the way out, and the name they typed survives it. -->
          <p class="warn" role="alert" data-testid="create-blocked">
            {dialog.blocked}
            <button type="button" data-testid="dialog-open-folder" onclick={() => void pickFolder()}>Choose a folder</button>
          </p>
        {/if}
        {#if dialogSaid !== null}<p class="warn" role="alert" data-testid="dialog-said">{dialogSaid}</p>{/if}
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

    {#if selected !== null}
      <!-- The tester asked twice for "a button to close a file from the editor". One existed since 0.0.16 — up in the
           sidebar toolbar, among the tree's rename/delete, nowhere near the page of text they were reading. A control
           belongs beside the thing it acts on. -->
      <div class="note-bar">
        <span class="note-name" title={selected}>{selected}</span>
        {#if note?.readonly}
          <span class="quiet">read-only</span>
        {/if}
        <button type="button" data-testid="editor-close" title="Close this note (Escape)" onclick={closeNote}>
          close
        </button>
      </div>
    {/if}

    <Editor
      saveSignal={saveSignal}
      highlight={hitQuery}
      highlightPrefix={prefixOn}
      readonly={note?.readonly ?? false}
      openFolder={() => void pickFolder()}
      rel={selected}
      {note}
      {external}
      kept={keptRel === selected}
      onphase={reportPhase}
      onsave={saveThroughCommand}
      onmirror={(rel, text) => {
        // A flush from a folder that is no longer on screen is dropped: the mirror is stored per folder, and this is
        // the one write path that can still fire after `open_vault` has moved core to another session.
        if (!bufferStillInPlace()) return
        void ipc.mirrorBuffer(rel, text)
      }}
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

  <StatusLine
    {status}
    consent={consent}
    mode={searchMode}
    model={modelFacts}
    onmodelclick={() => (showSettings = true)}
    onembed={() => void ipc.ingestAll()}
    onconsent={(granted) => void answerConsent(granted)}
  />

  {#if showDiagnostics}
    <Diagnostics
      snapshot={snapshot}
      live={traceRows}
      jottVersion={about?.jottVersion ?? 'unknown'}
      onClose={() => (showDiagnostics = false)}
    />
  {/if}

  {#if showSettings}
    <Settings
      mode={searchMode}
      prefix={prefixOn}
      vectors={results?.vectors ?? null}
      cardFacts={cardFacts}
      embeddingsLoading={embeddingsLoading}
      consentBusy={consentBusy}
      onmode={chooseMode}
      onprefix={choosePrefix}
      onconsent={(granted) => void setEmbeddings(granted)}
      onClose={() => (showSettings = false)}
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
  /* A strip, not a whisper: full width of the note pane, its own border on both edges, and text big enough to read
     while doing something else. `--surface` keeps it in the palette rather than inventing a second app colour. */
  .download {
    display: flex;
    align-items: center;
    gap: 0.8rem;
    padding: 0.65rem 1rem;
    border-block: 1px solid var(--line-strong);
    background: color-mix(in oklab, #b45309 14%, var(--surface));
  }

  .download .words {
    font-weight: 650;
    font-size: 0.95rem;
    white-space: nowrap;
  }

  .download progress {
    flex: 1 1 auto;
    min-width: 6rem;
    height: 0.7rem;
  }

  .download .stripe {
    display: block;
    width: 4.5rem;
    height: 0.55rem;
    border-radius: 99px;
    background: repeating-linear-gradient(
      115deg,
      currentColor 0 0.4rem,
      transparent 0.4rem 0.8rem
    );
    opacity: 0.65;
  }
  .window {
    display: grid;
    /* The sidebar used to start at 14 rem. At that width `Index now` and `+ note` wrapped onto two lines, the
       explanation paragraph ran five words to a line, and `INDEX.md` came out as `INDEX.ma` — a window whose own
       labels are cut off cannot be trusted about somebody else's notes. 19 rem is where the longest label in here
       stops breaking; 26 rem is where the tree starts stealing reading room from the note. */
    grid-template-columns: minmax(19rem, 26rem) 1fr;
    grid-template-rows: 1fr auto;
    height: 100vh;
    font: 14px/1.5 ui-sans-serif, system-ui, 'Segoe UI', sans-serif;
  }
  .side {
    grid-row: 1;
    display: flex;
    flex-direction: column;
    min-height: 0;
    border-right: 1px solid var(--line);
    /* Text stopped flush against the divider at the old width; a hair of breathing room keeps a wrapped line from
       looking like it is being clipped, which is what it looked like even when nothing was clipped. */
    padding-inline-end: 0.35rem;
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
    gap: 0.45rem;
    padding: 0.5rem 0.55rem 0.4rem;
  }
  /* The one thing in the header that is not about the vault, so it reads as a different kind of control. */
  .about {
    border: none;
    background: none;
    color: var(--muted);
    font: inherit;
    font-size: 0.8125rem;
    padding: 0.15rem 0.35rem;
    cursor: default;
  }
  .about:hover {
    color: inherit;
    background: none;
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
  .note-bar {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.2rem 0.5rem;
    border-block-end: 1px solid color-mix(in srgb, currentColor 18%, transparent);
    font-size: 0.8125rem;
  }
  .note-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 600;
  }
  .note-bar button {
    margin-inline-start: auto;
  }

  .tools {
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
    padding: 0 0.5rem 0.4rem;
  }
  /* Size and stretch only; the look of a field is `app.css`'s business now, so the search box and the rename bar's
     input cannot drift into two different heights again. */
  .tools input {
    width: 100%;
  }

  /* Buttons sit in a wrapping row with a real gap: at the old 0.3rem, five controls plus their borders read as one
     grey slab, and `disabled` ones were indistinguishable from enabled ones at a glance. */
  .tool-buttons {
    display: flex;
    flex-wrap: wrap;
    gap: 0.35rem;
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
  .hit-numbers {
    float: inline-end;
    font-variant-numeric: tabular-nums;
    font-size: 0.75rem;
    opacity: 0.7;
    white-space: nowrap;
  }
  .hit-score,
  .hit-meaning {
    display: inline-block;
    margin-inline-start: 0.45rem;
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
    font-size: 0.75rem;
    padding: 0.12rem 0.45rem;
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
  /* The one paragraph in the sidebar that explains rather than warns. It had no rule at all — a bare `<p>` against the
     window edge, in full-strength ink, at the same size as the file names above it, so the sentence that decides
     whether a folder of `.pdf`s is mysterious read as part of the list. */
  .formats {
    margin: 0.15rem 0 0.3rem;
    padding-inline: 0.7rem;
    font-size: 0.78rem;
    line-height: 1.45;
    color: var(--muted);
  }
  .warnings {
    margin-top: auto;
    padding: 0.45rem 0.55rem 0.5rem;
    font-size: 0.75rem;
    line-height: 1.45;
    color: var(--muted);
    border-top: 1px solid var(--line);
  }
  .warnings p {
    margin: 0.2rem 0;
  }
  .warnings .hint {
    display: block;
    color: var(--faint);
  }
  .warn,
  .np {
    color: #b8863b;
  }
  /* Every other child of the sidebar brings its own inset (`.tools` pads 0.5rem, the tree indents further), so a bare
     paragraph between them landed against the window edge — the tester's photograph of the orange refusal line, which
     read as part of the window frame rather than a sentence Jott was saying. Same inset as the search row above it. */
  .side > .warn {
    padding-inline: 0.5rem;
  }
  @media (prefers-color-scheme: dark) {
    .warn,
    .np {
      color: #d9a441;
    }
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
  /* The rename bar used to stretch its field the whole width of the note pane — a 900 px box for a file name, which
     looks less like a question about one file and more like a second address bar. */
  .ask input {
    flex: 1 1 12rem;
    min-width: 10rem;
    max-width: 30rem;
  }
  .ask label,
  .ask span {
    color: var(--muted);
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
