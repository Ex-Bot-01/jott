/**
 * Whether a model download is happening, decided from np's fetch markers.
 *
 * The tester's second report was blunt and correct: a 295 MB download ran with nothing on screen to say so. Two
 * reasons, both fixed here or nearby — the event stream died under a burst (`ipc.rs`, `bridge_stays`), and even a
 * live progress line only paints while it is arriving. np keeps a different fact: a marker file per in-flight
 * fetch, which `models.list` reports with bytes counted so far. Polling that costs one local pipe round trip and
 * cannot be missed by a slow paint.
 *
 * One caution decides the shape of this file. **A marker outlives the process that wrote it** — measured: kill np
 * mid-fetch and `models.list` still answers with the last byte count, forever, until np's own housekeeping runs.
 * So "np has a marker" is not "bytes are moving", and a banner that never went away would teach people to
 * distrust the one light meant to mean *wait for it*. Movement decides. A count that stops advancing is read as a
 * leftover, said nothing about, and asked about again later.
 */

/** One of np's markers, as `EmbeddingsStatus.downloading` delivers it (`crates/jott-core/src/ipc.rs`). */
export interface Downloading {
  modelId: string
  /** Bytes np had counted when it answered. Not a rate, not a prediction. */
  bytesDone: number
  /** The size the transfer announced, when it announced one. `null` is unknown, never zero. */
  bytesTotal: number | null
}

/** What the last poll saw. Kept outside Svelte state so the rule below is testable on its own. */
export interface WatchState {
  /** The byte count from the previous reading, or `null` if np reported no marker then. */
  lastBytes: number | null
  /** Readings in a row where the count did not move. */
  quietReads: number
}

/** How many quiet readings to forgive before calling a marker leftover. Two is about three seconds at this
    window's polling rate: long enough for a slow link and a busy disk, short enough that a stale marker stops
    bothering somebody before they finish reading the banner once. */
export const QUIET_LIMIT = 2

export const initialWatch: WatchState = { lastBytes: null, quietReads: 0 }

/**
 * One poll's decision: what to remember, and whether to say anything.
 *
 * The first reading of a marker is shown. That is a deliberate trade: a leftover marker flashes the banner for a
 * few seconds, which is mildly annoying, while hiding every first reading would hide real downloads that start and
 * finish between two polls — the exact complaint this file exists to answer.
 */
export function readMarker(prev: WatchState, marker: Downloading | null): { state: WatchState; show: boolean } {
  if (marker === null) return { state: initialWatch, show: false }
  if (prev.lastBytes !== null && marker.bytesDone > prev.lastBytes) {
    return { state: { lastBytes: marker.bytesDone, quietReads: 0 }, show: true }
  }
  // Either the first look at this marker, or a count that did not move.
  // The first look is not a quiet reading — nothing was compared. Counting it as one would cut the tolerance short
  // and clear a real download's banner after a single slow poll.
  if (prev.lastBytes === null) return { state: { lastBytes: marker.bytesDone, quietReads: 0 }, show: true }
  const quietReads = prev.quietReads + 1
  // Keep the remembered count so the next reading is compared against something real.
  return { state: { lastBytes: marker.bytesDone, quietReads }, show: quietReads <= QUIET_LIMIT }
}

/** The sentence on the banner. np's model id, np's bytes, and no invented speed or finish time — a bar that
    promises 14 seconds and takes four minutes costs more trust than a bar that says nothing. */
export function downloadSentence(marker: Downloading): string {
  // Both shapes carry the unit. "· 4" is not a sentence about bytes, and "of" needs a second number to be true to:
  // with no total from np there is nothing to be *of*, so the count stands alone.
  const soFar = mebibytes(marker.bytesDone)
  return marker.bytesTotal === null
    ? `downloading ${marker.modelId} · ${soFar} MiB`
    : `downloading ${marker.modelId} · ${soFar} of ${mebibytes(marker.bytesTotal)} MiB`
}

/** Whole MiB, rounded down. Fractions would imply a precision np's counter does not have between two polls. */
function mebibytes(bytes: number): string {
  return String(Math.floor(bytes / (1024 * 1024)))
}
