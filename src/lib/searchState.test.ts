// Search is the one place where the window waits on np while the person keeps typing, so it owns two decisions
// that used to be made inline: which answer is still current, and what an empty answer means.

import { describe, expect, it } from 'vitest'

import {
  emptySearchSentence,
  hitSuffix,
  meaningsNotice,
  noResultsSentence,
  resultsHeadline,
  searchCaveat,
  searchIsCurrent,
  semanticEmptySentence,
  suggestPrefix,
  wideningSentence,
} from './searchState'

describe('which answer is still current', () => {
  it('lets only the newest request write', () => {
    // Two keystrokes, two requests; np answers the first one late. Without this, the older list lands on top.
    expect(searchIsCurrent(1, 2)).toBe(false)
    expect(searchIsCurrent(2, 2)).toBe(true)
  })
})

describe('an empty answer', () => {
  it('names the reason a folder cannot answer at all', () => {
    // np#5: an unindexed vault and a vault with no matches look identical from `search` — both are an empty list.
    // Jott knows the folder's state, so it says which of the two this is instead of implying the words are wrong.
    const sentence = noResultsSentence('kumquat', 3)
    expect(sentence).toContain('not indexed yet')
    expect(sentence).toContain('3 notes')
  })

  it('says nothing matched only when the folder could have answered', () => {
    expect(noResultsSentence('kumquat', 0)).toBe('Nothing matched “kumquat”.')
  })
})

describe('the results header', () => {
  it('counts hits and claims no mode', () => {
    // np 0.3.1 answers `mode: hybrid` for a search where only keywords ran (filed as note-processor#5), so the
    // word on screen would be a promise about vectors that did not exist. A count cannot lie.
    expect(resultsHeadline(2)).toBe('2 results')
    expect(resultsHeadline(1)).toBe('1 result')
    expect(resultsHeadline(0)).toBeNull()
  })
})

describe('what np itself said about the answer (0.4.0)', () => {
  it('believes np over its own guess when np says the folder was never indexed', () => {
    // note-processor#5, fixed in 0.4.0: `indexed` distinguishes the two empties. When np answers, Jott quotes that
    // rather than reasoning from tree markers — and still says what to do next, because np's own sentence names a
    // command line nobody is holding.
    const sentence = emptySearchSentence({ indexed: false }, 'kumquat', 0)
    expect(sentence).toContain('not indexed yet')
    expect(sentence).toContain('Index now')
    expect(sentence).not.toContain('Nothing matched')
  })

  it('says nothing matched when np knew there was an index to search', () => {
    // The same words from a folder with 4,000 notes and a folder with none used to be indistinguishable. Now the
    // difference is np's answer, not our inference — and the local guess must not override it.
    expect(emptySearchSentence({ indexed: true }, 'kumquat', 12)).toBe('Nothing matched “kumquat”.')
  })

  it('falls back to what the tree knows when an older np says nothing', () => {
    // `undefined` is not `false`: a 0.3.x answer has no such key, and claiming "not indexed" from silence would be
    // inventing a diagnosis. Jott falls back to its own markers instead.
    expect(emptySearchSentence({}, 'kumquat', 2)).toContain('not indexed yet')
    expect(emptySearchSentence({}, 'kumquat', 0)).toBe('Nothing matched “kumquat”.')
    expect(emptySearchSentence({ indexed: null }, 'kumquat', 3)).toContain('not indexed yet')
  })
})

describe('the quiet line about what ran', () => {
  it('says so when only the words were searched', () => {
    // np's `mode` word said `hybrid` here. These two flags are the fact: keywords ran, vectors did not.
    expect(searchCaveat({ keywordUsed: true, semanticUsed: false })).toContain('words only')
  })

  it('stays quiet when both halves answered, and when np did not say', () => {
    expect(searchCaveat({ keywordUsed: true, semanticUsed: true })).toBeNull()
    expect(searchCaveat({ keywordUsed: null, semanticUsed: null })).toBeNull()
    // A 0.3.x answer is not a claim that something went wrong.
    expect(searchCaveat({})).toBeNull()
  })

  it('prefers np’s own sentence when it sent one', () => {
    // e.g. "fetched embeddinggemma-2 while searching": somebody's model download, said in np's words (§4).
    expect(searchCaveat({ notice: 'the local model was fetched during this search' })).toBe(
      'the local model was fetched during this search',
    )
  })

  it('is not shown for an empty list, where the sentence above already explains', () => {
    expect(searchCaveat({ keywordUsed: true, semanticUsed: false }, 0)).toBeNull()
  })
})

