// The formatting behind the diagnostics drawer, tested where the window cannot be: a sentence that says
// "0.0 KiB" or "3 notes are queued · 0 embedded" is a bug nobody screenshots.
//
// The key-set test at the bottom reads the Rust source and compares it to the TypeScript constants. That is the
// only thing standing between a renamed field in `ipc.rs` and a drawer that shows blanks while every Rust test
// still passes.
import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'

import {
  clip,
  mergeRows,
  connectionLabel,
  consentSentence,
  countsSentence,
  copyText,
  formatAgo,
  formatBytes,
  formatElapsed,
  folderOf,
  MAX_DETAIL_CHARS,
  newestFirst,
  queueSentence,
  rowDetail,
  rowLabel,
  RUNTIME_KEYS,
  QUEUE_KEYS,
  TRACE_KEYS,
  VAULT_COUNT_KEYS,
  DIAGNOSTICS_KEYS,
  type DiagnosticsSnapshot,
  type TraceEvent,
} from './diagnostics'

function row(over: Partial<TraceEvent> = {}): TraceEvent {
  return { atMs: 1_700_000_000_000, kind: 'request', method: 'scan', outcome: 'applied', elapsedMs: 12, detail: null, ...over }
}

function snapshot(over: Partial<DiagnosticsSnapshot> = {}): DiagnosticsSnapshot {
  return {
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
    recent: [row()],
    queue: { waiting: 0, heldForConsent: 0, backoff: null, retryInMs: null },
    vault: { notes: 12, embedded: 9, indexed: 2, waiting: 1, refused: 0, warnings: 0 },
    ...over,
  }
}

describe('sizes and durations', () => {
  it('says nothing rather than zero when a number was never sent', () => {
    expect(formatBytes(null)).toBe('')
    expect(formatElapsed(null)).toBe('')
  })

  it('names bytes the way a person reads them', () => {
    expect(formatBytes(512)).toBe('512 B')
    expect(formatBytes(2048)).toBe('2.0 KiB')
    // The model np's own default pins at this size: the prompt and the drawer should agree out loud.
    expect(formatBytes(309_855_456)).toBe('295.5 MiB')
  })

  it('keeps milliseconds where milliseconds are the story', () => {
    expect(formatElapsed(0)).toBe('0 ms')
    expect(formatElapsed(940)).toBe('940 ms')
    expect(formatElapsed(1500)).toBe('1.5 s')
    expect(formatElapsed(65_000)).toBe('1 min 5 s')
  })

  it('says when, in words a person can act on', () => {
    const now = 1_700_000_000_000
    expect(formatAgo(now, now)).toBe('just now')
    expect(formatAgo(now - 4_000, now)).toBe('just now')
    expect(formatAgo(now - 12_000, now)).toBe('12 s ago')
    expect(formatAgo(now - 90_000, now)).toBe('1 min ago')
    expect(formatAgo(now - 2 * 3600_000, now)).toBe('2 h ago')
    // A clock that went backwards mid-run (a laptop waking up) must not read "in 3 s ago".
    expect(formatAgo(now + 5_000, now)).toBe('just now')
  })
})

