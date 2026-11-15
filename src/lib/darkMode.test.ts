/**
 * Which colour scheme the window is in, as a module rather than as a `matchMedia` call buried in a component.
 *
 * The reason this exists at all: 0.0.15 never told CodeMirror it was dark. Our stylesheet painted dark paper and
 * light ink, while CodeMirror ran its **light** facet — caret `1.2px solid black`, selection background a bright
 * blue — so the two systems disagreed about what colour the window was, and the disagreement landed on the one
 * element a person stares at: where their typing will land.
 *
 * Testing it headlessly means testing the shape of the question rather than its colour: what the answer is for a
 * dark desktop, that a change reaches whoever asked, and that a host with no `matchMedia` at all gets an answer
 * instead of an exception.
 */
import { describe, expect, it } from 'vitest'
import { schemeOf, watchScheme } from './darkMode'

/** A stand-in for `window.matchMedia`, which jsdom does not implement. */
function fakeMedia(matches: boolean) {
  const listeners = new Set<(event: { matches: boolean }) => void>()
  const counts = { added: 0, removed: 0 }
  return {
    matches,
    counts,
    addEventListener: (_: string, fn: (event: { matches: boolean }) => void) => {
      counts.added += 1
      listeners.add(fn)
    },
    removeEventListener: (_: string, fn: (event: { matches: boolean }) => void) => {
      counts.removed += 1
      listeners.delete(fn)
    },
    /** Flip the scheme the way the OS would. */
    set(next: boolean) {
      this.matches = next
      for (const fn of [...listeners]) fn({ matches: next })
    },
  }
}

describe('what scheme the window is in', () => {
  it('is dark when the desktop asks for dark, light otherwise', () => {
    expect(schemeOf(true)).toBe('dark')
    expect(schemeOf(false)).toBe('light')
  })

  it('reads a media list that already matches', () => {
    expect(schemeOf(fakeMedia(true).matches)).toBe('dark')
  })
})

describe('following the scheme while the window is open', () => {
  it('tells whoever asked when the desktop changes its mind', () => {
    const media = fakeMedia(false)
    const seen: string[] = []
    watchScheme((scheme) => seen.push(scheme), media)

    expect(seen).toEqual(['light'])
    media.set(true)
    expect(seen).toEqual(['light', 'dark'])
  })

  it('stops telling after unsubscribe, because a closed window must not answer the OS forever', () => {
    const media = fakeMedia(false)
    const seen: string[] = []
    const stop = watchScheme((scheme) => seen.push(scheme), media)

    stop()
    media.set(true)
    expect(seen).toEqual(['light'])
  })

  it('asks the host once per subscription, not once per change', () => {
    const media = fakeMedia(false)
    const stop = watchScheme(() => {}, media)
    expect(media.counts.added, 'one question per subscription').toBe(1)
    stop()
    expect(media.counts.removed, 'and the same number of un-questions').toBe(1)
  })

  it('says light when the host has no matchMedia at all, and still gives something to unsubscribe', () => {
    // A webview without `matchMedia` is not a reason the window cannot paint. And the caller always gets its
    // cleanup function back, whether or not there was anything to clean up.
    const seen: string[] = []
    const stop = watchScheme((scheme) => seen.push(scheme), undefined)
    expect(seen).toEqual(['light'])
    expect(() => stop()).not.toThrow()
  })

  it('does not throw if the host answers nothing to addEventListener', () => {
    const broken = { matches: true }
    expect(() => watchScheme(() => {}, broken)).not.toThrow()
  })
})
