// What the Settings panel says about note-processor's ranking model (np 0.7.0), from `RerankerStatus`.
//
// Kept out of the markup so each state has one sentence a test can hold to account — in particular the failure, which
// has to carry np's own words *and* say what a person can do about it, because the most likely cause measured so far
// is a network that inspects TLS: np's downloader refuses the proxy's certificate while a browser on the same machine
// gets the file in seconds.

import type { RerankerStatus } from './ipc'

const MB = 1024 * 1024

function mb(bytes: number): string {
  return `${Math.round(bytes / MB)} MB`
}

/** One line about where the ranking model stands, and whether the by-hand steps should be on screen. */
export function rerankerSummary(
  status: RerankerStatus | null,
  rerankOn: boolean,
  installing: boolean,
): { line: string; manual: boolean; retry: boolean } {
  if (status === null) return { line: 'Asking note-processor about the ranking model…', manual: false, retry: false }
  if (!status.supported) {
    return {
      line: `This note-processor (${status.npVersion}) has no ranking model — reranking needs 0.7.0 or newer. Search works as before.`,
      manual: false,
      retry: false,
    }
  }
  if (installing || status.busy === 'installing') {
    return { line: 'Installing the file you chose — note-processor is checking it against its pinned digest…', manual: false, retry: false }
  }
  if (status.busy === 'fetching' || status.downloading !== null) {
    const done = status.downloading?.bytesDone ?? null
    const total = status.downloading?.bytesTotal ?? status.manual.bytes
    const progress = done === null ? '' : ` ${mb(done)} of ${mb(total)}`
    return { line: `Downloading the ranking model…${progress}`, manual: false, retry: false }
  }
  if (status.present === true) return { line: 'Ranking model ready.', manual: false, retry: false }
  if (status.error !== null) {
    return {
      // np's words first. Then Jott's: the cause measured so far is a network that inspects encrypted traffic, which
      // np reports as "could not resolve the configured host" — wording that sends people to check their DNS.
      line:
        `note-processor could not download the ranking model: ${status.error}. ` +
        'On a network that inspects encrypted traffic (common at work), note-processor cannot fetch it itself — your browser can.',
      manual: true,
      retry: true,
    }
  }
  if (status.present === false) {
    return rerankOn
      ? { line: `The ranking model is not downloaded yet (${mb(status.manual.bytes)}).`, manual: true, retry: true }
      : { line: `Not downloaded (${mb(status.manual.bytes)}) — it is fetched when reranking is switched on.`, manual: false, retry: false }
  }
  return { line: 'Jott could not ask note-processor about the ranking model this time.', manual: false, retry: true }
}

/** The by-hand steps, numbered by the panel. Only np's pinned values: the file, its size, and the digest np checks. */
export function manualSteps(status: RerankerStatus): string[] {
  return [
    `Download ${status.manual.fileName} (${mb(status.manual.bytes)}) with your browser from the address below.`,
    'Choose the downloaded file here. note-processor copies it into this folder’s index and refuses it unless it is exactly the pinned file.',
  ]
}

/** The Advanced number field: what to show, and whether a typed value is one np will accept (1–64, never clamped). */
export function topNValue(status: RerankerStatus | null): number {
  return status?.topN ?? status?.topNDefault ?? 5
}

export function topNAccepted(n: number): boolean {
  return Number.isInteger(n) && n >= 1 && n <= 64
}
