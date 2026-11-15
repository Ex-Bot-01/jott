// What the consent card says, in one tested module rather than in markup (§4).
//
// The card exists because of a specific failure: someone pressed "Allow the download", and the window answered with
// `keyword-only: embeddings.provider is not set` — a sentence from another program, with no hint that they had just
// been obeyed, or what was still missing. So every line here answers one of three questions: what Jott did, what
// note-processor still needs, and where to say so.

import type { ConsentHelp, ConsentOutcome } from './ipc'

export type { ConsentOutcome }

export interface ConsentFacts {
  /** What np's settings actually say right now, read by core. */
  help: ConsentHelp
  /** np's own refusal sentence, if it answered with one. Shown verbatim, never reworded or truncated. */
  refusal: string | null
  /** The answer note-processor gave to the button this window just pressed, if it pressed one. With it the card
   *  quotes what np recorded; without it the card can only describe the file. */
  outcome?: ConsentOutcome | null
}

/** Jott's part, stated as what happened to its key rather than as what was intended. "Wrote it" and "could not write
 *  it" are the two sentences a person can act on; the other two say there was no file to act on at all. */
function ourKeySentence(outcome: ConsentOutcome): string {
  const value = outcome.granted ? 'true' : 'false'
  switch (outcome.autoFetch) {
    case 'written':
      return `Jott wrote models.auto_fetch = ${value} in note-processor’s settings — its one key, and nothing else.`
    case 'untouched':
      return 'Jott left models.auto_fetch alone, so the download guardrail is exactly where it was.'
    case 'noConfigPath':
      return 'Jott found no note-processor settings file to write models.auto_fetch into, so that guardrail is unchanged.'
    case 'failed':
      return `Jott could not write its own key: ${outcome.autoFetchError ?? 'the write failed'}`
  }
}

/** The card once np has answered. Everything in the first line is np's own report — which keys it moved, into which
 *  file, what the fetch costs — with Jott's part said beside it, because they are different claims (§4). */
function outcomeSentences(outcome: ConsentOutcome): string[] {
  const where = outcome.persistedTo === null ? 'a file note-processor did not name' : `the file ${outcome.persistedTo}`

  if (outcome.npRefused !== null) {
    // np's sentence first: it is the specific fact, and it usually names the line worth fixing. Then the part a
    // person needs to know they are safe about — that Jott did not quietly allow anything.
    return [outcome.npRefused, ourKeySentence(outcome)]
  }

  if (outcome.granted) {
    const cost =
      outcome.fetchBytes === null
        ? ''
        : ` The download this allows is about ${Math.round((outcome.fetchBytes / (1024 * 1024)) * 10) / 10} MiB.`
    const moved =
      outcome.changed.length === 0
        ? 'note-processor had already recorded those settings, so nothing was rewritten.'
        : `note-processor wrote ${outcome.changed.join(' and ')} into ${where}.`
    return [moved + cost, ourKeySentence(outcome)]
  }

  const kept = outcome.model === null ? '' : ` It remembered the model you chose (${outcome.model}) in case you change your mind.`
  // "it wrote nothing" is true of the state and odd about the act: enabling what is already off changes no keys, and
  // np's own wording for that is that embeddings are off. Say that instead.
  const moved =
    outcome.changed.length === 0
      ? 'embeddings were already off, so no setting needed changing'
      : `it wrote ${outcome.changed.join(' and ')} into ${where}`
  return [`note-processor switched embeddings off: ${moved}.${kept}`, ourKeySentence(outcome)]
}

/** The clipboard line for Jott's own key — the same four states as `ourKeySentence`, in the terse form a log wants. */
function describeAutoFetch(outcome: ConsentOutcome): string {
  const value = outcome.granted ? 'true' : 'false'
  switch (outcome.autoFetch) {
    case 'written':
      return `wrote ${value}`
    case 'untouched':
      return 'left as it was'
    case 'noConfigPath':
      return 'not written: no settings file found'
    case 'failed':
      return `write failed: ${outcome.autoFetchError ?? 'unknown reason'}`
  }
}

/** The feature request for a way to answer this over the protocol instead of by hand. */
export const NP_ISSUE = 'https://github.com/Ex-Bot-01/note-processor/issues/7'

/** Jott's own part, stated first: it records an answer, and only an answer. */
function guardrail(autoFetch: boolean | null): string {
  if (autoFetch === true) {
    return 'You allowed model downloads. Jott recorded that as models.auto_fetch = true in note-processor’s settings — nothing else, and no note left this machine.'
  }
  if (autoFetch === false) {
    // No button is offered beside this sentence, so it must not point at one: the card that shows it carries Copy /
    // Try again and the issue link, and it stands in for the offer that would have allowed a download.
    return 'Model downloads are switched off in note-processor’s settings (models.auto_fetch = false), so nothing has been fetched.'
  }
  return 'Nobody has answered whether models may be downloaded yet, so Jott keeps models.auto_fetch = false and nothing arrives on its own.'
}

