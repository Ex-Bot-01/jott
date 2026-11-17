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
  if (kind === 'bullet' || kind === 'task') return blockEdit(text, sel, kind)
  if (kind === 'heading') return headingEdit(text, sel)
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

/** A list marker at the start of a line, and a checkbox sitting in front of it. */
const LIST_PREFIX = /^(\s*)([-*+]|\d+[.)])\s+/
const TASK_PREFIX = /^(\s*)([-*+]|\d+[.)])\s+\[[ xX]\]\s+/

/** Prefixes are per line, so a block mark is decided over whole lines — a `- ` in the middle of a selection would
 *  put a bullet on half a sentence. One change covers the whole touched block, which keeps one press one undo step. */
function blockEdit(text: string, sel: Sel, kind: 'bullet' | 'task'): Edit | null {
  if (text.length === 0) return null
  const from = Math.min(sel.from, sel.to)
  const to = Math.max(sel.from, sel.to)

  let start = from
  while (start > 0 && text.charAt(start - 1) !== '\n') start -= 1
  let end = to
  while (end < text.length && text.charAt(end) !== '\n') end += 1
  if (end === start) return null

  const lines = text.slice(start, end).split('\n')
  const blank = (line: string) => line.trim() === ''
  let changed: string[]

  if (kind === 'bullet') {
    // Every line already bulleted means the press takes the bullet off. One plain line among them means it puts
    // bullets on all of them — a control that only ever removed things would make the list harder to start than to
    // end, and nobody reads "nothing happened" as a partial success.
    const allMarked = lines.every((l) => blank(l) || LIST_PREFIX.test(l))
    changed = lines.map((l) => (blank(l) ? l : allMarked ? stripList(l) : addBullet(l)))
  } else {
    const allTasks = lines.every((l) => blank(l) || TASK_PREFIX.test(l))
    changed = lines.map((l) => (blank(l) ? l : allTasks ? stripCheckbox(l) : addTask(l)))
  }

  const block = text.slice(start, end)
  const insert = changed.join('\n')
  return { from: start, to: end, insert, anchor: shift(from, start, lines, changed), head: shift(to, start, lines, changed) }
}

/** Where an offset in the old block lands after the prefixes changed. Offsets before the block do not move; one
 *  inside a line moves by that line's prefix difference; one past the block moves by all of them. */
function shift(offset: number, start: number, lines: string[], changed: string[]): number {
  let pos = start
  let acc = 0
  for (let i = 0; i < lines.length; i++) {
    const lineStart = pos
    const lineEnd = pos + lines[i].length
    const delta = changed[i].length - lines[i].length
    if (offset <= lineStart) return offset + acc
    if (offset <= lineEnd) return Math.max(lineStart + acc, offset + acc + delta)
    acc += delta
    pos = lineEnd + 1
  }
  return offset + acc
}

function addBullet(line: string): string {
  if (LIST_PREFIX.test(line)) return line
  const indent = /^\s*/.exec(line)![0]
  return `${indent}- ${line.slice(indent.length)}`
}

function stripList(line: string): string {
  // A checkbox is part of the prefix: taking the bullet off a task leaves prose, not `- [ ] prose`.
  const task = TASK_PREFIX.exec(line)
  if (task) return task[1] + line.slice(task[0].length)
  const list = LIST_PREFIX.exec(line)
  return list ? list[1] + line.slice(list[0].length) : line
}

function addTask(line: string): string {
  const indent = /^\s*/.exec(line)![0]
  const list = LIST_PREFIX.exec(line)
  if (list) return `${indent}${list[2]} [ ] ${line.slice(list[0].length)}`
  return `${indent}- [ ] ${line.slice(indent.length)}`
}

function stripCheckbox(line: string): string {
  const task = TASK_PREFIX.exec(line)
  // No checkbox means the line was never a task; leave it exactly as it is rather than guessing at a prefix.
  return task ? `${task[1]}${task[2]} ${line.slice(task[0].length)}` : line
}

/** The caret's line, stepped through `##` and `###` and back out. */
function headingEdit(text: string, sel: Sel): Edit | null {
  if (text.length === 0) return null
  const caret = Math.min(sel.from, sel.to)
  let start = caret
  while (start > 0 && text.charAt(start - 1) !== '\n') start -= 1
  let end = caret
  while (end < text.length && text.charAt(end) !== '\n') end += 1

  const line = text.slice(start, end)
  const hashes = /^(#+)/.exec(line)?.[1].length ?? 0
  const body = line.slice(hashes).replace(/^ +/, '')

  // The cycle lives between two and three hashes: `#` is a note's title, and a button that quietly turned a title
  // into a section heading would be rewriting the one line the reader meant to leave alone.
  let insert: string
  if (hashes === 0) insert = `## ${line}`
  else if (hashes < 3) insert = `${'#'.repeat(hashes + 1)} ${body}`
  else if (hashes === 3) insert = body
  else insert = `### ${body}`

  const delta = insert.length - line.length
  const newHashes = /^(#+)/.exec(insert)?.[1].length ?? 0
  const map = (offset: number) =>
    offset <= start + hashes ? Math.min(offset, start + newHashes) : Math.max(start, offset + delta)
  return { from: start, to: end, insert, anchor: map(sel.from), head: map(sel.to) }
}
