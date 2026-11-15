// @vitest-environment jsdom
// jsdom has no layout engine: a `Range` there cannot answer `getClientRects`, and CodeMirror measures its text in an
// animation frame — often after the test that mounted the editor has already finished, so the throw arrives as an
// unhandled error and fails a suite whose tests all passed. These tests are about which words the window shows, not
// about pixel geometry, so the measurement gets the honest jsdom answer: nothing is on screen yet.
declare global {
  interface Range {
    getClientRects(): DOMRectList
    getBoundingClientRect(): DOMRect
  }
}
const emptyRect = { x: 0, y: 0, width: 0, height: 0, top: 0, left: 0, right: 0, bottom: 0 } as DOMRect
Range.prototype.getClientRects = () => Object.assign([], { length: 0, item: () => null }) as unknown as DOMRectList
Range.prototype.getBoundingClientRect = () => emptyRect
/**
 * The window rendering *something* is the one thing no test checked until a colleague installed 0.0.3 and
 * reported a white screen. Everything else here is logic; this is the mount itself: `main.ts` runs, Svelte
 * builds a tree, and the drawn shape from `index.html` gets out of the way.
 *
 * It is deliberately shallow. A jsdom mount cannot tell me whether CodeMirror laid out a line or whether a
 * colour is legible — it tells me whether the app got as far as having a DOM at all, which is what a white
 * window means when it isn't a missing file.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { IngestEvent } from './lib/ipc'

/** Which backend this window believes it is talking to. */
type Scene = 'empty' | 'vault' | 'indexed' | 'needs-embedding'
const scenario = vi.hoisted(() => ({ current: 'empty' as unknown as Scene }))
/** What the native confirm dialog answers. Tests set it before pressing a button that asks. */
const confirmed = vi.hoisted(() => ({ current: false }))
const commands = vi.hoisted(() => [] as string[])
/** The backend version every stub reports. One place, because the About assertions below check that this exact
    string travels from `app_info` through the panel to the screen — a second copy of it would pass either way. */
const NP_STUB_VERSION = '0.4.0'

/** Set by one test to say what np claims about the folder: `false` = never scanned, `true` = indexed and searched.
    The fixture's tree markers deliberately disagree with both, so whichever way the window resolves it is a choice. */
const searchSays: { indexed: boolean | null } = { indexed: null }

/** Set by one test to make `search_vault` answer a slow query *later* than the query typed after it — the order np
 *  is free to hand back in, and the reason the window has to know which answer belongs on screen. */
const slowSearch = vi.hoisted(() => ({ on: false }))
/** A second folder in the registry's recents, for the test about reopening it. */
const extraRecent = vi.hoisted(() => ({ current: null as string | null }))
/** Whether note-processor's settings name an engine and a model. False is the state behind the consent card:
    consent was given, and nothing was fetched. */
const incompleteSettings = vi.hoisted(() => ({ current: false }))
/** Every path the window asked the file manager to reveal, in order. */
const revealed = vi.hoisted(() => ({ current: [] as string[] }))
/** External links the window asked the browser to open. */
const opened = vi.hoisted(() => ({ current: [] as string[] }))
/** The OS notifier: what permission the window was told it had, and what it actually sent. */
const notified = vi.hoisted(() => ({
  /** What the OS already said. `not-determined` is what a first run looks like. */
  permission: 'granted' as 'granted' | 'denied' | 'not-determined',
  /** What a prompt would answer if the window asked — and, after it asks, what it now believes. */
  answer: 'granted' as 'granted' | 'denied',
  asked: 0,
  sent: [] as { title: string; body: string }[],
}))
/** Whether the window believes it has focus — the other half of "should I notify?". */
const focused = vi.hoisted(() => ({ current: true }))
/** Whether the journal opened a file this run. False is what a broken state folder looks like. */
const noLog = vi.hoisted(() => ({ current: false }))
/** The window's event listeners, so a test can be the backend: fire `ingest://event` and watch what the tree
 *  does about it. */
