import { describe, expect, it } from 'vitest'
import { QUIET_LIMIT, downloadSentence, initialWatch, readMarker, type Downloading } from './downloadWatch'

const main = (bytesDone: number): Downloading => ({
  modelId: 'embeddinggemma-2',
  bytesDone,
  bytesTotal: 309_855_456,
})

describe('reading np’s fetch marker', () => {
  it('says nothing when np reports no marker', () => {
    expect(readMarker(initialWatch, null).show).toBe(false)
  })

  it('shows the first reading it sees', () => {
    // Hiding a first reading would hide downloads that start and finish between two polls — which is the complaint
    // this module exists to answer. A leftover marker therefore costs a few seconds of banner. Measured cost, chosen.
    const first = readMarker(initialWatch, main(1_048_576))
    expect(first.show).toBe(true)
    expect(first.state.lastBytes).toBe(1_048_576)
  })

  it('keeps showing while the count moves', () => {
    let state = readMarker(initialWatch, main(1_048_576)).state
    for (const bytes of [20_971_520, 62_914_560, 184_550_345]) {
      const next = readMarker(state, main(bytes))
      expect(next.show).toBe(true)
      expect(next.state.quietReads).toBe(0)
      state = next.state
    }
  })

  it('stops calling a frozen count a download', () => {
    // Killed np leaves its marker where it was: `models.list` went on answering 184,550,345 bytes long after the
    // process died. A banner that never clears becomes furniture, and this is the light that means *wait for it*.
    let state = readMarker(initialWatch, main(184_550_345)).state
    const shown: boolean[] = []
    for (let i = 0; i < QUIET_LIMIT + 2; i++) {
      const next = readMarker(state, main(184_550_345))
      shown.push(next.show)
      state = next.state
    }
    expect(shown.slice(0, QUIET_LIMIT)).toEqual(new Array(QUIET_LIMIT).fill(true))
    expect(shown[QUIET_LIMIT]).toBe(false)
    expect(shown[shown.length - 1]).toBe(false)
  })

  it('starts over when the marker goes away, so a later download is not judged against an old count', () => {
    const stale = readMarker(readMarker(initialWatch, main(9_000)).state, null)
    expect(stale.state).toEqual(initialWatch)
    // A new download that happens to resume below the old count still gets announced.
    expect(readMarker(stale.state, main(5_000)).show).toBe(true)
  })

  it('does not call a smaller count progress, and shows again once bytes move', () => {
    // A restarted fetch starts from zero. The rule compares against the previous reading, so the new one is not
    // credited as movement — it rides the same quiet tolerance, then shows again on the next poll that moves.
    const state = readMarker(initialWatch, main(100_000_000)).state
    expect(readMarker(state, main(1_048_576)).show).toBe(true)
    const quiet = readMarker(state, main(1_048_576))
    const moving = readMarker({ lastBytes: 1_048_576, quietReads: 0 }, main(3_000_000))
    expect(moving.show).toBe(true)
    expect(moving.state.quietReads).toBe(0)
    expect(quiet.state.lastBytes).toBe(1_048_576)
  })
})

describe('the banner’s sentence', () => {
  it('names np’s model and np’s bytes', () => {
    expect(downloadSentence(main(184_550_345))).toBe('downloading embeddinggemma-2 · 176 of 295 MiB')
  })

  it('says nothing about a total np did not announce', () => {
    // "of 0 MiB" would be a size Jott invented, and a bar with no maximum is honest only if the words match it.
    expect(downloadSentence({ modelId: 'mmproj', bytesDone: 4_194_304, bytesTotal: null })).toBe(
      'downloading mmproj · 4 MiB',
    )
  })

  it('counts no byte np has not received, and never invents a bigger total', () => {
    // Whole MiB means one byte short of the end reads "295 of 295". That is the honest consequence of a coarse unit:
    // rounding *up* would claim bytes nobody received. The bar underneath keeps the exact ratio.
    expect(downloadSentence(main(309_855_455))).toBe('downloading embeddinggemma-2 · 295 of 295 MiB')
    expect(downloadSentence(main(1))).toBe('downloading embeddinggemma-2 · 0 of 295 MiB')
  })
})
