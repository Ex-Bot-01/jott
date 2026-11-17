import { describe, expect, it } from 'vitest'
import { applyMark, type MarkKind, type Sel } from './markdownEdit'

/** The text that results from applying an edit, so a test can read as a sentence about the note. */
function applied(text: string, sel: Sel, kind: MarkKind): { text: string; anchor: number; head: number } | null {
  const e = applyMark(text, sel, kind)
  return e === null ? null : { text: text.slice(0, e.from) + e.insert + text.slice(e.to), anchor: e.anchor, head: e.head }
}

describe('inline marks', () => {
  it('wraps the selection and keeps it on the words', () => {
    // "wind from Tuesday" — the caret range covers `Tuesday` at 10..17.
    expect(applied('wind from Tuesday', { from: 10, to: 17 }, 'bold')).toEqual({
      text: 'wind from **Tuesday**', anchor: 12, head: 19,
    })
    expect(applied('wind from Tuesday', { from: 10, to: 17 }, 'italic')).toEqual({
      text: 'wind from _Tuesday_', anchor: 11, head: 18,
    })
    expect(applied('wind from Tuesday', { from: 10, to: 17 }, 'code')).toEqual({
      text: 'wind from `Tuesday`', anchor: 11, head: 18,
    })
  })

  it('takes the word under an empty caret', () => {
    // The selection ends up on the word, not on the markers and not past them: `Tuesday` sits at 12..19 once the
    // pair goes in, which is where a second press has to land for "take it off" to mean anything. (The plan had
    // 14..21 here, which points at `sday**` — see the ledger.)
    expect(applied('wind from Tuesday', { from: 12, to: 12 }, 'bold')).toEqual({
      text: 'wind from **Tuesday**', anchor: 12, head: 19,
    })
  })

  it('inserts a pair around an empty caret and parks the caret inside it', () => {
    // The end of the sentence: nothing to wrap, so this is "start writing bold" — which is why the caret lands
    // between the markers rather than after them.
    expect(applied('wind from', { from: 9, to: 9 }, 'bold')).toEqual({
      text: 'wind from****', anchor: 11, head: 11,
    })
    expect(applied('wind from', { from: 9, to: 9 }, 'code')!.text).toBe('wind from``')
  })

  it('takes the pair off again while the caret is still inside it', () => {
    // The second press has to undo the first even when nothing was typed between them. Without this, "start writing
    // bold" is a one-way door: pressing Bold twice writes `********` and leaves the caret in the middle of it.
    // (The end-to-end test in Task 4 found this; the unit tests around it had only ever pressed once.)
    expect(applied('****', { from: 2, to: 2 }, 'bold')).toEqual({ text: '', anchor: 0, head: 0 })
    expect(applied('``', { from: 1, to: 1 }, 'code')).toEqual({ text: '', anchor: 0, head: 0 })
    expect(applied('[]()', { from: 3, to: 3 }, 'link')).toEqual({ text: '', anchor: 0, head: 0 })
    // A pair with something in it is the same request: the markers go, the words stay, and the caret keeps the place
    // it had inside them. This is the press that happens when somebody clicks into a bolded phrase and presses Bold.
    expect(applied('**a b**', { from: 4, to: 4 }, 'bold')).toEqual({ text: 'a b', anchor: 2, head: 2 })
  })

  it('unwraps a selection that already carries the markers', () => {
    // Review Focus 3: pressing Bold on `**Tuesday**` takes bold off instead of nesting four asterisks.
    expect(applied('from **Tuesday** now', { from: 5, to: 16 }, 'bold')).toEqual({
      text: 'from Tuesday now', anchor: 5, head: 12,
    })
    // …and the inside of the pair is the same request.
    expect(applied('from **Tuesday** now', { from: 7, to: 14 }, 'bold')).toEqual({
      text: 'from Tuesday now', anchor: 5, head: 12,
    })
  })

  it('counts a character outside the basic plane as what it is', () => {
    // Review Focus 2: `🧭` is two UTF-16 units, exactly as CodeMirror counts it. A wrap that assumed one would land
    // inside the surrogate pair and write a note that will not decode.
    expect(applied('🧭 Tuesday', { from: 3, to: 10 }, 'bold')).toEqual({
      text: '🧭 **Tuesday**', anchor: 5, head: 12,
    })
  })

  it('makes a link with the caret inside the parentheses', () => {
    // No dialog: `[text]()` with the caret where the URL goes is one keystroke fewer than a modal.
    expect(applied('from Tuesday now', { from: 5, to: 12 }, 'link')).toEqual({
      text: 'from [Tuesday]() now', anchor: 15, head: 15,
    })
    expect(applied('from [Tuesday](u) now', { from: 5, to: 17 }, 'link')).toEqual({
      text: 'from Tuesday now', anchor: 5, head: 12,
    })
  })

  it('leaves a mark alone in an empty buffer except the pairs, which it can still insert', () => {
    // Review Focus 1: offset 0 of nothing is a real place to press a button.
    expect(applied('', { from: 0, to: 0 }, 'bold')).toEqual({ text: '****', anchor: 2, head: 2 })
    expect(applied('', { from: 0, to: 0 }, 'heading')).toBeNull()
  })
})

