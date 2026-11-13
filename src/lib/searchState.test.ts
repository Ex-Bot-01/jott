// Search is the one place where the window waits on np while the person keeps typing, so it owns two decisions
// that used to be made inline: which answer is still current, and what an empty answer means.

import { describe, expect, it } from 'vitest'

import { noResultsSentence, resultsHeadline, searchIsCurrent } from './searchState'

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
