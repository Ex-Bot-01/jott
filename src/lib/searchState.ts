/**
 * What the search box knows that a `search_vault` answer does not: which request an answer belongs to, and what
 * the folder's state means about an empty list.
 */

/** Only the newest request may write to the screen (`Editor.svelte` learned this the hard way with its own counter). */
export function searchIsCurrent(seq: number, latest: number): boolean {
  return seq === latest
}

/** What an empty answer means, in one sentence a person can act on.
 *
 * np 0.3.1 cannot tell these two apart (filed as note-processor#5): a folder that was never indexed and a folder
 * where the words genuinely do not appear both come back as an empty list with no reason. "Nothing matched" sent
 * people re-typing a search that could never have answered, so Jott says which case it is from what it knows
 * locally — the tree's own markers.
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
