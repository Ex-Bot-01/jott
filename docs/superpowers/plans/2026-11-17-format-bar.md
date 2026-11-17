# Format bar for the note editor — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A bar above the note buffer with Bold, Italic, Code, Link, Bullet list, Task and Heading that inserts and removes the markdown characters, lights up when the caret already sits inside that markup, and shows which note is open.

**Architecture:** All offset arithmetic lives in `src/lib/markdownEdit.ts`, a pure module with no CodeMirror import: text plus selection in, one single-range change plus a new selection out. Whether a mark applies *now* comes from `src/lib/markdownState.ts`, which reads CodeMirror's syntax tree and the caret's line prefix — so the button and the screen cannot disagree. `Editor.svelte` owns no logic beyond wiring: it renders two rows, dispatches one transaction per press, and re-reads state on every change.

**Tech Stack:** Svelte 5 runes, CodeMirror 6 (`@codemirror/lang-markdown` 6.5.2 already installed), Vitest + jsdom, `xdotool`/`import` for the visual pass.

**Spec:** `docs/superpowers/specs/2026-11-17-format-bar-design.md`

## Global Constraints

- No new dependency. Everything used here is already in `package.json`.
- CodeMirror 6.5.2 ships **no** bold/italic commands — its exports are `markdown`, `markdownKeymap`, `deleteMarkupBackward`, `insertNewlineContinueMarkup`, `pasteURLAsLink`, `commonmarkLanguage`, `markdownLanguage`. The wrapping arithmetic is ours.
- Markdown markers stay visible and faded (`markdownStyle.ts`). Nothing in this plan hides, replaces or deletes a marker from the screen.
- A press edits a range or does nothing. It never rewrites text outside the range it was asked about — §14's conflict detection compares the hash of what Jott wrote against the bytes on disk, so silent normalisation would raise false conflicts.
- `src/lib/shortcuts.ts` claims `Ctrl+N`, `Ctrl+R`, `Ctrl+F`, `Ctrl+S`, `Ctrl+,`, `Ctrl+Shift+N`, `Ctrl+Shift+D`. The four new keys must stay outside that set.
- Pure-module rule: logic that can be tested headlessly goes in `src/lib/*.ts` with a sibling `.test.ts`. jsdom runs CodeMirror's DOM but computes no layout, so a test may assert buffer text and attributes — never a pixel or a wrapped line.
- Version bumps touch three files together: `apps/shell/Cargo.toml`, `apps/shell/tauri.conf.json` **and** `apps/shell/Cargo.lock`. A stale lock fails `cargo metadata --locked` with exit 101.
- Diagnostics privacy is untouched: no note content enters the drawer or its log.

## Review Focus

The spec's tests do not cover these; each line gets a pin in the named task.

1. **An empty buffer, or a caret at offset 0 of a one-character note.** Pressing Bold must insert `****` and leave the caret inside it — never throw, never swallow the keystroke, never place the caret past the end of the document.
2. **A character outside the Basic Multilingual Plane before the caret** (an emoji in the sentence), and a note whose markers are already there. Offsets are UTF-16 code units on both sides; a wrap that lands one unit inside a surrogate pair corrupts the note in a way the writer will not notice until they reopen it.
3. **A selection that already contains its own markers** — selecting all of `**word**` and pressing Bold must unwrap to `word`, not nest into `****word****`.
4. **note-processor's read-only catalogue.** No bar, no rows, and the four shortcuts must leave np's `INDEX.md` byte-identical — a control that offers to edit a file core refuses to write is the complaint this repo already answered once.
5. **One press, one undo step.** A wrap that dispatches changes and selection as two transactions leaves an undo that restores text but strands the caret, and reads as "Ctrl+Z did nothing".

---

### Task 1: Inline marks in `markdownEdit.ts`

**Files:**
- Create: `src/lib/markdownEdit.ts`
- Test: `src/lib/markdownEdit.test.ts`

