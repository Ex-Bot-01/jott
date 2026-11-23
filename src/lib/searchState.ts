import { hitHalfPhrase } from './scoreLabel'

/**
 * What the search box knows that a `search_vault` answer does not: which request an answer belongs to, and what
 * the folder's state means about an empty list.
 */

/** Only the newest request may write to the screen (`Editor.svelte` learned this the hard way with its own counter). */
export function searchIsCurrent(seq: number, latest: number): boolean {
  return seq === latest
}

/** The four keys np 0.4.0 added to a `search` answer. Every one is optional on the wire: an older np simply does
 * not send them, and "did not say" must never be read as "said no". */
export type SearchFacts = {
  indexed?: boolean | null
  keywordUsed?: boolean | null
  semanticUsed?: boolean | null
  notice?: string | null
}

/** What an empty answer means, in one sentence a person can act on.
 *
 * np 0.4.0 answers `indexed`, which settles the question note-processor#5 left open: a folder that was never
 * scanned and a folder where the words do not appear are different situations and now arrive as different answers.
 * Jott quotes np's fact but keeps its own wording, because np's sentence is "run `np scan …`" and the person reading
 * this is holding a window with an "Index now" button in it.
 */
export function emptySearchSentence(facts: SearchFacts, query: string, notesAwaitingIndex: number): string {
  // An unscanned folder keeps Jott's own sentence even if np sent one. np's is "run `np scan …`", and the person
  // reading this line is holding a window with an *Index now* button: quoting a terminal command over the instruction
  // they can actually follow would be the worse service. Checked first, deliberately, so a note cannot displace it.
  if (facts.indexed === false) return notIndexedSentence(notesAwaitingIndex)
  // np 0.6.0 answered note-processor#11 by saying something: an empty meanings-only search now arrives with
  // `reason: "semantic-without-vectors"` and a sentence naming the command that would change it, where 0.5.1 answered
  // `"result": null`. §4 is quote np's words when it sends them — and printing "Nothing matched" over an explanation
  // Jott was just handed would replace a fact with a guess.
  const notice = facts.notice?.trim()
  if (notice) return notice
  if (facts.indexed === true) return `Nothing matched “${query}”.`
  // Nothing said: fall back to what the tree's own markers know rather than guessing from silence.
  return noResultsSentence(query, notesAwaitingIndex)
}

/** A quiet line under the results about what actually ran — or np's own sentence when it sent one.
 *
 * The header used to print np's `mode` word, which said `hybrid` for keyword-only searches until 0.4.0 fixed it.
 * `keywordUsed` / `semanticUsed` are the facts behind that word, and a person who got 3 hits out of a folder whose
 * meanings were never embedded should know why "that note about the Kestrel programme" is not among them.
 */
export function searchCaveat(facts: SearchFacts, hitCount = 1): string | null {
  if (hitCount === 0) return null // an empty list already gets a sentence that explains it
  const notice = facts.notice?.trim()
  if (notice) return notice
  if (facts.keywordUsed === true && facts.semanticUsed === false) {
    return 'words only · meanings not searched — this folder has no embeddings yet'
  }
  return null
}

function notIndexedSentence(notesAwaitingIndex: number): string {
  if (notesAwaitingIndex > 0) {
    const plural = notesAwaitingIndex === 1 ? '1 note is' : `${notesAwaitingIndex} notes are`
    return `This folder is not indexed yet (${plural} waiting), so search has nothing to look in. Use “Index now” in the sidebar.`
  }
  return 'This folder is not indexed yet, so search has nothing to look in. Use “Index now” in the sidebar.'
}

/** What an empty answer means when np did not say what it means — Jott's own inference from the tree's markers.
 *
 * Kept because "no `indexed` key" is the normal case for an older backend, and silence is not an answer: this
 * function reasons from notes waiting to be indexed, which Jott can see for itself.
 */
export function noResultsSentence(query: string, notesAwaitingIndex: number): string {
  if (notesAwaitingIndex > 0) {
    const plural = notesAwaitingIndex === 1 ? '1 note is' : `${notesAwaitingIndex} notes are`
    return `This folder is not indexed yet (${plural} waiting), so search has nothing to look in. Use “Index now” in the sidebar.`
  }
  return `Nothing matched “${query}”.`
}

/** The header above the hits: a count, and nothing else.
 *
 * It used to print np's `mode` word. np answers `hybrid` for a search where only keywords ran (note-processor#5),
 * which put a claim about vector search on screen that the bytes did not support. A count is the part we can say.
 */
export function resultsHeadline(hits: number): string | null {
  if (hits === 0) return null
  return hits === 1 ? '1 result' : `${hits} results`
}

/** What np widened, in its own words — printed beside the result count. `null` means say nothing: an older backend
    that has no flag stayed silent, and "did not widen" would be Jott answering for it (§4). */
export function wideningSentence(facts: { prefixUsed: boolean | null; prefixTerms: string[] }): string | null {
  if (facts.prefixUsed !== true) return null
  // `prefix_used` without terms is a real answer — np widened and did not say what — so the sentence gets vaguer
  // rather than borrowing a word from Jott's own query.
  const terms = facts.prefixTerms.filter((term) => term.trim().length > 0)
  return terms.length === 0 ? 'also matched word beginnings' : `also matched word beginnings (${terms.join(', ')})`
}

