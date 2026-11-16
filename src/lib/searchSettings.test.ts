// What the settings panel remembers about how search matches, and what it must refuse to remember.
//
// Two keys, both per app rather than per folder: which engine answers, and whether word beginnings count. Both
// change what a query *means*, so both are decided once and expected to stay — and both have to survive a webview
// holding a value this version never wrote.

import { describe, expect, it } from 'vitest'
import { DEFAULTS, MODE_KEY, MODE_WORD, PREFIX_KEY, loadSettings, modeSentence, saveMode, savePrefix } from './searchSettings'

/** A stand-in for `Storage` with exactly the methods these two modules are allowed to use. */
function store(initial: Record<string, string> = {}) {
  const data = new Map(Object.entries(initial))
  return {
    data,
    getItem: (key: string) => (data.has(key) ? data.get(key)! : null),
    setItem: (key: string, value: string) => void data.set(key, value),
  }
}

/** A webview that throws on being read at all — hardened, exhausted, or run down a remote-origin code path. */
const hostile = {
  getItem: () => {
    throw new Error('SecurityError')
  },
  setItem: () => {
    throw new Error('QuotaExceededError')
  },
}

describe('loading the settings', () => {
  it('starts at both engines and whole words, which is what note-processor does by default', () => {
    // Absent means "never asked". The only honest reading of that is the backend's own default, not an opinion.
    expect(loadSettings(store())).toEqual({ mode: 'hybrid', prefix: false })
    expect(DEFAULTS).toEqual({ mode: 'hybrid', prefix: false })
  })

  it('keeps each mode this build can ask for', () => {
    for (const mode of ['keyword', 'hybrid', 'semantic'] as const) {
      expect(loadSettings(store({ [MODE_KEY]: mode })).mode).toBe(mode)
    }
  })

  it('reads anything else as the default rather than guessing', () => {
    // A future version, a hand-edited profile, or a tampered one can hold anything. `SEMANTIC` *means* something to a
    // human and would silently switch someone to meanings-only search, which is the mode that cannot explain an empty
    // result (note-processor#11). Case-folded reads are how that happens.
    for (const junk of ['fuzzy', '', 'SEMANTIC', 'Semantic', 'both', 'null', '0']) {
      expect(loadSettings(store({ [MODE_KEY]: junk })).mode).toBe('hybrid')
    }
  })

  it('reads the one value that means yes for word beginnings', () => {
    expect(loadSettings(store({ [PREFIX_KEY]: '1' })).prefix).toBe(true)
  })

  it('reads anything else as off for word beginnings, without throwing', () => {
    // "true", "yes" and "on" all mean something to a person; guessing from them would widen a search on a coin toss.
    for (const junk of ['true', 'yes', 'on', '0', '', '2']) {
      expect(loadSettings(store({ [PREFIX_KEY]: junk })).prefix).toBe(false)
    }
  })

  it('falls back to the defaults when the webview refuses to be read', () => {
    // A settings read is not worth losing the search box over.
    expect(loadSettings(hostile)).toEqual(DEFAULTS)
    expect(loadSettings(null)).toEqual(DEFAULTS)
    expect(loadSettings(undefined)).toEqual(DEFAULTS)
  })
})

describe('saving the settings', () => {
  it('writes the words this app owns, not a rendering of them', () => {
    const s = store()
    saveMode(s, 'semantic')
    savePrefix(s, true)
    expect(s.data.get(MODE_KEY)).toBe('semantic')
    expect(s.data.get(PREFIX_KEY)).toBe('1')
  })

  it('writes an explicit no rather than deleting the key', () => {
    // "answered no" is a fact a future version can act on; "never asked" is not, and the two must not collide.
    const s = store({ [MODE_KEY]: 'semantic', [PREFIX_KEY]: '1' })
    saveMode(s, 'keyword')
    savePrefix(s, false)
    expect(s.data.get(MODE_KEY)).toBe('keyword')
    expect(s.data.get(PREFIX_KEY)).toBe('0')
  })

  it('leaves the other half alone', () => {
    // One control writing both keys is how a panel ends up undoing a choice nobody touched.
    const s = store({ [MODE_KEY]: 'keyword', [PREFIX_KEY]: '1' })
    saveMode(s, 'hybrid')
    expect(s.data.get(PREFIX_KEY)).toBe('1')
    savePrefix(s, false)
    expect(s.data.get(MODE_KEY)).toBe('hybrid')
  })

  it('carries on when storage refuses to write', () => {
    // The choice stands for this window; throwing would take the click handler down with it.
    expect(() => saveMode(hostile, 'keyword')).not.toThrow()
    expect(() => savePrefix(hostile, true)).not.toThrow()
    expect(() => saveMode(null, 'keyword')).not.toThrow()
  })

  it('namespaces both keys, so another app on the same origin cannot answer for Jott', () => {
    // These strings are the on-disk contract. Renaming one silently resets every window in existence, and a test
    // that only imports the constant cannot notice.
    expect(MODE_KEY).toBe('jott.search.mode')
    expect(PREFIX_KEY).toBe('jott.search.prefix')
  })

  it('round-trips through what was just written', () => {
    const s = store()
    saveMode(s, 'semantic')
    savePrefix(s, true)
    expect(loadSettings(s)).toEqual({ mode: 'semantic', prefix: true })
  })
})

describe('describing a mode to the person choosing it', () => {
  it('says something different for each of the three', () => {
    const sentences = (['keyword', 'hybrid', 'semantic'] as const).map(modeSentence)
    expect(new Set(sentences).size).toBe(3)
    for (const sentence of sentences) expect(sentence.length).toBeGreaterThan(20)
  })

  it('does not promise meanings where there are none', () => {
    // "words only" is the whole truth about `keyword`; a sentence there that mentions meanings would be a lie in the
    // one mode that guarantees their absence.
    expect(modeSentence('keyword')).toContain('word')
    expect(modeSentence('keyword').toLowerCase()).not.toContain('meaning')
  })

  it('names what each of the other two asks note-processor for', () => {
    expect(modeSentence('hybrid').toLowerCase()).toContain('both')
    // The meanings-only sentence is where the panel warns that this mode needs vectors, so it has to be in it.
    expect(modeSentence('semantic').toLowerCase()).toContain('meaning')
  })
})

describe('the mode’s name, said once', () => {
  const modes = ['keyword', 'hybrid', 'semantic'] as const

  it('has a word for every mode, and the sentence opens with it', () => {
    // The status line quotes `MODE_WORD`; the Settings panel quotes the opening of `modeSentence`. Two places that
    // describe one choice in two vocabularies is how "Words only" in the panel ended up next to "meanings on" in
    // the corner, which a tester reasonably read as a toggle that had not taken effect.
    for (const mode of modes) {
      expect(MODE_WORD[mode].length).toBeGreaterThan(0)
      expect(modeSentence(mode), `${mode}’s sentence no longer opens with its own word`).toBe(`${MODE_WORD[mode]}: ${modeSentence(mode).slice(MODE_WORD[mode].length + 2)}`)
      expect(modeSentence(mode).startsWith(`${MODE_WORD[mode]}:`), `panel text drifted from the word for ${mode}`).toBe(true)
    }
  })

  it('names the choice rather than a result', () => {
    // A mode word must never read as a measurement of what search did — that is np's answer, and it travels with
    // each result (`searchState`), not with this setting.
    for (const mode of modes) {
      expect(MODE_WORD[mode].toLowerCase()).not.toMatch(/matched|found|result/)
    }
  })
})