describe('rows', () => {
  it('labels a request with the three facts that matter and no punctuation for what is missing', () => {
    expect(rowLabel(row())).toBe('scan · applied · 12 ms')
    expect(rowLabel({ ...row(), elapsedMs: null })).toBe('scan · applied')
    expect(rowLabel({ ...row(), outcome: null, elapsedMs: null })).toBe('scan')
  })

  it('labels the other kinds by what they are', () => {
    expect(rowLabel(row({ kind: 'progress', method: null, outcome: null, elapsedMs: null }))).toBe('np progress')
    expect(rowLabel(row({ kind: 'connection' }))).toBe('connection')
    expect(rowLabel(row({ kind: 'consent' }))).toBe('consent')
    expect(rowLabel(row({ kind: 'queue' }))).toBe('queue')
  })

  it('hands the whole np sentence to whoever copies, and shows a short one on screen', () => {
    // §4: the display may clip; the value never does. Truncating here would destroy the only copy of what np said.
    const sentence = 'np ended before answering — np said: error: the index at D:\\notes is locked by another process'
    const line = row({ kind: 'connection', method: null, outcome: null, elapsedMs: null, detail: sentence })
    expect(rowDetail(line)).toBe(sentence)

    // A short sentence is shown as np wrote it; a long one is shortened for the row and nothing else.
    expect(clip(sentence)).toBe(sentence)
    const long = `${sentence}: `.repeat(8).trim()
    const shown = clip(long)
    expect(shown.length).toBeLessThanOrEqual(MAX_DETAIL_CHARS)
    expect(shown.endsWith('…')).toBe(true)
    expect(rowDetail(line)).toBe(long === sentence ? sentence : rowDetail(line))
  })

  it('does not cut a character in half', () => {
    const cut = clip('naïve café ☕️ '.repeat(30))
    expect(cut).not.toContain('\uFFFD')
  })

  it('shows the newest row first and keeps equal stamps in the order they arrived', () => {
    const rows = [row({ atMs: 1 }), row({ atMs: 3, method: 'embed' }), row({ atMs: 2 }), row({ atMs: 3, method: 'scan' })]
    const shown = newestFirst(rows).map((r) => `${r.atMs}${r.method ?? ''}`)
    expect(shown).toEqual(['3embed', '3scan', '2scan', '1scan'])
  })

  it('does not show the same request twice when the snapshot and the live feed overlap', () => {
    const first = row({ atMs: 5 })
    const later = row({ atMs: 6, method: 'embed' })
    // The window subscribes at launch and asks for history when the drawer opens; the same scan arrives twice.
    expect(mergeRows([first, later], [first, later])).toHaveLength(2)
    expect(mergeRows([first], [first, later])).toHaveLength(2)
    // Same stamp and label but a different sentence is a different event, not a duplicate.
    expect(mergeRows([row({ atMs: 5, detail: 'locked' })], [row({ atMs: 5, detail: 'offline' })])).toHaveLength(2)
  })

  it('caps what it hands the renderer, because a DOM with 50 000 rows is another way to hang the window', () => {
    const many = Array.from({ length: 5_000 }, (_, i) => row({ atMs: i }))
    expect(newestFirst(many)).toHaveLength(200)
    expect(newestFirst(many)[0].atMs).toBe(4_999)
  })
})

describe('sentences', () => {
  it('says the queue is idle in words, not a row of zeros', () => {
    expect(queueSentence({ waiting: 0, heldForConsent: 0, backoff: null, retryInMs: null })).toBe('Nothing is waiting.')
  })

  it('names what is queued and why it is waiting', () => {
    expect(queueSentence({ waiting: 3, heldForConsent: 0, backoff: null, retryInMs: null })).toBe(
      '3 notes are queued.',
    )
    expect(queueSentence({ waiting: 1, heldForConsent: 0, backoff: 'locked', retryInMs: 4_200 })).toBe(
      '1 note is queued · np said locked, the next try is in 4 s.',
    )
    expect(queueSentence({ waiting: 0, heldForConsent: 2, backoff: null, retryInMs: null })).toBe(
      '2 notes are waiting for your answer about embeddings.',
    )
  })

  it('counts the folder out loud and only mentions what there is', () => {
    expect(countsSentence(snapshot().vault)).toBe('12 notes · 9 embedded · 2 indexed · 1 not indexed')
    expect(
      countsSentence({ notes: 4, embedded: 0, indexed: 0, waiting: 4, refused: 0, warnings: 0 }),
    ).toBe('4 notes · 4 not indexed')
    expect(
      countsSentence({ notes: 3, embedded: 1, indexed: 1, waiting: 1, refused: 1, warnings: 2 }),
    ).toBe('3 notes · 1 embedded · 1 indexed · 1 not indexed · 1 refused · 2 folders Jott could not read')
    expect(countsSentence({ notes: 0, embedded: 0, indexed: 0, waiting: 0, refused: 0, warnings: 0 })).toBe(
      'This folder has no notes.',
    )
  })

  it('uses the same consent words as the status line', () => {
    expect(consentSentence('undecided')).toBe('not asked yet')
    expect(consentSentence('granted')).toBe('downloads allowed'
    )
    expect(consentSentence('declined')).toBe('keyword search only')
  })

  it('names the folder a log sits in, on both separators', () => {
    expect(folderOf('C:\\Users\\chris\\AppData\\Roaming\\jott\\logs\\jott.jsonl')).toBe(
      'C:\\Users\\chris\\AppData\\Roaming\\jott\\logs',
    )
    expect(folderOf('/home/you/.local/share/jott/logs/jott.jsonl')).toBe('/home/you/.local/share/jott/logs')
    // Nothing to open when there was never a log, rather than a button that opens the desktop.
    expect(folderOf(null)).toBeNull()
  })

  it('says what a connection is doing in words a person can read once', () => {
    expect(connectionLabel('up')).toBe('connected')
    expect(connectionLabel('idle')).toBe('not started')
    expect(connectionLabel('down')).toBe('gone')
  })
})