**Interfaces:**
- Consumes: nothing.
- Produces:
  ```ts
  export type MarkKind = 'bold' | 'italic' | 'code' | 'link' | 'bullet' | 'task' | 'heading'
  export interface Sel { from: number; to: number }
  /** One range change plus the selection that follows it — shaped so `Editor.svelte` can hand it straight to
   *  `view.dispatch({ changes: edit, selection: { anchor: edit.anchor, head: edit.head } })`. */
  export interface Edit { from: number; to: number; insert: string; anchor: number; head: number }
  /** `null` when there is nothing honest to do (a block mark in an empty buffer). */
  export function applyMark(text: string, sel: Sel, kind: MarkKind): Edit | null
  ```

- [ ] **Step 1: Write the failing tests for bold, italic and code**

Create `src/lib/markdownEdit.test.ts`. Every assertion applies the edit to a copy of the text with this helper, because a bare `{from,to,insert}` is unreadable as an expectation:

```ts
import { describe, expect, it } from 'vitest'
import { applyMark, type Edit, type MarkKind, type Sel } from './markdownEdit'

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
    expect(applied('wind from Tuesday', { from: 12, to: 12 }, 'bold')).toEqual({
      text: 'wind from **Tuesday**', anchor: 14, head: 21,
    })
  })

  it('inserts a pair around an empty caret and parks the caret inside it', () => {
    // The end of the sentence: nothing to wrap, so this is "start writing bold" — which is why the caret lands
    // between the markers rather than after them.
    expect(applied('wind from', { from: 9, to: 9 }, 'bold')).toEqual({
      text: 'wind from****', anchor: 11, head: 11,
    })
    expect(applied('wind from', { from: 9, to: 9 }, 'code').text).toBe('wind from``')
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

  it('leaves a mark alone in an empty buffer except the pairs, which it can still insert', () => {
    // Review Focus 1: offset 0 of nothing is a real place to press a button.
    expect(applied('', { from: 0, to: 0 }, 'bold')).toEqual({ text: '****', anchor: 2, head: 2 })
    expect(applied('', { from: 0, to: 0 }, 'heading')).toBeNull()
  })
})
```

- [ ] **Step 2: Run them to verify they fail**

Run: `npx vitest run src/lib/markdownEdit.test.ts`
Expected: FAIL — `applyMark is not a function` / cannot resolve `./markdownEdit`.

- [ ] **Step 3: Implement the inline half of `applyMark`**

In `src/lib/markdownEdit.ts`, declare `MarkKind`, `Sel`, `Edit` exactly as in Interfaces, then `applyMark` dispatching on kind to `inlineEdit(text, sel, kind)` for `bold | italic | code | link` and returning `null` for the block kinds (Task 2 replaces those). The marker pairs: bold `**`/`**`, italic `_`/`_`, code `` ` ``/`` ` ``, link `[`/`]()`.

Two rules the tests settle and the implementation must state in comments: a word character immediately before *and* immediately after an empty caret means the caret is inside a word: expand to that word's bounds and wrap it; with no word, insert the pair and put the caret between (`link` puts it inside the parentheses, since that is where the URL goes). Unwrapping checks for the markers immediately outside the selection **and** immediately inside it, and removes whichever pair it finds — nothing else moves.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `npx vitest run src/lib/markdownEdit.test.ts`
Expected: PASS (the block-mark cases are not written yet).

- [ ] **Step 5: Write the link cases and run them to verify they fail**

Append to `describe('inline marks')`:

```ts
  it('makes a link with the caret inside the parentheses', () => {
    // No dialog: `[text]()` with the caret where the URL goes is one keystroke fewer than a modal.
    expect(applied('from Tuesday now', { from: 5, to: 12 }, 'link')).toEqual({
      text: 'from [Tuesday]() now', anchor: 15, head: 15,
    })
    expect(applied('from [Tuesday](u) now', { from: 5, to: 17 }, 'link')).toEqual({
      text: 'from Tuesday now', anchor: 5, head: 12,
    })
  })
```

