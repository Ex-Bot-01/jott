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
import { EditorView } from '@codemirror/view'
import { unmount, type Component } from 'svelte'
import { MODE_KEY, PREFIX_KEY } from './lib/searchSettings'
import type { IngestEvent } from './lib/ipc'

/** Which backend this window believes it is talking to. */
type Scene = 'empty' | 'vault' | 'indexed' | 'needs-embedding'
const scenario = vi.hoisted(() => ({ current: 'empty' as unknown as Scene }))
/** What the native confirm dialog answers. Tests set it before pressing a button that asks. */
const confirmed = vi.hoisted(() => ({ current: false }))
const commands = vi.hoisted(() => [] as string[])
/** Every argument set the window sent to `search_vault`, so a test can see what actually crossed the wire. */
const searchCalls = vi.hoisted(() => [] as Record<string, unknown>[])
/** The backend version every stub reports. One place, because the About assertions below check that this exact
    string travels from `app_info` through the panel to the screen — a second copy of it would pass either way. */
// The build this release ships with (§12). An older number here once hid a test that only passed against a backend
// Jott no longer accepts.
const NP_STUB_VERSION = '0.5.1'

/** Set by one test to say what np claims about the folder: `false` = never scanned, `true` = indexed and searched.
    The fixture's tree markers deliberately disagree with both, so whichever way the window resolves it is a choice. */
const searchSays: { indexed: boolean | null } = { indexed: null }

/** When on, `search_vault` answers the way np 0.5.1 does with word-beginning matching: the row was reached from
 *  `grand`, and np says so in `prefix_used`, `prefix_terms` and `matched_exactly`. */
const widening = vi.hoisted(() => ({ on: false }))

/** A stand-in for `matchMedia`, which jsdom does not have: the window asks which colour scheme the desktop is in,
 *  and CodeMirror has to be told the answer (0.0.15 never told it, so CM painted a black caret on dark paper). */
const media = vi.hoisted(() => {
  const listeners = new Set<(event: { matches: boolean }) => void>()
  return {
    matches: false,
    addEventListener: (_: string, fn: (event: { matches: boolean }) => void) => void listeners.add(fn),
    removeEventListener: (_: string, fn: (event: { matches: boolean }) => void) => void listeners.delete(fn),
    /** Start over: no leftover listeners from a window this test already destroyed. */
    reset(matches = false) {
      listeners.clear()
      this.matches = matches
    },
    /** Flip the scheme the way the OS would, with the window open. */
    set(next: boolean) {
      this.matches = next
      for (const fn of [...listeners]) fn({ matches: next })
    },
  }
})

/** Every `create_folder` / `move_entry` the window sent, with its arguments, in order. */
const moveCalls = vi.hoisted(() => ({ current: [] as Record<string, unknown>[] }))
/** Make `create_folder` refuse, to check what the dialog does when half a plan fails. */
const moveSays = vi.hoisted(() => ({ createFails: false, moveFails: false }))
/** What the desktop folder picker answers. `null` is a cancelled dialog; a path is a chosen folder. */
const picked = vi.hoisted(() => ({ current: null as string | null }))
/** The listing each scene answers with, whichever command asks for it. Opening a folder and re-listing it must not be
 *  able to disagree about what is inside — and `list_vault` with no session open is a refusal, not an empty listing. */
const listingFor = vi.hoisted(() => (scene: string, path?: string) => {
  const base = {
    vault: path || 'C:\\notes',
    nodes: [
      { rel: 'inbox.md', kind: 'Note', size: 12, mtimeMs: 1_700_000_000_000, state: 'Waiting' },
      { rel: 'INDEX.md', kind: 'Generated', size: 900, mtimeMs: null, state: null },
    ],
    warnings: ['One folder could not be read.'],
  }
  if (scene === 'needs-embedding') {
    return { ...base, nodes: [{ rel: 'inbox.md', kind: 'Note', size: 12, mtimeMs: 1_700_000_000_000, state: 'Indexed' }] }
  }
  if (scene === 'indexed') {
    return { ...base, nodes: [{ rel: 'inbox.md', kind: 'Note', size: 12, mtimeMs: 1_700_000_000_000, state: 'Embedded' }] }
  }
  return base
})
/** Whether `open_vault` succeeds. A folder can be chosen and then refused — missing, unreadable, nested in another. */
const openVaultSays = vi.hoisted(() => ({ fails: false }))

/** Whether `read_note` answers with note-processor's catalogue (read-only) for `INDEX.md`. */
const readNoteSays = vi.hoisted(() => ({ catalogue: false }))