describe('copy details', () => {
  it('names both versions, the np file, and the log with its size', () => {
    const text = copyText(snapshot(), '0.0.11')
    expect(text).toContain('Jott 0.0.11')
    expect(text).toContain('np 0.3.1 (protocol 1)')
    expect(text).toContain('D:\\Jott\\np\\np.exe')
    expect(text).toContain('writer connected · reader not started')
    expect(text).toContain('jott.jsonl (4.0 KiB)')
    expect(text).toContain('Nothing is waiting.')
    expect(text).toContain('12 notes · 9 embedded')
    expect(text).toContain('scan · applied · 12 ms')
  })

  it('keeps a fault sentence whole, and never grows fields of its own', () => {
    const sentence = 'np ended before answering — np said: configuration-disabled: [embeddings] enabled = false'
    const text = copyText(
      snapshot({ recent: [row({ kind: 'request', method: 'embed', outcome: 'fault', elapsedMs: null, detail: sentence })] }),
      '0.0.11',
    )
    expect(text).toContain(sentence)

    // The drawer's promise: no note text, ever. This is the same list §4 and the journal test enforce.
    for (const forbidden of ['"text"', '"snippet"', '"body"', '"contents"', '"query"']) {
      expect(text).not.toContain(forbidden)
    }
  })

  it('says there is no log when none was opened, instead of a path nobody wrote', () => {
    const bare = snapshot()
    bare.runtime.logPath = null
    bare.runtime.logBytes = null
    expect(copyText(bare, '0.0.11')).toContain('log: none')
  })
})

describe('the wire contract', () => {
  /** Pull every `pub const KEYS` out of the Rust source, with the name of the type that declares it. */
  function rustKeys(file: string): Map<string, string[]> {
    const source = readFileSync(fileURLToPath(new URL(file, import.meta.url)), 'utf8')
    const found = new Map<string, string[]>()
    // Two spellings exist in the Rust source: a fixed-size array whose length is part of the type, and a slice.
    const pattern = /pub const KEYS(?:: \[&'static str; (\d+)\]|: &'static \[&'static str\]) = &?\[([\s\S]*?)\];/g
    for (const match of source.matchAll(pattern)) {
      const declared = match[1] === undefined ? null : Number(match[1])
      const keys = [...match[2].matchAll(/"([^"]+)"/g)].map((k) => k[1])
      // The nearest `impl X` or `impl X ...` above the constant is the type that owns it.
      const before = source.slice(0, match.index)
      const owner = [...before.matchAll(/^impl (\w+)/gm)].pop()?.[1]
      expect(owner, `which type declares: ${match[0].slice(0, 40)}…`).toBeTruthy()
      if (declared !== null) expect(keys, `${owner} lists ${keys.length} keys but its array says ${declared}`).toHaveLength(declared)
      found.set(owner!, keys)
    }
    return found
  }

  it('agrees with Rust about every key the drawer reads', () => {
    const ipc = rustKeys('../../crates/jott-core/src/ipc.rs')
    const core = rustKeys('../../crates/jott-core/src/diagnostics.rs')

    expect(ipc.get('Diagnostics')).toEqual([...DIAGNOSTICS_KEYS])
    expect(ipc.get('RuntimeInfo')).toEqual([...RUNTIME_KEYS])
    expect(ipc.get('QueueState')).toEqual([...QUEUE_KEYS])
    expect(ipc.get('VaultCounts')).toEqual([...VAULT_COUNT_KEYS])
    expect(core.get('TraceEvent')).toEqual([...TRACE_KEYS])
  })

  it('keeps note contents off the list of things the drawer may show', () => {
    const all = [...DIAGNOSTICS_KEYS, ...RUNTIME_KEYS, ...QUEUE_KEYS, ...VAULT_COUNT_KEYS, ...TRACE_KEYS]
    for (const forbidden of ['text', 'snippet', 'body', 'contents', 'query', 'path']) {
      expect(all.map((k) => k.toLowerCase())).not.toContain(forbidden)
    }
  })

  it('reads the Rust source it claims to read', () => {
    // A typo in a path above would make the comparison vacuous — zero keys compared, everything green.
    const source = readFileSync(fileURLToPath(new URL('../../crates/jott-core/src/ipc.rs', import.meta.url)), 'utf8')
    expect(source).toContain('pub const KEYS')
  })
})
