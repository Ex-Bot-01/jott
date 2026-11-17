# Jott 0.0.26 — a sidebar you can size, and a diagnostics drawer that finally uses the window

Two layout decisions that were made once, in a stylesheet, for every window on every monitor — and one bug found by
clicking the exact button this release was about.

## The tree column was 364 px wide because I said so

`src/App.svelte` had `grid-template-columns: minmax(19rem, 26rem) 1fr`. Both numbers were measured rather than
guessed — below 266 px `Index now` and `+ note` wrap onto two lines and `INDEX.md` arrives as `INDEX.ma`, and past
about 364 px the tree starts taking reading room from the note — and they have been right ever since. What was wrong
is that they were *limits nobody could move*. In an 820 px window the sidebar still took 44 % of the screen; on a wide
display it stayed at 364 px while the path you were trying to read sat in the other column.

There is now a handle between the two columns: drag it, nudge it with the arrow keys (**Shift** for a bigger step),
**double-click** or **Home** for the default. It is focusable and tinted on hover and focus, because a handle that
only exists under a pointer is a handle the keyboard never finds; in forced-colours mode it gets a real border instead
of a tint. The limits stayed, as limits:

| | |
| --- | --- |
| floor | **266 px** — where the sidebar's own labels stop breaking |
| ceiling | **45 % of the window, never past 560 px** |
| remembered | yes, in `localStorage` (`jott.layout.sidePx`), re-clamped on load *and* on window resize |

That last row is the part worth testing on a laptop: a width dragged on a 27" monitor narrows to fit when the window
is small, and **the stored preference is not rewritten**, so the big monitor gets its width back tomorrow. The arrow
keys belong to the handle and to nothing else — arrows typed in the note or the search box move the caret and leave
the columns where they are.

## The diagnostics drawer was 364 px wide because of a grid rule nobody wrote

It was designed as a strip across the whole window — the sketch in the 2026-11-13 design spans it — but `.drawer` never
said which grid column it wanted, so auto-placement parked it in **row 3, column 1**: under the sidebar, about 364 px
wide, with dead space to its right and the status line pushed up *above* it. Then its height was capped twice (`45vh`
on the drawer, `22vh` on the activity list inside it), so the rows you were looking for were below the bottom of the
screen and dragging the window taller bought nothing.

Now it spans every column, sits above the status line so that strip is the last row again, and takes a height **you**
set — drag its top edge, or use the keys, **Home** for the default:

| | |
| --- | --- |
| default | **320 px** |
| floor | **180 px** |
| ceiling | **70 % of the window, or whatever leaves the note pane 200 px** — and never below the 180 px floor, which wins in a very short window (a 480×320 CSS-px window at 150 % scaling is one) |
| remembered | yes (`jott.layout.drawerPx`) |

The height you choose goes into the activity list rather than into an empty box: both caps are gone and the list is
what flexes. And because a full-width drawer with one long column wastes the width it just won, above ~900 px the
runtime facts sit on the left and the activity rows on the right — which is what makes paths like
`C:\Users\chris\AppData\Roaming\jott\logs\jott.jsonl` fit on one line instead of three. That is also why `word-break:
break-all` became `overflow-wrap: anywhere`: a path should break at a separator when it must and not at all when it
need not.

## Pressing *Diagnostics* in About closed About. And opened nothing you could read.

Found by clicking that exact path with the window in front of me. Two separate faults, one screen:

* About's dismiss handler sat on the scrim, and the panel is inside the scrim — so **every** click inside it bubbled
  up and closed the panel. *Copy details*, a link, and *Diagnostics* each dismissed the panel they were standing in.
  It now closes only when the click actually landed on the scrim. (Svelte 5 has no `|self`, hence the explicit target
  comparison.)
* Opening the drawer left About floating over it behind a dimming scrim — over the very rows you had pressed the
  button to read. **Diagnostics** now closes About: one thing on screen at a time.

Two more things showed up in the same pictures, and both are fixed here rather than filed. A 2 px ink line ran the
full width of the window when the drawer opened — that was `app.css`'s focus ring on an element with `tabindex="-1"`,
which no keyboard can reach and which only ever gets focused by our own code; a ring there communicates nothing and
looked like a rendering fault. And the drawer's grab strip began a few pixels *below* the border it belongs to, so
aiming at the line you could see dragged a text selection out of the note pane instead. The strip is flush with the
edge now.

## Caught by the review, before it shipped

The whole-branch review found four things the harness pass had not: the drawer's drag measured its height as
"everything below the pointer", which forgot that the status strip sits under it — so pressing the edge jumped the
drawer about 40 px taller and slid the strip out from under the cursor, making the promised double-click reset nearly
impossible to land. It is a delta from where the grab started now (`drawerHeightFromDrag`), which cannot drift whatever
else is underneath. A right-button press on either handle resized things, so both take the primary button only, and a
move that reports no button held ends the drag instead of latching into "resizes on hover". Neither handle set
`touch-action`, so on a touchscreen WebView2 answered a finger drag with a pan and a `pointercancel` — one property each.
And the drawer never re-clamped when the window changed shape, which is how undocking with the drawer open collapsed
the note pane and pushed the status strip off the bottom edge; it now follows the same rule as the sidebar — clamp for
this window, leave the memory alone. The clipboard-refused "copy it yourself" box also moved inside the part that
scrolls: it used to sit after the footer in a clipped column, unreachable at exactly the short heights where you need it.

## Numbers for this release

* Front end: **445 Vitest tests passing** (was 410), `npm run build` exit 0, `svelte-check` **0 errors, 0 warnings**.
* Rust: no code touched. `apps/shell/Cargo.lock` is bumped with the manifest, as every release commit since 0.0.23 has
  done — leaving it at 0.0.25 while `Cargo.toml` said 0.0.26 makes any `cargo build --locked` fail outright (checked:
  exit 101 against the stale lock, 0 against the fixed one), and CI's plain `npx tauri build` would have rewritten a
  tracked file mid-release.
* New module `src/lib/layoutSize.ts`: every limit and both memories, tested as arithmetic. `jsdom` has no layout
  engine, so the pixels were checked another way — the browser harness (`npm run ui`) at 1920/1200/820 px window
  widths, in light and dark, dragging both handles with a real pointer: sidebar wide to its 540 px ceiling with every
  label on one line, floor at 266 px, drawer grown and shrunk from its top edge, hover tint measured against paper in
  both schemes (250→190 light, 27→85 dark).

## What to look for

`docs/testing-on-windows.md` §2b-zb has the walk-through: drag the sidebar and reopen Jott, `Ctrl+Shift+D` and read a
path whole, drag the drawer taller and count the rows, and press **About → Diagnostics** expecting one panel.
