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
})
