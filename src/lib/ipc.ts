// The webview's side of the IPC surface. Types here mirror `apps/shell/src/ipc.rs`'s DTOs, and
// `crates/jott-core/tests/ipc_surface.rs` pins the serialized shape of every one of them, so a change on
// one side fails a test rather than showing up as `undefined` in the window.
//
// Reports from np (`ScanReport`, `EmbedReport`) keep np's own field spellings: they are np's documents,
// and §18 says to show np's words instead of a translation of them.

import { invoke } from '@tauri-apps/api/core'
import type { SearchMode } from './searchSettings'
import { listen } from '@tauri-apps/api/event'
import { confirm, open } from '@tauri-apps/plugin-dialog'
// The drawer's shapes live with the drawer (`lib/diagnostics.ts`), which pins them against the Rust key sets.
// Re-declaring them here would be a second copy of a contract nobody would keep.
import type { DiagnosticsSnapshot, TraceEvent } from './diagnostics'

export type StampState = 'Waiting' | 'Indexed' | 'Embedded' | { Refused: string }

export type EntryKind = 'Folder' | 'Note' | 'Image' | 'Attachment' | 'Generated'

export interface NodeEntry {
  rel: string
  kind: EntryKind
  size: number
  /** Milliseconds since the unix epoch, or null when the filesystem would not say. */
  mtimeMs: number | null
  /** The §9 marker's source. Null means this row has no index state to show, not that it waits. */
  state: StampState | null
}

export interface VaultListing {
  vault: string
  nodes: NodeEntry[]
  /** What the walk could not do, in words we did not invent: a folder that could not be read. */
  warnings: string[]
}

export interface NoteContent {
  rel: string
  text: string
  hash: string
  /** Bytes on disk when the buffer was filled, so an outside change is detectable (§14). */
  diskHash: string
  diskMtimeMs: number
  /** §18.13: a mirrored unsaved buffer newer than the file, replayed at launch. */
  /** §18.13's unsaved buffer from an earlier session. `hash` is what the buffer held when it was mirrored,
   * so "this is the text you already have on screen" is checkable rather than assumed. */
  mirrored?: { text: string; mtimeMs: number; hash: string }
  /** Jott may show this file and may not write it — true for what note-processor publishes at the vault root. The
   *  window shows a display rather than an editor, so nobody types into a buffer that would fail on save. */
  readonly?: boolean
}

export interface WriteResult {
  hash: string
  mtimeMs: number
}

export interface RenameOutcome {
  /** Notes whose text refers to what was renamed — detection only in M1 (§13). */
  incomingLinks: { src: string; line: number; target: string }[]
}

export interface SearchHit {
  path: string
  /** Absent when np returned no snippet. Its `[` `]` marks are np's own and are not escaped (§18.11). */
  snippet: string | null
  kind: string
  score: number
  bm25: number | null
  cosine: number | null
  /** Did this row match a word that was typed? `false` means np reached it by a **word beginning** (0.5.x,
      note-processor#6) and the row says so beside the snippet. `null` means np never word-tested it — a meaning-only
      hit — which is a different answer from a yes, and must not be rendered as one. */
  matchedExactly: boolean | null
}

export interface SearchResults {
  hits: SearchHit[]
  /** np's own word for which half answered. Kept for the log and the drawer, never printed on its own: until
      0.4.0 it said `hybrid` for searches where only keywords ran (note-processor#5). */
  mode: string
  vectors: boolean
  offline: boolean
  reason: string | null
  /** The four keys np 0.4.0 added, all `null` from an older backend — "did not say", never "said no". */
  indexed: boolean | null
  keywordUsed: boolean | null
  semanticUsed: boolean | null
  /** np's sentence about this answer, byte for byte (§4): a refusal, a fetch, a workspace it never scanned. */
  notice: string | null
  /** Did np widen any term to word beginnings? `null` from an older backend with no such flag: "did not widen" and
      "did not say" are two different sentences to show somebody (§4). */
  prefixUsed: boolean | null
  /** Which words np widened, as it named them. The header may print these words and no others. */
  prefixTerms: string[]
}

/** np's `models.list` cannot report an artifact size in 0.3.1, so `bytes` is null until something does. */
export type IngestEvent =
  | {
      working: {
        phase: string
        /** Bytes while np is fetching, chunks while it embeds or writes — the same counter np puts in the line. */
        done: number
        total: number | null
        /** Which of several model files is arriving. Both null when np sent no member keys, and when it says the
            model has one file: `1 of 1` would be a fact nobody needed. */
        memberIndex: number | null
        memberCount: number | null
      }
    }
  | { indexed: ScanReport }
  | { embedded: EmbedReport }
  | { behind: { reason: StateReason; retryInMs: number } }
  | { needsConsent: { modelId: string | null; bytes: number | null; dataDir: string } }
  | { refused: { message: string } }
  | { disabled: { key: string | null; modelId: string | null; hint: string | null } }