const handlers = vi.hoisted(() => new Map<string, (payload: unknown) => void>())

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => {
    commands.push(cmd)
    void args
    const vault = {
      vault: 'C:\\notes',
      nodes: [
        { rel: 'inbox.md', kind: 'Note', size: 12, mtimeMs: 1_700_000_000_000, state: 'Waiting' },
        { rel: 'INDEX.md', kind: 'Generated', size: 900, mtimeMs: null, state: null },
      ],
      warnings: ['One folder could not be read.'],
    }
    switch (cmd) {
      case 'registry_info':
        return Promise.resolve(
          scenario.current === 'vault'
            ? {
                recent: extraRecent.current ? ['C:\\notes', extraRecent.current] : ['C:\\notes'],
                consent: 'undecided',
              }
            : { recent: [], consent: 'undecided' },
        )
      case 'open_vault':
        return Promise.resolve(vault)
      case 'app_info':
        return Promise.resolve({
          jottVersion: '0.0.5',
          np: { found: true, version: NP_STUB_VERSION, source: 'bundled', path: 'C:\\Program Files\\Jott\\np\\np.exe' },
          dataDir: 'C:\\Users\\you\\AppData\\Roaming\\jott',
          logPath: 'C:\\Users\\you\\AppData\\Roaming\\jott\\logs\\jott.jsonl',
          repoUrl: 'https://github.com/Ex-Bot-01/jott',
          releasesUrl: 'https://github.com/Ex-Bot-01/jott/releases',
          docsUrl: 'https://github.com/Ex-Bot-01/jott/blob/main/docs/testing-on-windows.md',
          diagnostics: `Jott 0.0.5 · https://github.com/Ex-Bot-01/jott\nnp ${NP_STUB_VERSION} from bundled (C:\\Program Files\\Jott\\np\\np.exe)`,
        })
      case 'search_vault': {
        const query = String((args as { query?: string } | undefined)?.query ?? '')
        const hit = (path: string, snippet: string) => ({
          hits: [
            {
              path,
              snippet,
              kind: 'note',
              score: 1.4,
              bm25: 1.4,
              cosine: null,
              // This stub stands for a backend that has no word-beginning flag: unknown, not "matched exactly".
              matchedExactly: null,
            },
          ],
          mode: 'keyword',
          vectors: false,
          offline: false,
          reason: null,
          // np 0.4.0's four extra keys. `indexed: true` here even though the fixture's tree shows a note still
          // waiting to be indexed — which is exactly the disagreement the window has to settle in np's favour.
          indexed: searchSays.indexed,
          keywordUsed: true,
          semanticUsed: false,
          notice: null,
          prefixUsed: null,
          prefixTerms: [],
        })
        if (slowSearch.on && query === 'kumquat') {
          return new Promise((resolve) => setTimeout(() => resolve(hit('stale.md', '…an old kumquat page…')), 400))
        }
        if (/zzz/.test(query)) {
          // An empty list carries the same four keys as a full one — that is rather the point of note-processor#5.
          return Promise.resolve({
            hits: [],
            mode: 'hybrid',
            vectors: false,
            offline: false,
            reason: null,
            indexed: searchSays.indexed,
            keywordUsed: true,
            semanticUsed: false,
            notice: null,
            prefixUsed: null,
            prefixTerms: [],
          })
        }
        return Promise.resolve(hit('inbox.md', '…a kumquat tree in the corner…'))
      }
      case 'read_note':
        return Promise.resolve({
          rel: 'inbox.md',
          text: '# Inbox\n\nA kumquat tree in the corner.\n',
          hash: 'a'.repeat(64),
          diskHash: 'a'.repeat(64),
          diskMtimeMs: 1_700_000_000_000,
          mirrored: null,
        })
      case 'mirror_buffer':
      case 'discard_mirror':
        // The editor mirrors a dirty buffer on a timer. A test that ends before that timer fires would otherwise
        // reject into the void, and Vitest reads one unhandled rejection as a failed run even when every assertion
        // passed — so every command the window can legitimately send gets a stub, not just the ones a test waits on.
        return Promise.resolve()
      case 'embeddings_help':
        // What np's settings actually say. `incompleteSettings` is the state behind a tester's report: consent was
        // given, and np still has no embedding engine turned on.
        return Promise.resolve(
          incompleteSettings.current
            ? {
                gap: 'providerMissing' as const,
                provider: null,
                modelId: null,
                autoFetch: true,
                configPath: 'C:\\Users\\chris\\AppData\\Roaming\\note-processor\\config.toml',
                linesToAdd: ['[embeddings]', 'provider = "local-model"', 'model = "embeddinggemma-2"'],
              }
            : { gap: 'ready' as const, provider: 'local-model', modelId: 'embeddinggemma-2', autoFetch: true, configPath: null, linesToAdd: [] },
        )
      case 'embeddings_offer':
        // What core says about setting semantic search up. `null` means "nothing to offer", which is the arm
        // every other scenario takes: a vault with no notes waiting, or bytes already on disk.
        return Promise.resolve(
          scenario.current === 'needs-embedding'
            ? { modelId: 'embeddinggemma-2', bytes: 309_855_456, dataDir: 'C:\\Users\\chris\\AppData\\Roaming\\note-processor\\models' }
            : null,
        )
      case 'diagnostics':
        // The drawer's snapshot: what this run is talking to, and what it has been asked so far.
        return Promise.resolve({
          runtime: {
            npVersion: NP_STUB_VERSION,
            npPath: 'D:\\Jott\\np\\np.exe',
            protocol: 1,
            writer: 'up',
            reader: 'idle',
            dataDir: 'C:\\Users\\chris\\AppData\\Roaming\\note-processor',
            logPath: noLog.current
              ? null
              : 'C:\\Users\\chris\\AppData\\Roaming\\jott\\logs\\jott.jsonl',
            logBytes: 4096,
            consent: 'undecided',
            autoFetch: false,
          },
          recent: [{ atMs: 1_700_000_000_000, kind: 'request', method: 'scan', outcome: 'applied', elapsedMs: 12, detail: null }],
          queue: { waiting: 0, heldForConsent: 0, backoff: null, retryInMs: null },
          vault: { notes: 1, embedded: 0, indexed: 1, waiting: 0, refused: 0, warnings: 0 },
        })
      case 'create_folder':
      case 'move_entry':
      case 'save_note':
        return Promise.resolve({ hash: 'b'.repeat(64), mtimeMs: 1_700_000_000_500 })
      case 'list_vault':
        if (scenario.current === 'needs-embedding') {
          return Promise.resolve({
            vault: 'C:\\notes',
            nodes: [
              { rel: 'inbox.md', kind: 'Note', size: 12, mtimeMs: 1_700_000_000_000, state: 'Indexed' },
            ],
            warnings: [],
          })
        }
        if (scenario.current === 'indexed') {
          return Promise.resolve({
            vault: 'C:\\notes',
            nodes: [{ rel: 'inbox.md', kind: 'Note', size: 12, mtimeMs: 1_700_000_000_000, state: 'Embedded' }],
            warnings: [],
          })
        }
        return Promise.resolve({ vault: '', nodes: [], warnings: [] })
      default:
        return Promise.reject(new Error(`the window asked for a command nobody stubbed: ${cmd}`))
    }
  },
}))

