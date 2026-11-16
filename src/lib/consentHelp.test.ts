import { describe, expect, it } from 'vitest'
import { cardClipboard, cardSentences, panelLines, type ConsentOutcome } from './consentHelp'
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
  it('says nothing about a yes that went through', () => {
    // Removed at the tester's request, and they were right to ask: *"we don't need it anymore, because now the user
    // accepts it by downloading the model."* The card used to answer "what happened when I clicked?" — a question
    // that is now answered by a 295 MB strip moving across the screen (`lib/downloadWatch.ts`), and answered in
    // present tense instead of as a receipt. Its three buttons went with it: there is nothing left to copy, retry,
    // or complain about when the thing you asked for is visibly happening.
    const clean: Partial<ConsentOutcome>[] = [
      // np wrote both keys…
      {},
      // …or they were already written; either way the answer to the click is on screen, not here.
      { changed: [], fetchBytes: null },
      // `untouched`: Jott's guardrail was already where a yes puts it, so there is no news in that half either.
      { autoFetch: 'untouched' },
    ]
    for (const fields of clean) {
      expect(
        cardSentences({ help: help({ gap: 'ready', autoFetch: true }), refusal: null, outcome: outcome(fields) }),
        `a clean yes should say nothing: ${JSON.stringify(fields)}`,
      ).toEqual([])
    }
  })

  it('still speaks for a yes whose own half failed', () => {
    // Hiding the receipt is only honest when there was nothing wrong with it. Two states are actionable whatever np
    // did: Jott could not write its guardrail, or found no settings file to write it into — either way the download
    // they just agreed to may not be guarded, and a silent window would be hiding the one fact worth knowing.
    const why = 'could not write C:\\Users\\chris\\AppData\\Roaming\\note-processor\\config.toml: permission denied'
    const broken: Partial<ConsentOutcome>[] = [{ autoFetch: 'failed', autoFetchError: why }, { autoFetch: 'noConfigPath' }]
    for (const fields of broken) {
      const text = cardSentences({
        help: help({ gap: 'ready', autoFetch: true }),
        refusal: null,
        outcome: outcome(fields),
      }).join(' ')
      expect(text, JSON.stringify(fields)).not.toBe('')
    }
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

  it('keeps the answer on screen when np refuses something afterwards', () => {
    // Ten seconds after a press, np can refuse the embed pass for its own reason. The card must not revert to
    // paste-advice about a file np has just written, nor drop the refusal on the floor: both facts are true, and the
    // window used to lose one of them by clearing the receipt.
    const later = 'configuration error: config io: embeddings.model names no installed model'
    const text = cardSentences({
      help: help({ gap: 'ready', autoFetch: true, linesToAdd: ['provider = "local-model"'] }),
      refusal: later,
      outcome: outcome({}),
    }).join(' ')
    // The receipt half of this is gone since the tester asked for it (`outcomeNeedsCard`), but the half that can be
    // acted on stays: np refused something, and its sentence is the fact worth reading.
    expect(text).toContain(later)
    // The paste block is still not offered: those lines are in the file already.
    expect(text).not.toContain('provider = "local-model"')
  })

  it('does not print np’s sentence twice when the refusal is the answer', () => {
    const sentence = 'config io: no pinned model "no-such"'
    const sentences = cardSentences({
      help: help({ autoFetch: false }),
      refusal: sentence,
      outcome: outcome({ npRefused: sentence, changed: [], provider: null, model: null, persistedTo: null, fetchBytes: null, autoFetch: 'untouched' }),
    })
    expect(sentences.filter((line) => line === sentence)).toHaveLength(1)
  })

describe('what the settings panel says about embeddings', () => {
  it('says something true when nothing is missing', () => {
    // The card has no reason to speak when np's settings are complete — there is nothing to explain. The panel is
    // different: it is where somebody goes to find out the state, and it used to answer "Reading…" forever, because
    // an empty list of sentences is indistinguishable from a read still in flight.
    const facts = { help: help({ gap: 'ready', provider: 'local-model', modelId: 'embeddinggemma-2' }), refusal: null }
    const lines = panelLines(facts)
    expect(lines.length, 'a complete settings file is an answer, not a pending question').toBeGreaterThan(0)
    expect(lines.join(' ').toLowerCase()).toContain('embedding')
  })

  it('is the card’s sentences, and answers more only where the card holds back', () => {
    // One module answering both surfaces, so the panel cannot drift into describing settings the card contradicts.
    const missing = { help: help({ gap: 'providerMissing' }), refusal: null }
    expect(panelLines(missing)).toEqual(cardSentences(missing))

    // The one deliberate difference (`linesFor`'s `hideQuietYes`): a yes that went through has nothing to say in the
    // middle of the window and something to say in a panel somebody opened to ask. The panel is never *less* than
    // the card, so it cannot contradict it either.
    const quiet = { help: help({ gap: 'ready', autoFetch: true }), refusal: null, outcome: outcome({}) }
    expect(cardSentences(quiet)).toEqual([])
    expect(panelLines(quiet).length).toBeGreaterThan(0)
  })

  it('answers a clean yes with the state, while the card stays quiet', () => {
    // The two surfaces are allowed to differ *here* on purpose: the card has to earn its space over the download that
    // is visibly running; a panel somebody opened to ask "what is the state?" must not answer with silence.
    const outcome: ConsentOutcome = {
      granted: true,
      npRefused: null,
      changed: ['embeddings.provider', 'embeddings.model'],
      provider: 'local-model',
      model: 'embeddinggemma-2',
      persistedTo: FILE,
      fetchBytes: 309855456,
      autoFetch: 'written',
      autoFetchError: null,
    }
    const facts = { help: help({ gap: 'ready' }), refusal: null, outcome }
    expect(cardSentences(facts)).toEqual([])
    const said = panelLines(facts).join(' ')
    // What np recorded, in its own words, still readable where somebody goes looking for state.
    expect(said).toContain('note-processor wrote embeddings.provider')
    expect(said).toContain('AppData')
  })

  it('keeps np’s own answer when there is one', () => {
    const outcome: ConsentOutcome = {
      granted: true,
      npRefused: null,
      changed: ['embeddings.provider', 'embeddings.model'],
      provider: 'local-model',
      model: 'embeddinggemma-2',
      persistedTo: FILE,
      fetchBytes: 309855456,
      autoFetch: 'written',
      autoFetchError: null,
    }
    const facts = { help: help({ gap: 'ready' }), refusal: null, outcome }
    // Not equality with the card — a clean yes is quiet there (`outcomeNeedsCard`) — but the panel carries every word.
    const said = panelLines(facts).join(' ')
    expect(said).toContain('note-processor wrote embeddings.provider')
    expect(said).toContain('models.auto_fetch = true')
  })
})