Run: `npx vitest run src/lib/markdownEdit.test.ts`
Expected: FAIL — the link kind is not handled.

- [ ] **Step 6: Implement the link half and run the file again**

`[inner]()` for a wrap, caret at `from + inner.length + 3` so it lands inside the parentheses; unwrapping recognises `[text](` … `)` around or inside the selection and leaves `text`. Run `npx vitest run src/lib/markdownEdit.test.ts`, Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src/lib/markdownEdit.ts src/lib/markdownEdit.test.ts
git commit -m "feat: one module decides what bold does to a range of text"
```

---

### Task 2: Block marks in `markdownEdit.ts`

**Files:**
- Modify: `src/lib/markdownEdit.ts`
- Test: `src/lib/markdownEdit.test.ts`

**Interfaces:**
- Consumes: `applyMark`, `MarkKind`, `Sel`, `Edit` from Task 1.
- Produces: the same `applyMark` signature, now handling `'bullet' | 'task' | 'heading'`.

- [ ] **Step 1: Write the failing tests for bullet and task**

```ts
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
})
```

- [ ] **Step 2: Run them to verify they fail**

Run: `npx vitest run src/lib/markdownEdit.test.ts`
Expected: FAIL — the block cases return `null`.

- [ ] **Step 3: Implement `blockEdit(text, sel, kind)` for bullet and task**

Operate on whole lines: take every line whose range overlaps `[from, to)`, rebuild them, and emit one change spanning that line block. A line's existing list prefix is `/^(\s*)([-*+]|\d+[.)])\s+/`; a task adds `\[([ xX])\]\s*` after it. `bullet` off removes the prefix (and any checkbox with it); `task` on inserts `[ ] ` after the bullet, or `- ` plus `[ ] ` when the line has no bullet; `task` off removes the checkbox and keeps the bullet. Offsets for the new selection shift by the difference in prefix length on the first line only — that is what the assertions above encode.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `npx vitest run src/lib/markdownEdit.test.ts`
Expected: PASS.

- [ ] **Step 5: Write the heading cycle tests and run them to verify they fail**

```ts
  it('steps the caret’s line through ## and ### and out', () => {
    // The cycle never produces `#`: a title is typed by hand, so pressing Heading on one moves it down rather
    // than treating it as "no heading yet".
    expect(applied('north wind', { from: 3, to: 3 }, 'heading').text).toBe('## north wind')
    expect(applied('## north wind', { from: 3, to: 3 }, 'heading').text).toBe('### north wind')
    expect(applied('### north wind', { from: 3, to: 3 }, 'heading').text).toBe('north wind')
    expect(applied('# north wind', { from: 3, to: 3 }, 'heading').text).toBe('## north wind')
    expect(applied('#### north wind', { from: 3, to: 3 }, 'heading').text).toBe('### north wind')
  })

  it('changes only the caret’s line when a heading press meets a selection', () => {
    expect(applied('north\nsouth', { from: 0, to: 11 }, 'heading').text).toBe('## north\nsouth')
  })
```

Run: `npx vitest run src/lib/markdownEdit.test.ts`
Expected: FAIL — heading returns `null`.

- [ ] **Step 6: Implement heading and run the file again**

Count the leading `#` on the line containing `sel.from`: none → insert `## `; `n < 3` → `n + 1`; `n === 3` → remove them; `n > 3` → step down to `### `. The cycle lives between two and three hashes, which is what those assertions encode, and only the caret's line changes however wide the selection is.

