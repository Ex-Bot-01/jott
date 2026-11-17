/** How wide the tree column may be, how tall the diagnostics drawer may be, and what the window remembers about
 *  having been dragged.
 *
 *  Both numbers used to be a stylesheet's decision. The sidebar was `minmax(19rem, 26rem)` — right as a *limit* for
 *  the reasons written below, wrong as a decision made once for every window on every monitor — and the drawer was
 *  capped twice over (`45vh` on itself, `22vh` on its activity list), so the height it wanted could never be the
 *  height a person got. Now they are dragged, and this module is the only place that decides what a drag may
 *  produce.
 *
 *  Why the floors are what they are:
 *
 *  - **266 px (the old `19rem`)** is where the sidebar's own labels stop breaking. Below it `+ note` and `Index now`
 *    wrap onto two lines and `INDEX.md` arrives as `INDEX.ma`, and a window whose labels are cut off cannot be
 *    trusted about somebody else's notes. That stays true however wide somebody drags the handle.
 *  - **The ceiling is proportional rather than fixed.** A remembered width belongs to the monitor it was chosen on,
 *    so a stored `900` must mean "as wide as this window can stand" on a laptop and not "leave the note pane 120 px".
 *    45 % of the window, never past 560 px: past that the tree is the thing you look at and the note is the strip.
 *  - **The drawer keeps 200 px of note pane** because a drawer that takes the whole window debugs nothing — the
 *    activity worth reading is happening in the note behind it. That is also why its ceiling is `min(70 %, h − 200)`
 *    rather than one fraction.
 *
 *  No `window` access at import time: this module is imported by tests that run outside a browser, and a module that
 *  throws on import takes the whole suite down with it (`darkMode.ts` says the same).
 */

/** The narrowest the tree column may be. See the file comment. */
export const SIDE_MIN = 266
/** The widest it may be however large the window is — past this the note pane loses. */
export const SIDE_MAX_CAP = 560
/** The share of the window the tree column may claim when the window is the limiting factor. */
export const SIDE_FRACTION = 0.45
/** What a window with no memory draws: the width every version up to 0.0.25 actually rendered at. */
export const SIDE_DEFAULT = 364

/** The shortest drawer worth opening: below this the facts block alone fills it and the activity list cannot be seen. */
export const DRAWER_MIN = 180
/** The share of the window's height the drawer may claim, when the note pane does not limit it first. */
export const DRAWER_FRACTION = 0.7
/** What a window with no memory draws: enough for the facts and a dozen activity rows at 14 px. */
export const DRAWER_DEFAULT = 320

/** The one key each size owns. Namespaced like `jott.search.*`, so another app on this origin cannot answer for Jott. */
export const SIDE_KEY = 'jott.layout.sidePx'
export const DRAWER_KEY = 'jott.layout.drawerPx'

export interface Layout {
  sidePx: number
  drawerPx: number
}

/** The `Storage` methods, and nothing else — so tests can hand in a Map-backed stand-in. */
type Store = Pick<Storage, 'getItem'>

type WritableStore = Pick<Storage, 'setItem'>

/** The widest this window will hold: the proportional share, capped absolutely and floored at what the labels need. */
export function sideMax(viewportW: number): number {
  const share = Math.floor(viewportW * SIDE_FRACTION)
  return Math.max(SIDE_MIN, Math.min(SIDE_MAX_CAP, share))
}

/** A width this window can hold — a dragged one, a remembered one, or the default.
 *
 *  A non-finite number is not a size anybody chose, so it becomes the default before it is clamped: clamping `NaN`
 *  answers `NaN`, and an unset CSS custom property would hand the column back to the stylesheet's opinion. */
export function clampSide(px: number, viewportW: number): number {
  const wanted = Number.isFinite(px) ? px : SIDE_DEFAULT
  return Math.min(sideMax(viewportW), Math.max(SIDE_MIN, wanted))
}

/** The width the pointer is asking for.
 *
 *  The handle sits on the column's right edge and the column begins at x = 0, so `clientX` *is* the width: no drag
 *  origin to capture, nothing to accumulate, and a pointer that leaves the window can only be clamped, never lost. */
