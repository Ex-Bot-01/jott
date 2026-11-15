import { describe, expect, it } from 'vitest'
import { cardClipboard, cardSentences, type ConsentOutcome } from './consentHelp'
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

  it('says the file could not be read, and nothing else', () => {
    // Measured against np 0.4.0: a half-edited config answers `config parse error in …/config.toml: TOML parse
    // error at line 1, column 12 …`. The card must not add a claim about what that file says, or lines to paste
    // into it.
    const npSentence =
      'configuration error: config io: config parse error in /home/you/.config/note-processor/config.toml: TOML parse error at line 1, column 12'
    const facts = { help: help({ gap: 'configUnreadable', autoFetch: null, linesToAdd: [] }), refusal: npSentence }
    const sentences = cardSentences(facts)
    expect(sentences).toContain(npSentence)
    const text = sentences.join(' ')
    expect(text).toContain('could not be read')
    expect(text).not.toContain('models.auto_fetch = false')
    expect(text).not.toContain('auto_fetch = true')
    expect(text).not.toMatch(/lines? (are|is) missing/)
  })

  it('does not point at a button the card does not have', () => {
    // The guardrail sentence used to end "Say the word above to allow one." — while this branch of the window shows
    // Copy / Try again / the issue link, and suppresses the offer that would actually allow it.
    const text = cardSentences({ help: help({ autoFetch: false }), refusal: null }).join(' ')
    expect(text).toContain('models.auto_fetch = false')
    expect(text).not.toContain('Say the word above')
  })

/** What np answered over the pipe (0.5.x). Every field below is a measured shape of `embeddings.enable`’s answer. */
function outcome(fields: Partial<ConsentOutcome>): ConsentOutcome {
  return {
    granted: true,
    npRefused: null,
    changed: ['embeddings.provider', 'embeddings.model'],
    provider: 'local-model',
    model: 'embeddinggemma-2',
    persistedTo: 'C:\\Users\\chris\\AppData\\Roaming\\note-processor\\config.toml',
    fetchBytes: 309_855_456,
    autoFetch: 'written',
    autoFetchError: null,
    ...fields,
  }
}

describe('the card once np has answered', () => {
  it('leads with what note-processor recorded, in its own words', () => {
    const text = cardSentences({ help: help({ autoFetch: true }), refusal: null, outcome: outcome({}) }).join(' ')
    expect(text).toContain('embeddings.provider')
    expect(text).toContain('embeddings.model')
    expect(text).toContain('AppData')
    expect(text).toContain('models.auto_fetch = true')
  })

  it('drops the advice to edit settings by hand', () => {
    // np writes its own two keys now (note-processor#7). Telling somebody to paste lines into a file that just got
    // written for them is not belt-and-braces, it is a second, contradictory instruction.
    const h = help({ autoFetch: true, linesToAdd: ['[embeddings]', 'provider = "local-model"'] })
    const said = cardSentences({ help: h, refusal: null, outcome: outcome({}) }).join(' ')
    expect(said).not.toContain('provider = "local-model"')
    const clip = cardClipboard({ help: h, refusal: null, outcome: outcome({}) })
    expect(clip).not.toContain('Add these lines')
  })

  it('says so when np had nothing left to change', () => {
    // `changed: []` after an accepted yes means it was already on — a different sentence from "we turned it on".
    const text = cardSentences({
      help: help({ gap: 'ready', autoFetch: true }),
      refusal: null,
      outcome: outcome({ changed: [], fetchBytes: null }),
    }).join(' ')
    expect(text).toMatch(/already/i)
  })

  it('quotes the refusal and says what Jott did not touch', () => {
    const sentence = 'config io: no pinned model "no-such": this build can name "embeddinggemma-2"'
    const text = cardSentences({
      help: help({ autoFetch: false }),
      refusal: null,
      outcome: outcome({ npRefused: sentence, changed: [], provider: null, model: null, persistedTo: null, fetchBytes: null, autoFetch: 'untouched' }),
    }).join(' ')
    expect(text).toContain(sentence)
    // The guardrail claim is the important one: a refused yes must not read as "downloads allowed".
    expect(text).toMatch(/models.auto_fetch.*left alone|left.*models.auto_fetch/s)
  })

  it('reports a decline that np accepted, including the model it kept', () => {
    const text = cardSentences({
      help: help({ autoFetch: false }),
      refusal: null,
      outcome: outcome({ granted: false, changed: ['embeddings.provider'], provider: 'none', fetchBytes: null }),
    }).join(' ')
    expect(text).toContain('embeddings.provider')
    expect(text).toMatch(/switched embeddings off|no longer/i)
    // Revoking is not forgetting, and the card should not imply that it is.
    expect(text).toContain('embeddinggemma-2')
    expect(text).toContain('models.auto_fetch = false')
  })

  it('says when np said yes and Jott could not write its own key', () => {
    const why = 'could not write C:\\Users\\chris\\AppData\\Roaming\\note-processor\\config.toml: permission denied'
    const text = cardSentences({
      help: help({ autoFetch: false }),
      refusal: null,
      outcome: outcome({ autoFetch: 'failed', autoFetchError: why }),
    }).join(' ')
    expect(text).toContain(why)
    expect(text).toMatch(/could not|couldn’t/)
  })

  it('keeps the unreadable-file wording even after an answer', () => {
    // np will not rewrite a file it cannot parse, so 0.0.14’s sentence is still the true one (§5).
    const text = cardSentences({
      help: help({ gap: 'configUnreadable', autoFetch: null, linesToAdd: [] }),
      refusal: 'config io: config parse error in C:\\x\\config.toml: TOML parse error at line 1, column 12',
      outcome: outcome({ npRefused: 'config io: config parse error in C:\\x\\config.toml', autoFetch: 'untouched' }),
    }).join(' ')
    expect(text).toContain('not valid TOML')
  })

  it('still says nothing when there is nothing to report', () => {
    // `gap: ready` with no answer recorded is a healthy folder: the card stays out of the way.
    expect(cardSentences({ help: help({ gap: 'ready', autoFetch: true }), refusal: null, outcome: null })).toEqual([])
  })
})
