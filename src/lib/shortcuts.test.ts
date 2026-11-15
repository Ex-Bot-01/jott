// @vitest-environment jsdom
import { describe, expect, it } from 'vitest'

import { bareKeyFor, escapeAction, isTypingTarget, shortcutFor } from './shortcuts'

const key = (
  over: Partial<{ ctrlKey: boolean; metaKey: boolean; shiftKey: boolean; altKey: boolean; key: string }>,
) => ({
  ctrlKey: false,
  metaKey: false,
  shiftKey: false,
  altKey: false,
  key: '',
  ...over,
})

describe('the modified keys', () => {
  it('binds the five the plan promised', () => {
    expect(shortcutFor(key({ ctrlKey: true, key: 'n' }))).toBe('new-note')
    expect(shortcutFor(key({ ctrlKey: true, shiftKey: true, key: 'n' }))).toBe('new-folder')
    expect(shortcutFor(key({ ctrlKey: true, key: 'r' }))).toBe('rename')
    expect(shortcutFor(key({ ctrlKey: true, key: 'f' }))).toBe('search')
    expect(shortcutFor(key({ ctrlKey: true, key: 's' }))).toBe('save')
  })

  it('reads Cmd as Ctrl, because a tester on a Mac should get the same app', () => {
    expect(shortcutFor(key({ metaKey: true, key: 's' }))).toBe('save')
    expect(shortcutFor(key({ metaKey: true, shiftKey: true, key: 'd' }))).toBe('diagnostics')
  })

  it('does not read Ctrl+Shift+N as Ctrl+N', () => {
    // Two bindings on one letter, told apart by Shift. Collapsing them would make "new folder" unreachable.
    expect(shortcutFor(key({ ctrlKey: true, shiftKey: true, key: 'n' }))).toBe('new-folder')
    expect(shortcutFor(key({ ctrlKey: true, key: 'N' }))).toBe('new-note')
  })

  it('leaves every other combination alone', () => {
    expect(shortcutFor(key({ ctrlKey: true, key: 't' }))).toBeNull()
    expect(shortcutFor(key({ ctrlKey: true, shiftKey: true, key: 's' }))).toBeNull()
    expect(shortcutFor(key({ key: 's' }))).toBeNull()
  })

  it('keeps Escape for closing the drawer', () => {
    expect(shortcutFor(key({ key: 'Escape' }))).toBe('close-drawer')
  })
})

describe('the bare keys', () => {
  it('binds F2 and Delete to the selection', () => {
    expect(bareKeyFor(key({ key: 'F2' }))).toBe('rename')
    expect(bareKeyFor(key({ key: 'Delete' }))).toBe('delete')
  })

  it('does not fire on a plain letter, which is somebody typing', () => {
    expect(bareKeyFor(key({ key: 'd' }))).toBeNull()
    expect(bareKeyFor(key({ ctrlKey: true, key: 'Delete' }))).toBeNull()
  })

  it('knows when focus is on something being typed into', () => {
    document.body.innerHTML = `
      <div id="editor" contenteditable="true"></div>
      <input id="box" />
      <div id="row"><button>x</button></div>
    `
    expect(isTypingTarget(document.getElementById('editor'))).toBe(true)
    expect(isTypingTarget(document.getElementById('box'))).toBe(true)
    expect(isTypingTarget(document.getElementById('row'))).toBe(false)
  })

  /** Escape is the key people reach for to make something go away, and several things can be "something". */
  describe('what Escape is for', () => {
    it('dismisses the topmost thing first, and closes the note only when nothing covers it', () => {
      expect(escapeAction({ menuOpen: true, settingsOpen: false, aboutOpen: false, dialogOpen: false, drawerOpen: true, noteOpen: true, typing: false })).toBe('menu')
      expect(escapeAction({ menuOpen: false, settingsOpen: false, aboutOpen: false, dialogOpen: false, drawerOpen: true, noteOpen: true, typing: false })).toBe('drawer')
      expect(escapeAction({ menuOpen: false, settingsOpen: false, aboutOpen: false, dialogOpen: false, drawerOpen: false, noteOpen: true, typing: false })).toBe('note')
    })

    it('does not close a note out from under someone who is typing', () => {
      // The standing rule for unmodified keys: inside a text target they mean "type this". Dismissing a menu or
      // the drawer still happens from there — that is what Escape is for when something covers the screen — but
      // throwing away the visible note is not a dismissal.
      expect(escapeAction({ menuOpen: false, settingsOpen: false, aboutOpen: false, dialogOpen: false, drawerOpen: false, noteOpen: true, typing: true })).toBe('nothing')
      expect(escapeAction({ menuOpen: true, settingsOpen: false, aboutOpen: false, dialogOpen: false, drawerOpen: false, noteOpen: true, typing: true })).toBe('menu')
    })

    it('does nothing when there is nothing to dismiss', () => {
      expect(escapeAction({ menuOpen: false, settingsOpen: false, aboutOpen: false, dialogOpen: false, drawerOpen: false, noteOpen: false, typing: false })).toBe('nothing')
    })
  })

})

