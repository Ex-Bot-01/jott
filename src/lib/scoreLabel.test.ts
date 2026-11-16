import { describe, expect, it } from 'vitest'
import { SCORE_TITLE, formatScore, meaningLabel } from './scoreLabel'

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
})
