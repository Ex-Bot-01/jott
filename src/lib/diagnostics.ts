// The diagnostics drawer's shapes and its sentences.
//
// Two jobs live here, both kept out of the component on purpose:
//
// * The types mirror `crates/jott-core/src/ipc.rs` and `diagnostics.rs`. Nothing checks them at compile time —
//   Rust cannot see TypeScript — so the tests in `diagnostics.test.ts` read the Rust source and compare key sets.
// * Every string the drawer prints is built here, because a sentence like "0 notes are queued · 0 embedded" is a
//   bug nobody screenshots, and §4 forbids inventing a value where one is absent.

/** Whether a slot holds a live np child. `idle` means never needed, `down` means it was there. */
export type ConnectionState = 'up' | 'idle' | 'down'

export type TraceKind = 'request' | 'progress' | 'connection' | 'consent' | 'queue'

/** The registry's word for the embeddings decision, in the lowercase the wire carries. */
export type ConsentWord = 'undecided' | 'granted' | 'declined'

/** One line of this run's backend activity. `null` means the fact was never sent, and stays blank on screen. */
export interface TraceEvent {
  atMs: number
  kind: TraceKind
  method: string | null
  outcome: string | null
  elapsedMs: number | null
  /** np's own sentence when there is one. Shown clipped, copied whole (§4). */
  detail: string | null
}

export interface RuntimeInfo {
  npVersion: string | null
  npPath: string | null
  protocol: number | null
  writer: ConnectionState
  reader: ConnectionState
  dataDir: string
  logPath: string | null
  logBytes: number | null
  consent: ConsentWord
  autoFetch: boolean | null
}

export interface QueueState {
  waiting: number
  heldForConsent: number
  /** `locked` / `offline`, in np's words, and only while a retry is actually pending. */
  backoff: string | null
  retryInMs: number | null
}

export interface VaultCounts {
  notes: number
  embedded: number
  indexed: number
  waiting: number
  refused: number
  /** Folders Jott could not read — a different fact from a note np refused, so a different sentence. */
  warnings: number
}

export interface DiagnosticsSnapshot {
  runtime: RuntimeInfo
  /** Oldest first off the wire; `newestFirst` turns that around for display. */
  recent: TraceEvent[]
  queue: QueueState
  vault: VaultCounts
}

// The key sets, pinned against the Rust constants by `diagnostics.test.ts`. A field renamed on one side fails a
// test here instead of showing up as `undefined` in a window someone is already suspicious of.
export const TRACE_KEYS = ['atMs', 'kind', 'method', 'outcome', 'elapsedMs', 'detail'] as const
export const DIAGNOSTICS_KEYS = ['runtime', 'recent', 'queue', 'vault'] as const
export const RUNTIME_KEYS = [
  'npVersion',
  'npPath',
  'protocol',
  'writer',
  'reader',
  'dataDir',
  'logPath',
  'logBytes',
  'consent',
  'autoFetch',
] as const
export const QUEUE_KEYS = ['waiting', 'heldForConsent', 'backoff', 'retryInMs'] as const
export const VAULT_COUNT_KEYS = ['notes', 'embedded', 'indexed', 'waiting', 'refused', 'warnings'] as const

/** How much of an np sentence fits on a row. The full text is still one click away and in "copy details". */
export const MAX_DETAIL_CHARS = 120

/** Rows the drawer renders. More than this and the window spends its time laying out history. */
export const MAX_ROWS = 200

export function formatBytes(bytes: number | null): string {
  if (bytes === null) return ''
  if (bytes < 1024) return `${bytes} B`
  const kib = bytes / 1024
  if (kib < 1024) return `${kib.toFixed(1)} KiB`
  return `${(kib / 1024).toFixed(1)} MiB`
}