/** What `set_consent` hands back: np's own answer to the button (np 0.5.x). */
const consentAnswer = vi.hoisted(() => ({ current: 'accepted' as 'accepted' | 'refused' }))

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
          scenario.current === 'empty'
            ? { recent: [], consent: 'undecided' }
            : {
                recent:
                  scenario.current === 'vault' && extraRecent.current
                    ? ['C:\\notes', extraRecent.current]
                    : ['C:\\notes'],
                consent: 'undecided',
              },
        )
      case 'open_vault':
        if (openVaultSays.fails) {
          return Promise.reject({ kind: 'io', message: 'C:\\gone is not a folder any more' })
        }
        // The listing answers for the folder that was asked for. Handing back the same path every time hid a real
        // bug: the window could not tell a new folder from a re-list of the one already open.
        return Promise.resolve(listingFor(scenario.current, (args as { path?: string } | undefined)?.path))
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
        searchCalls.push({ ...(args ?? {}) })
        const query = String((args as { query?: string } | undefined)?.query ?? '')
        if (widening.on) {
          return Promise.resolve({
            hits: [
              {
                path: 'family.md',
                snippet: 'my grandmother kept a jar of kumquats',
                kind: 'note',
                score: 1.1,
                bm25: null,
                cosine: 0.82,
                matchedExactly: false,
              },
            ],
            mode: 'hybrid',
            vectors: true,
            offline: false,
            reason: null,
            indexed: true,
            keywordUsed: true,
            semanticUsed: true,
            notice: null,
            prefixUsed: true,
            prefixTerms: ['grand'],
          })
        }
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
        // `grand` is empty unless the widened fixture is switched on — which is how the offer test reaches a folder
        // that is indexed, whole-word-empty, and widened-with-a-hit.
        if (/zzz/.test(query) || (!widening.on && query === 'grand')) {
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
      case 'read_note': {
        const rel = String((args as { rel?: string } | undefined)?.rel ?? 'inbox.md')
        if (rel === 'INDEX.md' && readNoteSays.catalogue) {
          // What np publishes at the vault root: readable, and not Jott's to write.
          return Promise.resolve({
            rel,
            text: '# INDEX\n_generated by note-processor; do not edit_\n',
            hash: 'c'.repeat(64),
            diskHash: 'c'.repeat(64),
            diskMtimeMs: 1_700_000_000_000,
            mirrored: null,
            readonly: true,
          })
        }
        if (rel === 'family.md') {
          // Written so whole-word marking and prefix marking disagree: whole words mark the letters inside both
          // *agrand* and *grandmother*; prefix rules mark only the one that starts a word.
          return Promise.resolve({
            rel,
            text: '# Family\n\nthe agrand family kept a grandmother here\n',
            hash: 'f'.repeat(64),
            diskHash: 'f'.repeat(64),
            diskMtimeMs: 1_700_000_000_000,
            mirrored: null,
          })
        }
        return Promise.resolve({
          rel: 'inbox.md',
          text: '# Inbox\n\nA kumquat tree in the corner.\n',
          hash: 'a'.repeat(64),
          diskHash: 'a'.repeat(64),
          diskMtimeMs: 1_700_000_000_000,
          mirrored: null,
        })
      }
      case 'mirror_buffer':
      case 'discard_mirror':
        // The editor mirrors a dirty buffer on a timer. A test that ends before that timer fires would otherwise
        // reject into the void, and Vitest reads one unhandled rejection as a failed run even when every assertion
        // passed — so every command the window can legitimately send gets a stub, not just the ones a test waits on.
        return Promise.resolve()
      case 'set_consent':
        return Promise.resolve(
          consentAnswer.current === 'accepted'
            ? {
                granted: true,
                npRefused: null,
                changed: ['embeddings.provider', 'embeddings.model'],
                provider: 'local-model',
                model: 'embeddinggemma-2',
                persistedTo: 'C:\\Users\\chris\\AppData\\Roaming\\note-processor\\config.toml',
                fetchBytes: 309_855_456,
                autoFetch: 'written',
                autoFetchError: null,
              }
            : {
                granted: true,
                npRefused: 'config io: no pinned model "no-such": this build can name "embeddinggemma-2"',
                changed: [],
                provider: null,
                model: null,
                persistedTo: null,
                fetchBytes: null,
                autoFetch: 'untouched',
                autoFetchError: null,
              },
        )
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
        moveCalls.current.push({ cmd, ...(args as Record<string, unknown>) })
        if (scenario.current === 'empty') {
          // Core has no session to make a folder in. Stubbing this as a success is how the window could look
          // correct while its first-run buttons did nothing at all.
          return Promise.reject({ kind: 'noVault', message: 'no vault is open' })
        }
        if (moveSays.createFails) {
          // The shape Tauri hands back for a refused command: an object with a sentence in it.
          return Promise.reject({ kind: 'io', message: 'could not create ideas: permission denied' })
        }
        // `Result<(), IpcError>` over the wire is `null`. A stub that invented an object here let the window read
        // every real success as a failure and skip the refresh that follows one.
        return Promise.resolve(null)
      case 'create_note': {
        moveCalls.current.push({ cmd, ...(args as Record<string, unknown>) })
        if (scenario.current === 'empty') {
          return Promise.reject({ kind: 'noVault', message: 'no vault is open' })
        }
        return Promise.resolve({ hash: 'c'.repeat(64), mtimeMs: 1_700_000_000_500 })
      }
      case 'move_entry':
        moveCalls.current.push({ cmd, ...(args as Record<string, unknown>) })
        if (moveSays.moveFails) {
          return Promise.reject({ kind: 'intoItself', message: 'cannot move a folder into itself' })
        }
        // What core actually sends for `Result<(), IpcError>`: null. The window must tell "nothing to report" from
        // "it failed" without pretending the wire carries more than it does.
        return Promise.resolve(null)
      case 'save_note':
        return Promise.resolve({ hash: 'b'.repeat(64), mtimeMs: 1_700_000_000_500 })
      case 'list_vault':
        if (scenario.current === 'empty') {
          // With no session open, core refuses rather than inventing an empty listing (`app.current()?` in the shell).
          // The `{ vault: '' }` this stub used to answer is a state that cannot happen, and it let every "no folder is
          // open" test describe a window that was quietly holding one.
          return Promise.reject({ kind: 'noVault', message: 'no vault is open' })
        }
        return Promise.resolve(listingFor(scenario.current))
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
  open: async () => picked.current ?? undefined,
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

/** Mount a fresh window. The returned handle lets a test unmount it and mount another, which is the only honest way
    to ask "does this survive closing the app?" — `localStorage` outlives the component either way, but an orphaned
    instance left mounted would keep answering events behind the next window's back. */
async function start(): Promise<{ errors: unknown[]; app: Component }> {
  document.body.innerHTML = '<div id="app"></div><div id="boot"><p>Jott is starting…</p></div>'
  const errors: unknown[] = []
  vi.spyOn(console, 'error').mockImplementation((...seen: unknown[]) => {
    errors.push(seen)
  })
  vi.spyOn(console, 'warn').mockImplementation(() => {})

  // Reset first: a module imported once would not mount a second time.
  vi.resetModules()
  // `main.ts` default-exports what `mount()` returned, which is exactly what `unmount()` takes.
  const app = (await import('./main')).default as Component
  return { errors, app }
}

describe('the window mounts', () => {
  beforeEach(() => {
    commands.length = 0
    searchCalls.length = 0
    widening.on = false
    consentAnswer.current = 'accepted'
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
    moveCalls.current = []
    picked.current = null
    openVaultSays.fails = false
    moveSays.createFails = false
    moveSays.moveFails = false
    // Reset where they are set: both of these were restored at the *end* of their tests, so a failing assertion left
    // them moved for every test afterwards — and the ones after that went on passing for a reason nobody could see.
    searchSays.indexed = null
    readNoteSays.catalogue = false
    // Both remembered search choices are read at mount, so a leftover from the previous test would decide what this
    // one's first window asks for — and it would look like the feature working.
    localStorage.clear()
    handlers.clear()
    // The window asks the desktop for its colour scheme; jsdom has no `matchMedia`, so an unstubbed call is an
    // unhandled error after the test finished — which Vitest counts as a failed run over passing tests.
    media.reset()
    ;(window as unknown as { matchMedia: unknown }).matchMedia = () => media
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
    // A second run comes back to its folder by itself, so the listing arrives through `open_vault`.
    await vi.waitFor(() => expect(commands).toContain('open_vault'))
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

    const row = document.querySelector('.tree button[data-kind="Note"]') as HTMLElement
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

    const row = document.querySelector('.tree button[data-kind="Note"]') as HTMLElement
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


  it('remembers word beginnings across a relaunch, and says so on the wire', async () => {
    // note-processor#6 arrived in np 0.5.x: `grand` reaches *grandmother* only when the request asks for it. The
    // toggle is off by default — widening changes what a query means — so the first thing worth proving is that an
    // untouched window sends `prefix: false` rather than hoping np's default matches ours.
    localStorage.removeItem(PREFIX_KEY)
    const first = await start()

    const box = document.querySelector('input[type="search"]') as HTMLInputElement
    box.value = 'grand'
    box.dispatchEvent(new Event('input', { bubbles: true }))
    await vi.waitFor(() => expect(searchCalls.length).toBeGreaterThan(0))
    expect(searchCalls.at(-1)?.prefix, 'an untouched window searches whole words').toBe(false)

    const toggle = document.querySelector('input[aria-label="Match word beginnings"]') as HTMLInputElement
    expect(toggle, 'the window offers the choice').not.toBeNull()
    expect(toggle.checked).toBe(false)

    toggle.checked = true
    toggle.dispatchEvent(new Event('change', { bubbles: true }))
    await vi.waitFor(() => expect(localStorage.getItem(PREFIX_KEY)).toBe('1'))
    // Flipping it re-runs the query that is already typed, immediately: a debounce here would read as a click that
    // changed nothing about the results on screen.
    await vi.waitFor(() => expect(searchCalls.at(-1)?.prefix).toBe(true))

    unmount(first.app)
    const second = await start()

    const again = document.querySelector('input[aria-label="Match word beginnings"]') as HTMLInputElement
    expect(again.checked, 'the choice is the app’s memory, not this window’s').toBe(true)
    const box2 = document.querySelector('input[type="search"]') as HTMLInputElement
    box2.value = 'grand'
    box2.dispatchEvent(new Event('input', { bubbles: true }))
    await vi.waitFor(() => expect(searchCalls.at(-1)?.prefix).toBe(true))

    expect(second.errors).toEqual([])
  })


  it('says which words note-processor widened, and marks the rows it reached that way', async () => {
    // note-processor#6: a widened answer must not look like an exact one. np names the words it widened and flags
    // the row; the window prints both, because "3 results" for a search that silently grew is the same family of
    // lie as a `mode` word that claimed vectors ran when they did not.
    widening.on = true
    const { errors } = await start()

    const box = document.querySelector('input[type="search"]') as HTMLInputElement
    box.value = 'grand'
    box.dispatchEvent(new Event('input', { bubbles: true }))

    await vi.waitFor(() => expect(document.body.textContent ?? '').toContain('also matched word beginnings (grand)'))
    expect(document.body.textContent ?? '').toContain('from the start of a word')
    // The mark covers the letters that matched — `grand` — and not the whole word np widened it into.
    const marks = [...document.querySelectorAll('.results mark')].map((node) => node.textContent)
    expect(marks).toEqual(['grand'])

    widening.on = false
    expect(errors).toEqual([])
  })


  it('lets note-processor’s own answer fill the card', async () => {
    // The point of np 0.5.x (`embeddings.enable`, note-processor#7): pressing Allow gets an *answer*, and the card
    // should quote that answer instead of going on telling somebody to paste lines into a file that was just written
    // for them.
    incompleteSettings.current = true
    scenario.current = 'needs-embedding'
    confirmed.current = true
    const { errors } = await start()

    await vi.waitFor(() => expect(document.body.textContent ?? '').toContain('indexed but not embedded'))
    const button = [...document.querySelectorAll('button')].find((b) => b.textContent === 'Set up embeddings')
    button!.dispatchEvent(new MouseEvent('click', { bubbles: true }))

    const card = await vi.waitFor(() => {
      const el = document.querySelector('[data-testid="consent-card"]')
      expect(el, 'an answer from np belongs on the card').not.toBeNull()
      return el as HTMLElement
    }, { timeout: 2000 })

    expect(card.textContent).toContain('note-processor wrote embeddings.provider and embeddings.model')
    expect(card.textContent).toContain('AppData')
    // The paste block is gone: two contradictory instructions on one screen read as a window that did not hear itself.
    expect(card.textContent).not.toContain('provider = "local-model"')
    expect(card.textContent).toContain('Copy what happened')
    // Jott's part, stated beside np's — the card does not claim credit for settings it never wrote.
    expect(card.textContent).toContain('models.auto_fetch = true')

    incompleteSettings.current = false
    expect(errors).toEqual([])
  })

  it('says what np refused and that Jott changed nothing of its own', async () => {
    // A refusal has to be legible as a refusal. The important half is the guardrail: after a refused Yes,
    // `models.auto_fetch` must read as untouched rather than as allowed.
    incompleteSettings.current = true
    scenario.current = 'needs-embedding'
    confirmed.current = true
    consentAnswer.current = 'refused'
    const { errors } = await start()

    await vi.waitFor(() => expect(document.body.textContent ?? '').toContain('indexed but not embedded'))
    const button = [...document.querySelectorAll('button')].find((b) => b.textContent === 'Set up embeddings')
    button!.dispatchEvent(new MouseEvent('click', { bubbles: true }))

    const card = await vi.waitFor(() => {
      const el = document.querySelector('[data-testid="consent-card"]')
      expect(el, 'a refusal is the one thing the card must never soften').not.toBeNull()
      return el as HTMLElement
    }, { timeout: 2000 })

    expect(card.textContent).toContain('no pinned model "no-such"')
    expect(card.textContent).toContain('left models.auto_fetch alone')
    expect(card.textContent).not.toContain('provider = "local-model"')

    incompleteSettings.current = false
    expect(errors).toEqual([])
  })

  it('offers word beginnings on an empty answer, once, without deciding for anybody', async () => {
    // np says the folder is indexed and the whole-word search came back empty: the one moment where offering the
    // widening can change the answer. Pressing it widens *this* query and leaves the remembered preference alone.
    localStorage.removeItem(PREFIX_KEY)
    searchSays.indexed = true
    const { errors } = await start()

    const box = document.querySelector('input[type="search"]') as HTMLInputElement
    box.value = 'zzz'
    box.dispatchEvent(new Event('input', { bubbles: true }))

    const offer = await vi.waitFor(() => {
      const el = document.querySelector('[data-testid="widen-offer"]') as HTMLButtonElement | null
      expect(el, 'an indexed folder with no whole-word hits should offer the widening').not.toBeNull()
      return el!
    }, { timeout: 2000 })

    searchCalls.length = 0
    offer.click()
    await vi.waitFor(() => expect(searchCalls.at(-1)?.prefix).toBe(true))
    // The remembered choice is untouched — that is what the checkbox beside the box is for.
    expect(localStorage.getItem(PREFIX_KEY)).toBeNull()
    const toggle = document.querySelector('input[aria-label="Match word beginnings"]') as HTMLInputElement
    expect(toggle.checked, 'a one-off offer must not move the toggle').toBe(false)

    // Typing a new question starts from the remembered setting again.
    box.value = 'zzzu'
    box.dispatchEvent(new Event('input', { bubbles: true }))
    await vi.waitFor(() => expect(searchCalls.at(-1)?.prefix).toBe(false))

    searchSays.indexed = null
    expect(errors).toEqual([])
  })


  it('marks the note with the rules that produced its row', async () => {
    // The one-off widening (pressing *try matching word beginnings*) asks np with `prefix: true`. If the window then
    // paints the results and opens the note with whole-word rules, the letters underlined are not the ones that
    // matched — and in this fixture it is visibly wrong: whole-word marking lights up inside *agrand* as well.
    localStorage.removeItem(PREFIX_KEY)
    searchSays.indexed = true
    const { errors } = await start()

    const box = document.querySelector('input[type="search"]') as HTMLInputElement
    box.value = 'grand'
    box.dispatchEvent(new Event('input', { bubbles: true }))
    const offer = await vi.waitFor(() => {
      const el = document.querySelector('[data-testid="widen-offer"]') as HTMLButtonElement | null
      expect(el, 'the empty indexed folder should offer the widening').not.toBeNull()
      return el!
    }, { timeout: 2000 })

    // np answers the widened request with the family note.
    widening.on = true
    offer.click()
    const row = await vi.waitFor(() => {
      const el = [...document.querySelectorAll('.results button')].find((b) => b.textContent?.includes('family.md'))
      expect(el, 'the widened search should put a row on screen').not.toBeUndefined()
      return el as HTMLButtonElement
    }, { timeout: 2000 })

    // The row marks one stretch — `grand` at the start of *grandmother* — not two.
    expect([...row.querySelectorAll('mark')].map((m) => m.textContent)).toEqual(['grand'])

    row.click()
    await vi.waitFor(() => expect(document.body.textContent ?? '').toContain('grandmother'), { timeout: 2000 })
    // The note's own marks are the same `matchRanges` call with the same flag (`highlightPrefix`), and jsdom never
    // paints CodeMirror's document — `.cm-content` is empty here — so there is nothing on screen to count. That half
    // is pinned in `highlight.test.ts` (prefix rules mark only *grandmother*) and by the request below, which is what
    // decides the flag in the first place.
    expect(
      (searchCalls.at(-1) as { prefix?: boolean } | undefined)?.prefix,
      'the note was opened from a widened request',
    ).toBe(true)

    widening.on = false
    searchSays.indexed = null
    expect(errors).toEqual([])
  })


  it('keeps what np answered when a later refusal arrives', async () => {
    // After a press, np can still refuse the pass it was asked to run. The card used to drop the receipt at that
    // moment and go back to describing the file — the one thing it may not do after np has just written it.
    scenario.current = 'needs-embedding'
    confirmed.current = true
    const { errors } = await start()

    await vi.waitFor(() => expect(document.body.textContent ?? '').toContain('indexed but not embedded'))
    const button = [...document.querySelectorAll('button')].find((b) => b.textContent === 'Set up embeddings')
    expect(button, 'the offer line has no way to take it up').not.toBeNull()
    button!.dispatchEvent(new MouseEvent('click', { bubbles: true }))

    const said = await vi.waitFor(() => {
      const text = document.body.textContent ?? ''
      if (!text.includes('note-processor wrote embeddings.provider')) {
        console.log('BODY>>>', text.slice(0, 600))
      }
      expect(text).toContain('note-processor wrote embeddings.provider')
      return text
    }, { timeout: 1500 })
    expect(said).toContain('Jott wrote models.auto_fetch = true')

    // np refuses the next pass, for its own reason.
    ingest({ refused: { message: 'configuration error: config io: embeddings.model names no installed model' } })

    const both = await vi.waitFor(() => {
      const text = document.body.textContent ?? ''
      expect(text).toContain('embeddings.model names no installed model')
      return text
    }, { timeout: 2000 })

    // Both facts on one card — what np recorded, and what it says now — and never the paste-advice again, because
    // those lines are in the file already.
    expect(both).toContain('note-processor wrote embeddings.provider')
    expect(both).toContain('Jott wrote models.auto_fetch = true')
    expect(both).not.toContain('Copy these settings')
    expect(errors).toEqual([])
  })


  it('tells the editor which colour scheme the desktop is in', async () => {
    // The complaint: "the cursor is near invisible in dark mode". Part of the answer is ours to fix and part is
    // CodeMirror's — it painted its light-facet caret because nothing ever told it otherwise. `cm-dark` on the
    // editor element is the observable half of that being fixed.
    scenario.current = 'vault'
    media.reset(true)
    const { errors } = await start()

    const row = document.querySelector('.tree button[data-kind="Note"]') as HTMLElement
    expect(row, 'the fixture has a note to open').not.toBeNull()
    row.dispatchEvent(new MouseEvent('click', { bubbles: true }))

    await vi.waitFor(() => expect(document.querySelector('.cm-editor')).not.toBeNull(), { timeout: 2000 })
    expect(document.querySelector('.cm-editor.jott-dark'), 'a dark desktop means a dark CodeMirror').not.toBeNull()

    // The desktop changes its mind while the note is open. The editor follows — same element, so the buffer and any
    // unsaved typing inside it survive the switch.
    const before = document.querySelector('.cm-editor')
    media.set(false)
    await vi.waitFor(() => expect(document.querySelector('.cm-editor.jott-dark')).toBeNull(), { timeout: 2000 })
    expect(document.querySelector('.cm-editor'), 'rebuilding the editor would drop what was typed').toBe(before)

    expect(errors).toEqual([])
  })


  it('closes the note that is open', async () => {
    // The ask: "I see no close or X button where I can close my current note." There was not one, and there was no
    // other way out either — `selected` only ever changed by clicking another row.
    scenario.current = 'vault'
    const { errors } = await start()

    const close = () => document.querySelector('[data-testid="close-note"]') as HTMLButtonElement | null
    expect(close(), 'the window offers a way to close a note').not.toBeNull()
    expect(close()!.disabled, 'with nothing open there is nothing to close').toBe(true)

    ;(document.querySelector('.tree button[data-kind="Note"]') as HTMLElement).dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await vi.waitFor(() => expect(document.querySelector('.cm > .cm-editor')).not.toBeNull(), { timeout: 2000 })
    expect(close()!.disabled).toBe(false)

    close()!.dispatchEvent(new MouseEvent('click', { bubbles: true }))

    await vi.waitFor(() => expect(document.querySelector('.cm > .cm-editor')).toBeNull(), { timeout: 2000 })
    expect(document.querySelector('[data-testid="no-note"]'), 'an empty editor says what to do next').not.toBeNull()
    expect(close()!.disabled, 'and the button knows there is nothing open now').toBe(true)
    // Closing is not saving. Nothing was dirty, and even so: no write went anywhere.
    expect(commands).not.toContain('save_note')

    // The same key, from the body.
    ;(document.querySelector('.tree button[data-kind="Note"]') as HTMLElement).dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await vi.waitFor(() => expect(document.querySelector('.cm > .cm-editor')).not.toBeNull(), { timeout: 2000 })
    document.body.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    await vi.waitFor(() => expect(document.querySelector('.cm > .cm-editor')).toBeNull(), { timeout: 2000 })

    expect(errors).toEqual([])
  })

  it('leaves the note alone when Escape arrives from inside a text field', async () => {
    // Escape in the search box is not a request to throw the visible note away — that key belongs to whoever is
    // typing, same rule as F2 and Delete.
    scenario.current = 'vault'
    const { errors } = await start()

    ;(document.querySelector('.tree button[data-kind="Note"]') as HTMLElement).dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await vi.waitFor(() => expect(document.querySelector('.cm > .cm-editor')).not.toBeNull(), { timeout: 2000 })

    const box = document.querySelector('input[type="search"]') as HTMLInputElement
    box.focus()
    box.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    await new Promise((resolve) => setTimeout(resolve, 120))
    expect(document.querySelector('.cm > .cm-editor'), 'the note is still open').not.toBeNull()

    expect(errors).toEqual([])
  })


  it('opens note-processor’s own index without offering to edit it', async () => {
    // "I can not open the Index.md file. Can we make it visible but read only?" — so looking is allowed and
    // typing is not, and the window says which of the two it is showing.
    scenario.current = 'vault'
    readNoteSays.catalogue = true
    const { errors } = await start()

    const row = [...document.querySelectorAll('.tree button')].find((b) => b.textContent?.includes('INDEX.md'))
    expect(row, 'np’s catalogue is a row in the tree').not.toBeUndefined()
    expect((row as HTMLButtonElement).disabled, 'a row you cannot click reads as a broken file').toBe(false)

    ;(row as HTMLButtonElement).dispatchEvent(new MouseEvent('click', { bubbles: true }))

    await vi.waitFor(() => expect(document.querySelector('.cm-editor')).not.toBeNull(), { timeout: 2000 })
    const content = document.querySelector('.cm-content') as HTMLElement
    expect(content.getAttribute('contenteditable'), 'np wrote this file; Jott does not offer to change it').toBe('false')
    expect(document.body.textContent ?? '').toContain('_generated by note-processor')
    expect(document.body.textContent ?? '').toContain('note-processor’s own index')

    // Ctrl+S in a file nobody may write: no command, and the reason on screen.
    commands.length = 0
    document.body.dispatchEvent(new KeyboardEvent('keydown', { key: 's', ctrlKey: true, bubbles: true }))
    await new Promise((resolve) => setTimeout(resolve, 120))
    expect(commands, 'saving np’s catalogue is not something Jott sends').not.toContain('save_note')
    expect(document.body.textContent ?? '').toContain('cannot be edited')

    // An ordinary note next to it is still editable — the flag is per file, not a mode for the window.
    const note = [...document.querySelectorAll('.tree button')].find((b) => b.textContent?.includes('inbox.md'))
    ;(note as HTMLButtonElement).dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await vi.waitFor(() => expect(document.querySelector('.cm-content')?.getAttribute('contenteditable')).toBe('true'), { timeout: 2000 })

    readNoteSays.catalogue = false
    expect(errors).toEqual([])
  })


  it('moves a note into a folder that does not exist yet', async () => {
    // "I can right click on a note and select Move to…, but I see only an empty field — there is no folder inside
    // it. So add an add-folder button, and then the file moves there." The vault fixture has no folders at all, so
    // this is exactly the state that used to offer nothing.
    scenario.current = 'vault'
    const { errors } = await start()

    let row!: HTMLElement
    await vi.waitFor(() => {
      row = document.querySelector('.tree button[data-kind="Note"]') as HTMLElement
      expect(row, 'the fixture has a note to move').not.toBeNull()
    }, { timeout: 2000 })
    row.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, clientX: 40, clientY: 90 }))

    // The menu is rendered by Svelte, so it arrives on the next microtask — asking for it synchronously is how a
    // test manages to fail at a thing that works.
    const moveItem = await vi.waitFor(() => {
      const el = [...document.querySelectorAll('.row-menu [role="menuitem"]')].find((b) => b.textContent === 'Move to…')
      expect(el, 'a note offers a move').not.toBeUndefined()
      return el as HTMLButtonElement
    }, { timeout: 2000 })
    moveItem.click()

    const field = (await vi.waitFor(() => {
      const el = document.querySelector('[data-testid="new-folder-name"]') as HTMLInputElement | null
      expect(el, 'the dialog offers a name for a folder that is not there yet').not.toBeNull()
      return el!
    }, { timeout: 2000 })) as HTMLInputElement

    // A blank name is not a destination: nothing is sent, and the reason is on screen.
    ;(document.querySelector('[data-testid="create-and-move"]') as HTMLButtonElement).click()
    await new Promise((resolve) => setTimeout(resolve, 120))
    expect(moveCalls.current, 'a refusal sends nothing').toEqual([])
    expect(document.body.textContent ?? '').toContain('Name the folder')

    // Re-query rather than reuse: the refusal message rendered beside the field, and a stale node would type into
    // nothing.
    const typed = document.querySelector('[data-testid="new-folder-name"]') as HTMLInputElement
    typed.value = 'ideas'
    typed.dispatchEvent(new Event('input', { bubbles: true }))
    ;(document.querySelector('[data-testid="create-and-move"]') as HTMLButtonElement).click()

    await vi.waitFor(() => expect(moveCalls.current.map((c) => c.cmd)).toEqual(['create_folder', 'move_entry']), { timeout: 2000 })
    expect(moveCalls.current[0]).toMatchObject({ cmd: 'create_folder', rel: 'ideas' })
    expect(moveCalls.current[1]).toMatchObject({ cmd: 'move_entry', from: 'inbox.md', to: 'ideas/inbox.md' })
    await vi.waitFor(() => expect(document.querySelector('[data-testid="new-folder-name"]')).toBeNull(), { timeout: 2000 })

    expect(errors).toEqual([])
  })

  it('keeps the dialog open when the folder cannot be made', async () => {
    // A plan that half-succeeded is the worst outcome: the note is where it was, and the window said nothing. So a
    // failed create leaves the dialog up with the sentence, and sends no move after it.
    scenario.current = 'vault'
    moveSays.createFails = true
    const { errors } = await start()

    let row!: HTMLElement
    await vi.waitFor(() => {
      row = document.querySelector('.tree button[data-kind="Note"]') as HTMLElement
      expect(row).not.toBeNull()
    }, { timeout: 2000 })
    row.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, clientX: 40, clientY: 90 }))
    const item = await vi.waitFor(() => {
      const el = [...document.querySelectorAll('.row-menu [role="menuitem"]')].find((b) => b.textContent === 'Move to…')
      expect(el).not.toBeUndefined()
      return el as HTMLButtonElement
    }, { timeout: 2000 })
    item.click()

    const field = (await vi.waitFor(() => {
      const el = document.querySelector('[data-testid="new-folder-name"]') as HTMLInputElement | null
      expect(el).not.toBeNull()
      return el!
    }, { timeout: 2000 })) as HTMLInputElement
    field.value = 'ideas'
    field.dispatchEvent(new Event('input', { bubbles: true }))
    ;(document.querySelector('[data-testid="create-and-move"]') as HTMLButtonElement).click()

    await vi.waitFor(() => expect(document.body.textContent ?? '').toContain('permission denied'), { timeout: 2000 })
    expect(document.querySelector('[data-testid="new-folder-name"]'), 'the dialog stays so the name can be fixed').not.toBeNull()
    expect(moveCalls.current.map((c) => c.cmd), 'no move after a failed create').toEqual(['create_folder'])

    moveSays.createFails = false
    expect(errors).toEqual([])
  })

  describe('the settings panel', () => {
    /** Open the panel the way a person does from the header. */
    async function openPanel() {
      const button = await vi.waitFor(() => {
        const el = document.querySelector('button[aria-label="Settings"]') as HTMLButtonElement | null
        expect(el, 'the window says where its settings are').not.toBeNull()
        return el!
      }, { timeout: 2000 })
      button.click()
      await vi.waitFor(() => expect(document.querySelector('[data-testid="settings"]')).not.toBeNull(), { timeout: 2000 })
    }

    const modeRadio = (mode: string) => document.querySelector(`[data-testid="mode-${mode}"]`) as HTMLInputElement | null
    const typed = (mode: string) => {
      const radio = modeRadio(mode)!
      radio.checked = true
      radio.dispatchEvent(new Event('change', { bubbles: true }))
    }

    it('offers all three ways to match, and the remembered choices are the ones ticked', async () => {
      scenario.current = 'vault'
      // Written before the window mounts: these are read once, at startup, which is the point of remembering them.
      localStorage.setItem(MODE_KEY, 'keyword')
      localStorage.setItem(PREFIX_KEY, '1')
      const { errors } = await start()

      await openPanel()
      for (const mode of ['keyword', 'hybrid', 'semantic']) {
        expect(modeRadio(mode), `it offers ${mode}`).not.toBeNull()
      }
      expect(modeRadio('keyword')!.checked, 'what was stored is what is ticked').toBe(true)
      expect(modeRadio('hybrid')!.checked).toBe(false)
      // The prefix choice lives in the panel too, and shows its remembered value rather than a default.
      expect((document.querySelector('[data-testid="settings-prefix"]') as HTMLInputElement).checked).toBe(true)
      expect(errors).toEqual([])
    })

    it('remembers the mode it was told and asks note-processor that way', async () => {
      scenario.current = 'vault'
      const first = await start()
      const errors = first.errors

      const box = (await vi.waitFor(() => {
        const el = document.querySelector('input[type="search"]') as HTMLInputElement | null
        expect(el).not.toBeNull()
        return el!
      }, { timeout: 2000 })) as HTMLInputElement
      box.value = 'kumquat'
      box.dispatchEvent(new Event('input', { bubbles: true }))
      await vi.waitFor(() => expect(searchCalls.length).toBeGreaterThan(0), { timeout: 2000 })
      // A window with no stored choice asks for both engines: the backend's own default, not our opinion.
      expect(searchCalls.at(-1)!.mode).toBe('hybrid')

      await openPanel()
      typed('keyword')

      // Changing the engine re-runs the query that is already in the box.
      await vi.waitFor(() => expect(searchCalls.at(-1)!.mode).toBe('keyword'), { timeout: 2000 })
      expect(localStorage.getItem(MODE_KEY), 'and it is kept for the next window').toBe('keyword')

      // The panel and the checkbox beside the box write one key, so they cannot disagree after a restart.
      unmount(first.app)
      commands.length = 0
      searchCalls.length = 0
      scenario.current = 'vault'
      const second = await start()
      const box2 = (await vi.waitFor(() => {
        const el = document.querySelector('input[type="search"]') as HTMLInputElement | null
        expect(el).not.toBeNull()
        return el!
      }, { timeout: 2000 })) as HTMLInputElement
      box2.value = 'kumquat'
      box2.dispatchEvent(new Event('input', { bubbles: true }))
      await vi.waitFor(() => expect(searchCalls.length).toBeGreaterThan(0), { timeout: 2000 })
      expect(searchCalls.at(-1)!.mode, 'the second window starts where the first left off').toBe('keyword')
      expect(second.errors).toEqual([])
      expect(errors).toEqual([])
    })

    it('warns about meanings-only where note-processor says there are no vectors', async () => {
      scenario.current = 'vault'
      const { errors } = await start()

      // The fixture's search answers `vectors: false`, which is the fact behind the warning. Nothing here invents it.
      // "grand" is the query this fixture answers with an empty list, so the sentence under test is the one about
      // there being nothing to find.
      const box = (await vi.waitFor(() => {
        const el = document.querySelector('input[type="search"]') as HTMLInputElement | null
        expect(el).not.toBeNull()
        return el!
      }, { timeout: 2000 })) as HTMLInputElement
      box.value = 'grand'
      box.dispatchEvent(new Event('input', { bubbles: true }))
      await vi.waitFor(() => expect(searchCalls.length).toBeGreaterThan(0), { timeout: 2000 })

      await openPanel()
      const notice = document.querySelector('[data-testid="settings-notice"]')
      expect(notice, 'the panel says what a meanings-only search would find here').not.toBeNull()
      expect(notice!.textContent!.toLowerCase()).toContain('no vectors')

      // And an empty meanings-only answer does not claim nothing matched: note-processor#11 leaves it unable to say.
      typed('semantic')
      await vi.waitFor(() => expect(searchCalls.at(-1)!.mode).toBe('semantic'), { timeout: 2000 })
      const none = await vi.waitFor(() => {
        const el = document.querySelector('.results .none') as HTMLElement | null
        expect(el, 'an empty answer says something').not.toBeNull()
        return el!
      }, { timeout: 2000 })
      expect(none.textContent!.toLowerCase(), 'the sentence blames the missing vectors, not the notes').toContain('no vectors')
      expect(errors).toEqual([])
    })

    it('answers the embeddings question and shows what note-processor wrote', async () => {
      scenario.current = 'vault'
      incompleteSettings.current = true
      const { errors } = await start()

      await openPanel()
      ;(document.querySelector('[data-testid="embeddings-allow"]') as HTMLButtonElement).click()

      await vi.waitFor(() => expect(commands).toContain('set_consent'), { timeout: 2000 })
      const panel = await vi.waitFor(() => {
        const el = document.querySelector('[data-testid="settings-embeddings"]') as HTMLElement | null
        // The panel quotes np's own record rather than a hope about what the click did.
        expect(el!.textContent).toContain('embeddings.provider')
        return el!
      }, { timeout: 2000 })
      // …and says which key Jott itself wrote, beside it.
      expect(panel.textContent).toContain('models.auto_fetch')
      expect(errors).toEqual([])
    })

    it('closes on Escape before it closes the note, and opens on Ctrl+comma', async () => {
      scenario.current = 'vault'
      const { errors } = await start()

      // Open a note first: Escape with the panel up must dismiss the panel, not throw the note away behind it.
      ;(document.querySelector('.tree button[data-kind="Note"]') as HTMLElement).dispatchEvent(new MouseEvent('click', { bubbles: true }))
      await vi.waitFor(() => expect(document.querySelector('.cm > .cm-editor')).not.toBeNull(), { timeout: 2000 })

      document.body.dispatchEvent(new KeyboardEvent('keydown', { key: ',', ctrlKey: true, bubbles: true }))
      await vi.waitFor(() => expect(document.querySelector('[data-testid="settings"]')).not.toBeNull(), { timeout: 2000 })

      document.body.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
      await vi.waitFor(() => expect(document.querySelector('[data-testid="settings"]')).toBeNull(), { timeout: 2000 })
      expect(document.querySelector('.cm > .cm-editor'), 'the note is still open behind it').not.toBeNull()

      expect(document.querySelector('[data-testid="close-settings"]'), 'the panel is gone').toBeNull()
      // A second press now has nothing above the note to dismiss, so it closes the note — one thing per press.
      document.body.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
      await vi.waitFor(() => expect(document.querySelector('.cm > .cm-editor')).toBeNull(), { timeout: 2000 })
      expect(errors).toEqual([])
    })
  })

  it('dismisses one overlay per Escape press, even the ones that used to listen for the key themselves', async () => {
    // The bug: About and the drawer each had their own `window` Escape listener while `escapeAction`, which did not
    // know they existed, answered "close the note". One press closed About *and* threw the note away behind it.
    scenario.current = 'vault'
    const { errors } = await start()

    ;(document.querySelector('.tree button[data-kind="Note"]') as HTMLElement).dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await vi.waitFor(() => expect(document.querySelector('.cm > .cm-editor')).not.toBeNull(), { timeout: 2000 })

    ;(document.querySelector('button.about') as HTMLButtonElement).click()
    const about = await vi.waitFor(() => {
      const el = document.querySelector('.panel[role="dialog"]') as HTMLElement | null
      expect(el, 'About opened').not.toBeNull()
      return el!
    }, { timeout: 2000 })
    expect(about).not.toBeNull()

    document.body.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    await vi.waitFor(() => expect(document.querySelector('.panel[role="dialog"]')).toBeNull(), { timeout: 2000 })
    expect(document.querySelector('.cm > .cm-editor'), 'the note behind it is still open').not.toBeNull()

    // And a dialog: Escape cancels what was asked, rather than answering it by closing the note.
    const row = await vi.waitFor(() => {
      const el = document.querySelector('.tree button[data-kind="Note"]') as HTMLElement | null
      expect(el).not.toBeNull()
      return el!
    }, { timeout: 2000 })
    row.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, clientX: 30, clientY: 60 }))
    const item = await vi.waitFor(() => {
      const el = [...document.querySelectorAll('.row-menu [role="menuitem"]')].find((b) => b.textContent === 'Move to…')
      expect(el).not.toBeUndefined()
      return el as HTMLButtonElement
    }, { timeout: 2000 })
    item.click()
    await vi.waitFor(() => expect(document.querySelector('[data-testid="new-folder-name"]')).not.toBeNull(), { timeout: 2000 })

    document.body.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    await vi.waitFor(() => expect(document.querySelector('[data-testid="new-folder-name"]')).toBeNull(), { timeout: 2000 })
    expect(document.querySelector('.cm > .cm-editor'), 'cancelling a dialog is not closing a note').not.toBeNull()
    expect(moveCalls.current, 'and it sent nothing').toEqual([])

    // Now nothing covers the screen, so the next press is the one that closes the note.
    document.body.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    await vi.waitFor(() => expect(document.querySelector('.cm > .cm-editor')).toBeNull(), { timeout: 2000 })
    expect(errors).toEqual([])
  })


  it('mirrors what was typed when the note is closed inside the mirror window', async () => {
    // §18.13: half a second of typing is the most a crash may take — so a note closed 200 ms after the last
    // keystroke still has to leave its text with core. The editor's cleanup used to cancel the pending mirror, which
    // made the close button and Escape two new ways of losing unsaved work, while the release notes promised the
    // opposite ("an unsaved buffer keeps its marker if you come back").
    scenario.current = 'vault'
    const { errors } = await start()

    ;(document.querySelector('.tree button[data-kind="Note"]') as HTMLElement).dispatchEvent(new MouseEvent('click', { bubbles: true }))
    const view = await vi.waitFor(() => {
      const dom = document.querySelector('.cm > .cm-editor') as HTMLElement | null
      expect(dom).not.toBeNull()
      const editor = EditorView.findFromDOM(dom!)
      expect(editor, 'the harness can reach the buffer').not.toBeNull()
      return editor!
    }, { timeout: 2000 })

    view.dispatch({ changes: { from: 0, insert: 'a line typed and not yet saved\n' } })
    // Deliberately no waiting: the mirror tick is 500 ms away, and this test has to finish inside it to prove that
    // teardown flushed rather than a timer having had time to fire.
    ;(document.querySelector('[data-testid="close-note"]') as HTMLButtonElement).click()

    await vi.waitFor(() => expect(document.querySelector('.cm > .cm-editor')).toBeNull(), { timeout: 2000 })
    expect(commands, 'the buffer reached core before the editor was destroyed').toContain('mirror_buffer')
    // Closing still writes nothing to the note itself: the mirror is core's memory, not a save.
    expect(commands).not.toContain('save_note')
    expect(errors).toEqual([])
  })


  it('forgets the search answer and the open note when the folder changes', async () => {
    // Two claims carried across a folder switch: the settings panel warned "this folder has no vectors yet" from an
    // answer note-processor gave about the *previous* folder, and the editor kept showing a `rel` that means nothing
    // in the new one — with a dirty buffer behind it, one autosave away from being written into someone else's file.
    scenario.current = 'vault'
    extraRecent.current = 'D:\\other'
    const { errors } = await start()

    // The note first: a search replaces the tree with results, so this is also the order a person reaches the
    // situation in — something open, something searched, then yesterday's folder from the recents list.
    let noteRow!: HTMLElement
    await vi.waitFor(() => {
      const el = document.querySelector('.tree button[data-kind="Note"]') as HTMLElement | null
      expect(el, 'the fixture has a note to open').not.toBeNull()
      noteRow = el!
    }, { timeout: 2000 })
    noteRow.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await vi.waitFor(() => expect(document.querySelector('.cm > .cm-editor')).not.toBeNull(), { timeout: 2000 })

    const box = (await vi.waitFor(() => {
      const el = document.querySelector('input[type="search"]') as HTMLInputElement | null
      expect(el).not.toBeNull()
      return el!
    }, { timeout: 2000 })) as HTMLInputElement
    box.value = 'kumquat'
    box.dispatchEvent(new Event('input', { bubbles: true }))
    await vi.waitFor(() => expect(document.querySelector('.results')).not.toBeNull(), { timeout: 2000 })

    ;(document.querySelector('button[aria-label="Settings"]') as HTMLButtonElement).click()
    await vi.waitFor(() => expect(document.querySelector('[data-testid="settings-notice"]')).not.toBeNull(), { timeout: 2000 })
    ;(document.querySelector('[data-testid="close-settings"]') as HTMLButtonElement).click()

    const menu = document.querySelector('select.recents') as HTMLSelectElement
    menu.value = 'D:\\other'
    menu.dispatchEvent(new Event('change', { bubbles: true }))

    await vi.waitFor(() => expect(document.querySelector('.results')).toBeNull(), { timeout: 2000 })
    expect(document.querySelector('.cm > .cm-editor'), 'the other folder does not have this note open').toBeNull()

    // And the panel has no fact left to speak from: it warns about vectors only where np said there were none, in
    // *this* folder. Before this fix it repeated the old folder's answer as a property of the new one.
    ;(document.querySelector('button[aria-label="Settings"]') as HTMLButtonElement).click()
    await vi.waitFor(() => expect(document.querySelector('[data-testid="settings"]')).not.toBeNull(), { timeout: 2000 })
    expect(document.querySelector('[data-testid="settings-notice"]'), 'no answer, no warning').toBeNull()

    extraRecent.current = null
    expect(errors).toEqual([])
  })


  it('does not offer rename and delete for note-processor’s own catalogue', async () => {
    // The row menu already refuses: `actionsFor` returns nothing for a generated file. Making the catalogue openable
    // meant its row became selectable, which quietly enabled two toolbar buttons whose commands core will refuse. Two
    // doors in one window saying opposite things about whether a file is touchable is the thing to fix — the refusal
    // belongs in front of the click, not only behind it.
    scenario.current = 'vault'
    const { errors } = await start()

    let catalogue!: HTMLElement
    await vi.waitFor(() => {
      const el = document.querySelector('.tree button[data-kind="Generated"]') as HTMLElement | null
      expect(el, 'the fixture has np’s catalogue').not.toBeNull()
      catalogue = el!
    }, { timeout: 2000 })
    const toolbar = (label: string) =>
      [...document.querySelectorAll('button')].find((b) => (b.textContent ?? '').trim() === label) as HTMLButtonElement

    // Nothing selected: both are down.
    expect(toolbar('rename').disabled, 'nothing is selected').toBe(true)
    expect(toolbar('delete').disabled).toBe(true)

    catalogue.click()
    await vi.waitFor(() => expect(document.querySelector('[data-testid="readonly-banner"]') ?? document.querySelector('.cm-editor')).not.toBeNull(), { timeout: 2000 })
    expect(toolbar('rename').disabled, 'selecting np’s file must not enable rename').toBe(true)
    expect(toolbar('delete').disabled).toBe(true)

    // And a note of ours still enables both, so this is not the buttons going quiet.
    const note = document.querySelector('.tree button[data-kind="Note"]') as HTMLElement
    note.click()
    await vi.waitFor(() => expect(toolbar('rename').disabled).toBe(false), { timeout: 2000 })
    expect(toolbar('delete').disabled).toBe(false)
    expect(errors).toEqual([])
  })


  it('says which folder it made when the move after it failed', async () => {
    // "Create then move" is two commands, and the first can succeed while the second is refused. Leaving np's
    // sentence alone in the status line would hide the fact that the vault now has a folder nobody asked for on
    // their own — so the dialog says what it did before it says what went wrong.
    scenario.current = 'vault'
    moveSays.moveFails = true
    const { errors } = await start()

    let row!: HTMLElement
    await vi.waitFor(() => {
      const el = document.querySelector('.tree button[data-kind="Note"]') as HTMLElement | null
      expect(el).not.toBeNull()
      row = el!
    }, { timeout: 2000 })
    row.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, clientX: 40, clientY: 90 }))
    const item = await vi.waitFor(() => {
      const el = [...document.querySelectorAll('.row-menu [role="menuitem"]')].find((b) => b.textContent === 'Move to…')
      expect(el).not.toBeUndefined()
      return el as HTMLButtonElement
    }, { timeout: 2000 })
    item.click()

    const field = (await vi.waitFor(() => {
      const el = document.querySelector('[data-testid="new-folder-name"]') as HTMLInputElement | null
      expect(el).not.toBeNull()
      return el!
    }, { timeout: 2000 })) as HTMLInputElement
    field.value = 'ideas'
    field.dispatchEvent(new Event('input', { bubbles: true }))
    ;(document.querySelector('[data-testid="create-and-move"]') as HTMLButtonElement).click()

    await vi.waitFor(() => expect(moveCalls.current.map((c) => c.cmd)).toEqual(['create_folder', 'move_entry']), { timeout: 2000 })
    await vi.waitFor(() => {
      const said = document.querySelector('.ask [role="alert"]')?.textContent ?? ''
      expect(said, 'the dialog owns up to the folder it created').toContain('Created ideas')
    }, { timeout: 2000 })
    expect(document.querySelector('[data-testid="new-folder-name"]'), 'and leaves the dialog up').not.toBeNull()

    moveSays.moveFails = false
    expect(errors).toEqual([])
  })


  it('says what is missing when a note is asked for with no folder open', async () => {
    // Reported from Windows: first run, no folder open, click "+ folder", type a name, press Enter — nothing at all.
    // The window did have the sentence ("no vault is open"); it drew it in a branch that needs an open folder, so
    // nobody ever saw it. A refusal a person cannot read is the same as no refusal.
    scenario.current = 'empty'
    const { errors } = await start()
    await vi.waitFor(() => expect(document.querySelector('button[title^="New note"]')).not.toBeNull(), { timeout: 2000 })

    ;(document.querySelector('button[title^="New note"]') as HTMLButtonElement).click()
    const blocked = await vi.waitFor(() => {
      const el = document.querySelector('[data-testid="create-blocked"]')
      expect(el, 'the dialog says why this cannot happen yet. FORM=' + (document.querySelector('.ask')?.outerHTML ?? 'NO FORM')).not.toBeNull()
      return el!
    }, { timeout: 2000 })
    expect((blocked.textContent ?? '').toLowerCase()).toContain('no folder is open')

    const field = (await vi.waitFor(() => {
      const el = document.querySelector('#rel') as HTMLInputElement | null
      expect(el).not.toBeNull()
      return el!
    }, { timeout: 2000 })) as HTMLInputElement
    field.value = 'hello.md'
    field.dispatchEvent(new Event('input', { bubbles: true }))
    ;(document.querySelector('.ask button[type="submit"]') as HTMLButtonElement).click()
    await new Promise((resolve) => setTimeout(resolve, 40))

    expect(moveCalls.current, 'a blocked intention sends no command').toEqual([])
    const kept = document.querySelector('#rel') as HTMLInputElement
    expect(kept, 'the dialog stays open with the name typed').not.toBeNull()
    expect(kept.value).toBe('hello.md')
    expect(errors).toEqual([])
  })

  it('finishes the note it was asked for once a folder has been picked', async () => {
    // The other half of the report: "maybe open the folder as a workspace if nothing is opened yet." The dialog keeps
    // what was typed, so choosing a folder finishes the intention rather than starting it over.
    scenario.current = 'empty'
    const { errors } = await start()
    await vi.waitFor(() => expect(document.querySelector('button[title^="New note"]')).not.toBeNull(), { timeout: 2000 })
    ;(document.querySelector('button[title^="New note"]') as HTMLButtonElement).click()
    await vi.waitFor(() => expect(document.querySelector('[data-testid="create-blocked"]')).not.toBeNull(), { timeout: 2000 })

    const field = document.querySelector('#rel') as HTMLInputElement
    field.value = 'hello.md'
    field.dispatchEvent(new Event('input', { bubbles: true }))

    picked.current = 'C:\\notes'
    ;(document.querySelector('[data-testid="dialog-open-folder"]') as HTMLButtonElement).click()
    await vi.waitFor(() => expect(commands).toContain('open_vault'), { timeout: 2000 })

    const submit = (await vi.waitFor(() => {
      const el = document.querySelector('.ask button[type="submit"]')
      expect(el, 'the dialog is still open after picking').not.toBeNull()
      return el!
    }, { timeout: 2000 })) as HTMLButtonElement
    submit.click()

    await vi.waitFor(() => expect(moveCalls.current.map((c) => c.cmd)).toEqual(['create_note']), { timeout: 2000 })
    expect(moveCalls.current[0]).toMatchObject({ cmd: 'create_note', rel: 'hello.md' })
    expect(errors).toEqual([])
  })

  it('shows a refusal even when no folder is open to make room for one', async () => {
    // The class of bug behind "nothing happens": the window had the sentence and drew it in a branch that needs an
    // open folder. Whatever refuses, before or after a folder is chosen, the answer belongs above the tools.
    scenario.current = 'empty'
    const { errors } = await start()
    await vi.waitFor(() => expect(document.querySelector('button[title^="New note"]')).not.toBeNull(), { timeout: 2000 })
    ;(document.querySelector('button[title^="New note"]') as HTMLButtonElement).click()
    await vi.waitFor(() => expect(document.querySelector('[data-testid="create-blocked"]')).not.toBeNull(), { timeout: 2000 })

    picked.current = 'C:\\gone'
    openVaultSays.fails = true
    ;(document.querySelector('[data-testid="dialog-open-folder"]') as HTMLButtonElement).click()

    const said = await vi.waitFor(() => {
      const el = document.querySelector('[data-testid="global-problem"]')
      expect(el, 'the refusal is on screen').not.toBeNull()
      return el!
    }, { timeout: 2000 })
    expect(said.textContent).toContain('C:\\gone is not a folder any more')
    expect(errors).toEqual([])
  })

  it('asks for the tree again after making a folder', async () => {
    // Half of "+ folder → nothing happens" was this: `Ok(())` crosses the wire as `null`, and `null` is the window's
    // own marker for "the command failed", so every successful create skipped the refresh that would have shown it.
    scenario.current = 'vault'
    const { errors } = await start()
    await vi.waitFor(() => expect(document.querySelector('button[title^="New folder"]')).not.toBeNull(), { timeout: 2000 })

    ;(document.querySelector('button[title^="New folder"]') as HTMLButtonElement).click()
    const field = (await vi.waitFor(() => {
      const el = document.querySelector('#rel') as HTMLInputElement | null
      expect(el).not.toBeNull()
      return el!
    }, { timeout: 2000 })) as HTMLInputElement
    field.value = 'ideas'
    field.dispatchEvent(new Event('input', { bubbles: true }))

    commands.length = 0
    ;(document.querySelector('.ask button[type="submit"]') as HTMLButtonElement).click()

    await vi.waitFor(() => expect(moveCalls.current.map((c) => c.cmd)).toEqual(['create_folder']), { timeout: 2000 })
    await vi.waitFor(() => expect(commands, 'the tree is asked for again').toContain('list_vault'), { timeout: 2000 })
    expect(document.querySelector('.ask'), 'and the dialog closes on a folder that was made').toBeNull()
    expect(errors).toEqual([])
  })

  it('closes the move dialog when the move succeeded', async () => {
    // Same wire shape, same trap: a successful `move_entry` answers null, and the window read that as a failure —
    // leaving the dialog open over a tree that had already changed.
    scenario.current = 'vault'
    const { errors } = await start()

    let row!: HTMLElement
    await vi.waitFor(() => {
      row = document.querySelector('.tree button[data-kind="Note"]') as HTMLElement
      expect(row, 'the fixture has a note to move').not.toBeNull()
    }, { timeout: 2000 })
    row.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, clientX: 40, clientY: 90 }))
    const moveItem = await vi.waitFor(() => {
      const el = [...document.querySelectorAll('.row-menu [role="menuitem"]')].find((b) => b.textContent === 'Move to…')
      expect(el, 'a note offers a move').not.toBeUndefined()
      return el as HTMLButtonElement
    }, { timeout: 2000 })
    moveItem.click()

    const target = (await vi.waitFor(() => {
      const el = document.querySelector('#rel') as HTMLSelectElement | null
      expect(el).not.toBeNull()
      return el!
    }, { timeout: 2000 })) as HTMLSelectElement
    const into = [...target.options].map((o) => o.value).filter((v) => v !== '')
    if (into.length === 0) {
      // The vault fixture has no folders, so the destination has to be named instead of chosen.
      const name = document.querySelector('[data-testid="new-folder-name"]') as HTMLInputElement
      name.value = 'archive'
      name.dispatchEvent(new Event('input', { bubbles: true }))
      ;(document.querySelector('[data-testid="create-and-move"]') as HTMLButtonElement).click()
    } else {
      target.value = into[0]
      target.dispatchEvent(new Event('change', { bubbles: true }))
      ;(document.querySelector('.ask button[type="submit"]') as HTMLButtonElement).click()
    }

    await vi.waitFor(() => expect(moveCalls.current.map((c) => c.cmd)).toContain('move_entry'), { timeout: 2000 })
    await vi.waitFor(() => expect(document.querySelector('.ask'), 'a move that worked closes its dialog').toBeNull(), { timeout: 2000 })
    expect(errors).toEqual([])
  })

})
