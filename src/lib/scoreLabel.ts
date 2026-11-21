/**
 * The number beside a search result, spelled the way note-processor means it.
 *
 * Measured against `np 0.5.1` over JSON-RPC, a hit carries `score`, `bm25`, `cosine`, `keyword_rank` and
 * `semantic_rank`. `score` is what ordered the list: a reciprocal-rank fusion of the keyword and meaning rankings, so a
 * keyword hit came back around `0.0164` and a meaning-only one near `0.008`. It is not a similarity, not a percentage,
 * and has no useful interpretation as either — but it *is* the reason this row is above that one, which is what someone
 * comparing two results is reaching for. So the row prints np's own number unchanged in magnitude, and says next to it
 * what it is.
 *
 * `cosine` is a real similarity, and it is `null` whenever vectors did not run. Printing `0` for those rows would claim
 * every keyword hit has no meaning-similarity at all — an invented fact about a thing that was never measured — so the
 * label appears only when np sent a number.
 */

/** Shown as the row's tooltip when both halves ran. The last clause is the point: every search UI is tempted to turn
 *  this into "87% similar". */
export const SCORE_TITLE =
  'note-processor’s fused rank score — keyword ranking and meaning ranking combined. Higher means nearer the top of its list; it is not a percentage.'

/** The tooltip when only vectors ran, which is also the case where np's `score` *is* the cosine. */
const SIMILARITY_TITLE =
  'how similar this note is to your query — in meanings-only search this is the number note-processor ordered the list by, so it appears once and not twice.'

/** The tooltip when only words ran: np's score there is SQLite's own keyword figure with its sign flipped. */
const KEYWORD_TITLE =
  'how strongly this note matched your words — note-processor’s keyword ranking (SQLite’s bm25, flipped so a larger number sits nearer the top). Not a percentage.'

/** Which half of search reached this row, in words a person can act on.
 *
 *  np answers `matched_exactly` three ways and the third one used to print nothing at all: `true` (the typed word is
 *  in the bytes), `false` (np widened to a word beginning) and `null` (**this row was never word-tested** — it came
 *  from the meanings list). Drawn without a mark, a meaning-only row looks exactly like an exact-word match, with its
 *  `meaning 0.72` as the only figure on it, which is how a tester came to compare two rows that were never comparable:
 *  measured in note-processor#19, four unrelated notes sit within about 0.03 cosine of a one-word query. The row now
 *  says which half produced it, so the figures stop being asked to settle an argument they cannot (issue #25).

 *  The `null` case is decided from **two** of np's fields rather than one, because "no word test" and "this build has
 *  no word-test flag" arrive as the same absence; see the body.
 */
export function hitHalfPhrase(hit: { matchedExactly: boolean | null; cosine: number | null }): string | null {
  if (hit.matchedExactly === false) return 'from the start of a word'
  // The claim needs np's own evidence, not just the missing flag: `matched_exactly: null` *and* a cosine both mean
  // this row came out of the vector list. Absence alone would speak for a backend that never had the flag at all —
  // Jott refuses those below 0.5.1, but a window should not be in the business of concluding things from silence (§4).
  if (hit.matchedExactly === null && hit.cosine !== null && Number.isFinite(hit.cosine)) return 'by meaning only'
  return null
}

/** Which measurement np's `score` holds for this row, read off what np itself sent.
 *
 *  Note-processor assigns the field three different ways (`search.rs:1226-1231`): a fused rank in hybrid search, the
 *  cosine when only vectors ran, and `-bm25` when only words ran. A single label for all three would be a claim about
 *  a measurement nobody took — and in the second case it put the same number on screen twice, which is how a tester
 *  came to report *"two scores, two identical scores."*
 */
export type ScoreMeaning = 'fused' | 'similarity' | 'keyword'

export function scoreMeaning(hit: { score: number; cosine: number | null }): ScoreMeaning {
  if (hit.cosine === null) return 'keyword'
  // np assigns the same float, so this is an exact comparison in practice. Tolerance only guards against a backend
  // that rounds on the way through JSON — and it is deliberately tiny, because two measurements that merely *print*
  // alike (0.8274 and 0.8273) must stay two figures on the row.
  if (Math.abs(hit.score - hit.cosine) <= Math.abs(hit.score) * 1e-9) return 'similarity'
  return 'fused'
}

/** What the meaning figures look like in practice, quoted with its provenance rather than as a rule.
 *
 *  Recorded here on note-processor 0.6.0 and re-measured byte for byte on 0.6.1 with the pinned `embeddinggemma-2`:
 *  four unrelated notes (a birthday table, a recipe, a shopping list, three lines of chatter) all sat between 0.63 and
 *  0.78 similar to any query in the same language, and a search for one word left about 0.03 between them — the
 *  three-line note came back at 0.7195 against the family calendar's 0.7140. Two words separated the same notes by more
 *  than 0.05. `note-processor#19` carries the table. This is one model's measured behaviour, not a threshold Jott
 *  applies: it travels in a tooltip, where it can inform a reading of np's own figure without becoming a ranking
 *  nobody validated.
 *
 *  The pair is quoted from one index, and a second index of nearly the same notes put the calendar four thousandths
 *  elsewhere. That drift is not a reason to drop the sentence — it is a second example of the thing the sentence says.
 */
