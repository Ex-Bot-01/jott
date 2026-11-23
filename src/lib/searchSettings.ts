// The two search choices the settings panel owns, remembered per app: which engine answers, and whether word
// beginnings count.
//
// Both are memories rather than per-search decisions. Each changes what a query *means* — `grand` reaching
// *grandmother*, or meanings outranking words — so they are asked for once by somebody whose notes work that way and
// then expected to stay. Defaults are note-processor's own defaults (both engines, whole words): Jott should not be
// the thing that quietly changes what a search means on first run.
//
// `PREFIX_KEY` is deliberately the same key v0.0.15 wrote, so upgrading keeps somebody's toggle instead of silently
// turning it off and making them find it again.

/** What to ask note-processor to match on. Its three labels, spelled as its own wire format spells them. */
export type SearchMode = 'keyword' | 'hybrid' | 'semantic'

/** The one key each half owns. Namespaced so another app sharing an origin cannot answer for Jott. */
export const MODE_KEY = 'jott.search.mode'
export const PREFIX_KEY = 'jott.search.prefix'
/** Whether a second step reorders the top results with note-processor's ranking model (np 0.7.0). */
export const RERANK_KEY = 'jott.search.rerank'

export interface SearchSettings {
  mode: SearchMode
  prefix: boolean
  rerank: boolean
}

/** What a window with no stored choice does — which is also the reading of any value this version cannot parse.
 *
 *  `rerank` is the one default that is Jott's rather than note-processor's (np ships its stage off): it was asked for
 *  on by default, and it changes only the *order* of what retrieval already found, never which notes match. */
export const DEFAULTS: SearchSettings = { mode: 'hybrid', prefix: false, rerank: true }

const MODES: readonly SearchMode[] = ['keyword', 'hybrid', 'semantic']

/** The two `Storage` methods, and nothing else — so tests can hand in a Map-backed stand-in. */
type Store = Pick<Storage, 'getItem'>

type WritableStore = Pick<Storage, 'setItem'>

/** Both choices at once. A storage that throws (hardened webview, exhausted quota, remote-origin code path) reads as
 *  "never asked", which is the defaults — a settings read is not worth losing the search box over. */
export function loadSettings(store: Store | null | undefined): SearchSettings {
  let modeRaw: string | null = null
  let prefixRaw: string | null = null
  let rerankRaw: string | null = null
  try {
    modeRaw = store?.getItem(MODE_KEY) ?? null
    // Read both in one `try`: a webview that throws on the first key throws on the second, and half a setting read is
    // worse than none because it would silently pair a remembered mode with a default toggle.
    prefixRaw = store?.getItem(PREFIX_KEY) ?? null
    rerankRaw = store?.getItem(RERANK_KEY) ?? null
  } catch {
    return { ...DEFAULTS }
  }
  return {
    // Only an exact spelling this build can send counts. `SEMANTIC` means something to a person, and honouring it
    // would switch someone to meanings-only search — the mode that cannot explain an empty result (note-processor#11)
    // — on a value nobody chose deliberately.
    mode: MODES.includes(modeRaw as SearchMode) ? (modeRaw as SearchMode) : DEFAULTS.mode,
    // Exactly `"1"`, for the same reason as v0.0.15: "true"/"yes"/"on" are guesses about intent.
    prefix: prefixRaw === '1',
    // Default on, so only the exact "no" this module writes turns it off.
    rerank: rerankRaw !== '0',
  }
}

/** Remember which engine to ask. Writes the label itself, so a future version can tell `keyword` from a rendering of
 *  it. A storage that refuses is swallowed: the choice stands for this window either way. */
export function saveMode(store: WritableStore | null | undefined, mode: SearchMode): void {
  try {
    store?.setItem(MODE_KEY, mode)
  } catch {
    // Nothing to do.
  }
}

/** Remember whether word beginnings count. `false` writes `"0"` rather than deleting the key: "answered no" is a fact
 *  a future version can act on, "never asked" is not. */
export function savePrefix(store: WritableStore | null | undefined, on: boolean): void {
  try {
    store?.setItem(PREFIX_KEY, on ? '1' : '0')
  } catch {
    // Nothing to do. The toggle works until the window closes.
  }
}

/** Remember whether search reranks. Same spelling as the prefix toggle: `"1"` or `"0"`, never a deleted key. */
export function saveRerank(store: WritableStore | null | undefined, on: boolean): void {
  try {
    store?.setItem(RERANK_KEY, on ? '1' : '0')
  } catch {
    // Nothing to do. The toggle works until the window closes.
  }
}

/** One sentence per mode, for the panel that offers them.
 *
 *  Written so each says what note-processor will actually do — including the part where `semantic` needs vectors on
 *  disk. That warning belongs here rather than only in the panel's markup, because then a test can hold it to account
 *  and a reworded heading cannot quietly drop it. */
/** The name of a mode, spelled once.
 *
 *  It used to exist only inside the openings of three sentences, which is how the status line came to say
 *  something else about the same choice: the panel said *Words only* while the corner of the window talked about
 *  meanings. A test now holds the sentence to the word (`modeSentence` must open with it), so the two cannot
 *  drift apart again by rewording one of them. */
export const MODE_WORD: Record<SearchMode, string> = {
  keyword: 'Words only',
  hybrid: 'Both',
  semantic: 'Meanings only',
}

export function modeSentence(mode: SearchMode): string {
  switch (mode) {
    case 'keyword':
      return `${MODE_WORD.keyword}: notes containing the exact words you typed, ranked by how well they match.`
    case 'hybrid':
      return `${MODE_WORD.hybrid}: the words you typed, plus notes that mean the same thing even when the words differ.`
    case 'semantic':
      return `${MODE_WORD.semantic}: notes that mean what you typed, even with different words. Needs the embedding model, and reports nothing where a folder has no vectors yet.`
  }
}
