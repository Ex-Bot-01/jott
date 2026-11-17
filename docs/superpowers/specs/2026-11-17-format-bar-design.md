# A format bar for the note editor — design

Jott already draws markdown as formatting: headings take size, `**this**` goes bold with its asterisks faded
(`markdownStyle.ts`). What it does not have is any way to *ask* for formatting except typing the characters, and no
way to see which note is open. This adds a bar above the buffer that does both.

## Intent and the why

My reading of what was asked for: formatting should be something you press, like in Word, and the markdown syntax
should arrive by itself rather than being typed by hand. The sticky part — "as long as the button is active we can just
write" — is delivered as a caret sitting between freshly inserted markers, so continuing to type keeps writing bold
until you move out of them. Nothing about that needs a hidden formatting state, and nothing gets written to the file
that the user did not ask for.

Who it's for: somebody who has notes full of markdown from other tools and wants to fix a word or two without thinking
about asterisks, and somebody who does not know that `_italic_` is a thing.

## Goals

- A bar above the buffer with **Bold, Italic, Code, Link, Bullet list, Task, Heading** — press to apply, press again
  to take it off.
- Acts on the selection; with no selection it takes the word under the caret; with an empty caret inside nothing it
  inserts the pair (`****`, `__`, `` ` ` `` …) and puts the caret between them.
- Each control shows whether it applies right now: the caret inside `**bold**` lights **Bold**. The state comes from
  CodeMirror's syntax tree, so it cannot disagree with what is on screen.
- `Ctrl+B`, `Ctrl+I`, `Ctrl+E` (code), `Ctrl+K` (link). Verified free: `shortcuts.ts` claims `Ctrl+N/R/F/S/,` and
  `Ctrl+Shift+N/D`, and returns `null` for these, so they reach CodeMirror untouched.
- Install the `markdownKeymap` that ships with `@codemirror/lang-markdown` and is not currently installed: Enter
  continues a `- [ ]` item, and Backspace at the end of `**word|**` removes the whole pair
  (`deleteMarkupBackward`). Both are the same "feels richer" feeling for two lines.
- A thin row above the bar with the note's name — today nothing in the window says which note is open, not the pane,
  not the title.
- One undo step per press, because each press is one CodeMirror transaction and `history()` is already installed.

## Non-goals

Rendered preview, hidden markers, a rich-text model, tables, images, front matter, link autocomplete or a URL dialog,
spellcheck, touch gestures.

## Anti-goals

- A save that rewrites bytes nobody touched. §14's conflict detection compares the hash of what Jott wrote against the
  bytes on disk; a formatter that normalised a whole note would raise false conflicts and could flatten front matter or
  a table it does not model. The bar edits a range or it does nothing.
- A control that lies: a lit **Bold** with the caret in plain text is worse than no indicator.
- Anything making the file less plain markdown, or any new dependency.

## Constraints

- CodeMirror 6.5.2 exports no bold/italic commands (`markdown`, `markdownKeymap`, `deleteMarkupBackward`,
  `pasteURLAsLink`, `commonmarkLanguage` — checked against the installed package). The wrapping arithmetic is ours.
- Markers stay visible and faded, per the reasoning written into `markdownStyle.ts`: the file is what git, np and every
  other editor show, so hiding syntax in one window only is how people get surprised.
- Offset arithmetic lives in a pure module (`src/lib/markdownEdit.ts`), like `layoutSize.ts` — text plus selection in,
  new changes and selection out, no DOM, no CodeMirror import. jsdom renders no CodeMirror layout, so the component
  tests can only pin that a button exists and carries its `aria-pressed`; the behaviour lives in the module's tests.

## Approaches

**A — the bar edits markdown (chosen).** A press inserts or removes characters in one transaction; the buffer stays the
single source of truth, so saving, mirroring, conflict detection and np all see exactly what is on screen. Trades: the
markers remain on screen, and "bold mode" ends when the caret leaves the pair rather than when a button is unpressed.

**B — Typora-style live preview** (markers hidden except on the active line). Closer to rendered text, but it needs
decorations that replace visible characters, which fights selection math, undo and IME; it also reverses the standing
decision above for reasons this feature does not require.

**C — a rich-text model with markdown round-tripped on save.** The only way to get true sticky formatting, and it puts
the plain file at risk: re-parsing on open and serialising on save rewrites untouched regions and collides with §14.
Ruled out.

## Decided as mine, correct me freely

- **Seven controls**, in that order: Bold, Italic, Code, Link, Bullet list, Task, Heading. Tooltips carry the shortcut
  where one exists. Nothing else goes in the bar; a wider set is a menu, and a menu needs a reason.
- **Block controls work on whole lines.** List, Task and Heading toggle every line the selection touches — partial
  line ranges make no sense for a prefix. Heading steps the caret's line `none → ## → ### → none`; `#` is left to the
  first line of a note by hand, because a title button that fires on the wrong line is a mess to undo mentally.
- **Link** wraps as `[selected]()` with the caret between the parentheses, so you type or paste the URL there. No
  dialog: a modal for a URL in a note editor is heavier than the thing it saves. (`pasteURLAsLink` exists upstream and
  can come later.)
- **Two rows**, name above bar, each about 26 px, both inside `Editor.svelte`, which already receives `rel`. The
  existing save-phase line stays at the bottom where it is; moving it is not part of this.
- **The name row shows the file name in ink and its folder in muted text**, and says nothing when no note is open —
  the placeholder pane owns that state.
- **Read-only files get neither row** — np's catalogue is not editable, so offering controls that refuse would repeat
  the complaint this repo already answers for `INDEX.md`.
- The bar is plain `<button>`s in a row: focusable, keyboard-operable, each with `aria-pressed`, no roving tabindex.
  A toolbar widget with arrow-key navigation is a refinement nobody has asked for yet.

## Left to the builder

The module's exact signature and how it names a mark; whether block toggles share one function with inline ones; the
wording of tooltips; how the syntax-tree query for "am I inside strong" is written (`syntaxTree(state).resolve` and a
walk up through the node's tags is the obvious shape).
