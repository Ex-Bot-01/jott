// Search is the one place where the window waits on np while the person keeps typing, so it owns two decisions
// that used to be made inline: which answer is still current, and what an empty answer means.

import { describe, expect, it } from 'vitest'

import {
  emptySearchSentence,
  noResultsSentence,
  resultsHeadline,
  searchCaveat,
  searchIsCurrent,
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
