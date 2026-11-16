//! `invoke`, faked. Aliased over `@tauri-apps/api/core` by `vite.uiharness.config.ts`, so the production import path is
//! untouched and every return value below is checked against the same types the real commands are typed with — a field
//! renamed on one side fails `npm run check` here rather than showing up as `undefined` in somebody's window.

import type { DiagnosticsSnapshot, TraceEvent } from '../lib/diagnostics'
import type {
  AboutInfo,
  ConsentHelp,
  ConsentOffer,
  ConsentOutcome,
  EmbedReport,
  ModelStatus,
  NodeEntry,
  NoteContent,
  RegistryInfo,
  RenameOutcome,
  ScanReport,
  SearchHit,
  SearchResults,
  VaultListing,
  WriteResult,
} from '../lib/ipc'
import { bus, fixtureVault, listing, scenarioFrom, vault, type Scenario } from './harnessState'

const scenario: Scenario = scenarioFrom(typeof location === 'undefined' ? '' : location.search)
const started = Date.now()

function later<T>(value: T, ms = 40): Promise<T> {
  return new Promise((resolve) => setTimeout(() => resolve(value), ms))
}

/** np's own arithmetic, kept honest: `score` is reciprocal-rank fusion of the two rankings, not a similarity. */
function search(query: string, prefix: boolean, mode: string): SearchResults {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean)
  const scored: { file: (typeof vault.files)[number]; kwRank: number; semRank: number | null; exact: boolean }[] = []
  for (const file of vault.files) {
    if (file.kind !== 'Note') continue
    const hay = file.text.toLowerCase()
    const hitsWord = words.every((w) => (prefix ? hay.includes(w) : new RegExp(`\\b${w}\\b`).test(hay)))
    if (!hitsWord) continue
    const exact = words.every((w) => new RegExp(`\\b${w}\\b`).test(hay))
    scored.push({ file, kwRank: scored.length + 1, semRank: mode === 'semantic' ? scored.length + 1 : null, exact })
  }
  const hits: SearchHit[] = scored.map((s) => ({
    path: s.file.rel,
    snippet: s.file.text.split('\n').find((l) => l.trim() && !l.startsWith('#'))?.trim() ?? null,
    kind: 'note',
    score: 1 / (60 + (s.kwRank ?? 99)) + (s.semRank === null ? 0 : 1 / (60 + s.semRank)),
    bm25: -0.51 + 0.1 * (s.kwRank ?? 0),
    cosine: s.semRank === null ? null : 0.82 - 0.03 * s.semRank,
    matchedExactly: mode === 'semantic' && !s.exact ? null : s.exact,
  }))
  const widened = prefix ? words.filter((w) => !hits.some((h) => h.matchedExactly === true)) : []
  return {
    hits,
    mode: mode === 'semantic' ? 'hybrid' : 'keyword',
    vectors: mode === 'semantic',
    offline: false,
    reason: null,
    indexed: vault.open,
    keywordUsed: true,
    semanticUsed: mode === 'semantic',
    notice: null,
    prefixUsed: prefix && widened.length > 0,
    prefixTerms: widened,
  }
}

function scanReport(): ScanReport {
  return {
    added: 1,
    modified: 0,
    moved: 0,
    removed: 0,
    unchanged: vault.files.filter((f) => f.kind === 'Note').length - 1,
    skipped: [],
    notes: vault.files.filter((f) => f.kind === 'Note').length,
    unresolved_links: 0,
    unresolved_targets: [],
    index_files: ['INDEX.md'],
    elapsed_ms: 612,
    upgrade_orphans: null,
    merged_notes: null,
    recorded_upgrade_orphans: null,
    recorded_merged_notes: null,
    unindexable: null,
  }
}

function embedReport(): EmbedReport {
  return {
    model: 'mxbai-embed-large',
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
    verification: 'ok',
    offline: false,
    fetched: null,
  }
}