export type StateReason = 'offline' | { locked: { waitedMs: number } }

/** np's own record of a model it downloaded: which one, and how big in total — `null` when any member announced no
    size, because half a total is not a total. What np also reports (per-member digests, origin) stays unread: nothing
    on screen uses it, and a field nobody reads is a field nobody checks. */
export interface FetchedRecord {
  id: string
  bytesTotal: number | null
}

export interface ScanReport {
  added: number
  modified: number
  moved: number
  removed: number
  unchanged: number
  skipped: string[]
  notes: number
  unresolved_links: number
  unresolved_targets: string[]
  index_files: string[]
  elapsed_ms: number
  upgrade_orphans: string | null
  merged_notes: string | null
  recorded_upgrade_orphans: string | null
  recorded_merged_notes: string | null
  unindexable: { unreadable: number; unkeyable: number; duplicates: number; ambiguous: number; names: string[] } | null
}

export interface EmbedReport {
  model: string | null
  chunks_seen: number
  current: number
  embedded: number
  refused_budget: number
  busy_gap: number
  errors: number
  images_seen: number
  images_embedded: number
  images_refused: number
  images_pruned: number
  images_unchecked: number
  images_sweep_ran: boolean
  image_refusals: { path: string; class: string }[]
  verification: string
  offline: boolean
  /** The model this run downloaded, when it downloaded one. `null` is the ordinary case. */
  fetched: FetchedRecord | null
}


/** What the shell emits when np cannot be found or is too old, with np's install instructions attached. */
export interface NpMissingPayload {
  message: string
  installHint: string
}

export interface VaultChanged {
  rel: string
}

/** The error every command returns in place of a result. `kind` is for branching; `message` is shown. */
export interface IpcError {
  // camelCase because that is how `ErrorKind` serialises. `ipc_surface.rs` pins every spelling on the Rust side and
  // `ipc.test.ts` reads that same source and compares it to the union below, so a new kind cannot arrive here as
  // an unlisted string — which is what a silent fall-through looks like from the inside.
  kind:
    | 'escapesRoot'
    | 'notFound'
    | 'alreadyExists'
    | 'intoItself'
    | 'tooLarge'
    | 'notUtf8'
    | 'npGenerated'
    | 'noVault'
    | 'npGone'
    | 'npRefused'
    | 'npTooOld'
    /** Jott was asked for a search mode it cannot spell to note-processor. Its own verdict, raised at the boundary:
     *  the two sides are different languages with no compiler between them, so an unknown label has to come back as a
     *  sentence naming what is accepted rather than as a quiet `hybrid`. */
    | 'badMode'
    | 'io'
  message: string
}

/** What the registry remembers, beside the tree: the recents menu and the consent decision (§12). */
export interface RegistryInfo {
  recent: string[]
  /** Lower case because that is how the registry file has always spelled it (Task 6): one decision, one spelling. */
  consent: 'undecided' | 'granted' | 'declined'
}

/** What the About panel shows. Built in Rust (`jott_core::about`) so its wording has tests; the front end
 * only draws it, which is also why `diagnostics` arrives as a finished block of text. */
export interface NpReport {
  found: boolean
  version?: string
  /** `bundled`, `$NP_PATH` or `$PATH` — the three places Jott looks, spelled as a person would set them. */
  source?: string
  path?: string
  problem?: string
  hint?: string
}

export interface AboutInfo {
  jottVersion: string
  np: NpReport
  dataDir: string
  logPath: string
  repoUrl: string
  releasesUrl: string
  docsUrl: string
  diagnostics: string
}

export const listVault = () => invoke<VaultListing>('list_vault')
export const registryInfo = () => invoke<RegistryInfo>('registry_info')
/** Both versions and the np file this run is talking to; asks np at most once per run. */
export const appInfo = () => invoke<AboutInfo>('app_info')
export const openVault = (path: string) => invoke<VaultListing>('open_vault', { path })
/**
 * The native folder picker, asked from the webview. `null` is a cancelled dialog (§9's empty state then
 * stays as it was). Picking is the front end's job — one fewer command, and the plugin's own dialog runs
 * without anything of ours blocking a worker thread waiting for it.
 */
