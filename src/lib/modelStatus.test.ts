import { describe, expect, it } from 'vitest'
import { DOT_LABEL, dotState, dotTitle, type ModelFacts } from './modelStatus'

/** The measured 0.5.1 answer for a folder whose model is configured and sitting on the disk. */
function facts(over: Partial<ModelFacts> = {}): ModelFacts {
  return {
    provider: 'local-model',
    modelId: 'embeddinggemma-2',
    present: true,
    bytesTotal: 309_855_456,
    embeddedNotes: 12,
    configUnreadable: false,
    modelsDirError: null,
    ...over,
  }
}

describe('the status line’s model dot', () => {
  it('is green when the model is here and this folder has vectors', () => {
    expect(dotState(facts())).toBe('active')
    const title = dotTitle(facts())
    expect(title).toContain('embeddinggemma-2')
    expect(title).toContain('12 notes in this folder have vectors')
  })

  it('says the model is here but unused when nothing in this folder has been embedded', () => {
    // The tester's question was "is the model active/downloaded/online" — those are different states, and a dot that
    // blurs them makes someone fetch again or give up on a folder that is only waiting for [embed].
    expect(dotState(facts({ embeddedNotes: 0 }))).toBe('ready')
    expect(dotTitle(facts({ embeddedNotes: 0 }))).toContain('nothing in this folder has been embedded yet')
  })

  it('waits when the model is allowed but not on the disk, and names what it is waiting for', () => {
    expect(dotState(facts({ present: false }))).toBe('waiting')
    const title = dotTitle(facts({ present: false }))
    expect(title).toContain('not on this disk yet')
    // 309,855,456 bytes is what np’s own pinned table quotes for embeddinggemma-2.
    expect(title).toContain('296 MiB')
  })

  it('is off only when note-processor’s settings say so — including the “none” that disable writes', () => {
    expect(dotState(facts({ provider: null }))).toBe('off')
    expect(dotState(facts({ provider: 'none' }))).toBe('off')
    expect(dotTitle(facts({ provider: 'none' }))).toContain('words only')
  })

  it('says unknown rather than off when nothing answered', () => {
    // The whole point of the fifth state. `present: null` means np was unreachable or its models directory would not
    // list; drawing that as grey tells the person their embeddings are switched off, and the honest reading is that
    // nobody knows right now.
    const unanswered = facts({ present: null, embeddedNotes: 0 })
    expect(dotState(unanswered)).toBe('unknown')
    const title = dotTitle(unanswered)
    expect(title).toContain('did not answer')
    expect(title.toLowerCase()).not.toContain('switched off')
  })

  it('names a provider Jott does not know instead of guessing what its weights look like', () => {
    // Somebody who pointed np at their own server has embeddings configured, so "off" is a claim about a setting that
    // says the opposite. And "on this disk" is Jott's story about `local-model`, not a fact about somebody else's
    // endpoint — np lists no rows for it, which would otherwise read as a download that never arrives.
    expect(dotState(facts({ provider: 'openai', present: false }))).toBe('other')
    const title = dotTitle(facts({ provider: 'openai', present: false }))
    expect(title.toLowerCase()).toContain('openai')
    expect(title).not.toContain('disk')
  })

  it('does not name a model note-processor never named', () => {
    // An older np, or a config that names no model: the dot still reports the state it knows, without inventing an id.
    const title = dotTitle(facts({ modelId: null }))
    expect(title).not.toContain('embeddinggemma-2')
    expect(title).toContain('the model note-processor would use')
  })

  it('gives every state a word, so the dot is not the only thing carrying the meaning', () => {
    // Colour alone is not information: a colour-blind tester, a screen reader, and a screenshot in a bug report all
    // need the word. `DOT_LABEL` is what the aria-label leads with.
    for (const state of ['active', 'ready', 'waiting', 'off', 'unknown', 'other'] as const) {
      expect(DOT_LABEL[state].length).toBeGreaterThan(1)
    }
    expect(dotTitle(facts()).startsWith(DOT_LABEL.active)).toBe(true)
  })
describe('what the dot says when nobody could answer', () => {
  it('does not call an unreadable settings file "embeddings off"', () => {
    // The consent card already refuses this flattening (`configUnreadable`): a TOML that fails to parse names no
    // provider, and telling somebody to turn embeddings on in a folder whose np cannot read its own config is advice
    // that cannot work.
    const unreadable = facts({ configUnreadable: true, provider: null, present: null, embeddedNotes: 0 })
    expect(dotState(unreadable)).toBe('unknown')
    const title = dotTitle(unreadable)
    expect(title).toContain("settings file could not be read")
    expect(title.toLowerCase()).not.toContain('words only')
    expect(title.toLowerCase()).not.toContain('switched off')
  })

  it('quotes note-processor when it is running but cannot look', () => {
    // np answers an unlistable models directory with a *result* carrying its own sentence. "did not answer" would be
    // false about a process that answered, and told us exactly what it could not do.
    const stalled = facts({ present: null, modelsDirError: 'permission denied reading the model store' })
    expect(dotState(stalled)).toBe('unknown')
    expect(dotTitle(stalled)).toContain('models directory')
    expect(dotTitle(stalled)).toContain('permission denied reading the model store')
  })

  it('says which half is missing when note-processor named no model', () => {
    // No `pinned` entry and no configured key: np answered, and what it said was "I cannot tell you which model".
    const unnamed = facts({ modelId: null, present: null, bytesTotal: null })
    expect(dotState(unnamed)).toBe('unknown')
    expect(dotTitle(unnamed)).toContain('which model this folder would use')
  })

  it('keeps the plain silence sentence for a real non-answer', () => {
    const title = dotTitle(facts({ present: null }))
    expect(title).toContain('did not answer')
  })
})

})

describe('the word beside the dot', () => {
  it('says what was measured, not what a search did', () => {
    // "meanings on" was this label, and a tester read it as *then why is my search using words only?* — a fair
    // reading of words that claim more than the dot can know. It measures np's provider, the weights on disk and
    // how many notes in this folder carry vectors; nothing about the next query.
    expect(DOT_LABEL.active.toLowerCase()).toContain('embedded')
    for (const label of Object.values(DOT_LABEL)) {
      expect(label.toLowerCase()).not.toContain('meanings on')
    }
  })

  it('still leaves the mode to the setting beside it', () => {
    // The dot must not start spelling the search mode, or the two facts merge back into one ambiguous word.
    for (const label of Object.values(DOT_LABEL)) {
      expect(label.toLowerCase()).not.toMatch(/words only|both\b|meanings only/)
    }
  })
})
