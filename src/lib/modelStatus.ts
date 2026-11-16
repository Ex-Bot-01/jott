import { mebibytes } from './statusLine'

/**
 * The model dot in the status line, decided from note-processor's answers.
 *
 * The ask was "a green dot when the embedding model is active / downloaded / online" — three states wearing one hat.
 * They are measurable separately, and blurring them costs a person their afternoon: a green dot on a folder nobody has
 * embedded yet reads as "search by meaning is working", and a grey dot because np did not answer reads as "I switched
 * it off", which is how someone ends up clicking Allow on a download they already own.
 *
 * So the states come from three distinct facts:
 *   · `provider`  — np's `embeddings.provider`, read from its settings file (no round trip). `"none"` is what
 *                   `embeddings.disable` writes and means off; absence was never configured.
 *   · `present`   — whether np lists the model's rows, i.e. its weights are on this disk. `null` means nobody answered:
 *                   np unreachable, or a models directory that would not list. That is its own state, not "no".
 *   · `embeddedNotes` — how many notes in *this* folder carry an embedded marker, from Jott's per-file stamps.
 */
export type ModelFacts = {
  /** np's `embeddings.provider`, quoted as written. `null` is "the file names none", not "none". */
  provider: string | null
  /** The model this folder would embed with, when np named one. Never guessed at. */
  modelId: string | null
  /** Are the weights on this disk? `null` = nobody was able to say. */
  present: boolean | null
  /** What the download would cost, from np's pinned table, so a waiting dot can name what it waits for. */
  bytesTotal: number | null
  /** Notes in this folder that carry an embedded marker — Jott's own stamps, not np's claim about the vault. */
  embeddedNotes: number
  /** np's settings file exists and will not parse. Not "no provider": the same flattening the consent card refuses, in
      a place that used to say "embeddings off" about a file nobody could read. */
  configUnreadable: boolean
  /** np's own sentence for a models directory it could not list, when it sent one. It is running and reporting that it
      cannot look, which is a different thing from silence. */
  modelsDirError: string | null
}

export type DotState = 'active' | 'ready' | 'waiting' | 'off' | 'unknown' | 'other'

/**
 * One word per state. Colour cannot carry meaning on its own — a colour-blind tester, a screen reader, and a screenshot
 * pasted into a bug report all need the word, so it leads the aria-label and is drawn as text in forced-colors mode.
 */
export const DOT_LABEL: Record<DotState, string> = {
  // Was "meanings on", and a tester read it exactly as it sounds: *then why is my search using words?*. This dot
  // measures the model and this folder's vectors; it has no idea what the next search will ask for, and Settings'
  // own choice is now spelled in the corner beside it. So the word names the fact that was measured.
  active: 'notes embedded',
  ready: 'model ready',
  waiting: 'model not downloaded',
  off: 'embeddings off',
  unknown: 'model state unknown',
  other: 'another provider',
}

/** `local-model` is the one provider whose weights live in np's models directory — measured, and what `embeddings.enable` writes. */
const LOCAL_PROVIDER = 'local-model'

export function dotState(facts: ModelFacts): DotState {
  // Before anything else: a settings file that will not parse is not a settings file that says "off". It outranks even
  // a provider quoted from it, because nothing could have been quoted from a file that does not parse.
  if (facts.configUnreadable) return 'unknown'
  const provider = facts.provider
  if (provider === null || provider.toLowerCase() === 'none') return 'off'
  if (provider.toLowerCase() !== LOCAL_PROVIDER) return 'other'
  // Order matters: an unanswered question outranks a folder's markers, because "nobody knows" is the fact about np and
  // the markers only say what used to be true.
  if (facts.present === null) return 'unknown'
  if (!facts.present) return 'waiting'
  return facts.embeddedNotes > 0 ? 'active' : 'ready'
}

/** The sentence behind the dot — also its `aria-label` and tooltip, so the meaning survives a colourless rendering. */
export function dotTitle(facts: ModelFacts): string {
  const state = dotState(facts)
  const model = facts.modelId !== null ? facts.modelId : 'the model note-processor would use'
  switch (state) {
    case 'active':
      return `${DOT_LABEL[state]} — ${model} is on this disk and ${facts.embeddedNotes} ${
        facts.embeddedNotes === 1 ? 'note in this folder has' : 'notes in this folder have'
      } vectors.`
    case 'ready':
      return `${DOT_LABEL[state]} — ${model} is on this disk; nothing in this folder has been embedded yet. [embed] does the whole folder.`
    case 'waiting': {
      const price = facts.bytesTotal === null ? '' : ` (about ${mebibytes(facts.bytesTotal)} MiB)`
      return `${DOT_LABEL[state]} — embeddings are allowed, but ${model} is not on this disk yet${price}. Settings can start the download.`
    }
    case 'off':
      return `${DOT_LABEL[state]} — search uses words only. Settings turns embeddings on.`
    case 'unknown': {
      // Never "off", which would be a claim about a setting nobody could read. Four ways to get here and four honest
      // sentences: an unreadable file, a models directory np cannot list (which it *told* us about), no model named,
      // and plain silence from a process that did not reply.
      const cause = facts.configUnreadable
        ? 'note-processor’s settings file could not be read'
        : facts.modelsDirError !== null
          ? `note-processor could not read its models directory (${facts.modelsDirError})`
          : facts.modelId === null
            ? 'note-processor could not say which model this folder would use'
            : 'note-processor did not answer'
      return `${DOT_LABEL[state]} — ${cause}, so whether the model is here is not known yet.`
    }
    case 'other':
      return `${DOT_LABEL[state]} — this folder embeds through ${facts.provider}, so Jott cannot say what its weights look like. Whether meanings were searched appears with each search result.`
  }
}
