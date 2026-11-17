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

/** The tooltip's first sentence for this row: what the number beside it actually measured. */
export function scoreTitleFor(hit: { score: number; cosine: number | null }): string {
  switch (scoreMeaning(hit)) {
    case 'similarity':
      return SIMILARITY_TITLE
    case 'keyword':
      return KEYWORD_TITLE
    case 'fused':
      return SCORE_TITLE
  }
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
export function rowLabel(row: { path: string; snippet: string | null; score: number; cosine: number | null }): string {
  const parts = [row.path]
  const snippet = row.snippet === null ? '' : stripMarks(row.snippet).trim()
  if (snippet !== '') parts.push(snippet)
  // Same rule as the visible row: one number per measurement, and a phrase that says which kind of figure it was.
  const figures = rowFigures(row)
  const shown = [figures.rank, figures.meaning].filter((piece): piece is string => piece !== null).join(', ')
  parts.push(shown === '' ? 'no figure' : shown)
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
