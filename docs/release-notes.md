# Jott 0.0.28 — the markdown styling starts working

A note should look like the markdown you typed: heading bigger than prose, `**this**` bold with its asterisks faded,
a link underlined and its brackets quiet. That has been the design since 0.0.9, in a file called `markdownStyle.ts`,
with rules in `Editor.svelte` for every class it names, and a test that refuses a class with no rule behind it.

**None of it was reaching the screen.** Not since 0.0.9. Headings were never sized by us, bold was never ours, and
what looked like deliberately faded markers was another style's grey. This release switches it on, which is why the
notes look different rather than merely better.

## What you'll see

- **Headings are the size they were written to be.** `#` at 1.5em, `##` at 1.3em, `###` at 1.15em, then weight alone
  down to `######`. Before, every heading in a note was body text with an underline under it.
- **Titles are no longer underlined.** Underline is what a link means in this editor — see the `[field log]` label in
  any note — and having every heading wear it too made a page of titles look like a page of things to click. That
  underline came from CodeMirror's default style, which was answering for us; now our own rules answer and headings
  say "heading" with size and weight.
- **The format bar sits over the text it formats.** It used to keep a flat padding while the buffer centres a 74ch
  measure, so on a wide window the `B` sat about 120 px to the left of the first character — controls floating above
  empty gutter. Both now read the same measure, which is also why the bar moves inward on a wide window: it follows
  your text there.
- **A button that's on looks on.** Pressed was a faint fill with a thin border, and the keyboard-focus ring is a 2 px
  ink outline — at three-quarters of a centimetre those are one drawing, so "this mark is in what you're looking at"
  read as "the mouse is over me". Pressed is now a deeper fill with an inner edge; focus keeps its outer ring.

## Why it never worked, and why nothing said so

CodeMirror takes several highlight styles at once, and `fallback: true` decides **precedence between them** — which
style answers for a tag two of them both claim. It has nothing to do with painting text before the parser catches up,
which is what a comment of mine in `Editor.svelte` claimed for it. We installed the default style as a fallback and
then ours as a fallback too, behind it. A fallback behind a fallback loses every tag both want, and ours wants
heading, strong, emphasis, code, link and the markers — so CodeMirror answered all six and our classes were never
placed on the page at all.

The tests could not see this because they were the wrong shape. They read `Editor.svelte` as text and asked whether a
rule exists for each class in `MARKDOWN_CLASSES`. Every rule does exist. A class named in a stylesheet is easy to
prove and impossible to see, and there was nothing asking what the rendered editor actually put on the page. There is
now: mount the window, open a note, look for `.cm-content .md-h1` and `.md-meta`, then select a word, press Bold and
wait for `.md-strong`. That test failed for nineteen releases' worth of code before this fix, which is what it is for.

Two attempts at this went wrong before that was written, and both the same way: I added `text-decoration: none` for
headings, watched the pin go green, and looked at a screen with an underline still on it. The second attempt used a
deeper selector and did the same. The rule was fine; there was simply no element carrying that class to style it onto.

## One thing I said last time that was wrong

I reported that files without a status dot fail to reserve the dot's column, so attachments sit at the wrong depth in
the tree. Retracted: `Tree.svelte` renders both the twisty and the marker as fixed-width slots on every row, and
indentation comes only from depth. My two attempts to measure it off screenshots were confounded by the selection
highlight and the window border. What is real — and I have not changed it, because it's a judgement rather than a
fault — is that depth is carried *only* by indentation, so a file at the top of the vault that sorts just under a
collapsed folder reads as that folder's child.

## Numbers for this release

* Front end: **483 Vitest tests passing** (was 479), `npm run build` exit 0, `svelte-check` **0 errors, 0 warnings**.
  Verified on the merged result on `main`, not only on the branch.
* No new dependency; no change to any note's bytes — this is decoration over the buffer you type in, and the file on
  disk is untouched by opening it.
* Rust, Tauri and note-processor: unchanged.
* Checked by eye in both themes at 1400 px before tagging, because three of these four changes are things a
  stylesheet test can only claim about itself.

## What to look for

`docs/testing-on-windows.md` §2b-zd walks it: a heading should be visibly bigger than its own paragraph and not
underlined; the bold/italic row should start above the first character of text rather than out in the margin; pressing
Bold should shade the button in a way you can tell apart from Tab-focusing it. One thing I'd genuinely like to hear
even if it isn't a bug: if your notes look *worse* now — headings too big, code tint too loud — say which note and
what bothers you. The styling that's now switched on is what was designed in 0.0.9 and never displayed; if nine
releases of the other look is what you'd miss, that's a design conversation worth having rather than a defect to log.
