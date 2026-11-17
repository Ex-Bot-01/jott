// What the window is allowed to be shaped into, and what it remembers about having been shaped.
//
// Two numbers: how wide the tree column is, and how tall the diagnostics drawer is. Both are decided by a person's
// drag rather than by a stylesheet, which means both need a floor that keeps every label in one piece, a ceiling
// that leaves the note pane readable, and a memory that survives a restart without outliving its usefulness — a
// width chosen on a 27" monitor must not arrive on a laptop and crush it.
//
// All of that is arithmetic, so it lives here rather than in a component: `jsdom` has no layout engine, which makes
// these the only part of the feature a test can hold to account, and the part worth holding.

import { describe, expect, it } from 'vitest'
import {
  DRAWER_DEFAULT,
  DRAWER_KEY,
  DRAWER_MIN,
  SIDE_DEFAULT,
  SIDE_KEY,
  SIDE_MAX_CAP,
  SIDE_MIN,
  clampDrawer,
  clampSide,
  drawerFromPointer,
  drawerHeightFromDrag,
  drawerMax,
  loadLayout,
  saveDrawer,
  saveSide,
  sideFromPointer,
  sideMax,
} from './layoutSize'

/** A stand-in for `Storage` with exactly the methods this module is allowed to use — same shape as
    `searchSettings.test.ts`'s, because a settings read should not need two different fakes to be tested. */
function store(initial: Record<string, string> = {}) {
  const data = new Map(Object.entries(initial))
  return {
    data,
    getItem: (key: string) => (data.has(key) ? data.get(key)! : null),
    setItem: (key: string, value: string) => void data.set(key, value),
  }
}

/** A webview that refuses to be read or written at all. Sizing a window is not worth losing the note for. */
const hostile = {
  getItem: () => {
    throw new Error('SecurityError')
  },
  setItem: () => {
    throw new Error('QuotaExceededError')
  },
}

describe('the limits', () => {
  it('never lets the sidebar take more than 45 % of the window, or 560 px', () => {
    expect(sideMax(1920)).toBe(SIDE_MAX_CAP) // capped, not 45 % of a huge screen
    expect(sideMax(1200)).toBe(540)
    expect(sideMax(820)).toBe(369)
  })

  it('keeps the floor when the window is too narrow for either rule', () => {
    // 266 px is where `+ note` and `Index now` stop wrapping. Below it the window's own labels break, which is the
    // bug the old fixed track was written to avoid — so the floor holds even when it leaves the note pane thin.
    expect(sideMax(400)).toBe(SIDE_MIN)
  })

  it('holds a dragged or remembered width inside those limits', () => {
    expect(clampSide(100, 1200)).toBe(SIDE_MIN)
    expect(clampSide(4000, 1200)).toBe(540)
  })

  it('reads a number nobody chose as the default width', () => {
    // `NaN` arrives from arithmetic on a measurement that was never taken (a webview with no innerWidth yet). The
    // honest answer is the width this window would have had anyway, not zero and not the ceiling.
    expect(clampSide(Number.NaN, 1200)).toBe(SIDE_DEFAULT)
    expect(clampDrawer(Number.POSITIVE_INFINITY, 768)).toBe(DRAWER_DEFAULT)
  })

  it('takes a sidebar width straight from the pointer', () => {
    // The handle sits at the column's right edge and the column starts at x = 0, so the pointer's own x *is* the
    // width — no drag origin to remember, and nothing to drift when the pointer leaves the window.
    expect(sideFromPointer(500, 1200)).toBe(500)
    expect(sideFromPointer(-40, 1200)).toBe(SIDE_MIN)
  })

  it('caps the drawer at 70 % of the window but never below the note pane', () => {
    expect(drawerMax(768)).toBe(537)
    expect(drawerMax(400)).toBe(200) // h - 200: a short window keeps a note pane worth looking at
  })

  it('holds a dragged or remembered height inside those limits', () => {
    expect(clampDrawer(10, 768)).toBe(DRAWER_MIN)
    expect(clampDrawer(5000, 768)).toBe(537)
  })

  it('keeps the height you grabbed when you grab the edge where it already is', () => {
    // The drawer's bottom edge is not the window's: the status strip sits below it, and a border plus the bottom
    // padding sit inside its declared height. So "everything below the pointer" overshoots — pressing the visible
    // edge jumped the drawer about 40 px taller, which also slid the strip out from under the pointer so a
    // double-click never landed twice. A drag is therefore a delta from where the grab started.
    expect(drawerHeightFromDrag(418, 320, 418, 768)).toBe(320)
    expect(drawerHeightFromDrag(418, 320, 378, 768)).toBe(360)
    expect(drawerHeightFromDrag(418, 320, 100, 768)).toBe(537) // still clamped
    expect(drawerHeightFromDrag(418, 320, 700, 768)).toBe(DRAWER_MIN)
  })

  it('grows the drawer when its top edge is dragged up', () => {
    // The handle is the drawer's top edge, so the height is everything below the pointer: `clientY` 400 in a 768 px
    // window means 368 px of drawer. Dragging it to 200 makes it taller, not shorter.
    expect(drawerFromPointer(400, 768)).toBe(368)
    expect(drawerFromPointer(200, 768)).toBe(537) // clamped: the note pane keeps its 200 px
  })
})

describe('the memory', () => {
  it('starts at the default shape when nobody has dragged anything', () => {
    expect(loadLayout(null, 1200, 768)).toEqual({ sidePx: 364, drawerPx: 320 })
    expect(loadLayout(store(), 1200, 768)).toEqual({ sidePx: 364, drawerPx: 320 })
  })

  it('reads back both numbers as they were left', () => {
    expect(loadLayout(store({ [SIDE_KEY]: '512', [DRAWER_KEY]: '240' }), 1200, 768)).toEqual({ sidePx: 512, drawerPx: 240 })
  })

  it('re-clamps a remembered width to the window it landed in', () => {
    // The point of clamping on load: this is the laptop that inherited the desktop's drag. The stored value itself is
    // left alone by `loadLayout` — it only answers what *this* window can hold.
    const wide = { [SIDE_KEY]: '900' }
    expect(loadLayout(store(wide), 820, 768).sidePx).toBe(369)
    expect(loadLayout(store(wide), 1920, 768).sidePx).toBe(SIDE_MAX_CAP)
  })

  it('refuses every value this version never wrote', () => {
    // `'512px'` and `'3.5'` are what a hand-edited key or another app on the same origin looks like. Guessing at them
    // means sizing somebody's window from a string Jott could not have produced.
    for (const junk of ['512px', '', '-4', '3.5', 'yes']) {
      expect(loadLayout(store({ [SIDE_KEY]: junk, [DRAWER_KEY]: junk }), 1200, 768)).toEqual({
        sidePx: SIDE_DEFAULT,
        drawerPx: DRAWER_DEFAULT,
      })
    }
  })

  it('writes a plain integer per key', () => {
    const s = store()
    saveSide(s, 412)
    saveDrawer(s, 300)
    expect(s.data.get(SIDE_KEY)).toBe('412')
    expect(s.data.get(DRAWER_KEY)).toBe('300')
  })

  it('treats a storage that throws as "never asked" in both directions', () => {
    expect(loadLayout(hostile, 1200, 768)).toEqual({ sidePx: 364, drawerPx: 320 })
    expect(() => saveSide(hostile, 400)).not.toThrow()
    expect(() => saveDrawer(hostile, 400)).not.toThrow()
  })
})
