import { describe, expect, it } from 'vitest'
import {
  SCORE_TITLE,
  figuresNote,
  formatScore,
  meaningLabel,
  rowLabel,
} from './scoreLabel'

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
    expect(rowLabel({ path: 'a.md', snippet: 'x', score: 0.01, cosine: 0.82 })).toContain('meaning 0.82')
    expect(rowLabel({ path: 'a.md', snippet: 'x', score: 0.01, cosine: null })).not.toContain('meaning')
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