function diagnostics(): DiagnosticsSnapshot {
  const row = (method: string, outcome: string, elapsedMs: number): TraceEvent => ({
    atMs: Date.now(),
    kind: 'request',
    method,
    outcome,
    elapsedMs,
    detail: null,
  })
  return {
    runtime: {
      npVersion: '0.5.1',
      npPath: '/usr/lib/Jott/np/np',
      protocol: 1,
      writer: 'up',
      reader: 'idle',
      dataDir: '/home/example/.local/share/note-processor',
      stateDir: '/home/example/.local/share/jott',
      stateFallback: false,
      logPath: '/home/example/.local/share/jott/logs/jott.jsonl',
      logBytes: 18_402,
      consent: 'granted',
      autoFetch: true,
    },
    recent: [row('search', 'ok', 38), row('scan', 'ok', 612), row('embed', 'ok', 9021)],
    queue: { waiting: 0, heldForConsent: 0, backoff: null, retryInMs: null },
    vault: {
      notes: vault.files.filter((f) => f.kind === 'Note').length,
      embedded: vault.files.filter((f) => f.state === 'Embedded').length,
      indexed: vault.files.filter((f) => f.state === 'Indexed').length,
      waiting: vault.files.filter((f) => f.state === 'Waiting').length,
      refused: 0,
      warnings: vault.warnings.length,
    },
  }
}

/** Bytes for the harness's pretend fetch. Advanced a measured ~24 MiB per reading — np 0.5.1 sent a progress line
    about every 1.1 s at roughly this rate on a fast link — so `?s=indexing` shows what the banner looks like while a
    real download is running, without anybody downloading 295 MB to look at it. */
let harnessFetchBytes = 0

function modelStatus(): ModelStatus {
  if (scenario === 'np-missing') {
    return {
      provider: null,
      downloading: null,
      modelId: null,
      present: null,
      bytesTotal: null,
      configUnreadable: false,
      modelsDirError: null,
    }
  }
  if (scenario === 'indexing') {
    harnessFetchBytes = Math.min(harnessFetchBytes + 25_165_824, 309_855_456)
    return {
      provider: 'local-model',
      downloading: { modelId: 'embeddinggemma-2', bytesDone: harnessFetchBytes, bytesTotal: 309_855_456 },
      modelId: 'embeddinggemma-2',
      present: false,
      bytesTotal: 309_855_456,
      configUnreadable: false,
      modelsDirError: null,
    }
  }
  if (scenario === 'consent') {
    return {
      provider: 'none',
      downloading: null,
      modelId: 'mxbai-embed-large',
      present: false,
      bytesTotal: 309_855_456,
      configUnreadable: false,
      modelsDirError: null,
    }
  }
  return {
    provider: 'local-model',
    downloading: null,
    modelId: 'mxbai-embed-large',
    present: true,
    bytesTotal: 309_855_456,
    configUnreadable: false,
    modelsDirError: null,
  }
}

function opened(): VaultListing {
  if (!vault.files.length) vault.files = fixtureVault()
  vault.open = true
  return listing()
}

/** Commands the window may call. An unknown name throws the way a missing command would, rather than answering null:
 *  a harness that quietly succeeds where core refuses is how a UI bug hides until somebody's real folder. */