export async function pickVault(): Promise<VaultListing | null> {
  const picked = await open({ directory: true, title: 'Open a notes folder' })
  if (typeof picked !== 'string') return null
  return openVault(picked)
}
export const readNote = (rel: string) => invoke<NoteContent>('read_note', { rel })
export const saveNote = (rel: string, text: string) => invoke<WriteResult>('save_note', { rel, text })
export const createNote = (rel: string) => invoke<WriteResult>('create_note', { rel })
export const createFolder = (rel: string) => invoke<void>('create_folder', { rel })
export const renameEntry = (from: string, to: string) => invoke<RenameOutcome>('rename_entry', { from, to })
export const deleteEntry = (rel: string) => invoke<string>('delete_entry', { rel })
export const moveEntry = (from: string, to: string) => invoke<void>('move_entry', { from, to })
/** Mirror text the editor holds but has not written (§18.13). Autosave calls this on its own tick. */
export const mirrorBuffer = (rel: string, text: string) => invoke<void>('mirror_buffer', { rel, text })
/** §18.13's "Discard it": without this the draft would reappear next time the note is opened. */
export const discardMirror = (rel: string) => invoke<void>('discard_mirror', { rel })
/** `prefix` is the window's "match word beginnings" toggle: always sent, because np's own default would otherwise
    decide what somebody's search meant. */
/** Ask note-processor. `mode` and `prefix` are sent every time rather than omitted: both change what the query
 *  *means*, and leaving either out hands the decision to a default in another process, which is how a search ends up
 *  answering a question nobody asked. */
export const searchVault = (query: string, limit: number, prefix: boolean, mode: SearchMode) =>
  invoke<SearchResults>('search_vault', { query, limit, prefix, mode })
export const ingestAll = () => invoke<void>('ingest_all')
/** What np answered when the consent button was pressed (`ConsentAnswerDto` in Rust). The four words of `autoFetch`
 *  are the whole difference between "the guardrail is down" and "Jott could not lower it", so they travel as data. */
export interface ConsentOutcome {
  granted: boolean
  /** np's refusal sentence, byte for byte. Non-null means np did not do the thing. */
  npRefused: string | null
  /** Which settings np moved itself, in the order it named them. Empty after an accepted yes: already recorded. */
  changed: string[]
  provider: string | null
  /** The model np kept in mind. A decline keeps naming it — revoking permission is not erasing a choice. */
  model: string | null
  /** The file np says it wrote. Null means np did not say, which is not the same as "the file you would guess". */
  persistedTo: string | null
  fetchBytes: number | null
  autoFetch: 'written' | 'untouched' | 'noConfigPath' | 'failed'
  /** The sentence naming the file when `autoFetch` is `failed`. */
  autoFetchError: string | null
}

export const setConsent = (granted: boolean) => invoke<ConsentOutcome>('set_consent', { granted })

/** Whether setting embeddings up is still on the table: np's model id and folder when it can name them, and
    `null` when there is nothing to offer (`ConsentOffer` mirrors `jott_core::ingest::queue::ConsentOffer`). */
export type ConsentOffer = { modelId: string | null; bytes: number | null; dataDir: string }
export const embeddingsOffer = () => invoke<ConsentOffer | null>('embeddings_offer')

/** Why vectors still do not exist, in np's own terms. Everything here is read back out of np's config: which
    setting is short, the lines that would close it, and what `auto_fetch` says today. Note what is missing —
    there is no field claiming Jott wrote any of it, because presence in someone's file proves only presence. */
export interface ConsentHelp {
  gap: 'providerMissing' | 'modelMissing' | 'providerOther' | 'configUnreadable' | 'ready'
  provider: string | null
  modelId: string | null
  linesToAdd: string[]
  configPath: string | null
  autoFetch: boolean | null
}
export const embeddingsHelp = () => invoke<ConsentHelp>('embeddings_help')

/** A question with two answers, asked outside the status line. False when nobody answered. */
export const askConfirm = (message: string, title: string) => confirm(message, { title })
// §14's two answers are not one command: *reload* re-reads the file (`readNote`), and *keep* is a
// decision the window makes locally — it changes nothing on disk until the next save.

/** Everything the diagnostics drawer shows: runtime facts, the queue, folder counts, and this run's rows. Built
    from memory, so opening it never waits on np — and never starts it. */
export const diagnostics = () => invoke<DiagnosticsSnapshot>('diagnostics')

/** Live backend activity. Subscribed from launch, not when the drawer opens: a row about the save that hung ten
    minutes ago is the reason somebody opens it. */
export const onTraceRow = (handler: (row: TraceEvent) => void) => listen<TraceEvent>('trace://row', (e) => handler(e.payload))

export const onIngestEvent = (handler: (event: IngestEvent) => void) => listen<IngestEvent>('ingest://event', (e) => handler(e.payload))
export const onVaultChanged = (handler: (change: VaultChanged) => void) => listen<VaultChanged>('vault://changed', (e) => handler(e.payload))
export const onNpMissing = (handler: (payload: NpMissingPayload) => void) => listen<NpMissingPayload>('np://missing', (e) => handler(e.payload))

/** Asked once when the window closes, so the queue can finish what it started (§11). */
export const flushOnClose = () => invoke<void>('flush_on_close')

/** For the unload hook: Tauri has no `beforeunload` save of its own. */
export async function saveBeforeUnload(rel: string, text: string): Promise<void> {
  try {
    await saveNote(rel, text)
  } finally {
    await flushOnClose()
  }
}