export function cardSentences(facts: ConsentFacts): string[] {
  const { help, refusal } = facts
  // Which setting is short is core's verdict (`settings_gap` in Rust), read off np's own file. Re-deriving it here
  // would be a second opinion the two sides could disagree about.

  // An answer pressed ten seconds ago outranks the file's state: it says what just happened, in np's words, and it is
  // the only version that can admit a refusal. `configUnreadable` is the exception — np will not rewrite a file it
  // cannot parse, so the sentence about the file stays true whichever button was pressed.
  if (facts.outcome && help.gap !== 'configUnreadable') {
    const lines = outcomeSentences(facts.outcome)
    // A refusal that arrives *after* the press is a second fact, not a replacement for it: np wrote those settings,
    // and ten seconds later it refused something else. Clearing the receipt (which this window used to do) put the
    // paste-advice back over a file np had just written.
    if (refusal !== null && refusal !== facts.outcome.npRefused) lines.push(refusal)
    return lines
  }

  if (help.gap === 'ready') return []

  if (help.gap === 'providerOther') {
    // An engine Jott does not recognise is not a problem to explain. "openai-compatible means an HTTP endpoint"
    // would be a guess about what someone chose it for; the honest sentence is that this window has no business here.
    return [
      `note-processor’s embedding engine is set to “${help.provider}”. Jott does not know what that means and will not guess about it — search keeps matching whole words meanwhile, and if that setting is not what you chose, note-processor’s own settings are the place to look.`,
    ]
  }

  if (help.gap === 'configUnreadable') {
    // Nothing can be quoted out of a file that will not parse, and lines to paste into it would land somewhere np has
    // already refused to read. So: np's own sentence first when it sent one, then the plain fact.
    const where =
      help.configPath === null ? 'note-processor’s settings file' : `note-processor’s settings file at ${help.configPath}`
    return [
      ...(refusal === null ? [] : [refusal]),
      `${where} could not be read: it is not valid TOML. Until that is fixed, no embedding setting takes effect, whichever way it is set.`,
    ]
  }

  const sentences: string[] = []
  if (refusal !== null) sentences.push(refusal)
  sentences.push(guardrail(help.autoFetch))
  sentences.push(
    help.gap === 'providerMissing'
      ? `Nothing was downloaded and search still matches whole words only: note-processor has no embedding engine turned on yet. ${help.linesToAdd.length} lines are missing from its settings.`
      : `An embedding engine is named but no model is, so nothing was downloaded and search still matches whole words only. ${help.linesToAdd.length} more line${help.linesToAdd.length === 1 ? '' : 's'} ${help.linesToAdd.length === 1 ? 'is' : 'are'} missing there.`,
  )
  return sentences
}

/** The text behind "Copy these settings": the file, the lines in order, and where to complain about having to do it. */
export function cardClipboard(facts: ConsentFacts): string {
  const { help, refusal } = facts

  // With an answer in hand there is nothing to paste: what belongs in the file went there through np itself.
  if (facts.outcome && help.gap !== 'configUnreadable') {
    const outcome = facts.outcome
    const lines = [`Jott answered the embeddings question (${outcome.granted ? 'allowed' : 'declined'})`]
    lines.push(`note-processor said: ${outcome.npRefused ?? 'ok'}`)
    if (outcome.changed.length > 0) lines.push(`note-processor changed: ${outcome.changed.join(', ')}`)
    if (outcome.persistedTo !== null) lines.push(`written to: ${outcome.persistedTo}`)
    if (outcome.fetchBytes !== null) lines.push(`fetch of: ${outcome.fetchBytes} bytes`)
    lines.push(`models.auto_fetch: ${describeAutoFetch(outcome)}`)
    return lines.join('\n')
  }

  const out = ['Jott could not start embeddings']
  if (refusal !== null) out.push(`note-processor said: ${refusal}`)
  out.push(`note-processor settings file: ${help.configPath ?? 'none found'}`)
  if (help.linesToAdd.length > 0) {
    out.push('Add these lines:', ...help.linesToAdd)
  }
  out.push(
    'Jott records your consent in models.auto_fetch and nothing else; choosing the engine itself is a request on note-processor:',
    NP_ISSUE,
  )
  return out.join('\n')
}