/** The mark beside a row that says which half of search reached it — `hitHalfPhrase` owns the wording so the visible
    suffix and the accessible name cannot drift apart (`scoreLabel.ts`).

    All three of np's answers now say something. `null` used to be silent on the reasoning that marking a meaning-only
    hit would "accuse the reader's typing of something nobody checked", but silence turned out to cost more: the row
    then looked identical to an exact-word match, and its meaning figure was the only number there to read. Measured in
    note-processor#19 how badly that misleads — four unrelated notes sit about 0.03 cosine apart for a one-word query. */
export function hitSuffix(hit: { matchedExactly: boolean | null; cosine: number | null }): string | null {
  return hitHalfPhrase(hit)
}

/** Should the empty answer offer word-beginning matching? Only in the one situation where pressing it can change
    the answer: np says this folder *is* indexed, the whole-word search found nothing, the query is something, and the
    person is not already widening — an offer that cannot help is noise dressed as help (§4). */
export function suggestPrefix(
  facts: { indexed: boolean | null; hits: unknown[]; keywordUsed?: boolean | null },
  query: string,
  prefixOn: boolean,
): boolean {
  if (prefixOn) return false
  if (query.trim().length === 0) return false
  if (facts.hits.length > 0) return false
  // Word beginnings are a fact about the *word* half of a search. np says which halves ran (0.4.0), so an answer that
  // came from meanings alone does not get advice about widening words — pressing it could not change anything, and
  // this module's contract is to offer it only where it can. Silence from an older backend is not evidence that
  // meanings-only was asked for, so `null` keeps the v0.0.15 behaviour.
  if (facts.keywordUsed === false) return false
  return facts.indexed === true
}

/** What a meanings-only search that returned nothing can honestly say.
 *
 *  note-processor#11: `mode:"semantic"` in a folder with no vectors answers `result: null` — no `indexed`, no
 *  `reason`, no `note`. Every other empty answer in this file has a fact behind its sentence; this one has the absence
 *  of one, which is exactly why it needs three branches instead of the friendly "nothing matched". A person who reads
 *  that in a folder they never embedded will conclude their notes are wrong rather than that the search had nothing to
 *  work with. */
export function semanticEmptySentence(facts: { vectors?: boolean | null }, query: string): string {
  if (facts.vectors === false) {
    return (
      `No notes matched “${query}” by meaning — this folder has no vectors yet, so a meanings-only search has ` +
      'nothing to compare against. Index it with embeddings, or ask for words as well.'
    )
  }
  if (facts.vectors === true) {
    return `No note in this folder matches “${query}” by meaning, as far as its vectors can tell.`
  }
  // No `vectors` fact to speak from, so the sentence names that rather than filling it in. Current core answers
  // `vectors` as a plain bool on every search result, so this is the shape of an answer Jott does not produce; np's
  // mute reply to a meanings-only search (`"result": null`, note-processor#11) never reaches here at all — it faults
  // in `search_answer` (crates/jott-core/src/np/outcomes.rs) and is quoted as a refusal.
  return (
    `Note-processor returned nothing for “${query}” and gave no reason — a meanings-only search reports no ` +
    'explanation when there are no vectors yet (note-processor#11).'
  )
}

/** What the window says about the ranking step under a result list. `null` means say nothing: reranking was not asked
 *  for, an older np has no stage (`rerank: null`), or there was nothing to reorder.
 *
 *  Jott's words rather than np's when the model is not ready: np's refusal names terminal commands (`np models
 *  install …`), and the person reading this has a Settings panel with a button for it — the same reasoning as
 *  `emptySearchSentence` keeping its own sentence for an unscanned folder. np's sentence travels in `title`. */
export function rerankSentence(
  rerank: { ran: boolean; reason: string | null; notice: string | null; candidates: number } | null,
  asked: boolean,
  pending: boolean,
): { text: string; title: string | null } | null {
  if (!asked) return null
  if (pending) return { text: 'reranking the top results…', title: null }
  if (rerank === null) return null
  if (rerank.ran) {
    const n = rerank.candidates
    return { text: n === 1 ? 'top result checked by the ranking model' : `top ${n} reordered by the ranking model`, title: null }
  }
  if (rerank.reason === 'no-candidates' || rerank.reason === 'disabled') return null
  return {
    text: 'not reranked — the ranking model is not ready (see Settings)',
    title: rerank.notice?.trim() || rerank.reason,
  }
}

/** A row the ranking model moved says where it came from — the reorder is otherwise invisible, and "why is this
 *  first?" has an answer. Rows it scored but left in place, and rows it never scored, say nothing. */
export function rerankMove(hit: { rerankRankBefore: number | null; rerankRankAfter: number | null }): string | null {
  const { rerankRankBefore: before, rerankRankAfter: after } = hit
  if (before === null || after === null || before === after) return null
  return `${after < before ? 'moved up' : 'moved down'} from #${before}`
}

/** The line under the *meanings only* choice, from np's own `vectors` fact about the last search.
 *
 *  Silence answers with silence: before anyone has searched, Jott does not know whether this folder has vectors, and
 *  "this folder has no vectors yet" would be a claim about a thing nobody asked about. */
export function meaningsNotice(vectors: boolean | null | undefined): string | null {
  if (vectors !== false) return null
  return (
    'This folder has no vectors yet, so meanings-only search will find nothing here. ' +
    'Index it with embeddings first, or keep both engines on.'
  )
}
