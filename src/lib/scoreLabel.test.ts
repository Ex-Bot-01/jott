import { describe, expect, it } from 'vitest'
import { SCORE_TITLE, figuresNote, formatScore, meaningLabel, rowLabel, rowFigures, scoreTitleFor } from './scoreLabel'
import { hitSuffix } from './searchState'

describe('the note-processor score on a result row', () => {
  it('prints the number note-processor actually sent, at three significant digits', () => {
    // Measured against 0.5.1: `score` is a reciprocal-rank fusion value, so keyword hits land around 0.016 and a
    // meaning-only hit near 0.008. Rounding to two decimals would print both as `0.02` and the row would say nothing.
    expect(formatScore(0.01639344262295082)).toBe('0.0164')
    expect(formatScore(1.23456)).toBe('1.23')
  })

  it('never rounds a small score down to zero', () => {
    // "0" reads as "not at all", which is the opposite of "ranked last but still matched".
    expect(formatScore(0.000123)).toBe('0.000123')
    expect(formatScore(0.000123)).not.toBe('0')
  })

  it('prints a real zero as zero, because that is what note-processor said', () => {
    expect(formatScore(0)).toBe('0')
  })

  it('shows the cosine only when there was one', () => {
    // Measured: `cosine` is null whenever vectors did not run. A meaning score of 0 would be a lie; an absent one is
    // the truth, and printing "meaning 0" for every keyword hit is exactly how a status line starts being ignored.
    expect(meaningLabel(null)).toBeNull()
    expect(meaningLabel(0.8271)).toBe('meaning 0.83')
    // Two decimals for every real number, including a true zero: rows whose labels line up are rows a person can
    // compare, and "meaning 0" beside "meaning 0.83" reads as a different kind of answer rather than the same one low.
    expect(meaningLabel(0)).toBe('meaning 0.00')
  })

  it('says in the tooltip what the number is not', () => {
    // The temptation every search UI gives in is to render this as "87% similar". It is a fused rank, not a
    // percentage, and the row has to say so where the number sits.
    expect(SCORE_TITLE).toContain('not a percentage')
    expect(SCORE_TITLE.toLowerCase()).toContain('rank')
  })
describe('the accessible name of a result row', () => {
  it('says what the number is to anyone who cannot hover it', () => {
    // A `title` is a mouse affordance. Read aloud, "inbox.md … 0.0164" sounds like a score to compare, which is the
    // one misreading this row exists to avoid.
    const label = rowLabel({ path: 'inbox.md', snippet: 'a kumquat tree', score: 0.01639344262295082, cosine: null })
    expect(label).toContain('inbox.md')
    expect(label).toContain('a kumquat tree')
    expect(label).toContain('0.0164')
    expect(label.toLowerCase()).toContain('not a percentage')
  })

  it('mentions the meaning figure only when there was one', () => {
    expect(rowLabel({ path: 'a.md', snippet: 'x', score: 0.01, cosine: 0.82, matchedExactly: true })).toContain('meaning 0.82')
    // `matchedExactly: true` is what a keyword hit always carries (np sets the flag from its own word test), so this
    // fixture is the shape np sends. A row with no cosine *and* no word test describes a half that never ran, which is
    // why the phrase for it — issue #25 — appears in its own test below rather than here.
    expect(rowLabel({ path: 'a.md', snippet: 'x', score: 0.01, cosine: null, matchedExactly: true })).not.toContain('meaning')
  })

  it('strips note-processors own highlight marks rather than reading them as words', () => {
    // np brackets what it matched: `[kumquat]`. An accessible name that says "bracket kumquat bracket" is noise.
    expect(rowLabel({ path: 'a.md', snippet: 'a [kumquat] tree', score: 0.01, cosine: null })).toContain('a kumquat tree')
  })
})

})

describe('the figures that stay in the tooltip', () => {
  it('says nothing about a measurement nobody took', () => {
    expect(figuresNote({ bm25: null, cosine: null })).toBe('')
    expect(figuresNote({ bm25: Number.NaN, cosine: null })).toBe('')
  })

  it('keeps bm25 negative, because the sign is the meaning', () => {
    // SQLite reports bm25 as negative-is-better. Taking the absolute value to make it "look like a score" would
    // invert the ordering while keeping the digits — the sort of tidy lie a formatter tells.
    const note = figuresNote({ bm25: -8.953488e-7, cosine: null })
    expect(note).toContain('bm25 -8.95e-7')
    expect(note.toLowerCase()).not.toContain('%')
    expect(note).toMatch(/negative/i)
  })

  it('names both figures when both ran, and never a percentage', () => {
    const note = figuresNote({ bm25: -1.5, cosine: 0.827341 })
    expect(note).toContain('bm25 -1.5')
    expect(note).toContain('cosine 0.8273')
    expect(note).not.toContain('%')
  })
})