export function sideFromPointer(clientX: number, viewportW: number): number {
  return clampSide(clientX, viewportW)
}

/** The height a drag asks for, measured from where the grab started.
 *
 *  Not "everything below the pointer", which is what the first version did: the drawer's bottom edge is *not* the
 *  window's — the status strip sits below it, and a border plus the bottom padding sit inside its declared height — so
 *  that reading overshot by about 40 px. Pressing the visible edge jumped the drawer taller, which slid the strip out
 *  from under the pointer and made the double-click reset impossible to land. A delta cannot drift, whatever else is
 *  underneath. */
export function drawerHeightFromDrag(originClientY: number, originHeightPx: number, clientY: number, viewportH: number): number {
  return clampDrawer(originHeightPx + (originClientY - clientY), viewportH)
}

/** The tallest this window will hold: 70 % of it, or whatever leaves the note pane 200 px — whichever is smaller,
 *  with the floor last so a comically short window still gets a drawer it can read. */
export function drawerMax(viewportH: number): number {
  const share = Math.floor(viewportH * DRAWER_FRACTION)
  return Math.max(DRAWER_MIN, Math.min(share, viewportH - 200))
}

/** A height this window can hold. Same non-finite rule as `clampSide`. */
export function clampDrawer(px: number, viewportH: number): number {
  const wanted = Number.isFinite(px) ? px : DRAWER_DEFAULT
  return Math.min(drawerMax(viewportH), Math.max(DRAWER_MIN, wanted))
}

/** The height the pointer is asking for. The handle is the drawer's top edge, so everything below it is the drawer:
 *  dragging up makes it taller, which is the direction people expect and the one a naive `clientY` gets backwards. */
export function drawerFromPointer(clientY: number, viewportH: number): number {
  return clampDrawer(viewportH - clientY, viewportH)
}

/** Both sizes as this window should draw them.
 *
 *  A stored value counts only as a plain integer — the same refusal `searchSettings.ts` makes, for the same reason:
 *  `'512px'` is a hand-edited key or another app's key, and sizing a window from a string Jott could not have written
 *  is a guess about intent. Whatever it reads is then clamped to the viewport it was asked about, which is how a
 *  desktop's drag survives arriving on a laptop. The stored value itself is **not** rewritten here: today's small
 *  window is no reason to forget tomorrow's large one. */
export function loadLayout(store: Store | null | undefined, viewportW: number, viewportH: number): Layout {
  let sideRaw: string | null = null
  let drawerRaw: string | null = null
  try {
    // Read both in one `try`: a webview that throws on the first key throws on the second, and half a read would pair
    // a remembered width with a default height without anybody having chosen either.
    sideRaw = store?.getItem(SIDE_KEY) ?? null
    drawerRaw = store?.getItem(DRAWER_KEY) ?? null
  } catch {
    return { sidePx: clampSide(SIDE_DEFAULT, viewportW), drawerPx: clampDrawer(DRAWER_DEFAULT, viewportH) }
  }
  const side = sideRaw !== null && /^\d+$/.test(sideRaw) ? Number(sideRaw) : SIDE_DEFAULT
  const drawer = drawerRaw !== null && /^\d+$/.test(drawerRaw) ? Number(drawerRaw) : DRAWER_DEFAULT
  return { sidePx: clampSide(side, viewportW), drawerPx: clampDrawer(drawer, viewportH) }
}

/** Remember the width of the tree column. A storage that refuses is swallowed: the drag works for this window either
 *  way, and a failed write is not a reason to tell somebody their window is broken. */
export function saveSide(store: WritableStore | null | undefined, px: number): void {
  try {
    store?.setItem(SIDE_KEY, String(px))
  } catch {
    // Nothing to do.
  }
}

/** Remember the height of the diagnostics drawer. Same rule as `saveSide`. */
export function saveDrawer(store: WritableStore | null | undefined, px: number): void {
  try {
    store?.setItem(DRAWER_KEY, String(px))
  } catch {
    // Nothing to do.
  }
}
