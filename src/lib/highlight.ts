/**
 * Where the words you searched for are.
 *
 * np answers a search with a snippet and no offsets (§5), and its `[` `]` marks are not escaped, so they are
 * shown as text and the window works out its own highlighting (§18.11). Both halves of that — the marks in a
 * result row and the scroll-to-passage in the editor — come from here, so a hit's list and the note it opens
 * cannot disagree about what matched.
 */

export type Segment = { text: string; matched: boolean }

/** A query shorter than this is not highlighted. One keystroke matches half the vault, and painting every `a`
    in a note reads as a rendering fault rather than as an answer. np's own scorer wants two characters too. */
const MIN_TERM = 2

export function markTerms(text: string, query: string, prefix = false): Segment[] {
  const ranges = matchRanges(text, query, prefix)
  if (ranges.length === 0) return [{ text, matched: false }]
  const segments: Segment[] = []
  let at = 0
  for (const range of ranges) {
    if (range.from > at) segments.push({ text: text.slice(at, range.from), matched: false })
    segments.push({ text: text.slice(range.from, range.to), matched: true })
    at = range.to
  }
  if (at < text.length) segments.push({ text: text.slice(at), matched: false })
  return segments
}

/** Where to scroll an editor so the reader lands on the answer. `null` means the words are not in this note —
    which a vector-only hit can genuinely mean, and "no place to go" is truer than jumping to the top.
    `prefix` must be what the search was run with, or the row and the opened note disagree about what matched. */
export function firstMatch(text: string, query: string, prefix = false): number | null {
  const ranges = matchRanges(text, query, prefix)
  return ranges.length === 0 ? null : ranges[0].from
}

/** Every occurrence of every word in `query`, in document order and never overlapping. */
export function matchRanges(text: string, query: string, prefix = false): { from: number; to: number }[] {
  const terms = termsOf(query)
  if (terms.length === 0 || text.length === 0) return []

  // Folded once, up front: the search runs over the folded copy and reports offsets into the original, which
  // is why `toLocaleLowerCase` on the whole string is safe here — it preserves length for every case mapping
  // this corpus can produce (ASCII markdown, and accented letters that fold to a same-count sequence).
  const haystack = text.toLocaleLowerCase()
  const ranges: { from: number; to: number }[] = []
  let at = 0
  while (at < haystack.length) {
    let best: { from: number; to: number } | null = null
    for (const term of terms) {
      const found = nextOccurrence(haystack, term, at, prefix)
      if (found === -1) continue
      // Earliest wins; on a tie the longer word does, so `kumquat` beats the `ku` of another term and the
      // mark covers the thing the reader searched for.
      if (best === null || found < best.from || (found === best.from && term.length > best.to - best.from)) {
        best = { from: found, to: found + term.length }
      }
    }
    if (best === null) break
    ranges.push(best)
    // Past this match, and never back into it: two terms sharing letters must not highlight the same span twice.
    at = Math.max(best.to, best.from + 1)
  }
  return ranges
}

/** The next occurrence of `term` at or after `from`, and in prefix mode the next one that **begins a word**.
    Word-beginning is np's rule for widening (note-processor#6), so the window marks exactly what np matched: `grand`
    lights up in *grandmother* and stays dark in *agrand*. */
function nextOccurrence(haystack: string, term: string, from: number, prefix: boolean): number {
  let at = haystack.indexOf(term, from)
  while (at !== -1 && prefix && !startsWithWord(haystack, at)) at = haystack.indexOf(term, at + 1)
  return at
}

function startsWithWord(haystack: string, at: number): boolean {
  if (at === 0) return true
  const before = haystack[at - 1]!
  // Underscore counts: a vault full of `note_processor_run` identifiers is exactly where widening does harm, and
  // treating `_` as a word character is what keeps `grand` from matching inside `my_grandmother`.
  return !/[\p{L}\p{N}_]/u.test(before)
}

function termsOf(query: string): string[] {
  return [...new Set(query.toLocaleLowerCase().split(/\s+/).filter((term) => term.length >= MIN_TERM))]
}