Run: `npx vitest run src/lib/markdownEdit.test.ts`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src/lib/markdownEdit.ts src/lib/markdownEdit.test.ts
git commit -m "feat: lists, tasks and headings answer the same one press"
```

---

### Task 3: `markdownState.ts` — what applies where the caret is

**Files:**
- Create: `src/lib/markdownState.ts`
- Test: `src/lib/markdownState.test.ts`

**Interfaces:**
- Consumes: `MarkKind` from `./markdownEdit`.
- Produces:
  ```ts
  import type { EditorState } from '@codemirror/state'
  /** The marks that hold the current selection, so a button can be lit rather than guessed. Inline kinds come from
   *  the syntax tree; block kinds from the caret's line prefix, because `Task` nodes only exist with the GFM
   *  extension this language setup does not enable. */
  export function activeMarks(state: EditorState): MarkKind[]
  ```

- [ ] **Step 1: Write the failing tests against a real editor state**

```ts
import { EditorState } from '@codemirror/state'
import { markdown } from '@codemirror/lang-markdown'
import { describe, expect, it } from 'vitest'
import { activeMarks } from './markdownState'

const at = (doc: string, caret: number) => activeMarks(EditorState.create({ doc, selection: { anchor: caret }, extensions: [markdown()] }))

describe('active marks', () => {
  it('knows bold from italic from code from a link', () => {
    const doc = '**b** _i_ `c` [l](u)'
    expect(at(doc, 3)).toContain('bold')
    expect(at(doc, 7)).toContain('italic')
    expect(at(doc, 11)).toContain('code')
    expect(at(doc, 16)).toContain('link')
    expect(at(doc, 21)).toEqual([]) // the space after the link holds nothing
  })

  it('reports a nested mark as both', () => {
    expect(at('**a _b_ c**', 5)).toEqual(expect.arrayContaining(['bold', 'italic']))
  })

  it('reads the caret’s line for list, task and heading', () => {
    const doc = '- plain\n- [x] done\n### head'
    expect(at(doc, 2)).toEqual(['bullet'])
    expect(at(doc, 10)).toEqual(expect.arrayContaining(['bullet', 'task']))
    expect(at(doc, 20)).toEqual(['heading'])
  })

  it('does not mistake a minus in a sentence for a bullet', () => {
    expect(at('wind - north', 6)).toEqual([])
  })

  it('holds nothing in an empty note', () => {
    expect(at('', 0)).toEqual([])
  })
})
```

- [ ] **Step 2: Run them to verify they fail**

Run: `npx vitest run src/lib/markdownState.test.ts`
Expected: FAIL — cannot resolve `./markdownState`.

- [ ] **Step 3: Implement `activeMarks`**

Walk up from `syntaxTree(state).resolve(from, -1, 1)` through `.parent`, mapping `StrongEmphasis → bold`, `Emphasis → italic`, `InlineCode → code`, `Link → link` (node names verified against this install; `Task` does not appear without GFM). Use `state.selection.main.from`. For the line containing that offset, test the prefix with `/^\s*[-*+]\s+(\[[ xX]\]\s+)?/` for bullet and task and with `/^#{1,6}\s/` for heading. Return kinds in the order `bold, italic, code, link, bullet, task, heading` so a test comparing an array is meaningful.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `npx vitest run src/lib/markdownState.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/lib/markdownState.ts src/lib/markdownState.test.ts
git commit -m "feat: the bar knows what the caret is sitting in"
```

---

### Task 4: The two rows above the buffer

**Files:**
- Modify: `src/lib/Editor.svelte` (the `<section class="editor">` block and its `<style>`)
- Test: `src/App.smoke.test.ts`

**Interfaces:**
- Consumes: `applyMark`, `Edit`, `MarkKind` from `./markdownEdit`; `activeMarks` from `./markdownState`.
- Produces: DOM contract the visual pass and later tests rely on — `[data-testid="note-name"]` containing the file name, `[data-testid="format-bar"]` with seven `button[data-mark="<kind>"]`, each carrying `aria-pressed="true|false"`.

- [ ] **Step 1: Write the failing smoke tests**

Add to `describe('the window mounts')` in `src/App.smoke.test.ts`. Open a note the way the existing tests do — click the `.tree button` whose text contains `inbox.md`, then wait for `.cm-editor` (see "opens note-processor's own index without offering to edit it" for the pattern).

```ts
  it('offers the format bar above an editable note', async () => {
    scenario.current = 'vault'
    const { errors } = await start()
    // …open inbox.md…
    const bar = document.querySelector('[data-testid="format-bar"]') as HTMLElement
    expect(bar).not.toBeNull()
    const kinds = [...bar.querySelectorAll('button[data-mark]')].map((b) => b.getAttribute('data-mark'))
    expect(kinds).toEqual(['bold', 'italic', 'code', 'link', 'bullet', 'task', 'heading'])
    for (const b of bar.querySelectorAll('button[data-mark]')) {
      expect(b.getAttribute('aria-pressed'), `${b.dataset.mark} says whether it applies`).toMatch(/^(true|false)$/)
    }
    // The name row answers "which note am I looking at", which nothing in the window did before.
    expect(document.querySelector('[data-testid="note-name"]')?.textContent).toContain('inbox.md')
    expect(errors).toEqual([])
  })

  it('presses bold into the buffer, and takes it off again', async () => {
    // jsdom runs CodeMirror for real here — the existing dark-mode test reads `.cm-content` — so a click is
    // allowed to prove the wiring end to end. What it may not claim is anything about how the bar looks.
    // …open inbox.md, then:
    const bold = document.querySelector('button[data-mark="bold"]') as HTMLButtonElement
    bold.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await vi.waitFor(() => expect(document.querySelector('.cm-content')?.textContent).toContain('****'))
    expect(bold.getAttribute('aria-pressed'), 'the caret is inside the pair it just inserted').toBe('true')

    bold.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await vi.waitFor(() => expect(document.querySelector('.cm-content')?.textContent).not.toContain('****'))

    // Review Focus 5: one press is one undo step. A wrap dispatched as two transactions puts the text back and
    // strands the caret, which reads as Ctrl+Z having done nothing.
    bold.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await vi.waitFor(() => expect(document.querySelector('.cm-content')?.textContent).toContain('****'))
    document.querySelector('.cm-content')!.dispatchEvent(new KeyboardEvent('keydown', { key: 'z', ctrlKey: true, bubbles: true }))
    await vi.waitFor(() => expect(document.querySelector('.cm-content')?.textContent).not.toContain('****'))
  })

  it('gives note-processor’s catalogue neither rows nor shortcuts', async () => {
    scenario.current = 'vault'
    readNoteSays.catalogue = true
    const { errors } = await start()
    // …open INDEX.md…
    expect(document.querySelector('[data-testid="format-bar"]')).toBeNull()
    expect(document.querySelector('[data-testid="note-name"]')).toBeNull()
    const before = document.querySelector('.cm-content')?.textContent ?? ''
    document.body.dispatchEvent(new KeyboardEvent('keydown', { key: 'b', ctrlKey: true, bubbles: true }))
    await new Promise((resolve) => setTimeout(resolve, 120))
    expect(document.querySelector('.cm-content')?.textContent, 'np wrote this; a shortcut must not touch it').toBe(before)
    readNoteSays.catalogue = false
    expect(errors).toEqual([])
  })
