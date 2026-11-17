/**
 * What a formatting press does to a range of text.
 *
 * Everything about formatting is arithmetic on offsets, and offset arithmetic is the kind that goes quietly wrong:
 * a wrap one character off does not crash, it writes `**bold` into somebody's note and waits until they reopen it.
 * So the arithmetic lives here, away from the editor, where a test can hold it against exact strings — including
 * the two cases a hand-built expectation gets wrong most often: a selection that already carries its markers, and
 * a character outside the basic plane in front of the caret (`🧭` is two UTF-16 units, exactly as CodeMirror counts
 * them, and so is this file).
 *
 * The buffer stays the only source of truth. An edit says which range to replace and where the selection ends up;
 * it never rewrites a character outside that range, because §14's conflict detection compares the hash of what Jott
 * wrote against the bytes on disk, and a formatter that tidied a note on the way past would raise a false conflict
 * for text nobody touched.
 */

/** The seven things the format bar offers. Order matters: `markdownState.ts` returns these in the same order, and
 *  a test comparing an array reads it as a sentence about which marks hold the caret. */
export type MarkKind = 'bold' | 'italic' | 'code' | 'link' | 'bullet' | 'task' | 'heading'

/** The selection the press acts on, in UTF-16 code units — the same unit CodeMirror counts in. */
export interface Sel {
  from: number
  to: number
}

/** One range change plus the selection that follows it — shaped so `Editor.svelte` can hand it straight to
 *  `view.dispatch({ changes: edit, selection: { anchor: edit.anchor, head: edit.head } })`. */
export interface Edit {
  from: number
  to: number
  insert: string
  anchor: number
  head: number
}

/** The characters an inline mark is made of. `link` is not a pair — it wraps as `[text](url)` — so it is absent. */
const PAIRS: Partial<Record<MarkKind, { open: string; close: string }>> = {
  bold: { open: '**', close: '**' },
  italic: { open: '_', close: '_' },
  code: { open: '`', close: '`' },
}

const WORD_CHAR = /[\p{L}\p{N}]/u

/** The mark's edit for this text and selection, or `null` when there is nothing honest to do. */
export function applyMark(text: string, sel: Sel, kind: MarkKind): Edit | null {
  if (kind === 'link') return linkEdit(text, sel)
  const pair = PAIRS[kind]
  if (pair) return pairEdit(text, sel, pair.open, pair.close)
  // The block marks come in Task 2. Until then a press on them does nothing at all, which is what "we have not
  // built that" should look like from a button's point of view.
  return null
}

/** Wrap a range in `open`/`close`, take them off again if they are already there, or open a pair around the caret. */
function pairEdit(text: string, sel: Sel, open: string, close: string): Edit {
  let from = Math.min(sel.from, sel.to)
  let to = Math.max(sel.from, sel.to)

  if (from === to) {
    // A word character on *both* sides means the caret is inside a word, and "bold" plainly means that word. One
    // side only — the start of a line, a space, the end of the note — and there is nothing to wrap: this press is
    // "start writing bold", so the pair goes in and the caret stays between the halves.
    const before = WORD_CHAR.test(text.charAt(from - 1))
    const after = from < text.length && WORD_CHAR.test(text.charAt(from))
    if (before && after) {
      while (from > 0 && WORD_CHAR.test(text.charAt(from - 1))) from -= 1
      while (to < text.length && WORD_CHAR.test(text.charAt(to))) to += 1
    } else {
      return { from, to, insert: open + close, anchor: from + open.length, head: from + open.length }
    }
  }

  // Already marked, in one of the two ways a person selects it: the markers are inside the selection (they dragged
  // across `**word**`) or outside it (they selected `word` and their eye was better than their caret). Both are a
  // request to take the mark off — nesting `****word****` would be the second press doing something nobody asked.
  const insideIsMarked = text.slice(from, from + open.length) === open && text.slice(to - close.length, to) === close
  if (insideIsMarked && to - close.length >= from + open.length) {
    const inner = text.slice(from + open.length, to - close.length)
    return { from, to, insert: inner, anchor: from, head: from + inner.length }
  }

  const outsideIsMarked =
    text.slice(from - open.length, from) === open && text.slice(to, to + close.length) === close
  if (outsideIsMarked) {
    // The words stay: this branch removes the two marker runs and nothing else. Replacing the whole span with an
    // empty string — which is what I wrote first, and the test caught it — would delete `Tuesday` along with its
    // asterisks, which is not "take bold off" under any reading.
    const inner = text.slice(from, to)
    return {
      from: from - open.length,
      to: to + close.length,
      insert: inner,
      anchor: from - open.length,
      head: from - open.length + inner.length,
    }
  }

  const inner = text.slice(from, to)
  return { from, to, insert: open + inner + close, anchor: from + open.length, head: from + open.length + inner.length }
}

/** `[text](url)` — the one control whose two halves are not a matched pair, so it gets its own branch. */
function linkEdit(text: string, sel: Sel): Edit {
  let from = Math.min(sel.from, sel.to)
  let to = Math.max(sel.from, sel.to)

  if (from === to) {
    const before = WORD_CHAR.test(text.charAt(from - 1))
    const after = from < text.length && WORD_CHAR.test(text.charAt(from))
    if (before && after) {
      while (from > 0 && WORD_CHAR.test(text.charAt(from - 1))) from -= 1
      while (to < text.length && WORD_CHAR.test(text.charAt(to))) to += 1
    } else {
      // `[]()` with the caret between the parentheses: the next thing typed is the URL, which is the half a person
      // has to fill in anyway.
      return { from, to, insert: '[]()', anchor: from + 3, head: from + 3 }
    }
  }

  // Already a link, selected whole → keep the label and drop the syntax. This is the press that reads as "take the
  // link off" in every editor anybody has used.
  const whole = /^\[([^\]]*)\]\([^)]*\)$/.exec(text.slice(from, to))
  if (whole) return { from, to, insert: whole[1], anchor: from, head: from + whole[1].length }

  // Or the selection is the label of a link whose URL runs on past it — `[Tuesday](u)` with `Tuesday` highlighted.
  // The closing parenthesis is outside the selection, so the change has to reach it; that is the one place in this
  // file where the edit is wider than what was selected, and it is wider only over link syntax.
  if (text.charAt(from - 1) === '[' && text.slice(to, to + 2) === '](') {
    const close = text.indexOf(')', to + 2)
    if (close !== -1) {
      const label = text.slice(from, to)
      return { from: from - 1, to: close + 1, insert: label, anchor: from - 1, head: from - 1 + label.length }
    }
  }

  const inner = text.slice(from, to)
  // Caret inside the parentheses, so the URL is typed rather than hunted for. `from + label + [ ] ( ` = +3 past the
  // opening bracket and label.
  return { from, to, insert: `[${inner}]()`, anchor: from + inner.length + 3, head: from + inner.length + 3 }
}
