# Jott 0.0.20 — the window gets one set of controls, and dark mode gets the colours it always asked for

A tester asked me to look at how Jott *looks*, not just what it does. Fair: eleven releases had gone out on the
strength of test counts, and nobody had looked at a screenshot of the whole window with fresh eyes. I did. This is
what that found.

The backend inside is unchanged: **note-processor 0.5.1**, protocol 1. Nothing about your notes, your index or your
search changes in this release. Everything below is the skin — with one exception, in the last section, which is a
bug that only existed on dark desktops.

## First, how I looked

Not honestly, at first. This build box has no working webview: the released **0.0.19** binary starts here and its log
says so (Jott 0.0.19, bundled np 0.5.1, protocol 1), but WebKitGTK refuses to paint on a headless Xvfb display with no GPU
— `Could not create default EGL display: EGL_BAD_PARAMETER` — and there is no root to fix it. I checked my capture
path against `xclock` first, so the blank window was the environment and not my eyes.

What I could do is serve **the same components** — the same `App.svelte`, the same CSS, the same status line — in a
browser, with the Tauri calls replaced by an in-memory fake folder. That harness is now in the repo (`npm run ui`),
and it is how every claim below was looked at. It shows me your layout and my typography; it does **not** show me
WebView2's font rendering or Windows' high-contrast mode, so treat anything about those as unverified until you say
otherwise.

## What changed

**One control language.** Before this, each component drew its own native button with its own padding: the toolbar's
were 20 px tall, the rename field 24 px, and `Index now` and `+ note` wrapped onto two lines because nothing insisted
a label stay on one. There is now a single set of rules in `app.css` — a real ~29 px target, labels that do not split,
one visible keyboard ring, and a filled "primary" button made of the ink itself rather than a new colour. Tree rows
and menu items deliberately stay flat; what they can no longer do is disagree with each other about what a button is.

**A sidebar wide enough to read.** It opened at 14 rem. At that width `INDEX.md` came out as `INDEX.ma`, the toolbar
wrapped, and the sentence explaining `.md` files ran five words to a line. It starts at 19 rem now.

**File names you can read in full.** This one is a real bug, not a preference. A name that is greyed — np's own
`INDEX.md` — is drawn italic, an oblique glyph's ink reaches slightly past its advance, and `overflow: hidden` cut the
letter there. So the one file Jott shows you in italics was the one file whose name it would not finish. Names now get
a little slack, plus a proper ellipsis when a name genuinely does not fit.

**Writing gets a prose face and a measure.** Notes were set in CodeMirror's default monospace at whatever width the
window happened to be: on a maximised window that is a line length you cannot follow back to its start. Prose is now
the interface face at 15 px, held to about 74 characters, with code blocks still monospace because a table loses its
shape without it. The measure is padding *inside* the text area rather than a narrower column, so clicking anywhere
along a line still puts your caret where you clicked.

**An empty pane you can act on.** With nothing open, Jott said "Choose a note on the left, or make one." — correct,
and not something you could do anything about from the middle of the window. It now offers the same open-folder action
as the toolbar's `Open…`, in the place your eye already went.

**The explanation paragraph had no styling at all.** The sentence about what note-processor reads sat flush against
the window edge, in full-strength ink, at the same size as the file names above it — so the thing that decides whether
a folder of PDFs is mysterious read as part of the list.

## And one that was a bug, not a style choice

Three stylesheets hung their dark-mode colours off `:global(.dark)` — and **nothing in Jott ever adds a `.dark`
class**. The whole mechanism is `darkMode.ts` reading `prefers-color-scheme` and telling CodeMirror which facet to use;
no element was ever wearing that class. So on a dark desktop you were getting the light-mode amber (`#b8863b`) for
unsaved changes, for conflicts, for the read-only index banner — a colour chosen for white paper, on near-black. The
dark variants are media-query driven now, which is the only mechanism that exists, and there are two new tests: one
asserts no stylesheet styles a class nothing can produce, one follows the amber into a block that can actually apply.
I checked both could fail by putting the old selector back.

That is the same shape as two bugs this project has already been burned by — a `.greyed` class whose rule had been
deleted, and a `--cm-cursor` variable that CodeMirror does not have. A style written against something nobody produces
is not a design decision waiting for a renderer. It is dead code that looks like coverage.

## What I did not verify

- **Windows rendering.** Everything above was seen in Firefox on Linux. Spacing, wrapping and contrast transfer; hinting and
  ClearType do not.
- **High-contrast / forced-colours mode.** The caret and the model dot have explicit rules for it from earlier releases and I
  did not re-test them here.
- **Your first-run experience on a real desktop**, which is still the thing I most need eyes on from v0.0.19: whether the
  window now arrives in one place instead of jumping, and what colour you see before the page paints.

§2b-x of the tester guide lists the four things worth a look. If any of this made something worse rather than better —
particularly the prose face, which is the most opinionated change here — say so and I will put the monospace back; it
is one rule, not a refactor.