export async function invoke<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  const rel = String(args.rel ?? '')
  const answer = ((): unknown => {
    switch (cmd) {
      case 'registry_info': {
        const info: RegistryInfo = {
          recent: vault.open ? [vault.path] : scenario === 'empty' ? [] : ['/home/example/Notes'],
          consent: scenario === 'consent' ? 'undecided' : 'granted',
        }
        return info
      }
      case 'app_info': {
        const info: AboutInfo = {
          jottVersion: '0.0.19',
          np: { found: true, version: '0.5.1', source: 'bundled', path: '/usr/lib/Jott/np/np' },
          dataDir: '/home/example/.local/share/jott',
          logPath: '/home/example/.local/share/jott/logs/jott.jsonl',
          repoUrl: 'https://github.com/Ex-Bot-01/jott',
          releasesUrl: 'https://github.com/Ex-Bot-01/jott/releases',
          docsUrl: 'https://github.com/Ex-Bot-01/jott/blob/main/docs/testing-on-windows.md',
          diagnostics: 'jott 0.0.19 · np 0.5.1 · protocol 1',
        }
        return info
      }
      case 'list_vault':
        if (!vault.open && scenario !== 'empty') return opened()
        if (scenario === 'empty' && !vault.open) throw { kind: 'noVault', message: 'no folder is open' }
        return listing()
      case 'open_vault':
        vault.path = String(args.path ?? vault.path)
        return opened()
      case 'read_note': {
        const file = vault.find(rel)
        if (!file) throw { kind: 'notFound', message: `${rel} is not in this folder` }
        const content: NoteContent = {
          rel,
          text: file.text,
          hash: `h${file.size}`,
          diskHash: `h${file.diskText.length}`,
          diskMtimeMs: file.diskMtime,
          readonly: file.readonly || undefined,
        }
        return content
      }
      case 'save_note': {
        const file = vault.find(rel)
        if (file) vault.touch(file, String(args.text ?? ''))
        const result: WriteResult = { hash: `h${String(args.text ?? '').length}`, mtimeMs: Date.now() }
        return result
      }
      case 'create_note': {
        const name = rel.endsWith('.md') ? rel : `${rel}.md`
        if (!vault.find(name)) vault.files.push({ rel: name, kind: 'Note', size: 0, mtimeMs: Date.now(), state: 'Waiting', text: '', diskText: '', diskMtime: Date.now() })
        return { hash: 'h0', mtimeMs: Date.now() } satisfies WriteResult
      }
      case 'create_folder':
        if (!vault.find(rel)) vault.files.push({ rel, kind: 'Folder', size: 0, mtimeMs: Date.now(), state: null, text: '', diskText: '', diskMtime: Date.now() })
        return undefined
      case 'rename_entry': {
        const from = String(args.from ?? '')
        const to = String(args.to ?? '')
        const file = vault.find(from)
        if (file) file.rel = to
        const outcome: RenameOutcome = { incomingLinks: [] }
        return outcome
      }
      case 'move_entry': {
        const file = vault.find(String(args.from ?? ''))
        if (file) file.rel = String(args.to ?? '')
        return undefined
      }
      case 'delete_entry':
        vault.files = vault.files.filter((f) => f.rel !== rel)
        return `/home/example/.local/share/jott/trash/${rel.split('/').pop()}`
      case 'mirror_buffer':
      case 'discard_mirror':
        return undefined
      case 'search_vault':
        return search(String(args.query ?? ''), Boolean(args.prefix), String(args.mode ?? 'hybrid'))
      case 'ingest_all': {
        // Watch the status line: the same three lines core would send, on the same channel.
        setTimeout(() => bus.emit('ingest://event', { working: { phase: 'embedding', done: 0, total: 12, memberIndex: null, memberCount: null } }), 300)
        setTimeout(() => bus.emit('ingest://event', { working: { phase: 'embedding', done: 7, total: 12, memberIndex: null, memberCount: null } }), 900)
        setTimeout(() => bus.emit('ingest://event', { indexed: scanReport() }), 1400)
        setTimeout(() => bus.emit('ingest://event', { embedded: embedReport() }), 1800)
        for (const f of vault.files) if (f.kind === 'Note') f.state = 'Indexed'
        return undefined
      }
      case 'set_consent': {
        const outcome: ConsentOutcome = {
          granted: Boolean(args.granted),
          npRefused: null,
          changed: ['models.auto_fetch'],
          provider: 'local-model',
          model: 'mxbai-embed-large',
          persistedTo: '/home/example/.config/note-processor/config.toml',
          fetchBytes: 309_855_456,
          autoFetch: 'written',
          autoFetchError: null,
        }
        return outcome
      }
      case 'embeddings_offer': {
        if (scenario !== 'consent') return null
        const offer: ConsentOffer = { modelId: 'mxbai-embed-large', bytes: 309_855_456, dataDir: '/home/example/.local/share/note-processor' }
        return offer
      }
      case 'embeddings_help': {
        const help: ConsentHelp = {
          gap: scenario === 'consent' ? 'providerMissing' : 'ready',
          provider: scenario === 'consent' ? null : 'local-model',
          modelId: 'mxbai-embed-large',
          linesToAdd: ['[embeddings]', 'provider = "local-model"', 'model = "mxbai-embed-large"'],
          configPath: '/home/example/.config/note-processor/config.toml',
          autoFetch: scenario === 'consent' ? false : true,
        }
        return help
      }
      case 'embeddings_status':
        return modelStatus()
      case 'diagnostics':
        return diagnostics()
      case 'flush_on_close':
        return undefined
      default:
        throw { kind: 'npRefused', message: `the harness has no command named ${cmd}` }
    }
  })()
  return later(answer as T, scenario === 'indexing' ? 220 : 30)
}

/** So a screenshot can say what it is a picture of, and so `Tree.svelte`'s rows are typed rather than inferred. */
export const harness = { scenario, started, rowFor: (rel: string): NodeEntry | undefined => vault.files.find((f) => f.rel === rel) }