vi.mock('@tauri-apps/plugin-opener', () => ({
  // The window may open links externally and reveal one file in the folder it lives in — that is the whole surface
  // the capability set grants, and a call outside it fails at runtime rather than here.
  openUrl: (url: string) => {
    opened.current.push(url)
    return Promise.resolve()
  },
  revealItemInDir: (path: string) => {
    revealed.current.push(path)
    return Promise.resolve()
  },
}))

vi.mock('@tauri-apps/plugin-notification', () => ({
  isPermissionGranted: () => Promise.resolve(notified.permission === 'granted'),
  requestPermission: () => {
    notified.asked += 1
    notified.permission = notified.answer
    return Promise.resolve(notified.answer)
  },
  sendNotification: (note: { title: string; body: string }) => {
    notified.sent.push(note)
  },
}))

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ isFocused: () => Promise.resolve(focused.current) }),
}))

vi.mock('@tauri-apps/api/event', () => ({
  listen: async (event: string, handler: (envelope: { payload: unknown }) => void) => {
    handlers.set(event, (payload) => handler({ payload }))
    return () => {}
  },
}))
vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: async () => undefined,
  confirm: async () => confirmed.current,
}))

/** Run the app's real entry point against a fresh document, and say what it printed. */
/** Both reports as core actually sends them — every field. Partial literals used to stand in here, which meant a
    renamed wire field could be "fixed" by renaming it in the test and nothing downstream would notice. */
const scanReport = {
  added: 1,
  modified: 0,
  moved: 0,
  removed: 0,
  unchanged: 0,
  skipped: [],
  notes: 1,
  unresolved_links: 0,
  unresolved_targets: [],
  index_files: ['INDEX.md'],
  elapsed_ms: 40,
  upgrade_orphans: null,
  merged_notes: null,
  recorded_upgrade_orphans: null,
  recorded_merged_notes: null,
  unindexable: null,
}

const embedReport = {
  model: 'embeddinggemma-2',
  chunks_seen: 12,
  current: 12,
  embedded: 12,
  refused_budget: 0,
  busy_gap: 0,
  errors: 0,
  images_seen: 0,
  images_embedded: 0,
  images_refused: 0,
  images_pruned: 0,
  images_unchecked: 0,
  images_sweep_ran: false,
  image_refusals: [],
  verification: 'self-consistent, not verified',
  offline: false,
  fetched: null,
}

/** Fire an ingest event the way core does. Typed on purpose: a partial literal here used to be legal, which meant a
    renamed or dropped field on the wire could be renamed in the tests and nothing would notice (§18.5). */
function ingest(event: IngestEvent) {
  const fire = handlers.get('ingest://event')
  expect(fire, 'the window never subscribed to ingest events').toBeDefined()
  fire!(event)
}

async function start(): Promise<{ errors: unknown[] }> {
  document.body.innerHTML = '<div id="app"></div><div id="boot"><p>Jott is starting…</p></div>'
  const errors: unknown[] = []
  vi.spyOn(console, 'error').mockImplementation((...seen: unknown[]) => {
    errors.push(seen)
  })
  vi.spyOn(console, 'warn').mockImplementation(() => {})

  // Reset first: a module imported once would not mount a second time.
  vi.resetModules()
  await import('./main')
  return { errors }
}