describe('what np widened', () => {
  it('names the words note-processor said it widened', () => {
    // note-processor#6: `grand` reaching *grandmother* is a different search from the one that was typed, and the
    // header is where that gets admitted. The words come from np's `prefix_terms`, so the sentence cannot promise a
    // widening np did not report.
    const sentence = wideningSentence({ prefixUsed: true, prefixTerms: ['grand', 'kestr'] })
    expect(sentence).toContain('word beginnings')
    expect(sentence).toContain('grand')
    expect(sentence).toContain('kestr')
  })

  it('says nothing when np did not widen', () => {
    expect(wideningSentence({ prefixUsed: false, prefixTerms: [] })).toBeNull()
  })

  it('says nothing when an older np did not say', () => {
    // `null` is "this build has no flag", and printing "did not widen" would be Jott speaking for a backend that
    // stayed quiet — the same mistake §4 refuses elsewhere.
    expect(wideningSentence({ prefixUsed: null, prefixTerms: [] })).toBeNull()
  })

  it('admits the widening even when np did not name the words', () => {
    // `prefix_used: true` with an empty term list is a real shape: something was widened and np would not say what.
    // The sentence stays truthful and vaguer — inventing a word here would put Jott's guess in np's mouth.
    const sentence = wideningSentence({ prefixUsed: true, prefixTerms: [] })
    expect(sentence).toContain('word beginnings')
    expect(sentence).not.toMatch(/grand/)
  })

  it('ignores terms np sent without saying it widened anything', () => {
    // Both keys have to agree before the header claims a widening.
    expect(wideningSentence({ prefixUsed: false, prefixTerms: ['grand'] })).toBeNull()
  })
})

describe('a row reached by a word beginning', () => {
  it('is marked when np says it was not an exact match', () => {
    expect(hitSuffix({ matchedExactly: false, cosine: null })).toContain('start of a word')
  })

  it('is unmarked when it matched what was typed', () => {
    expect(hitSuffix({ matchedExactly: true, cosine: null })).toBeNull()
  })

  it('says so when np never word-tested it, rather than staying silent', () => {
    // A vector-only hit has no words to compare, so "start of a word" would accuse the reader's typing of something
    // nobody tested — that part of the old reasoning stands. What did not stand was drawing no mark at all: the row
    // then read as an exact-word match, and its `meaning 0.72` was the only figure on it. Measured in
    // note-processor#19 how far apart those two rows really are (issue #25).
    const suffix = hitSuffix({ matchedExactly: null, cosine: 0.7195 })
    expect(suffix).not.toBeNull()
    expect(suffix).not.toContain('start of a word')
    expect(suffix).toContain('by meaning')
  })

  it('claims nothing when np sent neither a word test nor a vector figure', () => {
    // Two absences are not a finding. A backend without the flag at all answers `matched_exactly: null` for its
    // keyword rows too, and Jott refuses those builds today — but "by meaning only" would still be a claim about a
    // measurement nobody reported, so the row stays unmarked.
    expect(hitSuffix({ matchedExactly: null, cosine: null })).toBeNull()
  })
})

describe('offering the widening', () => {
  const empty = { indexed: true as boolean | null, hits: [] as unknown[] }

  it('offers it when a folder that could answer said nothing', () => {
    // np says this workspace is indexed, the whole-word search came back empty, and the person is not already
    // widening: that is the one moment where "try word beginnings" is a suggestion rather than a shrug.
    expect(suggestPrefix(empty, 'grand', false)).toBe(true)
  })

  it('does not offer it when np says the folder has never been indexed', () => {
    // Nothing to widen yet. Offering the toggle here would teach somebody to press a button that cannot help.
    expect(suggestPrefix({ indexed: false, hits: [] }, 'grand', false)).toBe(false)
  })

  it('does not offer it when np did not say', () => {
    // An older backend's silence is not evidence that an index exists.
    expect(suggestPrefix({ indexed: null, hits: [] }, 'grand', false)).toBe(false)
  })

  it('does not offer it when something matched', () => {
    expect(suggestPrefix({ indexed: true, hits: ['a.md'] }, 'grand', false)).toBe(false)
  })

  it('does not offer it on a blank query', () => {
    for (const query of ['', '   ', '\t\n']) {
      expect(suggestPrefix(empty, query, false), JSON.stringify(query)).toBe(false)
    }
  })

  it('does not offer it when word beginnings are already on', () => {
    // The empty answer already came from widening. Saying "try widening" would be the window arguing with itself.
    expect(suggestPrefix(empty, 'grand', true)).toBe(false)
  })
})

