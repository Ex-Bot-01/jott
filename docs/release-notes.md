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

Two things this release *documents* rather than adds, because they already worked and nothing had ever said so:
`markdown()` installs the language pack's own keys, so **Enter continues a list** — `- [ ] ` at the start of the next
line instead of a bare newline — and Backspace at the end of `**word|**` takes the whole pair off rather than one
asterisk at a time. Both have worked since the markdown language went in. I spent part of this pass installing a second
copy of that keymap, which sat underneath the one that answers and could never run; a test now pins the call that
actually does it, so the next person does not have to rediscover which of the two is load-bearing.

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

Then a reviewer read the whole branch with fresh eyes and found nine more, all fixed here. They are worth listing,
because none of them made a test go red on their own:

- **A wrap that ended on a newline wrote broken markdown.** Selecting to the end of a line with Shift+Down and pressing
  Bold gave `**ab
**cd` — the pair at the head of the next line can only *open*, so nothing rendered bold and two stray
  marker runs went into the note, which autosave would have put on disk two seconds later. A wrap now stops at a line
  break, and at a blank line: emphasis cannot cross a paragraph, so `**a

b**` shows every character of itself.
- **Two presses close together were one undo step.** CodeMirror composes adjacent edits into one history event when the
  dispatch carries no `userEvent` and they land inside 500 ms. Press Bold, press Italic, press Ctrl+Z: the note went
  back to before *both*, which is what "Ctrl+Z does nothing" feels like. Each press now names its own event.
- **• and ☑ said nothing at all on an empty line** — the single most natural way to start a list from a bar that exists
  so you don't type `- `. Heading worked there; these two answered with silence, and on a whitespace-only line they went
  further and recorded an undo step that changed nothing. A press now either changes something or says nothing.
- **Pressing Bold at offset 0 of `## Storm` un-headed it.** The bar's own smoke test did exactly this and passed, because
  the assertion only looked for `****`. An ATX heading has to start its line, so the note's title had become a
  paragraph; the pair now goes in after the `## `.
- **A list marker inside a code fence lit the buttons.** A caret on `- item` inside a ```bash``` fence is reading shell
  text. The parser knows that and reports nothing there; my line-prefix rule contradicted it, which is the one thing
  this bar promised not to do.
- **`Ctrl+K` with the caret inside an existing URL** wrote `[label]([]()https://…)`. It now declines, because the link
  already is a link.
- **Three assertions asserted less than they looked to.** The read-only check sent its keystroke to `document.body`, and
  CodeMirror listens on `.cm-content` — events bubble up, never down, so that test stayed green with the read-only guard
  deleted (I deleted it, to be sure). One assertion in `markdownState.test.ts` was inside a `//` comment, because I put
  a literal `
` in a line instead of a newline. And the keymap pin proved only that a string was present. All three now
  fail when the thing they describe stops being true — which I checked by breaking each one.
- **The muted folder in the name row was not muted.** `.quiet` inside that row had no rule at all, so folder and filename
  both came out in full-weight ink while these notes promised otherwise. Found by looking at the screen.

## Numbers for this release

* Front end: **479 Vitest tests passing** (was 445), `npm run build` exit 0, `svelte-check` **0 errors, 0 warnings**.
* Thirty-four of them are new, and eight of those exist only to hold a fix from the review pass. Three of the fixes were
  checked by *breaking* the thing they guard — deleting the read-only guard, moving the keymap back, unmuting the
  folder — and watching the suite go red.
* New modules: `src/lib/markdownEdit.ts` (what a press does to a range of text — pure, no editor in sight) and
  `src/lib/markdownState.ts` (which marks hold the caret). The arithmetic lives there because offset arithmetic fails
  quietly: a wrap one character off does not crash, it writes `**bold` into your note and waits until you reopen it.
* No new dependency. Everything used here was already in `package.json`.
* Rust and note-processor: untouched.

## What to look for

`docs/testing-on-windows.md` §2b-zc has the walk-through: bold a word mid-sentence and watch the asterisks arrive,
keep typing inside them, take the formatting off again with the same button, `Ctrl+B` without selecting anything,
Enter inside a `- [ ]` list, and open `INDEX.md` to confirm there is no bar at all on a file Jott will not write.
