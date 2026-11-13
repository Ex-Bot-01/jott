// Where a search hit actually is. np sends a snippet with its own `[` `]` marks (§5) and no offsets, so the
// window finds the words itself — §18.11's ruling, written down because "highlight what matched" sounds
// trivial until someone has to say what "matched" means when the query is three words and the note has both.

import { describe, expect, it } from 'vitest'
import { firstMatch, markTerms } from './highlight'

describe('markTerms', () => {
  it('marks each word of the query, whatever case either side used', () => {
    expect(markTerms('The Kumquat Tree', 'kumquat')).toEqual([
      { text: 'The ', matched: false },
      { text: 'Kumquat', matched: true },
      { text: ' Tree', matched: false },
    ])
  })

  it('marks every word of a multi-word query, not just the first', () => {
    const segments = markTerms('citrus and kumquats', 'citrus kumquat').filter((s) => s.matched)
    expect(segments.map((s) => s.text)).toEqual(['citrus', 'kumquat'])
  })

  it('marks a word inside another word, because that is what keyword search found', () => {
    // np's BM25 hits `kumquat` inside `kumquats`. A highlight that only matched whole words would disagree
    // with the result list it is explaining.
    expect(markTerms('three kumquats here', 'kumquat').filter((s) => s.matched).map((s) => s.text)).toEqual([
      'kumquat',
    ])
  })

  it('leaves the text alone when nothing matched, and when there was no query to match', () => {
    expect(markTerms('nothing here', 'kumquat')).toEqual([{ text: 'nothing here', matched: false }])
    expect(markTerms('anything', '   ')).toEqual([{ text: 'anything', matched: false }])
  })

  it('reassembles into the original text, marks or no marks', () => {
    const text = 'Kumquat, kumquats, and a KUMQUAT tree'
    expect(markTerms(text, 'kumquat').map((s) => s.text).join('')).toBe(text)
  })

  it('does not mark one-keystroke queries, which match half the vault', () => {
    // A single letter is a search as fast as you can type it, and highlighting every `a` in the note reads as
    // a rendering fault rather than as information. np needs two characters to score anything either.
    expect(markTerms('banana', 'a')).toEqual([{ text: 'banana', matched: false }])
  })
})

describe('firstMatch', () => {
  it('says where to look, in the same terms the marks used', () => {
    // Counted, not guessed: 7 for '# Notes' plus two newlines, plus 'A '.
    expect(firstMatch('# Notes\n\nA kumquat tree.\n', 'kumquat')).toBe(11)
  })

  it('says nothing when the words are not there', () => {
    // Which happens: a vector-only hit has no keyword match in it at all, and the honest answer is "no place
    // to scroll to", not "the top of the file".
    expect(firstMatch('a note about pears', 'kumquat')).toBeNull()
  })
})