const MEANING_BAND =
  ' · what these meaning figures look like: measured on note-processor 0.6.0 with embeddinggemma-2 over four unrelated notes, every one sat between about 0.63 and 0.78 similar to any query in the same language, and a search for one word left roughly 0.03 between them — differences in the hundredths are not a ranking to act on.'

/** The tooltip's first sentence for this row: what the number beside it actually measured. */
export function scoreTitleFor(hit: { score: number; cosine: number | null }): string {
  const title =
    scoreMeaning(hit) === 'similarity' ? SIMILARITY_TITLE : scoreMeaning(hit) === 'keyword' ? KEYWORD_TITLE : SCORE_TITLE
  // Only rows where vectors ran draw a meaning figure, so only those rows explain it. A keyword-only row gets the band
  // on no account: saying "0.63 to 0.78" beside a figure that is bm25 would be Jott confusing two measurements.
  return hit.cosine === null || !Number.isFinite(hit.cosine) ? title : title + MEANING_BAND
}

/** The figures to draw beside a row, with one measurement never drawn twice.
 *
 *  `rank` is null exactly when it would repeat the similarity figure. The visible face keeps one number either way;
 *  which number it is stays in the tooltip and the accessible name rather than being guessed at by the reader.
 */
export function rowFigures(hit: { score: number; cosine: number | null }): { rank: string | null; meaning: string | null } {
  const meaning = meaningLabel(hit.cosine)
  if (scoreMeaning(hit) === 'similarity') return { rank: null, meaning }
  return { rank: formatScore(hit.score), meaning }
}

/** Three significant digits, because two would print every keyword hit as `0.02` and say nothing. */
export function formatScore(score: number): string {
  if (!Number.isFinite(score)) return '—' // np said something we cannot read; a dash is honest, a 0 is not
  if (score === 0) return '0'
  return String(Number(score.toPrecision(3)))
}

/** `meaning 0.83` when note-processor compared vectors for this hit, and nothing at all when it did not. */
export function meaningLabel(cosine: number | null): string | null {
  if (cosine === null || !Number.isFinite(cosine)) return null
  return `meaning ${cosine.toFixed(2)}`
}

/** The plain text of np's snippet: its `[` `]` marks are for the eye, and reading them aloud is noise. */
function stripMarks(snippet: string): string {
  return snippet.replaceAll('[', '').replaceAll(']', '')
}

/**
 * What a result row says out loud. The visible row is path, snippet, then two numbers; a screen reader given only that
 * hears "0.0164" and reasonably files it under scores-to-compare — the exact misreading the tooltip exists to prevent,
 * except a tooltip is a mouse affordance. So the explanation travels in the accessible name too, and the meaning figure
 * appears there only when np sent one.
 */
export function rowLabel(row: {
  path: string
  snippet: string | null
  score: number
  cosine: number | null
  matchedExactly?: boolean | null
}): string {
  const parts = [row.path]
  const snippet = row.snippet === null ? '' : stripMarks(row.snippet).trim()
  if (snippet !== '') parts.push(snippet)
  // Same rule as the visible row: one number per measurement, and a phrase that says which kind of figure it was.
  const figures = rowFigures(row)
  const shown = [figures.rank, figures.meaning].filter((piece): piece is string => piece !== null).join(', ')
  parts.push(shown === '' ? 'no figure' : shown)
  // `aria-label` replaces the row's own text for a screen reader, so the visible suffix has to be said here as well or
  // the explanation of which half answered exists only for somebody with a mouse. One wording, from one place.
  const half = hitHalfPhrase({ matchedExactly: row.matchedExactly ?? null, cosine: row.cosine })
  if (half !== null) parts.push(half)
  parts.push(
    scoreMeaning(row) === 'similarity'
      ? 'similarity, not a percentage'
      : scoreMeaning(row) === 'keyword'
        ? 'keyword ranking, not a percentage'
        : 'rank score, not a percentage',
  )
  return parts.join(' — ')
}

/**
 * np's other figure for this row, in the tooltip instead of on the face of the result.
 *
 * A hit also carries `bm25` — SQLite FTS5's own keyword score, which is the TF-IDF family with length normalisation
 * and term-frequency saturation added. It is a real measurement and it is useless to compare by eye: negative is
 * better, its scale depends on how many notes mention the word, and two rows with the same words in them can differ
 * by orders of magnitude. A second number on every row would be decoration that invites exactly the wrong reading,
 * so it appears where the score's own explanation already lives.
 *
 * `null` means that half did not run — a meaning-only hit has no bm25, a keyword-only one no cosine — and nothing is
 * said about a measurement nobody took.
 */
export function figuresNote(hit: { bm25: number | null; cosine: number | null }): string {
  const parts: string[] = []
  if (hit.bm25 !== null && Number.isFinite(hit.bm25)) parts.push(`bm25 ${formatScore(hit.bm25)}`)
  if (hit.cosine !== null && Number.isFinite(hit.cosine)) parts.push(`cosine ${hit.cosine.toFixed(4)}`)
  if (parts.length === 0) return ''
  // The sign is kept because it carries the meaning: in bm25 as SQLite reports it, nearer zero is worse.
  return ` note-processor also measured ${parts.join(' and ')}. Lower (more negative) bm25 means a weaker keyword match; neither figure is a percentage.`
}