describe('one measurement, printed once', () => {
  // Read off note-processor's source rather than guessed (`search.rs:1226-1231`): the number that orders the list is
  // the cosine when only vectors ran, and SQLite's negated bm25 when only words ran — a fused rank only in hybrid
  // mode. The tester caught this by eye: *"when I have meaning only … I still see two scores, two identical scores."*
  // Two numbers on a row said "two measurements"; there was one.
  const similarity = { score: 0.8274, cosine: 0.8274 }

  it('prints one figure when note-processor’s rank score *is* the cosine', () => {
    const figures = rowFigures(similarity)
    expect(figures.meaning).toBe('meaning 0.83')
    expect(figures.rank, 'the same number twice is not a second measurement').toBeNull()
  })

  it('calls that figure what it is, not a fused rank', () => {
    const title = scoreTitleFor(similarity)
    expect(title.toLowerCase()).toContain('similar')
    expect(title.toLowerCase()).not.toContain('fused')
  })

  it('prints both figures when they are two different measurements', () => {
    // A hybrid row: fused rank ordering plus the similarity that went into it. Both are np's, both mean something.
    const figures = rowFigures({ score: 0.0164, cosine: 0.83 })
    expect(figures.rank).toBe('0.0164')
    expect(figures.meaning).toBe('meaning 0.83')
    expect(scoreTitleFor({ score: 0.0164, cosine: 0.83 }).toLowerCase()).toContain('fused')
  })

  it('names the keyword figure on a row where no vectors ran', () => {
    // `cosine: null` is "vectors did not run", and in that mode np's score is `-bm25`. Calling it a fused rank would
    // be Jott describing a fusion that did not happen.
    const title = scoreTitleFor({ score: 1.7, cosine: null })
    expect(title.toLowerCase()).toMatch(/word|bm25/)
    expect(title.toLowerCase()).not.toContain('fused')
    const figures = rowFigures({ score: 1.7, cosine: null })
    expect(figures.rank).toBe('1.7')
    expect(figures.meaning).toBeNull()
  })

  it('does not read the same number twice to a screen reader', () => {
    const label = rowLabel({ path: 'a.md', snippet: null, ...similarity })
    const occurrences = label.split('0.83').length - 1
    expect(occurrences).toBe(1)
  })

  it('treats a figure that merely rounds to the same text as two measurements', () => {
    // The rule compares np's numbers, not their printed forms: `0.8274` and `0.8273` both read "0.83" but came from
    // different places, and hiding one would be hiding a measurement.
    expect(rowFigures({ score: 0.8274, cosine: 0.8273 }).rank).toBe('0.827')
  })
})

describe('the band a meaning figure sits in (issue #25)', () => {
  // Recorded from np 0.6.0 with `embeddinggemma-2` over four unrelated German notes: the three-line note scored
  // cosine 0.7195 against the query `Familie` while the family birthday calendar behind it scored 0.7101. Printed bare,
  // those two figures read as a ranking. They are not one: see note-processor#19 for the table.
  const meaningOnly = { score: 0.7195, cosine: 0.7195 }

  it('says what the figures look like in practice wherever one is drawn', () => {
    const title = scoreTitleFor(meaningOnly)
    expect(title).toContain('hundredths')
    expect(title).toContain('0.63')
    expect(title.toLowerCase()).toContain('one word')
  })

  it('rides along on a fused row too, because that row draws the meaning figure as well', () => {
    const hybrid = { score: 0.032522, cosine: 0.7164 } // keyword rank 1 + semantic rank 2, measured
    expect(scoreTitleFor(hybrid)).toContain('hundredths')
  })

  it('says nothing about meanings on a row where vectors never ran', () => {
    expect(scoreTitleFor({ score: 0.93, cosine: null })).not.toContain('hundredths')
  })

  it('tells the accessible name which half reached the row', () => {
    // `aria-label` replaces the row's text for a screen reader, so the visible suffix has to travel here too or the
    // explanation exists only for people with a mouse.
    const hit = { path: 'Hallo.md', snippet: 'Hi', score: 0.016393, cosine: 0.7195 }
    expect(rowLabel({ ...hit, matchedExactly: null })).toContain(hitSuffix({ matchedExactly: null, cosine: 0.7195 })!)
    expect(rowLabel({ ...hit, matchedExactly: false })).toContain(hitSuffix({ matchedExactly: false, cosine: 0.7195 })!)
    expect(rowLabel({ ...hit, matchedExactly: true })).not.toContain('by meaning only')
  })
})
