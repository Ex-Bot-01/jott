import { describe, expect, it } from 'vitest'
import { cardClipboard, cardSentences } from './consentHelp'
import type { ConsentHelp } from './ipc'

const FILE = 'C:\\Users\\chris\\AppData\\Roaming\\note-processor\\config.toml'
/** Measured against np 0.4.0: the fault sentence it answers when a provider is named and no model is. */
const REFUSAL =
  'configuration error: config io: embeddings.provider = "local-model" needs embeddings.model to name an installed model; none is set'

function help(fields: Partial<ConsentHelp>): ConsentHelp {
  return {
    gap: 'providerMissing',
    provider: null,
    modelId: null,
    linesToAdd: [],
    configPath: FILE,
    autoFetch: true,
    ...fields,
  }
}

describe('the card after a consent answer', () => {
  it('says what Jott recorded, and in which file', () => {
    const sentences = cardSentences({ help: help({ autoFetch: true }), refusal: null })
    const text = sentences.join(' ')
    expect(text).toContain('models.auto_fetch = true')
    // The card says plainly that consent was obeyed and that no download followed it — the two halves the
    // old one-liner left ambiguous.
    expect(text).toContain('Nothing was downloaded')
    expect(cardClipboard({ help: help({ autoFetch: true }), refusal: null })).toContain(FILE)
  })

  it('quotes np’s refusal byte for byte, in its own sentence', () => {
    const sentences = cardSentences({ help: help({ gap: 'modelMissing', provider: 'local-model', linesToAdd: ['model = "embeddinggemma-2"'] }), refusal: REFUSAL })
    expect(sentences).toContain(REFUSAL)
  })

  it('hands over the lines to copy, in order, with the file named', () => {
    const h = help({ linesToAdd: ['[embeddings]', 'provider = "local-model"', 'model = "embeddinggemma-2"'] })
    const text = cardClipboard({ help: h, refusal: REFUSAL })
    expect(text).toContain('[embeddings]\nprovider = "local-model"\nmodel = "embeddinggemma-2"')
    expect(text).toContain(FILE)
    expect(text).toContain('https://github.com/Ex-Bot-01/note-processor/issues/7')
  })

  it('advises nothing when the engine is something Jott does not know', () => {
    // "openai-compatible means an HTTP endpoint" would be a guess about what the person chose it for (§4).
    const facts = { help: help({ gap: 'providerOther', provider: 'openai-compatible' }), refusal: null }
    const text = cardSentences(facts).join(' ') + cardClipboard(facts)
    expect(text).toContain('openai-compatible')
    expect(text).not.toContain('provider = "local-model"')
  })

  it('says nothing when np is ready to embed', () => {
    const sentences = cardSentences({ help: help({ gap: 'ready', provider: 'local-model', modelId: 'embeddinggemma-2', autoFetch: true }), refusal: null })
    expect(sentences).toEqual([])
  })

  it('says the guardrail is what stands in the way when nobody has answered yet', () => {
    // A fresh np downloads by default; Jott writes `false` until asked. Nobody having answered is not the same
    // sentence as somebody having allowed it.
    const text = cardSentences({ help: help({ autoFetch: false }), refusal: null }).join(' ')
    expect(text).toContain('models.auto_fetch = false')
    expect(text).not.toContain('auto_fetch = true')
  })

  it('names no file when np had none to read', () => {
    const facts = { help: help({ autoFetch: null, configPath: null }), refusal: null }
    expect(cardSentences(facts).join(' ')).toContain('models.auto_fetch')
    expect(cardClipboard(facts)).not.toContain(FILE)
  })
})
