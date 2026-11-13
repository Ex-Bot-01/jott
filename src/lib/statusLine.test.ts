// The status line's words, in one file, because §11 is specific about them: durations and counts come
// from np's own numbers, `offline` has a fixed sentence, and a refusal shows np's message unchanged.
// A component that formats these inline cannot be tested for inventing something.

import { describe, expect, it } from 'vitest'
import { consentSentence, statusFor } from './statusLine'
import type { IngestEvent } from './ipc'

describe('statusFor', () => {
  it('says what is embedding with np\'s counters', () => {
    const event: IngestEvent = { working: { phase: 'embedding', done: 3, total: 7 } }
    expect(statusFor(event)).toMatchObject({ text: 'embedding 3/7 chunks', tone: 'busy' })
  })

  it('names a counterless phase without pretending it is at zero', () => {
    // §18.12: no counters means unknown. "indexing 0/0" would be a made-up measurement.
    const event: IngestEvent = { working: { phase: 'indexing', done: 0, total: null } }
    expect(statusFor(event)).toMatchObject({ text: 'indexing…', tone: 'busy' })
  })

  it('shows a download in megabytes from np\'s byte counters', () => {
    const event: IngestEvent = { working: { phase: 'fetching', done: 87_000_000, total: 309_366_528 } }
    expect(statusFor(event)?.text).toBe('fetching 83 MiB / 295 MiB')
  })

  it('reports a scan with the time np said it took', () => {
    const event: IngestEvent = {
      indexed: { added: 1, modified: 0, moved: 0, removed: 0, unchanged: 12, skipped: [], notes: 13, unresolved_links: 0, unresolved_targets: [], index_files: ['INDEX.md'], elapsed_ms: 1240, upgrade_orphans: null, merged_notes: null, recorded_upgrade_orphans: null, recorded_merged_notes: null, unindexable: null },
    }
    expect(statusFor(event)?.text).toBe('indexed in 1.2 s')
  })

  it('says how many chunks failed when np said some did', () => {
    const event: IngestEvent = {
      embedded: { model: 'bge-m3', chunks_seen: 14, current: 11, embedded: 3, refused_budget: 0, busy_gap: 0, errors: 3, images_seen: 0, images_embedded: 0, images_refused: 0, images_pruned: 0, images_unchecked: 0, images_sweep_ran: false, image_refusals: [], verification: 'not verified', offline: false },
    }
    expect(statusFor(event)).toMatchObject({ text: 'embedded · 3 chunks failed', tone: 'warn' })
  })

  it('says index behind with the retry time it was given', () => {
    const event: IngestEvent = { behind: { reason: { locked: { waitedMs: 5000 } }, retryInMs: 5000 } }
    expect(statusFor(event)).toMatchObject({ text: 'index behind · retrying in 5 s', tone: 'warn' })
  })

  it('says offline in keyword-search terms, not as an error', () => {
    const event: IngestEvent = { behind: { reason: 'offline', retryInMs: 2000 } }
    expect(statusFor(event)).toMatchObject({ text: 'offline — keyword search only', tone: 'warn' })
  })

  it('asks for consent with the model and folder it named, and no size it was not given', () => {
    const event: IngestEvent = { needsConsent: { modelId: 'bge-m3', bytes: null, dataDir: '/home/ana/.local/share/note-processor' } }
    const status = statusFor(event)
    expect(status?.text).toBe(
      'semantic search needs a model (bge-m3) downloaded into /home/ana/.local/share/note-processor. Allow?',
    )
    expect(status?.action).toBe('consent')
  })

  it('includes a size when np gave one', () => {
    const event: IngestEvent = { needsConsent: { modelId: 'bge-m3', bytes: 309_366_528, dataDir: '/x' } }
    expect(statusFor(event)?.text).toContain('about 295 MiB')
  })

  it('names np\u2019s built-in model when nothing is configured yet', () => {
    // A fresh np names no model in its config and still carries a pin, so the prompt has to say something
    // truer than "a model". The id and bytes come from core; this only words them.
    const event: IngestEvent = { needsConsent: { modelId: null, bytes: 309_855_456, dataDir: 'C:\\Users\\chris\\AppData\\Roaming\\note-processor\\models' } }
    expect(statusFor(event)?.text).toBe(
      'semantic search needs a model (np\u2019s built-in model, about 296 MiB) downloaded into C:\\Users\\chris\\AppData\\Roaming\\note-processor\\models. Allow?',
    )
  })

  it('says the same sentence in the status line and in the dialog that asks again later', () => {
    const offer = { modelId: 'embeddinggemma-2', bytes: 309_855_456, dataDir: '/models' }
    expect(consentSentence(offer)).toBe(statusFor({ needsConsent: { ...offer } })?.text)
  })

  it('shows a refusal as np wrote it', () => {
    const sentence = 'workspace is mid-migration: run `np migrate` first'
    expect(statusFor({ refused: { message: sentence } })?.text).toBe(sentence)
  })

  it('says embedding is off in configuration, quoting what np named', () => {
    const event: IngestEvent = { disabled: { key: '[embeddings] model', modelId: null, hint: 'set [embeddings] model in np config' } }
    const status = statusFor(event)
    expect(status?.text).toBe("keyword-only: [embeddings] model is not set — set [embeddings] model in np config")
    expect(status?.action).toBe('consent')
  })

  it('says keyword-only when the model simply is not installed', () => {
    // §9's status line, for the state where a model is configured and its bytes are not here.
    const event: IngestEvent = { disabled: { key: null, modelId: 'bge-m3', hint: null } }
    expect(statusFor(event)?.text).toBe('keyword-only: model not installed')
  })
})
