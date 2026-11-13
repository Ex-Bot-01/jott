/**
 * The keyboard, decided in one place so the window answers a key the same way wherever focus happens to be.
 *
 * A webview has no menu bar, which means these bindings are the menu. Each one is also the answer to something a
 * person tried before it existed: Ctrl+S pressed and nothing happening but a silent autosave two seconds later is
 * not the same as being told "saved".
 */

export type Shortcut =
  | 'new-note'
  | 'new-folder'
  | 'rename'
  | 'delete'
  | 'save'
  | 'search'
  | 'diagnostics'
  | 'close-drawer'

type KeyLike = { ctrlKey: boolean; metaKey: boolean; shiftKey: boolean; key: string }

/** `meta` counts as `ctrl`: Windows is the target, and a Mac tester should get the same app. */
export function shortcutFor(event: KeyLike): Shortcut | null {
  const mod = event.ctrlKey || event.metaKey
  const key = event.key.toLowerCase()
  if (!mod) return event.key === 'Escape' ? 'close-drawer' : null
  // Shift is spelled out per binding rather than ignored, so Ctrl+Shift+N cannot also be read as Ctrl+N.
  if (event.shiftKey) {
    switch (key) {
      case 'n':
        return 'new-folder'
      case 'd':
        return 'diagnostics'
      default:
        return null
    }
  }
  switch (key) {
    case 'n':
      return 'new-note'
    case 'r':
      return 'rename'
    case 'f':
      return 'search'
    case 's':
      return 'save'
    default:
      return null
  }
}

/** Delete and F2 have no modifier, and are the two bindings that must not fire while someone is typing a note. */
export function bareKeyFor(event: { key: string; ctrlKey: boolean; metaKey: boolean; altKey: boolean }): 'rename' | 'delete' | null {
  if (event.ctrlKey || event.metaKey || event.altKey) return null
  if (event.key === 'F2') return 'rename'
  if (event.key === 'Delete') return 'delete'
  return null
}

/** Whether a keypress belongs to the tree rather than to whatever has focus: typing in CodeMirror or in the
 *  search box must not rename or throw away the selected note. */
export function isTypingTarget(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) return false
  // CodeMirror's content element is a `contenteditable` div, which `tagName` alone would call safe to ignore.
  return Boolean(target.closest('input, textarea, [contenteditable="true"], [role="textbox"]'))
}