describe('the window mounts', () => {
  beforeEach(() => {
    commands.length = 0
    scenario.current = 'empty'
    incompleteSettings.current = false
    confirmed.current = false
    revealed.current = []
    opened.current = []
    noLog.current = false
    notified.permission = 'granted'
    notified.answer = 'granted'
    notified.asked = 0
    notified.sent = []
    focused.current = true
    handlers.clear()
  })

  it('shows the empty state and takes the startup skeleton away', async () => {
    const { errors } = await start()

    // The skeleton is what a person sees while the bundle loads. Once there is a real window behind it, it
    // has to go — a leftover overlay is its own kind of white screen.
    expect(document.getElementById('boot')).toBeNull()

    const text = document.body.textContent ?? ''
    expect(text).toContain('Open a folder')
    expect(document.querySelector('input[type="search"]')).not.toBeNull()
    expect(errors).toEqual([])
  })

  it('catches the tree up when a pass finishes', async () => {
    // Reported from Windows: the status line said "completed" while every dot still said "waiting for the
    // index". The tree is a snapshot of the stamps as they were when it was listed, so a finished pass has to
    // ask for it again or the two halves of one window tell different stories.
    scenario.current = 'vault'
    await start()

    expect(document.body.textContent ?? '').toContain('not indexed yet')

    scenario.current = 'indexed'
    ingest({ indexed: { ...scanReport, elapsed_ms: 412 } })

    await vi.waitFor(() => expect(document.body.textContent ?? '').not.toContain('not indexed yet'))
    expect(document.querySelector('[title="embedded"]'), 'the marker should say embedded by now').not.toBeNull()
    expect(commands).toContain('list_vault')
  })

  it('marks the words it searched for in each hit', async () => {
    // np sends a snippet and no offsets, so the window finds the words itself (§18.11). What it marks here has
    // to be what it marks when the note opens — one module answers both, and this is the visible half.
    scenario.current = 'vault'
    await start()

    const box = document.querySelector('input[type="search"]') as HTMLInputElement
    box.value = 'kumquat'
    box.dispatchEvent(new Event('input', { bubbles: true }))

    await vi.waitFor(() => expect(document.querySelector('.hit-snippet mark')).not.toBeNull(), { timeout: 2000 })
    expect(document.querySelector('.hit-snippet mark')!.textContent).toBe('kumquat')
    expect(document.body.textContent ?? '').toContain('tree in the corner')
  })

  it('drops an answer that arrived after a newer one', async () => {
    // Three keystrokes are three requests, and np may answer them out of order. The list must end up showing what
    // was typed last — otherwise the box looks like it is ignoring you, which is what this read as in the field.
    scenario.current = 'vault'
    slowSearch.on = true
    try {
      await start()
      const box = document.querySelector('input[type="search"]') as HTMLInputElement
      box.value = 'kumquat'
      box.dispatchEvent(new Event('input', { bubbles: true }))
      // Long enough for the debounce to fire and the slow request to be in flight — not long enough for it to answer.
      await new Promise((resolve) => setTimeout(resolve, 300))
      box.value = 'kumquats'
      box.dispatchEvent(new Event('input', { bubbles: true }))

      await vi.waitFor(() => expect(document.querySelector('.hit-path')?.textContent).toBe('inbox.md'), { timeout: 2000 })
      // The slow one answers at ~650 ms, after the newer answer is already on screen; it must not land.
      await new Promise((resolve) => setTimeout(resolve, 600))
      expect(document.body.textContent ?? '').not.toContain('stale.md')
      expect(document.querySelector('.hit-path')!.textContent).toBe('inbox.md')
    } finally {
      slowSearch.on = false
    }
  })

  it('says the folder is not indexed rather than that nothing matched', async () => {
    scenario.current = 'vault'
    await start()
    const box = document.querySelector('input[type="search"]') as HTMLInputElement
    // The stub answers with an empty list for a query nobody has typed in this test run.
    box.value = 'zzznothing'
    box.dispatchEvent(new Event('input', { bubbles: true }))

    await vi.waitFor(() => expect(document.querySelector('.results .none')).not.toBeNull(), { timeout: 2000 })
    // The vault fixture has one note at state `Waiting`: the honest sentence is "not indexed yet", not "no matches".
    expect(document.querySelector('.results .none')!.textContent).toContain('not indexed yet')
  })

  it('lists the folder once for a pass, not once per event', async () => {
    // Every finished half-pass asks for the tree again, and one pass fires several events. A re-list is a walk
    // of the whole folder — hashing every note in it — so three events meant three walks inside a few hundred
    // milliseconds of each other, on the machine where that is the difference between a snappy window and a
    // stuttery one.
    scenario.current = 'vault'
    await start()
    const before = commands.filter((c) => c === 'list_vault').length

    ingest({ indexed: { ...scanReport } })
    ingest({ working: { phase: 'embedding', done: 1, total: 3, memberIndex: null, memberCount: null } })
    ingest({ embedded: { ...embedReport } })

    await vi.waitFor(() => expect(commands.filter((c) => c === 'list_vault').length).toBeGreaterThan(before))
    expect(commands.filter((c) => c === 'list_vault').length).toBe(before + 1)
  })

  it('does not ask np about embeddings when nothing is waiting for them', async () => {
    // The offer needs a `models.list`, and asking means starting the second np connection. Nothing that is
    // merely *loading a folder* should cost 8 MB of idle model-fetcher.
    scenario.current = 'indexed'
    await start()
    await vi.waitFor(() => expect(commands).toContain('list_vault'))
    expect(commands).not.toContain('embeddings_offer')
  })

  it('offers to set embeddings up when notes are indexed but have no vectors', async () => {
    // The state a tester reached in 0.0.7 and read as broken: everything indexed, nothing embedded, search
    // keyword-only, and no way in the window to change any of it. "Stay keyword-only" answers the moment, not
    // the year — so the offer has to be findable again, and pressing it has to say what it will download.
    scenario.current = 'needs-embedding'
    confirmed.current = true
    const { errors } = await start()

    await vi.waitFor(() => expect(document.body.textContent ?? '').toContain('indexed but not embedded'))
    const button = [...document.querySelectorAll('button')].find((b) => b.textContent === 'Set up embeddings')
    expect(button, 'the offer line has no way to take it up').not.toBeNull()

    button!.dispatchEvent(new MouseEvent('click', { bubbles: true }))

    await vi.waitFor(() => expect(commands).toContain('set_consent'))
    expect(commands).toContain('embeddings_offer')
    // Granting is not a note in a diary: the vault goes through the pass straight away.
    expect(commands).toContain('ingest_all')
    expect(errors).toEqual([])
  })

  it('comes back to the folder it had, and shows what the walk reported', async () => {
    scenario.current = 'vault'
    const { errors } = await start()

    // §9: the last vault reopens by itself, warnings and all.
    await vi.waitFor(() => expect(document.body.textContent ?? '').toContain('inbox.md'))
    const text = document.body.textContent ?? ''
    expect(commands).toContain('open_vault')
    expect(text).toContain('One folder could not be read.')
    // np's own catalogue is in the tree and greyed, never editable — here, at least, it is a row.
    expect(text).toContain('INDEX.md')
    expect(errors).toEqual([])
  })

  it('answers \"which np is this running\" without leaving the window', async () => {
    // This panel exists because that question used to require a conversation with whoever reported a problem.
    const { errors } = await start()

    const button = document.querySelector('button[aria-label=\"About Jott\"]')
    expect(button, 'the header has no way to open the About panel').not.toBeNull()
    button!.dispatchEvent(new MouseEvent('click', { bubbles: true }))

    await vi.waitFor(() => expect(document.body.textContent ?? '').toContain(NP_STUB_VERSION))
    const text = document.body.textContent ?? ''
    expect(text).toContain('C:\\Program Files\\Jott\\np\\np.exe')
    expect(text).toContain('bundled')
    expect(text).toContain('jott.jsonl')

    // The panel is also the way into the drawer: whoever opens About opened it because something is wrong, and
    // nobody reads a keyboard shortcut in a README at the moment they need one.
    const diag = [...document.querySelectorAll('button')].find((b) => b.textContent?.trim() === 'Diagnostics')
    expect(diag, 'the About panel has no way into the diagnostics drawer').not.toBeUndefined()
    diag!.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await vi.waitFor(() => expect(document.body.textContent ?? '').toContain('What Jott is doing'))

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    await vi.waitFor(() => expect(document.body.textContent ?? '').not.toContain('What Jott is doing'))
    expect(errors).toEqual([])
  })

  it('shows what the backend has been doing, without a second window', async () => {
    // The reason this exists: "the save hung" used to mean reading a JSONL file in a text editor at 11pm.
    scenario.current = 'vault'
    const { errors } = await start()

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'd', ctrlKey: true, shiftKey: true, bubbles: true }))
    await vi.waitFor(() => expect(document.body.textContent ?? '').toContain('What Jott is doing'))
    const text = document.body.textContent ?? ''
    expect(commands).toContain('diagnostics')
    // The np file, the log with its size, and the one request this run made.
    expect(text).toContain('D:\\Jott\\np\\np.exe')
    expect(text).toContain('jott.jsonl')
    expect(text).toContain('4.0 KiB')
    expect(text).toContain('scan · applied · 12 ms')
    // §4's promise, said out loud where the rows are: no note text in here.
    expect(text).toContain('Nothing you have typed appears here')

    // A row that arrives while the drawer is open shows up without reopening it.
    handlers.get('trace://row')?.({
      atMs: Date.now(),
      kind: 'request',
      method: 'embed',
      outcome: 'fault',
      elapsedMs: null,
      detail: 'np said the index is locked by another process',
    })
    await vi.waitFor(() => expect(document.body.textContent ?? '').toContain('embed · fault'))
    expect(document.body.textContent ?? '').toContain('np said the index is locked by another process')

    // Escape closes it, and Ctrl+Shift+D again closes it too — one key for opening should be one for shutting.
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    await vi.waitFor(() => expect(document.body.textContent ?? '').not.toContain('What Jott is doing'))

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'D', ctrlKey: true, shiftKey: true, bubbles: true }))
    await vi.waitFor(() => expect(document.body.textContent ?? '').toContain('What Jott is doing'))
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'd', ctrlKey: true, shiftKey: true, bubbles: true }))
    await vi.waitFor(() => expect(document.body.textContent ?? '').not.toContain('What Jott is doing'))

    expect(errors).toEqual([])
  })

  it('offers only what can be done to the row that was right-clicked', async () => {
    // `create_folder` and `move_entry` have been wired commands since Task 8 and nothing in the window ever called
    // them: the features existed and were unreachable. The menu is how they stop being so.
    scenario.current = 'vault'
    await start()

    const row = document.querySelector('.tree button:not(:disabled)') as HTMLElement
    row.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, clientX: 40, clientY: 90 }))

    let items: string[] = []
    await vi.waitFor(() => {
      items = [...document.querySelectorAll('.row-menu [role="menuitem"]')].map((item) => item.textContent?.trim() ?? '')
      expect(items.length).toBeGreaterThan(0)
    })
    expect(items).toEqual(['Rename  (F2)', 'Move to…', 'Move to trash  (Delete)'])

    ;(document.querySelector('.row-menu button') as HTMLElement).click()
    await vi.waitFor(() => expect(document.body.textContent ?? '').toContain('Rename to:'))
  })

  it('makes a folder where the keyboard said to', async () => {
    scenario.current = 'vault'
    commands.length = 0
    await start()

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'n', ctrlKey: true, shiftKey: true, bubbles: true }))
    await vi.waitFor(() => expect(document.body.textContent ?? '').toContain('New folder'))

    const field = document.querySelector('#rel') as HTMLInputElement
    field.value = 'reading'
    field.dispatchEvent(new Event('input', { bubbles: true }))
    ;(document.querySelector('form.ask') as HTMLFormElement).dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }))

    await vi.waitFor(() => expect(commands).toContain('create_folder'))
  })

  it('answers Ctrl+F and Ctrl+S wherever focus is', async () => {
    scenario.current = 'vault'
    await start()

    const box = document.querySelector('input[type="search"]') as HTMLInputElement
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'f', ctrlKey: true, bubbles: true }))
    expect(document.activeElement).toBe(box)

    // Ctrl+S is the key people press. Whether the buffer then needs writing is the editor's call (§11's saveState);
    // the window's job is to take the key and hand it in rather than let the browser try to save the page.
    const event = new KeyboardEvent('keydown', { key: 's', ctrlKey: true, bubbles: true, cancelable: true })
    window.dispatchEvent(event)
    expect(event.defaultPrevented).toBe(true)
  })

  it('reopens a folder from the recents list', async () => {
    // The registry has kept recent folders since Task 6 and the window showed none of them, so "back to yesterday's
    // notes" meant walking the folder picker every time.
    scenario.current = 'vault'
    extraRecent.current = 'D:\\other'
    commands.length = 0
    try {
      await start()
      const menu = document.querySelector('select.recents') as HTMLSelectElement
      expect(menu).not.toBeNull()
      expect([...menu.options].map((option) => option.textContent?.trim())).toContain('other')

      menu.value = 'D:\\other'
      menu.dispatchEvent(new Event('change', { bubbles: true }))
      await vi.waitFor(() => expect(commands.filter((cmd) => cmd === 'open_vault').length).toBeGreaterThan(1))
    } finally {
      extraRecent.current = null
    }
  })

  it('does not rename the selection while somebody is typing in the editor', async () => {
    // F2 and Delete belong to the tree — until focus is inside text, where they mean "type this". Without the
    // guard, one keystroke in a note can throw the file away.
    scenario.current = 'vault'
    await start()

    const row = document.querySelector('.tree button:not(:disabled)') as HTMLElement
    row.click()
    // The editor's own surface, not any element the shell always draws: `.editor` exists before a note is open,
    // and waiting on it says nothing.
    await vi.waitFor(() => expect(document.querySelector('[contenteditable="true"]')).not.toBeNull())

    const typing = document.querySelector('[contenteditable="true"]') ?? (document.querySelector('input[type="search"]') as Element)
    typing.dispatchEvent(new KeyboardEvent('keydown', { key: 'Delete', bubbles: true }))
    expect(document.body.textContent ?? '').not.toContain('Move ')// no trash prompt raised

    // And from the tree, where the key does belong, it asks before trashing anything.
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Delete', bubbles: true }))
    await vi.waitFor(() => expect(document.body.textContent ?? '').toContain('to the trash?'))
  })

  it('takes np’s word over its own guess about the index', async () => {
    // note-processor#5, fixed in 0.4.0: `indexed` settles which kind of empty list this is. The fixture's tree has a
    // note still waiting — Jott's old inference would have said "not indexed yet" — while np answers that the folder
    // *is* indexed and simply did not match. np's fact wins, because it looked.
    scenario.current = 'vault'
    searchSays.indexed = true
    try {
      await start()
      const box = document.querySelector('input[type="search"]') as HTMLInputElement
      box.value = 'zzznothing'
      box.dispatchEvent(new Event('input', { bubbles: true }))

      await vi.waitFor(() => expect(document.querySelector('.results .none')).not.toBeNull(), { timeout: 2000 })
      const sentence = document.querySelector('.results .none')!.textContent ?? ''
      expect(sentence).toContain('Nothing matched')
      expect(sentence).not.toContain('not indexed yet')
    } finally {
      searchSays.indexed = null
    }
  })

  it('says which half of the search ran', async () => {
    // The stub answers with keywords only. Until 0.4.0 the only word available was `mode`, and it said `hybrid`
    // for exactly this answer, so the header would have promised meanings that were never looked for.
    scenario.current = 'vault'
    await start()
    const box = document.querySelector('input[type="search"]') as HTMLInputElement
    box.value = 'kestrel'
    box.dispatchEvent(new Event('input', { bubbles: true }))

    await vi.waitFor(() => expect(document.querySelector('.hit-path')).not.toBeNull(), { timeout: 2000 })
    expect(document.querySelector('.results')!.textContent).toContain('words only')
  })

  it('explains a consent answer note-processor could not act on', async () => {
    // The report this answers: "Allow the download" pressed, then one line — `keyword-only: embeddings.provider is
    // not set` — that read as if the answer had been refused.
    incompleteSettings.current = true
    scenario.current = 'vault'
    await start()

    ingest({ disabled: { key: 'embeddings.provider', modelId: null, hint: null } })
    const card = await vi.waitFor(() => {
      const el = document.querySelector('[data-testid="consent-card"]')
      expect(el, 'a settings gap after consent gets an explanation, not a state token').not.toBeNull()
      return el as HTMLElement
    }, { timeout: 2000 })

    expect(card.textContent).toContain('models.auto_fetch = true')
    expect(card.textContent).toContain('provider = "local-model"')
    expect(card.textContent).toContain('Copy these settings')

    // "Try again" runs the pass again — the settings may have been edited in another window by then.
    commands.length = 0
    ;(card.querySelector('[data-testid="consent-retry"]') as HTMLButtonElement).click()
    await vi.waitFor(() => expect(commands).toContain('ingest_all'), { timeout: 2000 })
  })

  it('shows the card for a refusal alone', async () => {
    // Measured against np 0.4.0: an engine named with no model is answered with a *fault*
    // (`-32000 … needs embeddings.model to name an installed model`), which reaches the window as `refused`, not
    // `disabled`. Reading the settings only on `disabled` left this state with the same one opaque sentence 0.0.13
    // showed — one of the two states the release notes promise a card for.
    const sentence = 'configuration error: config io: embeddings.provider = "local-model" needs embeddings.model to name an installed model; none is set'
    incompleteSettings.current = true
    scenario.current = 'vault'
    await start()

    ingest({ refused: { message: sentence } })

    const card = await vi.waitFor(() => {
      const el = document.querySelector('[data-testid="consent-card"]')
      expect(el, 'np said embedding cannot run; that is enough to read its settings').not.toBeNull()
      return el as HTMLElement
    }, { timeout: 2000 })
    expect(card.textContent).toContain(sentence)
    expect(card.textContent).toContain('model = "embeddinggemma-2"')
  })

  it('quotes note-processor’s own refusal on the card, unchanged', async () => {
    // The odd quoting is the point: this is np's sentence, so the window shows np's sentence.
    const sentence = 'configuration error: config io: embeddings.provider = "local-model" needs embeddings.model to name an installed model; none is set'
    incompleteSettings.current = true
    scenario.current = 'vault'
    await start()

    ingest({ refused: { message: sentence } })
    ingest({ disabled: { key: 'embeddings.model', modelId: null, hint: null } })

    const card = await vi.waitFor(() => {
      const el = document.querySelector('[data-testid="consent-card"]')
      expect(el).not.toBeNull()
      return el as HTMLElement
    }, { timeout: 2000 })
    expect(card.textContent).toContain(sentence)
  })

  it('keeps quiet when note-processor is ready to embed', async () => {
    scenario.current = 'vault'
    await start()

    ingest({ disabled: { key: 'embeddings.model', modelId: null, hint: null } })
    await new Promise((resolve) => setTimeout(resolve, 120))
    // Ready to embed is not a story worth telling: no card, no lecture about settings nobody asked about.
    expect(document.querySelector('[data-testid="consent-card"]')).toBeNull()
  })

  it('announces a model that arrived, and only when one did', async () => {
    scenario.current = 'vault'
    await start()

    // An ordinary embed run downloaded nothing, so there is nothing to announce (§18.12: no invented facts).
    ingest({ embedded: { ...embedReport } })
    await new Promise((resolve) => setTimeout(resolve, 120))
    expect(document.querySelector('[data-testid="model-ready"]')).toBeNull()

    ingest({ embedded: { ...embedReport, fetched: { id: 'embeddinggemma-2', bytesTotal: 309_855_456 } } })
    const banner = await vi.waitFor(() => {
      const el = document.querySelector('[data-testid="model-ready"]')
      expect(el, 'a finished download is worth a line that stays').not.toBeNull()
      return el as HTMLElement
    }, { timeout: 2000 })
    expect(banner.textContent).toContain('model ready')
    expect(banner.textContent).toContain('embeddinggemma-2')

    // It outlives the status line's next change, and goes when the person moves on to another note.
    ingest({ working: { phase: 'indexing', done: 0, total: null, memberIndex: null, memberCount: null } })
    await new Promise((resolve) => setTimeout(resolve, 60))
    expect(document.querySelector('[data-testid="model-ready"]')).not.toBeNull()

    ingest({ embedded: { ...embedReport } })
    await new Promise((resolve) => setTimeout(resolve, 60))
    expect(document.querySelector('[data-testid="model-ready"]'), 'the next pass supersedes the announcement').toBeNull()
  })

  const readyReport = { ...embedReport, chunks_seen: 1, current: 1, embedded: 1, fetched: { id: 'embeddinggemma-2', bytesTotal: 309_855_456 } }

  it('asks once and then uses the answer', async () => {
    scenario.current = 'vault'
    focused.current = false
    notified.permission = 'not-determined'
    notified.answer = 'granted'
    await start()

    ingest({ embedded: { ...readyReport } })
    await vi.waitFor(() => expect(notified.sent.length).toBe(1), { timeout: 2000 })
    // The prompt is not re-asked for the next model: the answer is on record.
    ingest({ embedded: { ...readyReport } })
    await new Promise((resolve) => setTimeout(resolve, 120))
    expect(notified.asked).toBe(1)
    expect(notified.sent.length).toBe(2)
  })

  it('keeps the taskbar out of it while the window is focused', async () => {
    scenario.current = 'vault'
    focused.current = true
    await start()
    ingest({ embedded: { ...readyReport } })
    await vi.waitFor(() => expect(document.querySelector('[data-testid="model-ready"]')).not.toBeNull(), { timeout: 2000 })
    await new Promise((resolve) => setTimeout(resolve, 80))
    // The banner is the announcement when they are looking at it; a second copy is noise.
    expect(notified.sent).toEqual([])
  })

  it('says so in the taskbar when nobody is looking', async () => {
    scenario.current = 'vault'
    focused.current = false
    await start()
    ingest({ embedded: { ...readyReport } })
    await vi.waitFor(() => expect(notified.sent.length).toBe(1), { timeout: 2000 })
    expect(notified.sent[0].body).toContain('embeddinggemma-2')
    // No permission question when the OS already answered one.
    expect(notified.asked).toBe(0)
  })

  it('asks once about notifications, and takes a no for an answer', async () => {
    scenario.current = 'vault'
    focused.current = false
    notified.permission = 'not-determined'
    notified.answer = 'denied'
    await start()

    ingest({ embedded: { ...readyReport } })
    await vi.waitFor(() => expect(notified.asked).toBe(1), { timeout: 2000 })
    expect(notified.sent, 'a question is not permission').toEqual([])

    // The next finished model must not ask the same question again.
    ingest({ embedded: { ...readyReport } })
    await new Promise((resolve) => setTimeout(resolve, 120))
    expect(notified.asked, 'a refusal is not a question to repeat').toBe(1)
    expect(notified.sent).toEqual([])
  })

  it('forgets the announcement when the folder changes', async () => {
    // "model ready · meanings on now" is a fact about the folder whose notes were embedded. Carried into the next
    // folder it becomes a claim about a folder where nothing was fetched.
    scenario.current = 'vault'
    extraRecent.current = 'D:\\other'
    await start()

    ingest({ embedded: { ...readyReport } })
    await vi.waitFor(() => expect(document.querySelector('[data-testid="model-ready"]')).not.toBeNull(), { timeout: 2000 })

    const menu = document.querySelector('select.recents') as HTMLSelectElement
    menu.value = 'D:\\other'
    menu.dispatchEvent(new Event('change', { bubbles: true }))
    await vi.waitFor(() => expect(document.querySelector('[data-testid="model-ready"]')).toBeNull(), { timeout: 2000 })
    extraRecent.current = null
  })

  it('draws a download bar out of np’s bytes', async () => {
    // The bar exists to answer "is anything happening?" during a several-minute download. It is drawn from np's own
    // counters — and when np does not announce a total, the window shows movement without pretending to know how far.
    scenario.current = 'vault'
    await start()

    ingest({ working: { phase: 'fetching', done: 155 * 1024 * 1024, total: 310 * 1024 * 1024, memberIndex: 1, memberCount: 2 } })
    const bar = await vi.waitFor(() => {
      const found = document.querySelector('footer.status progress')
      expect(found, 'a fetch with a known size gets a determinate bar').not.toBeNull()
      return found as HTMLProgressElement
    }, { timeout: 2000 })
    expect(Number(bar.getAttribute('max'))).toBe(310 * 1024 * 1024)
    expect(Number(bar.value)).toBe(155 * 1024 * 1024)
    await vi.waitFor(() => expect(document.querySelector('footer.status .text')!.textContent).toContain('model part 1 of 2'))

    ingest({ working: { phase: 'fetching', done: 1024, total: null, memberIndex: null, memberCount: null } })
    await vi.waitFor(() => {
      // Unknown size is movement without a number, never a bar parked at zero.
      expect(document.querySelector('footer.status progress')).toBeNull()
      expect(document.querySelector('footer.status [role="progressbar"]')).not.toBeNull()
    }, { timeout: 2000 })

    // And no counters at all means no bar: `indexing` arrives with nothing measurable (§18.12).
    ingest({ working: { phase: 'indexing', done: 0, total: null, memberIndex: null, memberCount: null } })
    await vi.waitFor(() => {
      expect(document.querySelector('footer.status progress')).toBeNull()
      expect(document.querySelector('footer.status [role="progressbar"]')).toBeNull()
    }, { timeout: 2000 })
  })
  it('reveals the log file itself, not a box of text', async () => {
    // The report: "Open log folder" opened a small window full of copied diagnostics text. It was `openPath` being
    // refused by the capability set, and the failure fallback showing what had been copied for "Copy details".
    scenario.current = 'vault'
    await start()
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'd', ctrlKey: true, shiftKey: true, bubbles: true }))
    await vi.waitFor(() => expect(document.body.textContent ?? '').toContain('What Jott is doing'))

    const button = document.querySelector('[data-testid="show-log"]') as HTMLButtonElement
    expect(button, 'the drawer offers one way to the log').not.toBeNull()
    expect(button.textContent).toContain('Show log file')
    expect(button.disabled).toBe(false)
    button.dispatchEvent(new MouseEvent('click', { bubbles: true }))

    // The file, named in full, so Explorer opens with it selected rather than loose in a folder of stamps.
    await vi.waitFor(() =>
      expect(revealed.current).toEqual(['C:\\Users\\chris\\AppData\\Roaming\\jott\\logs\\jott.jsonl']),
    )
  })

  it('will not offer to reveal a log that was never opened', async () => {
    scenario.current = 'vault'
    noLog.current = true
    await start()
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'd', ctrlKey: true, shiftKey: true, bubbles: true }))
    await vi.waitFor(() => expect(document.body.textContent ?? '').toContain('What Jott is doing'))

    const button = document.querySelector('[data-testid="show-log"]') as HTMLButtonElement
    expect(button.disabled, 'nothing to reveal means a disabled button, not a reveal of nothing').toBe(true)
    button.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await new Promise((resolve) => setTimeout(resolve, 60))
    expect(revealed.current).toEqual([])
  })

})
