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
  // The range is clamped before anything else reads it; see `clampToParagraph` for why a wrap may not reach over a
  // blank line or leave a marker at the head of one.
  let { from, to } = clampToParagraph(text, sel)

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
      // Nothing to expand to — but there may still be a pair around the caret, and the second press has to undo the
      // first. Adjacent is the case that press actually makes (`**|`); one step wider covers a caret that landed in
      // the whitespace inside a marked phrase, which is where clicking rather than typing puts it.
      const enclosed = enclosingPair(text, from, open, close)
      if (enclosed !== null) return removePair(text, enclosed.start, enclosed.end, from, open, close)
      const at = pastHeadingMarker(text, from)
      return { from: at, to: at, insert: open + close, anchor: at + open.length, head: at + open.length }
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
function linkEdit(text: string, sel: Sel): Edit | null {
  let { from, to } = clampToParagraph(text, sel)

  if (from === to) {
    const before = WORD_CHAR.test(text.charAt(from - 1))
    const after = from < text.length && WORD_CHAR.test(text.charAt(from))
    if (before && after) {
      while (from > 0 && WORD_CHAR.test(text.charAt(from - 1))) from -= 1
      while (to < text.length && WORD_CHAR.test(text.charAt(to))) to += 1
    } else if (text.slice(from - 1, from) === '(' && text.slice(to, to + 1) === ')' && /\]\(\)$/.test(text.slice(0, to + 1))) {
      // The second press on a fresh link: `[label](|)` goes back to plain text rather than nesting another pair.
      const labelStart = text.lastIndexOf('[', from - 2)
      if (labelStart >= 0) {
        const label = text.slice(labelStart + 1, from - 2)
        return { from: labelStart, to: to + 1, insert: label, anchor: labelStart, head: labelStart + label.length }
      }
    } else if (insideFilledAddress(text, from)) {
      // The caret is already inside the address of a real link. This branch only ever knew how to undo an *empty*
      // pair, so it used to write `[]()` in among the characters of the URL: `[label]([]()https://…)`. Pressing
      // link there means "this should be a link", and it already is — declining is the smaller lie.
      return null
    } else {
      // `[]()` with the caret between the parentheses: the next thing typed is the URL, which is the half a person
      // has to fill in anyway.
      const at = pastHeadingMarker(text, from)
      return { from: at, to: at, insert: '[]()', anchor: at + 3, head: at + 3 }
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

  // `end === start` is the caret sitting on an empty line, which used to read as "no block here" and answer the press
  // with silence. Pressing • on a blank line under a heading is the most natural way to start a list there is.
  const lines = end === start ? [''] : text.slice(start, end).split('\n')
  let changed: string[]

  if (kind === 'bullet') {
    // Every marked line and nothing unmarked means the press takes the bullets off. One plain line among them means
    // it puts bullets on all of them — a control that only ever removed things would make a list harder to start than
    // to end, and nobody reads "nothing happened" as a partial success. Blank lines count as marked, so a list with a
    // blank line in it still un-bullets; a press on blanks alone is a press that wants bullets.
    const allMarked =
      lines.some((l) => LIST_PREFIX.test(l)) && lines.every((l) => l.trim() === '' || LIST_PREFIX.test(l))
    changed = lines.map((l) => (allMarked ? stripList(l) : addBullet(l)))
  } else {
    const allTasks =
      lines.some((l) => TASK_PREFIX.test(l)) && lines.every((l) => l.trim() === '' || TASK_PREFIX.test(l))
    changed = lines.map((l) => (allTasks ? stripCheckbox(l) : addTask(l)))
  }

  const block = text.slice(start, end)
  const insert = changed.join('\n')
  // A press that would write back exactly what it read must not dispatch: the change lands in the undo history
  // anyway, and Ctrl+Z then looks like a key that does nothing.
  if (insert === block) return null
  // The caret on an empty line goes *after* the prefix that just appeared. `shift` leaves an offset at a line's start
  // where it is, which is right for a selection spanning real text — but here it would park the caret in front of the
  // `- `, so the next character typed would come out `x- `.
  if (end === start) return { from: start, to: end, insert, anchor: start + insert.length, head: start + insert.length }
  return { from: start, to: end, insert, anchor: shift(from, start, lines, changed), head: shift(to, start, lines, changed) }
}

/** The range a wrap is allowed to cover: the selection with any line breaks off both ends, and cut short at the first
 *  blank line inside it.
 *
 *  Both halves are the same rule as the closing asterisk. A break at an end leaves the marker at the head of a line,
 *  where it can only open; a blank line in the middle ends the paragraph, and emphasis cannot span paragraphs, so
 *  `**a\n\nb**` renders nothing bold and shows every character of itself. A single soft break between the two is left
 *  alone, because that renders as intended. */
function clampToParagraph(text: string, sel: Sel): { from: number; to: number } {
  let from = Math.min(sel.from, sel.to)
  let to = Math.max(sel.from, sel.to)
  while (to > from && text.charAt(to - 1) === '\n') to -= 1
  while (from < to && text.charAt(from) === '\n') from += 1
  const breakInside = text.slice(from, to).indexOf('\n\n')
  if (breakInside !== -1) to = from + breakInside
  return { from, to }
}

/** A pair inserted at the front of a heading line belongs after the `## `, not before it: an ATX heading has to start
 *  its line, so `****## Storm` is a paragraph and the note's title has stopped being one. Past the marker, the same
 *  press means "start writing bold in the title", which is what it looks like it is doing. */
function pastHeadingMarker(text: string, offset: number): number {
  const lineStart = text.lastIndexOf('\n', Math.max(0, offset - 1)) + 1
  if (lineStart > offset) return offset
  const marker = /^#{1,6}\s/.exec(text.slice(lineStart, lineStart + 8))
  return marker && offset <= lineStart + marker[0].length ? lineStart + marker[0].length : offset
}

/** True when `offset` sits between the parentheses of a link address that already has something in it. The `]` in
 *  front of the `(` is what makes it link syntax rather than a parenthetical aside in a sentence. */
function insideFilledAddress(text: string, offset: number): boolean {
  const open = text.lastIndexOf('(', offset)
  const close = text.indexOf(')', offset)
  return open > 0 && close > open + 1 && text.charAt(open - 1) === ']' && close > offset
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

/** The nearest pair of `open`/`close` holding `caret` on one line, or `null`. Content may not contain the closing
 *  run or a newline, which keeps a caret between two separate bold phrases from being read as one marked span. */
function enclosingPair(text: string, caret: number, open: string, close: string): { start: number; end: number } | null {
  const lineStart = text.lastIndexOf('\n', Math.max(0, caret - 1)) + 1
  const lineEndNewline = text.indexOf('\n', caret)
  const lineEnd = lineEndNewline === -1 ? text.length : lineEndNewline

  for (let start = caret - open.length; start >= lineStart; start -= 1) {
    if (text.slice(start, start + open.length) !== open) continue
    const inner = text.slice(start + open.length, caret)
    if (inner.includes(close)) continue
    for (let end = caret; end + close.length <= lineEnd; end += 1) {
      if (text.slice(end, end + close.length) !== close) continue
      // A closing run between the caret and here belongs to an earlier span, not to ours.
      if (text.slice(caret, end).includes(close)) break
      return { start, end: end + close.length }
    }
  }
  return null
}

/** Drop a pair's two marker runs, keeping everything between them and the caret where it was. */
function removePair(text: string, start: number, end: number, caret: number, open: string, close: string): Edit {
  const inner = text.slice(start + open.length, end - close.length)
  // The caret follows the words it was sitting among: it moves left by one marker run when a run was removed in
  // front of it, and lands at the start when it was between the two runs already.
  const where = caret > start + open.length ? caret - open.length : start
  return { from: start, to: end, insert: inner, anchor: where, head: where }
}
