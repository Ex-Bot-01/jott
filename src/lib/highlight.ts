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

export function markTerms(text: string, query: string): Segment[] {
  const ranges = matchRanges(text, query)
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
    which a vector-only hit can genuinely mean, and "no place to go" is truer than jumping to the top. */
export function firstMatch(text: string, query: string): number | null {
  const ranges = matchRanges(text, query)
  return ranges.length === 0 ? null : ranges[0].from
}

/** Every occurrence of every word in `query`, in document order and never overlapping. */
export function matchRanges(text: string, query: string): { from: number; to: number }[] {
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
      const found = haystack.indexOf(term, at)
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

function termsOf(query: string): string[] {
  return [...new Set(query.toLocaleLowerCase().split(/\s+/).filter((term) => term.length >= MIN_TERM))]
}