```

- [ ] **Step 2: Run them to verify they fail**

Run: `npx vitest run src/App.smoke.test.ts -t "format bar|presses bold|catalogue"`
Expected: FAIL — `[data-testid="format-bar"]` is null.

- [ ] **Step 3: Render the rows and dispatch the edits**

In `Editor.svelte`: keep a `let marks = $state<MarkKind[]>([])`, recompute it in `onUpdate` on `update.docChanged || update.selectionSet` (the existing early return for non-text updates must move below this, or the bar goes stale on every arrow key), and add `function runMark(kind: MarkKind)` that reads `view.state`, calls `applyMark(view.state.doc.toString(), { from: sel.from, to: sel.to }, kind)`, returns when it gives `null` or when `readonly`, and otherwise dispatches **one** transaction — `{ changes: { from, to, insert }, selection: { anchor, head }, scrollIntoView: true }` (Review Focus 5) — then `view.focus()`.

Markup inside `<section class="editor">`, above the `.cm` host, wrapped in `{#if rel && !readonly}`: a `[data-testid="note-name"]` paragraph with the file name in ink and its folder muted (derive both from `rel`; `rel` uses `/` separators as core reports them), then `[data-testid="format-bar"]` holding seven `<button type="button" data-mark="…">` labelled **B**, *I*, `</>`, **link**, **•**, **☑**, **H**, each with `aria-pressed={marks.includes(kind)}`, `title` naming the shortcut where one exists, and `onclick={() => runMark(kind)}`. Style the bar as a flex row, `flex: 0 0 auto`, about 28 px tall, with a pressed state that reads on both papers (`background: color-mix(in srgb, currentColor 14%, transparent)`), and give it `data-testid` hooks exactly as named — the visual pass looks for them.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `npx vitest run src/App.smoke.test.ts -t "format bar|presses bold|catalogue"`
Expected: PASS.

- [ ] **Step 5: Run the whole suite and the type check**

Run: `npm test && npx svelte-check --tsconfig ./tsconfig.app.json --output human`
Expected: all tests pass; `svelte-check found 0 errors and 0 warnings`.

- [ ] **Step 6: Commit**

```bash
git add src/lib/Editor.svelte src/App.smoke.test.ts
git commit -m "feat: the note says its name, and formatting is a press away"
```

---

### Task 5: The four shortcuts, and list continuation

**Files:**
- Modify: `src/lib/Editor.svelte` (the `keymap.of([...])` inside `extensions()`)
- Test: `src/lib/shortcuts.test.ts`, `src/App.smoke.test.ts`

**Interfaces:**
- Consumes: `runMark` from Task 4, `markdownKeymap` from `@codemirror/lang-markdown`.
- Produces: `Ctrl+B/I/E/K` formatting the buffer, Enter continuing list markup.

- [ ] **Step 1: Write the fence that keeps the window off these keys**

Append to `src/lib/shortcuts.test.ts`:

```ts
  it('leaves the four formatting keys to the editor', () => {
    // `shortcutFor` is the window's claim on a keystroke. Anything it answers with would fire a panel over the note
    // instead of formatting a word, and the two are indistinguishable from the keyboard.
    for (const key of ['b', 'i', 'e', 'k']) {
      expect(shortcutFor({ key, ctrlKey: true }), `Ctrl+${key.toUpperCase()}`).toBeNull()
    }
  })
```

- [ ] **Step 2: Run it**

Run: `npx vitest run src/lib/shortcuts.test.ts`
Expected: PASS immediately — those four keys are already free. This is a fence, not a driver; say so in the ledger rather than letting a green test look as though it bought a change.

- [ ] **Step 3: Write the failing editor tests**

In `src/App.smoke.test.ts`, open `inbox.md` and dispatch Ctrl+B at the document level:

```ts
  it('formats from the keyboard as well as the bar', async () => {
    // …open inbox.md…
    const content = document.querySelector('.cm-content') as HTMLElement
    content.dispatchEvent(new KeyboardEvent('keydown', { key: 'b', ctrlKey: true, bubbles: true }))
    await vi.waitFor(() => expect(document.querySelector('.cm-content')?.textContent).toContain('****'))
  })

  it('keeps a list going when you press Enter', () => {
    // A claim about what `Editor.svelte` installs, not about a rendered newline: jsdom gives CodeMirror no layout to
    // measure, so the keystroke itself cannot be driven here.
    const source = readFileSync(join(process.cwd(), 'src', 'lib', 'Editor.svelte'), 'utf8')
    expect(source).toMatch(/\.\.\.markdownKeymap/)
  })
```

- [ ] **Step 4: Run them to verify they fail**

Run: `npx vitest run src/App.smoke.test.ts -t "keyboard|Enter"`
Expected: FAIL — no binding, and no `markdownKeymap` in the file.

- [ ] **Step 5: Install the bindings**

In `extensions()`, put the four formatting keys in a keymap **before** `defaultKeymap` so ours win, each `run: () => runMark('<kind>')` returning its boolean, with `preventDefault` implied by CodeMirror when `run` returns true: `Mod-b → bold`, `Mod-i → italic`, `Mod-e → code`, `Mod-k → link`. Add `...markdownKeymap` after `historyKeymap` (Enter continues `- [ ]`; Backspace at the end of `**word|**` deletes the pair through `deleteMarkupBackward`). Import `markdownKeymap` from `@codemirror/lang-markdown`.

- [ ] **Step 6: Run the tests to verify they pass**

Run: `npx vitest run src/App.smoke.test.ts -t "keyboard|Enter"`
Expected: PASS.

- [ ] **Step 7: Run everything, then commit**

Run: `npm test && npm run build && npx svelte-check --tsconfig ./tsconfig.app.json --output human`
Expected: green suite, build exit 0, `0 errors and 0 warnings`.

```bash
git add src/lib/Editor.svelte src/App.smoke.test.ts src/lib/shortcuts.test.ts
git commit -m "feat: Ctrl+B means bold here, and Enter keeps a list alive"
```

---

### Task 6: See it, write it up, ship the number

**Files:**
- Modify: `docs/release-notes.md`, `docs/testing-on-windows.md`, `apps/shell/Cargo.toml`, `apps/shell/tauri.conf.json`, `apps/shell/Cargo.lock`
- Test: none (this task's verification is the browser and the gates)

**Interfaces:**
- Consumes: the DOM contract from Task 4.
- Produces: release `0.0.27` with notes, a tester walkthrough, and three version files agreeing.

- [ ] **Step 1: Look at it in the browser**

`npm run ui`, then Firefox on `DISPLAY=:99` at `http://127.0.0.1:5199/?s=folder`. Capture and read six pictures with a note open: 1920, 1200 and 820 px CSS width, each in light and in dark (`GTK_THEME=Adwaita:dark` on the launch).

- [ ] **Step 2: Fix what the pictures show**

The bar stays one row down to 820 px and does not push the buffer's first line off screen; the pressed state reads on both papers; the name row does not wrap at 820 px, where the muted folder may ellipsise. Where a jsdom assertion is honest, write it first; a claim about CSS goes in a source-pin test labelled as one.

- [ ] **Step 3: Write the release note**

Replace `docs/release-notes.md` entirely — it carries one release per file, and CI publishes it verbatim (`gh release create --notes-file docs/release-notes.md`). Cover: the bar and what each control does to the characters in the file; that markers stay visible on purpose; the caret-between-markers behaviour in place of a latch; the four shortcuts; Enter continuing a list; the note's name at last. End with the numbers, from the run you just did, not from memory.

- [ ] **Step 4: Write the tester walkthrough**

Add a section to `docs/testing-on-windows.md` in the existing voice (`## 2b-zc. Since 0.0.27: formatting without asterisks`): open a note, click Bold mid-word and see `**` appear around it, keep typing inside them, press Right; select a bolded phrase and press Bold to take it off; `Ctrl+B` on a word; Enter in a `- [ ]` list; open `INDEX.md` and confirm no bar at all. Name what is a bug report and what is a preference ("if the pressed state is hard to see, tell me which scheme you were in").

- [ ] **Step 5: Bump the version in all three places**

`0.0.26` → `0.0.27` in `apps/shell/Cargo.toml`, `apps/shell/tauri.conf.json`, and the `jott-shell` entry of `apps/shell/Cargo.lock`.

Run: `cd apps/shell && cargo metadata --locked >/dev/null; echo $?`
Expected: `0`. (A stale lock answers `101` — that is how 0.0.26 nearly shipped.)

- [ ] **Step 6: Final gates**

Run: `npm test && npm run build && npx svelte-check --tsconfig ./tsconfig.app.json --output human`
Expected: every test passing, exit 0, `0 errors and 0 warnings`. Commit.