describe('a meanings-only search that found nothing', () => {
  // note-processor#11: `mode:"semantic"` in a folder with no vectors answers `result: null` — no `indexed`, no
  // `reason`, no sentence. The window is left holding the question "nothing matched, or nothing to match against?",
  // and the one thing it must not do is pick the flattering reading.

  it('says there was nothing to match against when np says there are no vectors', () => {
    const sentence = semanticEmptySentence({ vectors: false }, 'grandmother')
    expect(sentence.toLowerCase()).toContain('no vectors')
    // "Nothing matched" alone would read as "your notes do not mention this", which is a different claim entirely.
    expect(sentence.toLowerCase()).not.toContain('nothing matched')
  })

  it('says what it did search when np says vectors exist', () => {
    const sentence = semanticEmptySentence({ vectors: true }, 'grandmother')
    expect(sentence).toContain('grandmother')
    expect(sentence.toLowerCase()).toContain('meaning')
  })

  it('admits the backend said nothing when it said nothing', () => {
    // `vectors: null` is np's silence, and the honest sentence names that silence rather than filling it in.
    const sentence = semanticEmptySentence({ vectors: null }, 'grandmother')
    expect(sentence.toLowerCase()).toContain('no reason')
    expect(sentence.toLowerCase()).not.toContain('nothing matched')
  })

  it('names the query it could not answer', () => {
    for (const facts of [{ vectors: true }, { vectors: false }, { vectors: null }]) {
      expect(semanticEmptySentence(facts, 'kumquat')).toContain('kumquat')
    }
  })
})

describe('the warning under the meanings-only choice', () => {
  it('says nothing where vectors are known to exist', () => {
    expect(meaningsNotice(true)).toBeNull()
  })

  it('warns where np says there are no vectors', () => {
    const notice = meaningsNotice(false)
    expect(notice).not.toBeNull()
    expect(notice!.toLowerCase()).toContain('no vectors')
    // It has to say what would help, not just that something is missing.
    expect(notice!.toLowerCase()).toContain('index')
  })

  it('says nothing when nobody has asked np yet', () => {
    // No fact, no claim: "this folder has no vectors" from silence would be the invention this whole file exists to stop.
    expect(meaningsNotice(null)).toBeNull()
    expect(meaningsNotice(undefined)).toBeNull()
  })
})

describe('the widening offer needs evidence that words were tried', () => {
  it('does not offer it when no keyword search ran', () => {
    // "Try matching word beginnings" is advice about the *word* half of a search. In a meanings-only answer nothing
    // tested words at all, so pressing it cannot change the answer — and this module's own contract is to offer it
    // only where pressing it can. np said which halves ran; use that rather than assume both did.
    expect(suggestPrefix({ indexed: true, hits: [], keywordUsed: false }, 'grand', false)).toBe(false)
  })

  it('still offers it when keywords ran and came back empty', () => {
    expect(suggestPrefix({ indexed: true, hits: [], keywordUsed: true }, 'grand', false)).toBe(true)
  })

  it('offers it when an older backend did not say which halves ran', () => {
    // Silence is not evidence that meanings-only was asked for; the v0.0.15 behaviour stands unless np says otherwise.
    expect(suggestPrefix({ indexed: true, hits: [], keywordUsed: null }, 'grand', false)).toBe(true)
  })
})

describe('what an empty meanings-only answer says on np 0.6.0', () => {
  // note-processor#11 was fixed in np 0.6.0 by *saying something*: `mode:"semantic"` with the artifact installed and
  // nothing embedded now answers a document carrying `reason: "semantic-without-vectors"` and a sentence of its own,
  // where 0.5.1 answered `"result": null`. Jott already prefers np's sentence when it sends one (that is §4, and
  // `consentHelp.ts` takes the same posture) — but an empty list used to be exempt, so the upgrade would have traded
  // a real explanation for "Nothing matched", which is Jott contradicting an answer it was just given.
  const npSentence =
    'no vectors in this workspace: a meanings-only search has nothing to rank and no keyword half to fall back on — `np models fetch` or `np embed` adds the semantic half'

  it('prints np’s own sentence instead of guessing when it sent one', () => {
    expect(emptySearchSentence({ indexed: true, notice: npSentence }, 'kestrel', 0)).toBe(npSentence)
  })

  it('still keeps its own wording for a folder that was never scanned', () => {
    // np's sentence for that state is "run `np scan …`", and the person reading this line is holding a window with an
    // *Index now* button in it. A note must not cost them the instruction they can actually follow.
    const sentence = emptySearchSentence({ indexed: false, notice: 'index the folder first' }, 'kestrel', 3)
    expect(sentence).toContain('not indexed yet')
    expect(sentence).toContain('Index now')
  })

  it('says nothing matched when np said nothing', () => {
    expect(emptySearchSentence({ indexed: true }, 'kestrel', 0)).toBe('Nothing matched “kestrel”.')
  })
})

