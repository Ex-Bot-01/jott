/** Which colour scheme the window is in, and how to hear about it changing.
 *
 *  This used to be nobody's job. The stylesheet named ink and paper for both schemes, `index.html` painted its
 *  skeleton, and CodeMirror — never asked — ran its **light** facet the whole time: caret `1.2px solid black`,
 *  selection background a bright blue, native caret `black`. On a dark-mode Windows desktop that disagreement
 *  landed on the element a person looks at most, which is why the caret read as missing (§9's "readable first"
 *  is worth little if you cannot see where you are writing).
 *
 *  No `window` access at import time: this module is imported by tests that run outside a browser, and a module
 *  that throws on import takes the whole suite down with it. */

export type Scheme = 'light' | 'dark'

/** The dark query, spelled once. */
const DARK_QUERY = '(prefers-color-scheme: dark)'

/** A media list, or as much of one as this host bothers to provide — which is also the whole shape a test needs
 *  to hand in. Spelled structurally rather than as DOM's `MediaQueryList` so a fake does not have to impersonate
 *  an event target it will never be asked to be. */
export interface SchemeSource {
  matches: boolean
  addEventListener?: (type: string, listener: (event: { matches: boolean }) => void) => void
  removeEventListener?: (type: string, listener: (event: { matches: boolean }) => void) => void
}

/** The answer to "does the desktop want dark?", with nothing inferred beyond what it said. */
export function schemeOf(prefersDark: boolean): Scheme {
  return prefersDark ? 'dark' : 'light'
}

function hostMedia(): SchemeSource | undefined {
  if (typeof window === 'undefined') return undefined
  const matchMedia = (window as unknown as { matchMedia?: (query: string) => SchemeSource }).matchMedia
  if (typeof matchMedia !== 'function') return undefined
  try {
    return matchMedia.call(window, DARK_QUERY)
  } catch {
    // A webview that has the function and refuses this query is a host with no opinion, not a broken window.
    return undefined
  }
}

/** What scheme to paint right now. No media list means no opinion from the host, which means light — the scheme
 *  the palette was written for before dark mode existed here. */
export function currentScheme(): Scheme {
  return schemeOf(Boolean(hostMedia()?.matches))
}

/** Follow the scheme while the window is open.
 *
 *  `onChange` hears immediately as well as on every change: a caller that has to paint something needs an answer
 *  now, not at the next desktop toggle. Returns the unsubscribe, always callable — including when there was
 *  nothing to unsubscribe from, because a caller should never have to know which host it is on. */
export function watchScheme(
  onChange: (scheme: Scheme) => void,
  media?: SchemeSource | undefined,
): () => void {
  const list: SchemeSource | undefined = media === undefined ? hostMedia() : media
  if (!list) {
    onChange(schemeOf(false))
    return () => {}
  }

  const listener = (event: { matches: boolean }) => onChange(schemeOf(Boolean(event.matches)))
  onChange(schemeOf(Boolean(list.matches)))
  try {
    list.addEventListener?.('change', listener)
  } catch {
    // An older host that only has `addListener`, or none at all: the initial answer above still stands, and a
    // window that stops following a scheme change is a cosmetic problem, not a broken editor.
  }

  return () => {
    try {
      list.removeEventListener?.('change', listener)
    } catch {
      // Same reasoning as above: cleanup never fails loudly enough to be worth propagating.
    }
  }
}