export function formatElapsed(ms: number | null): string {
  if (ms === null) return ''
  if (ms < 1000) return `${ms} ms`
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)} s`
  const minutes = Math.floor(ms / 60_000)
  return `${minutes} min ${Math.round((ms % 60_000) / 1000)} s`
}

/** "When" in the words a person uses, because an epoch stamp on a debugging screen answers nobody's question. */
export function formatAgo(atMs: number, nowMs: number): string {
  const delta = nowMs - atMs
  // A clock that went backwards mid-run (a laptop waking, a sync correcting the time) is not a row from the future.
  if (delta < 5_000) return 'just now'
  if (delta < 60_000) return `${Math.round(delta / 1000)} s ago`
  if (delta < 3_600_000) return `${Math.floor(delta / 60_000)} min ago`
  return `${Math.floor(delta / 3_600_000)} h ago`
}

/** Shorten for display without cutting a character in half. The value itself is never mutated anywhere else. */
export function clip(sentence: string, max = MAX_DETAIL_CHARS): string {
  if (sentence.length <= max) return sentence
  let cut = max - 1
  const code = sentence.charCodeAt(cut - 1)
  if (code >= 0xd800 && code <= 0xdbff) cut -= 1 // a high surrogate would be split
  return `${sentence.slice(0, cut)}…`
}

const KIND_LABELS: Record<TraceKind, string> = {
  request: 'request',
  progress: 'np progress',
  connection: 'connection',
  consent: 'consent',
  queue: 'queue',
}

/** The left-hand part of a row: what was asked, how it ended, how long it took — with no separators for facts
 *  that were never sent. "scan · · 12 ms" reads like the app is broken. */
export function rowLabel(row: TraceEvent): string {
  if (row.kind !== 'request') return KIND_LABELS[row.kind]
  const parts = [row.method, row.outcome, row.elapsedMs === null ? null : formatElapsed(row.elapsedMs)]
  return parts.filter((part): part is string => Boolean(part)).join(' · ') || KIND_LABELS[row.kind]
}

/** The right-hand part: np's sentence, whole. Clipping happens where it is drawn, never here. */
export function rowDetail(row: TraceEvent): string | null {
  return row.detail
}

export function newestFirst(rows: TraceEvent[], limit = MAX_ROWS): TraceEvent[] {
  return rows
    .map((row, index) => ({ row, index }))
    .sort((a, b) => b.row.atMs - a.row.atMs || a.index - b.index)
    .slice(0, limit)
    .map((kept) => kept.row)
}

/** Join the history the backend handed over with what has arrived since. The drawer subscribes at launch and
 *  also asks for a snapshot when it opens, so the same request can reach it twice — and a screen where every row
 *  appears twice reads like a broken app rather than a duplicated feed. */
export function mergeRows(history: TraceEvent[], live: TraceEvent[]): TraceEvent[] {
  const seen = new Set<string>()
  const merged: TraceEvent[] = []
  for (const row of [...history, ...live]) {
    const key = `${row.atMs}\u0000${row.kind}\u0000${rowLabel(row)}\u0000${row.detail ?? ''}`
    if (seen.has(key)) continue
    seen.add(key)
    merged.push(row)
  }
  return merged
}

/** The folder a log file sits in, for "open the folder" — without asking a filesystem, because this module runs
 *  in a webview that has no business touching one. Both separators, because the Windows build is the one where
 *  anybody clicks this button. */
export function folderOf(path: string | null): string | null {
  if (path === null) return null
  const trimmed = path.replace(/[\\/]+$/, '')
  const cut = Math.max(trimmed.lastIndexOf('/'), trimmed.lastIndexOf('\\'))
  return cut <= 0 ? null : trimmed.slice(0, cut)
}

function notes(count: number, singular: string, plural: string): string {
  return `${count} ${count === 1 ? singular : plural}`
}

export function queueSentence(queue: QueueState): string {
  const parts: string[] = []
  if (queue.waiting > 0) parts.push(notes(queue.waiting, 'note is queued', 'notes are queued'))
  if (queue.heldForConsent > 0) {
    parts.push(notes(queue.heldForConsent, 'note is', 'notes are') + ' waiting for your answer about embeddings')
  }
  if (queue.backoff !== null) {
    parts.push(
      queue.retryInMs === null
        ? `np said ${queue.backoff}, and will try again shortly`
        : `np said ${queue.backoff}, the next try is in ${Math.max(1, Math.round(queue.retryInMs / 1000))} s`,
    )
  }
  return parts.length === 0 ? 'Nothing is waiting.' : `${parts.join(' · ')}.`
}

export function countsSentence(counts: VaultCounts): string {
  if (counts.notes === 0) return 'This folder has no notes.'
  const parts = [`${counts.notes} notes`]
  if (counts.embedded > 0) parts.push(`${counts.embedded} embedded`)
  if (counts.indexed > 0) parts.push(`${counts.indexed} indexed`)
  if (counts.waiting > 0) parts.push(`${counts.waiting} not indexed`)
  if (counts.refused > 0) parts.push(`${counts.refused} refused`)
  if (counts.warnings > 0) {
    parts.push(counts.warnings === 1 ? '1 folder Jott could not read' : `${counts.warnings} folders Jott could not read`)
  }
  return parts.join(' · ')
}

export function consentSentence(consent: ConsentWord): string {
  if (consent === 'granted') return 'downloads allowed'
  if (consent === 'declined') return 'keyword search only'
  return 'not asked yet'
}

export function connectionLabel(state: ConnectionState): string {
  if (state === 'up') return 'connected'
  if (state === 'down') return 'gone'
  return 'not started'
}

/** The block behind "copy details". Full sentences, no clipping, and — as always in this feature — no note text:
 *  everything here comes from fields the backend chose to send, never from a file's contents. */
export function copyText(snapshot: DiagnosticsSnapshot, jottVersion: string): string {
  const { runtime, queue, vault, recent } = snapshot
  const lines = [`Jott ${jottVersion} — diagnostics`, '']
  const np = runtime.npVersion ? `np ${runtime.npVersion}` : 'np version unknown'
  const withProtocol = runtime.protocol === null ? np : `${np} (protocol ${runtime.protocol})`
  lines.push(`${withProtocol}${runtime.npPath ? ` — ${runtime.npPath}` : ''}`)
  lines.push(`writer ${connectionLabel(runtime.writer)} · reader ${connectionLabel(runtime.reader)}`)
  const fetch = runtime.autoFetch === null ? 'unknown' : runtime.autoFetch ? 'yes' : 'no'
  lines.push(`embeddings consent: ${consentSentence(runtime.consent)} · np may fetch a model on its own: ${fetch}`)
  lines.push(`np data folder: ${runtime.dataDir || 'unknown'}`)
  lines.push(
    runtime.logPath === null
      ? 'log: none'
      : `log: ${runtime.logPath}${runtime.logBytes === null ? '' : ` (${formatBytes(runtime.logBytes)})`}`,
  )
  lines.push(`queue: ${queueSentence(queue)}`)
  lines.push(`folder: ${countsSentence(vault)}`)
  lines.push('', '-- this run --')
  for (const row of newestFirst(recent)) {
    const stamp = new Date(row.atMs).toISOString().slice(11, 19)
    const detail = rowDetail(row)
    lines.push(detail === null ? `${stamp}  ${rowLabel(row)}` : `${stamp}  ${rowLabel(row)} — ${detail}`)
  }
  return lines.join('\n')
}
