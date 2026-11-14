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
            logPath: 'C:\\Users\\chris\\AppData\\Roaming\\jott\\logs\\jott.jsonl',
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
    handlers.get('ingest://event')!({ indexed: { elapsed_ms: 412 } })

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

    handlers.get('ingest://event')!({ indexed: { elapsed_ms: 40 } })
    handlers.get('ingest://event')!({ working: { phase: 'embedding', done: 1, total: 3 } })
    handlers.get('ingest://event')!({ embedded: { elapsed_ms: 900 } })

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
