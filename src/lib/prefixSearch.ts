// The remembered "match word beginnings" choice (note-processor#6, np 0.5.x).
//
// Deliberately a memory rather than a per-search decision: turning prefix matching on changes what a query *means*,
// and it is asked for once by somebody whose notes are full of word families — `grand` reaching *grandmother* — and
// then expected to stay. Off is the default because the same flag in a vault of identifiers reports notes nobody
// asked about, and because np's own default is off: Jott should not be the thing that quietly widens a search.

/** The one key this module owns. Namespaced so another app sharing an origin cannot answer for Jott. */
export const PREFIX_KEY = 'jott.search.prefix'

/** The two `Storage` methods, and nothing else — so tests can hand in a Map-backed stand-in. */
type Store = Pick<Storage, 'getItem'>

type WritableStore = Pick<Storage, 'setItem'>

/** Whether word beginnings should match. Anything other than exactly `"1"` means no, including a value this version
 *  does not recognise; guessing from `"true"` or `"yes"` would widen somebody's search on a coin toss. */
export function loadPrefix(store: Store | null | undefined): boolean {
  try {
    return store?.getItem(PREFIX_KEY) === '1'
  } catch {
    // A hardened or exhausted webview throws on access. The search box still works, on whole words.
    return false
  }
}

/** Remember the answer. `false` writes `"0"` rather than deleting the key: "answered no" is a fact a future version
 *  can act on, "never asked" is not. A storage that refuses is swallowed — the toggle works for this window either
 *  way, and throwing would take the search handler down with it. */
export function savePrefix(store: WritableStore | null | undefined, on: boolean): void {
  try {
    store?.setItem(PREFIX_KEY, on ? '1' : '0')
  } catch {
    // Nothing to do. The choice stands until the window closes.
  }
}