describe('the settings key', () => {
  it('is Ctrl+comma, and Cmd+comma on a Mac tester', () => {
    expect(shortcutFor(key({ ctrlKey: true, key: ',' }))).toBe('settings')
    expect(shortcutFor(key({ metaKey: true, key: ',' }))).toBe('settings')
  })

  it('is not any other comma', () => {
    // Typing a comma into a note is the commonest comma there is.
    expect(shortcutFor(key({ key: ',' }))).toBeNull()
    expect(shortcutFor(key({ ctrlKey: true, shiftKey: true, key: ',' }))).toBeNull()
  })
})

describe('Escape with the settings panel open', () => {
  it('closes the panel before the drawer or the note', () => {
    // Whichever overlay is on top is the thing the person is trying to get out of the way.
    expect(escapeAction({ menuOpen: false, settingsOpen: true, aboutOpen: false, dialogOpen: false, drawerOpen: true, noteOpen: true, typing: false })).toBe('settings')
    expect(escapeAction({ menuOpen: true, settingsOpen: true, aboutOpen: false, dialogOpen: false, drawerOpen: true, noteOpen: true, typing: false })).toBe('menu')
  })

  it('still stands down on a note when nothing covers the screen', () => {
    expect(escapeAction({ menuOpen: false, settingsOpen: false, aboutOpen: false, dialogOpen: false, drawerOpen: false, noteOpen: true, typing: true })).toBe('nothing')
  })
})

describe('Escape with the other overlays', () => {
  it('closes About rather than the note behind it', () => {
    // The bug this pins: About dismisses itself in its own markup while `escapeAction`, blind to it, also returns
    // "note". One press, one thing — and the thing is never the note someone was writing.
    expect(escapeAction({ menuOpen: false, settingsOpen: false, aboutOpen: true, dialogOpen: false, drawerOpen: false, noteOpen: true, typing: false })).toBe('about')
  })

  it('dismisses a dialog before it closes the note behind it', () => {
    // Pressing Escape to cancel "Move to…" must not throw away the note visible behind the dialog.
    expect(escapeAction({ menuOpen: false, settingsOpen: false, aboutOpen: false, dialogOpen: true, drawerOpen: false, noteOpen: true, typing: false })).toBe('dialog')
  })

  it('still takes one press per thing, topmost first', () => {
    const order = [
      'menu', 'settings', 'about', 'dialog', 'drawer', 'note',
    ] as const
    for (const [index, expected] of order.entries()) {
      const facts = {
        menuOpen: index === 0,
        settingsOpen: index === 1,
        aboutOpen: index === 2,
        dialogOpen: index === 3,
        drawerOpen: index === 4,
        noteOpen: true,
        typing: false,
      }
      expect(escapeAction(facts), `only ${expected} is on top`).toBe(expected)
    }
  })

  it('does not close a note when only the note would be next and someone is typing', () => {
    expect(escapeAction({ menuOpen: false, settingsOpen: false, aboutOpen: false, dialogOpen: false, drawerOpen: false, noteOpen: true, typing: true })).toBe('nothing')
  })
})
