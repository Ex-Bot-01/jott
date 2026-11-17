# Jott 0.0.27 — formatting you press, in a file that stays markdown

The note pane has drawn markdown as formatting since 0.0.9 — headings take size, `**this**` goes bold with its
asterisks faded to 42 %, inline code gets a tint. What it never had was a way to *ask* for formatting without typing
the characters. Now there is a bar for that, and the file on disk is still the same plain file you would have written
by hand.

## The bar

Above the buffer, under the note's name: **B** *I* `</>` **link** **•** **☑** **H**. Press one and it puts the
markdown in; press it again and it takes it out.

- With words selected, it wraps those words.
- With nothing selected but a word under the caret, it takes that word — double-click is not required.
- With an empty caret in whitespace, it inserts the pair and leaves the caret *between* them, so you keep typing bold
  until you press Right or End. That is the "bold stays on" behaviour, and it needs no hidden state deciding when to
  close the markers: the markers are there, faded, and the caret is inside them.
- Each control lights when it already applies. Put the caret in a bolded phrase and **B** goes shaded — that comes
  from CodeMirror's parser rather than from a guess, so it cannot disagree with what you can see. An empty pair counts
  too, because a button that returns to "off" on the press that lit it reads as a press that failed.

`Ctrl+B`, `Ctrl+I`, `Ctrl+E` (code) and `Ctrl+K` (link) do the same as the buttons. Lists, tasks and headings are
buttons only for now: `H` steps the caret's line `none → ## → ### → off`, and deliberately never produces `#`, because
a title is typed by hand and a button that quietly demoted one would be rewriting the line you meant to leave alone.

Two things came free from the markdown language pack, which this window had installed but never wired up: **Enter now
continues a list** — `- [ ] ` at the start of the next line instead of a bare newline — and Backspace at the end of
`**word|**` takes the whole pair off rather than one asterisk at a time.

## Why the markers stay on screen

A rendered preview, or Typora-style hiding of syntax except on the line you are editing, was the other design. It is
not what this app does, and `markdownStyle.ts` says why in its own words: the file is what git, note-processor and
every editor that is not Jott will show you, so hiding the syntax in one window is how somebody gets surprised. It
would also mean decorations that replace visible characters, which fight selection maths, undo and input methods in a
way that produces cursor bugs a person feels immediately. The markers stay, faded. You get the formatting without the
file changing shape.

A full rich-text model — parse markdown on open, serialise it back on save — was the third option, and it is the one
that could have damaged notes: re-writing bytes you never touched collides with the way Jott decides whether a file
changed outside the window, which compares hashes of what it wrote against the bytes on disk.

## The note's name, in one place

`stormcock.md`, with its folder muted in front of it when it is not at the top of the vault. This row has existed
since 0.0.16 beside the **close** button; my first cut of this feature added a second name line inside the editor
pane, and opened a note to find the filename printed twice. A screenshot caught it. No test would have: nothing was
counting.

## Caught before it shipped

- Pressing Bold twice wrote `********` instead of undoing the first press. The unit tests had only ever pressed once;
  the end-to-end test that clicks the button twice found it in the browser-independent way, and the fix is a rule: a
  second press on a pair you are still inside takes that pair off, whether or not you typed anything between them.
- A caret sitting in `**|` counts as bold even though CommonMark has nothing to report for an empty span — see above.

## Numbers for this release

* Front end: **471 Vitest tests passing** (was 445), `npm run build` exit 0, `svelte-check` **0 errors, 0 warnings**.
* New modules: `src/lib/markdownEdit.ts` (what a press does to a range of text — pure, no editor in sight) and
  `src/lib/markdownState.ts` (which marks hold the caret). The arithmetic lives there because offset arithmetic fails
  quietly: a wrap one character off does not crash, it writes `**bold` into your note and waits until you reopen it.
* No new dependency. Everything used here was already in `package.json`.
* Rust and note-processor: untouched.

## What to look for

`docs/testing-on-windows.md` §2b-zc has the walk-through: bold a word mid-sentence and watch the asterisks arrive,
keep typing inside them, take the formatting off again with the same button, `Ctrl+B` without selecting anything,
Enter inside a `- [ ]` list, and open `INDEX.md` to confirm there is no bar at all on a file Jott will not write.
