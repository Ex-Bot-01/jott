// The webview's side of the IPC surface. Types here mirror `apps/shell/src/ipc.rs`'s DTOs, and
// `crates/jott-core/tests/ipc_surface.rs` pins the serialized shape of every one of them, so a change on
// one side fails a test rather than showing up as `undefined` in the window.
//
// Reports from np (`ScanReport`, `EmbedReport`) keep np's own field spellings: they are np's documents,
// and §18 says to show np's words instead of a translation of them.

import { invoke } from '@tauri-apps/api/core'
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
}

export interface SearchResults {
  hits: SearchHit[]
  mode: string
  vectors: boolean
  offline: boolean
  reason: string | null
}

/** np's `models.list` cannot report an artifact size in 0.3.1, so `bytes` is null until something does. */
export type IngestEvent =
  | { working: { phase: string; done: number; total: number | null } }
  | { indexed: ScanReport }
  | { embedded: EmbedReport }
  | { behind: { reason: StateReason; retryInMs: number } }
  | { needsConsent: { modelId: string | null; bytes: number | null; dataDir: string } }
  | { refused: { message: string } }
  | { disabled: { key: string | null; modelId: string | null; hint: string | null } }

export type StateReason = 'offline' | { locked: { waitedMs: number } }

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
  // camelCase because that is how `ErrorKind` serialises (`ipc_surface.rs` pins each spelling). A kind the
  // front end does not list is a branch that silently falls through, so this union is the whole list.
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
export const searchVault = (query: string, limit: number) => invoke<SearchResults>('search_vault', { query, limit })
export const ingestAll = () => invoke<void>('ingest_all')
export const setConsent = (granted: boolean) => invoke<void>('set_consent', { granted })

/** Whether setting embeddings up is still on the table: np's model id and folder when it can name them, and
    `null` when there is nothing to offer (`ConsentOffer` mirrors `jott_core::ingest::queue::ConsentOffer`). */
export type ConsentOffer = { modelId: string | null; bytes: number | null; dataDir: string }
export const embeddingsOffer = () => invoke<ConsentOffer | null>('embeddings_offer')

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
