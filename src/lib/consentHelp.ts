// What the consent card says, in one tested module rather than in markup (§4).
//
// The card exists because of a specific failure: someone pressed "Allow the download", and the window answered with
// `keyword-only: embeddings.provider is not set` — a sentence from another program, with no hint that they had just
// been obeyed, or what was still missing. So every line here answers one of three questions: what Jott did, what
// note-processor still needs, and where to say so.

import type { ConsentHelp } from './ipc'

export interface ConsentFacts {
  /** What np's settings actually say right now, read by core. */
  help: ConsentHelp
  /** np's own refusal sentence, if it answered with one. Shown verbatim, never reworded or truncated. */
  refusal: string | null
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
