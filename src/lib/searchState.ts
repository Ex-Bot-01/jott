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
  if (facts.indexed === false) return notIndexedSentence(notesAwaitingIndex)
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

/** The mark beside a row that np reached by a word beginning. Only `false` gets one: `true` is an ordinary match, and
    `null` is a hit np never word-tested — a meaning-only result has no word to compare against, so marking it would
    accuse the reader's typing of something nobody checked. */
export function hitSuffix(matchedExactly: boolean | null): string | null {
  return matchedExactly === false ? 'from the start of a word' : null
}

/** Should the empty answer offer word-beginning matching? Only in the one situation where pressing it can change
    the answer: np says this folder *is* indexed, the whole-word search found nothing, the query is something, and the
    person is not already widening — an offer that cannot help is noise dressed as help (§4). */
export function suggestPrefix(
  facts: { indexed: boolean | null; hits: unknown[] },
  query: string,
  prefixOn: boolean,
): boolean {
  if (prefixOn) return false
  if (query.trim().length === 0) return false
  if (facts.hits.length > 0) return false
  return facts.indexed === true
}
