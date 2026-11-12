// One line about what the index is doing (§9, §11). It lives here rather than in the component because
// the rules are specific: every number is np's, `offline` has a fixed sentence that is not an error, and
// a refusal shows np's message with nothing added. A formatter you can test is the only way to keep that
// promise once the window grows.

import type { IngestEvent } from './ipc'

export interface Status {
  text: string
  /** `busy` is working; `warn` is a state someone should notice but that is not a failure (§11). */
  tone: 'neutral' | 'busy' | 'warn'
  /** The affordance to offer beside the text. `[embed]` is always there in the component itself. */
  action?: 'consent'
}

const MIB = 1024 * 1024

export function statusFor(event: IngestEvent): Status | null {
  if ('working' in event) {
    const { phase, done, total } = event.working
    if (phase === 'embedding' || phase === 'writing') {
      // np's chunk counters (§11's `embedding 3/7 chunks`). No total means np did not know yet, and a
      // bar at zero would be a measurement nobody made (§18.12).
      return { text: total === null ? `${phase}…` : `${phase} ${done}/${total} chunks`, tone: 'busy' }
    }
    if (phase === 'fetching') {
      const soFar = `${mebibytes(done)} MiB`
      return { text: total === null ? `fetching ${soFar}` : `fetching ${soFar} / ${mebibytes(total)} MiB`, tone: 'busy' }
    }
    // `locking`, `indexing`, `resolving`, `completed`: np sends no counters with these, so the phase is
    // the whole truth.
    return { text: `${phase}…`, tone: 'busy' }
  }

  if ('indexed' in event) {
    const elapsed = event.indexed.elapsed_ms
    return { text: elapsed >= 1000 ? `indexed in ${(elapsed / 1000).toFixed(1)} s` : `indexed in ${elapsed} ms`, tone: 'neutral' }
  }

  if ('embedded' in event) {
    const report = event.embedded
    // np's own failure count, shown when it is not zero (§18.2: its embed report is aggregates, so this
    // is as specific as the truth gets).
    return report.errors > 0
      ? { text: `embedded · ${report.errors} chunks failed`, tone: 'warn' }
      : { text: 'embedded', tone: 'neutral' }
  }

  if ('behind' in event) {
    const { reason, retryInMs } = event.behind
    if (reason === 'offline') {
      // §11's sentence, verbatim: offline is a state with a working half, not an outage.
      return { text: 'offline — keyword search only', tone: 'warn' }
    }
    const seconds = Math.max(1, Math.round(retryInMs / 1000))
    return { text: `index behind · retrying in ${seconds} s`, tone: 'warn' }
  }

  if ('needsConsent' in event) {
    const { modelId, bytes, dataDir } = event.needsConsent
    // The size appears only when something named it. np 0.3.1 reports no artifact size anywhere, and §4
    // is explicit that a number in the app's own strings has to be a number somebody measured.
    const size = bytes === null || bytes === undefined ? '' : `, ${mebibytes(bytes)} MiB download`
    const model = modelId ? ` (${modelId})` : ''
    return { text: `semantic search needs the local model${model}${size}, downloaded to ${dataDir}`, tone: 'warn', action: 'consent' }
  }

  if ('refused' in event) {
    // np's sentence, unchanged (§11). It already names what to do; rewriting it loses that.
    return { text: event.refused.message, tone: 'warn' }
  }

  if ('disabled' in event) {
    const { key, modelId, hint } = event.disabled
    const why = key ? `${key} is not set` : modelId ? 'model not installed' : 'embedding is off in np’s configuration'
    return { text: `keyword-only: ${why}${hint ? ` — ${hint}` : ''}`, tone: 'warn', action: 'consent' }
  }

  // An event from a newer shell than this file knows about: say nothing rather than guess (§4). The
  // `[embed]` button stays, so the line is never empty in a way that looks broken.
  return null
}

function mebibytes(bytes: number): string {
  return Math.round(bytes / MIB).toString()
}
