// The "match word beginnings" choice is remembered per app, not per folder: note-processor#6 changed what a search
// *means*, so the one place it can be decided has to be tested — including what a corrupt or hostile stored value
// does to it, and what happens when the webview refuses storage altogether.

import { describe, expect, it } from 'vitest'
import { PREFIX_KEY, loadPrefix, savePrefix } from './prefixSearch'

/** A stand-in for `Storage` that keeps exactly the two methods this module is allowed to use. */
function store(initial: Record<string, string> = {}) {
  const data = new Map(Object.entries(initial))
  return {
    data,
    getItem: (key: string) => (data.has(key) ? data.get(key)! : null),
    setItem: (key: string, value: string) => void data.set(key, value),
  }
}

describe('loading the choice', () => {
  it('is off when nothing was ever stored', () => {
    // Absent means "never asked", and the only safe reading of that is np's own default: whole words.
    expect(loadPrefix(store())).toBe(false)
  })

  it('is on for the one value that means yes', () => {
    expect(loadPrefix(store({ [PREFIX_KEY]: '1' }))).toBe(true)
  })

  it('reads anything else as off, without throwing', () => {
    // A webview can hold a value written by a future version, or a tampered one. "true", "yes" and "on" all *mean*
    // something to a human; guessing from them would widen somebody's search on a coin toss.
    for (const value of ['0', '', 'true', 'yes', 'on', 'null', 'undefined', '1 ', ' 1', '2']) {
      expect(loadPrefix(store({ [PREFIX_KEY]: value })), `${JSON.stringify(value)} must read as off`).toBe(false)
    }
  })

  it('is off when storage throws on the way in', () => {
    // Some hardened webviews throw on any access. A search box that refuses to work because a preference could not be
    // read is a worse failure than one that defaults to whole words.
    const broken = {
      getItem: () => {
        throw new Error('storage disabled')
      },
      setItem: () => {},
    }
    expect(loadPrefix(broken)).toBe(false)
  })

  it('tolerates being handed nothing at all', () => {
    // `window.localStorage` is undefined in a worker and in some test environments.
    expect(loadPrefix(null)).toBe(false)
    expect(loadPrefix(undefined)).toBe(false)
  })
})

describe('saving the choice', () => {
  it('writes 1 for yes and 0 for no', () => {
    // `false` writes "0" rather than removing the key: "answered no" and "never asked" are different facts, and a
    // later version may want to stop asking the people who already answered.
    const s = store()
    savePrefix(s, true)
    expect(s.data.get(PREFIX_KEY)).toBe('1')
    savePrefix(s, false)
    expect(s.data.get(PREFIX_KEY)).toBe('0')
  })

  it('survives a round trip', () => {
    const s = store()
    savePrefix(s, true)
    expect(loadPrefix(s)).toBe(true)
    savePrefix(s, false)
    expect(loadPrefix(s)).toBe(false)
  })

  it('says nothing when storage refuses to hold anything', () => {
    // The toggle still works for this window; the memory just does not survive it. Throwing here would take the
    // search box down with it, because saving happens inside the change handler.
    const s = {
      getItem: () => null,
      setItem: () => {
        throw new Error('quota exceeded')
      },
    }
    expect(() => savePrefix(s, true)).not.toThrow()
  })

  it('tolerates a storage that is not there', () => {
    expect(() => savePrefix(null, true)).not.toThrow()
  })
})

describe('the key itself', () => {
  it('is namespaced so another app on the same origin cannot answer for Jott', () => {
    expect(PREFIX_KEY).toBe('jott.search.prefix')
  })
})