describe('block marks', () => {
  it('puts a bullet on every line the selection touches', () => {
    expect(applied('north\nsouth\nwest', { from: 0, to: 16 }, 'bullet')).toEqual({
      text: '- north\n- south\n- west', anchor: 0, head: 22,
    })
  })

  it('takes the bullet off again, whichever marker it was written with', () => {
    expect(applied('- north\n* south\n+ west', { from: 0, to: 22 }, 'bullet')).toEqual({
      text: 'north\nsouth\nwest', anchor: 0, head: 16,
    })
  })

  it('turns a bullet into a task and back without losing the line', () => {
    expect(applied('- north', { from: 0, to: 7 }, 'task')).toEqual({ text: '- [ ] north', anchor: 0, head: 11 })
    expect(applied('- [x] north', { from: 0, to: 11 }, 'task')).toEqual({ text: '- north', anchor: 0, head: 7 })
    expect(applied('north', { from: 0, to: 5 }, 'task')).toEqual({ text: '- [ ] north', anchor: 0, head: 11 })
  })

  it('steps the caret’s line through ## and ### and out', () => {
    // The cycle never produces `#`: a title is typed by hand, so pressing Heading on one moves it down rather
    // than treating it as "no heading yet".
    expect(applied('north wind', { from: 3, to: 3 }, 'heading')!.text).toBe('## north wind')
    expect(applied('## north wind', { from: 3, to: 3 }, 'heading')!.text).toBe('### north wind')
    expect(applied('### north wind', { from: 3, to: 3 }, 'heading')!.text).toBe('north wind')
    expect(applied('# north wind', { from: 3, to: 3 }, 'heading')!.text).toBe('## north wind')
    expect(applied('#### north wind', { from: 3, to: 3 }, 'heading')!.text).toBe('### north wind')
  })

  it('changes only the caret’s line when a heading press meets a selection', () => {
    expect(applied('north\nsouth', { from: 0, to: 11 }, 'heading')!.text).toBe('## north\nsouth')
  })

  it('does nothing when a block mark is asked for in an empty note', () => {
    // There is no line to put a bullet on, and inventing one would write a character nobody pressed for.
    expect(applied('', { from: 0, to: 0 }, 'bullet')).toBeNull()
    expect(applied('', { from: 0, to: 0 }, 'task')).toBeNull()
  })
})

describe('presses that reach the awkward edges', () => {
  it('wraps words, not the line break between them', () => {
    // One Shift+Down from the start of a note selects `ab\n`. Wrapping that verbatim gives `**ab\n**cd`, and the
    // pair at the head of line two can only *open* — nothing renders bold, and the note carries two stray marker
    // runs that autosave will write to disk two seconds later. The wrap belongs to the words.
    expect(applied('ab\ncd', { from: 0, to: 3 }, 'bold')).toEqual({ text: '**ab**\ncd', anchor: 2, head: 4 })
    expect(applied('a\n\nb', { from: 0, to: 5 }, 'bold')).toEqual({ text: '**a**\n\nb', anchor: 2, head: 3 })
    // A newline inside a selection is fine: emphasis and link labels both render across a soft break, so only the
    // ends get pulled back. A label ending in one would not be a link at all.
    expect(applied('see a\nnext', { from: 4, to: 6 }, 'link')).toEqual({ text: 'see [a]()\nnext', anchor: 8, head: 8 })
  })

  it('leaves a heading a heading when bold starts at the front of the line', () => {
    // The bar's own first press in the smoke test lands here. `****# Inbox` is not a heading — an ATX heading has to
    // start the line — so pressing Bold at offset 0 would quietly turn a note's title into a paragraph.
    expect(applied('## Storm', { from: 0, to: 0 }, 'bold')).toEqual({ text: '## ****Storm', anchor: 5, head: 5 })
  })

  it('starts a list on an empty line instead of doing nothing', () => {
    // Pressing • on the blank line under a heading is the most natural way to begin a list from a bar that exists
    // so you don't type `- `. Heading already worked there; bullet and task answered with silence.
    expect(applied('a\n\nb', { from: 2, to: 2 }, 'bullet')).toEqual({ text: 'a\n- \nb', anchor: 4, head: 4 })
    expect(applied('a\n\nb', { from: 2, to: 2 }, 'task')).toEqual({ text: 'a\n- [ ] \nb', anchor: 8, head: 8 })
  })

  it('refuses rather than nesting inside a link that already has an address', () => {
    // Click into the URL of `[label](https://exe.dev)` and press Ctrl+K: the second-press rule only knows an empty
    // pair, so before this it wrote `[label]([]()https://…)`. Declining is the smaller lie.
    expect(applyMark('[label](https://exe.dev)', { from: 14, to: 14 }, 'link')).toBeNull()
  })

  it('never dispatches a range it did not change', () => {
    // A press that replaces text with identical text still lands in the undo history, and Ctrl+Z then reads as a key
    // that does nothing. Whatever a press decides, it decides to change something or to say nothing.
    const docs = ['', 'a\n\nb', '- x\n\n- y', '# H', '   ', '**w**', '- [ ] north', '[l](u)']
    const kinds: MarkKind[] = ['bold', 'italic', 'code', 'link', 'bullet', 'task', 'heading']
    for (const doc of docs) {
      for (const kind of kinds) {
        for (const sel of [{ from: 0, to: 0 }, { from: doc.length, to: doc.length }, { from: 0, to: doc.length }]) {
          const e = applyMark(doc, sel, kind)
          if (e) {
            expect(e.insert, `${kind} at ${sel.from} of ${JSON.stringify(doc)}`).not.toBe(
              doc.slice(e.from, e.to),
            )
          }
        }
      }
    }
  })
})
