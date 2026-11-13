// @vitest-environment jsdom
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
            ? { recent: ['C:\\notes'], consent: 'undecided' }
            : { recent: [], consent: 'undecided' },
        )
      case 'open_vault':
        return Promise.resolve(vault)
      case 'app_info':
        return Promise.resolve({
          jottVersion: '0.0.5',
          np: { found: true, version: '0.3.1', source: 'bundled', path: 'C:\\Program Files\\Jott\\np\\np.exe' },
          dataDir: 'C:\\Users\\you\\AppData\\Roaming\\jott',
          logPath: 'C:\\Users\\you\\AppData\\Roaming\\jott\\logs\\jott.jsonl',
          repoUrl: 'https://github.com/Ex-Bot-01/jott',
          releasesUrl: 'https://github.com/Ex-Bot-01/jott/releases',
          docsUrl: 'https://github.com/Ex-Bot-01/jott/blob/main/docs/testing-on-windows.md',
          diagnostics: 'Jott 0.0.5 · https://github.com/Ex-Bot-01/jott\nnp 0.3.1 from bundled (C:\\Program Files\\Jott\\np\\np.exe)',
        })
      case 'search_vault':
        return Promise.resolve({
          hits: [
            {
              path: 'inbox.md',
              snippet: '…a kumquat tree in the corner…',
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
        })
      case 'read_note':
        return Promise.resolve({
          rel: 'inbox.md',
          text: '# Inbox\n\nA kumquat tree in the corner.\n',
          hash: 'a'.repeat(64),
          diskHash: 'a'.repeat(64),
          diskMtimeMs: 1_700_000_000_000,
          mirrored: null,
        })
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
            npVersion: '0.3.1',
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

    await vi.waitFor(() => expect(document.body.textContent ?? '').toContain('0.3.1'))
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
})
