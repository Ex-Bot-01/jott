// @vitest-environment jsdom
import { describe, expect, it } from 'vitest'

import { bareKeyFor, isTypingTarget, shortcutFor } from './shortcuts'

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
})
