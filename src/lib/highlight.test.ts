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

describe('word beginnings', () => {
  it('marks the beginning of a longer word when asked', () => {
    // note-processor#6: with prefix on, np answers *grandmother* for `grand`. The row and the opened note have to
    // mark the letters that actually matched — `grand` — rather than the whole word or nothing at all.
    expect(markTerms('my grandmother kept', 'grand', true)).toEqual([
      { text: 'my ', matched: false },
      { text: 'grand', matched: true },
      { text: 'mother kept', matched: false },
    ])
  })

  it('does not mark a word that merely contains the term', () => {
    // `agrand` is not a word beginning. Marking inside a word is how a prefix mode starts lying about matches, and
    // it is the one behaviour difference from today, so it gets its own test rather than a comment.
    const segments = markTerms('the agrand family', 'grand', true)
    expect(segments.filter((part) => part.matched)).toEqual([])
  })

  it('marks at the start of a line and after punctuation', () => {
    for (const text of ['grandmother arrived', '(grandmother)', 'line two\ngrandmother', '“grandmother”']) {
      const matched = markTerms(text, 'grand', true).filter((part) => part.matched)
      expect(matched.map((part) => part.text), text).toEqual(['grand'])
    }
  })

  it('still marks an exact word in prefix mode', () => {
    expect(markTerms('the grand design', 'grand', true).filter((part) => part.matched).map((part) => part.text)).toEqual([
      'grand',
    ])
  })

  it('is byte-identical to whole-word marking when the flag is off or missing', () => {
    // The regression pin: everything shipped before np 0.5.x behaves exactly as it did, and a `prefix` that never
    // reached the request cannot change what the window paints.
    const cases: [string, string][] = [
      ['a kumquat tree', 'kumquat'],
      ['Grandmother knew about grand pianos', 'grand'],
      ['agrand and a-grand too', 'grand'],
      ['kestrels and a kestrel nest', 'kestrel'],
      ['', 'grand'],
      ['nothing here matches', 'zzz'],
    ]
    for (const [text, query] of cases) {
      expect(markTerms(text, query, false)).toEqual(markTerms(text, query))
      if (query !== 'grand') {
        // The whole-word cases above are chosen so prefix mode differs; the rest must not change at all.
        expect(markTerms(text, query, true)).toEqual(markTerms(text, query))
      }
    }
  })

  it('scrolls to a word beginning in prefix mode', () => {
    // Whole-word mode has always marked the first *occurrence*, letters anywhere (see "marks a word inside another
    // word" above): np's BM25 answers `kumquat` for `kumquats`, and a highlight that disagreed with the row would be
    // worse than one that is slightly generous. Prefix mode is the opposite trade — it marks what np widened, so it
    // skips *agrand* and lands on the word that really begins with the term.
    const text = 'the agrand family kept a grandmother here'
    expect(firstMatch(text, 'grand')).toBe(text.indexOf('agrand') + 1)
    expect(firstMatch(text, 'grand', true)).toBe(text.indexOf('grandmother'))
  })

  it('keeps its minimum term length in prefix mode too', () => {
    // One keystroke matching half the vault by word beginnings would be worse than today's whole-word behaviour.
    expect(markTerms('grandmother arrived', 'g', true).filter((part) => part.matched)).toEqual([])
  })
})
