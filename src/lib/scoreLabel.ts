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

/** Shown as the row's tooltip. The last clause is the point: every search UI is tempted to turn this into "87% similar". */
export const SCORE_TITLE =
  'note-processor’s fused rank score — keyword ranking and meaning ranking combined. Higher means nearer the top of its list; it is not a percentage.'

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
  const meaning = meaningLabel(row.cosine)
  parts.push(`${formatScore(row.score)}${meaning === null ? '' : `, ${meaning}`}`)
  parts.push('rank score, not a percentage')
  return parts.join(' — ')
}
