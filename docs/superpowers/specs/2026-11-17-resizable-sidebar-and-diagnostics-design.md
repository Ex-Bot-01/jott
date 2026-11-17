# A sidebar you can size, and a diagnostics drawer that uses the window — design

Approved by the person who asked for it on 2026-11-17: approach **A** for both halves. Small change; this is the
short design, so it covers what was discussed and nothing more. A second pass over the rest of the window — with
screenshots and proposals — is agreed to follow this one landing, and is deliberately not specified here.

## Intent and why

Two things in the window are set by whoever wrote the CSS rather than by the person using it:

- **The sidebar cannot be resized.** `src/App.svelte` fixes the first grid track at `minmax(19rem, 26rem)` —
  266 px to 364 px at this app's 14 px root. The floors were chosen for good reasons (below 266 px `+ note` wraps
  and `INDEX.md` comes out as `INDEX.ma`; above 364 px the tree starts stealing reading room) and they are still
  right as *limits*. They are wrong as a decision made once for every window, on every monitor, at every zoom. On
  an 820 px-wide window the sidebar takes 44 % of the screen; on a wide external display it stays at 364 px while
  a long note path is what you actually need to see.
- **The diagnostics drawer never gets the window's width.** `.window` is a grid and `.drawer` carries no
  `grid-column`, so auto-placement parks it in row 3, column 1: about 364 px wide, sitting under the sidebar with
  dead space to its right. Its own `max-height: 45vh` plus a nested `.activity { max-height: 22vh }` cap the height
  twice, so the activity list falls off the bottom of the screen — and because the drawer lands *after* the status
  line in the grid, the status strip ends up above it rather than at the bottom edge.

The drawer was designed wide — the sketch in
[`2026-11-13-diagnostics-drawer-design.md`](2026-11-13-diagnostics-drawer-design.md) spans the window — and the
stylesheet never delivered it. Its purpose is to answer "what did the backend do" without a round trip through a
log file on somebody else's machine, and a 364 px column with its rows cut off below the fold answers it badly.

## Goals

- Drag the sidebar wider or narrower with the mouse, and set it from the keyboard, within limits that keep every
  label in one piece.
- Remember both sizes across restarts, the way the search choices are remembered.
- Give the diagnostics drawer the full width of the window, a height that is yours to choose, and enough room that
  the activity list is the thing that grows when you drag it taller.
- Put the status line back as the last row of the window.

## Non-goals

- Collapsing the sidebar away to nothing, and any other change to what the window contains. The whole-window pass
  comes after this one, with its own design.
- A second OS window for diagnostics — still refused for the reasons the 2026-11-13 design gives (its own webview,
  capability entries and shell code, none of it checkable from the box that builds Jott).
- Remembering sizes *per vault*. These are facts about the window on this desk, not about the folder open in it.

## Anti-goals

- A handle nobody finds. If resizing is invisible until you happen to grab the right pixel, the feature does not exist.
- A remembered size that outlives its usefulness: a 560 px sidebar saved on a 27" monitor must not arrive on a
  laptop and leave a 90 px note pane.
- Any change to what the drawer *says*. Its privacy rule — no note contents, closed key set pinned by tests — is
  untouched here; this is layout and sizing only.

## Constraints

- Front-end logic that can be tested headlessly lives in a pure TS module with Vitest tests, not in a component
  (README ground rule). Clamp math and storage parsing are exactly that kind of logic.
- `jsdom` has no layout engine (`src/App.smoke.test.ts` says so at the top), so no test here can measure a pixel.
  Tests hold the *arithmetic* and the DOM shape; the pixels are verified in the browser harness with screenshots,
  which is what that harness exists for.
- The note pane must keep enough width to read at all times, including while dragging.
- Svelte 5 has no event modifiers (`onclick:self` is not a thing), so the About fix is an explicit target check in
  the handler rather than a `|self`.

## Approaches considered

**Sizing the sidebar.**

- **A drag handle driving one CSS variable** *(chosen)* — a real, focusable separator between the columns. Works
  with the mouse and the keyboard, keeps the existing floors as clamps, and costs about sixty lines plus tests.
- *Width follows the window automatically* — no interaction at all, but it is not yours to set, which is what was
  asked for. Rejected.
- *Handle plus a collapse-to-zero button* — genuinely useful on a laptop, but it is a second feature and it changes
  what Escape and Tab mean in the window. Deferred to the whole-window pass.

**The diagnostics surface.**

- **A full-width bottom drawer you can drag taller** *(chosen)* — delivers the shape the original design drew, keeps
  the note visible while the queue moves, and is a stylesheet change plus one handle.
- *A right-hand panel* — better for very long paths, but it takes reading width off the note permanently, which is
  the part of the window you look at for hours.
- *A modal like Settings* — tidier, and consistent with About, but it covers the note. The 2026-11-13 design
  refused that on purpose: what you are debugging is usually happening in the note behind the panel.

## Decided shape

```
┌ tree ──────────────╫ note ─────────────────────────────────┐
│                    │                                       │
│                    │                                       │
╞════════ drag ══════╧═══════════════════════════════════════╡
│ What Jott is doing                                     ✕  │
│ ┌ runtime ────────────────┐  ┌ activity ─────────────────┐ │
│ │ np   0.5.1 (protocol 1) │  │ just now · scan · 41 ms   │ │
│ │ log  …\jott.jsonl 18 KiB│  │ 2 min ago · embed · ok    │ │
│ └─────────────────────────┘  └───────────────────────────┘ │
├────────────────────┴───────────────────────────────────────┤
│ Both · notes embedded              idle          [embed]   │
└────────────────────────────────────────────────────────────┘
                              ▲ the ╫ between the columns is the sidebar handle
```

**The handle.** A 5 px strip between the tree and the note pane: `role="separator"`,
`aria-orientation="vertical"`, focusable, with `ew-resize` cursor, a tint on hover and focus so it can be found,
and a tooltip saying what it does. Dragging moves the first grid track; **double-click** returns it to the default;
**ArrowLeft/ArrowRight** nudge by 16 px (**Shift** 48 px) and **Home** resets, so the keyboard gets the same control
rather than a worse one. While dragging, text selection is suppressed and the cursor stays put even when the pointer
leaves the strip.

**Limits.** Never narrower than 266 px — that is where the labels stop breaking, and it stays true whatever is
remembered. Never wider than 45 % of the window or 560 px, whichever is smaller, so the note pane always keeps a
readable column. The remembered width is re-clamped against the *current* window on load and again whenever the
window is resized.

**Remembering.** Two numbers in `localStorage`, next to the search choices and following that module's rules
exactly — namespaced keys (`jott.layout.sidePx`, `jott.layout.drawerPx`), a stored value counts only if it parses as
a plain integer, a storage that throws reads as "never asked", and a write that fails is swallowed. *My call:* per
app rather than per vault, because a window size belongs to the desk; and no new Rust-side config field, so this
needs nothing from `jott-core` or from np.

**The drawer.** It spans the window (`grid-column: 1 / -1`) and sits *before* the status line in the grid, so the
status strip is the last row again. Its height — default 320 px, minimum 180 px, maximum 70 % of the window — is set
by a drag strip along its top edge, remembered like the sidebar's, and it is a real budget: the drawer becomes a
column flex where the activity list is the part that grows (`flex: 1 1 auto; min-height: 0`) instead of hitting a
second cap of its own. The `max-height: 45vh` and `.activity { max-height: 22vh }` rules go away. *My call:* above
roughly 900 px of window width the drawer lays its two long lists side by side — runtime facts on the left, activity
on the right — because that is what the new width buys you (more rows visible, paths no longer broken mid-word), and
below that it stacks back into one column.

**About.** Its scrim closes the panel only when the click *landed on the scrim* (`event.target === event.currentTarget`),
so pressing **Copy details** or a link no longer dismisses the panel — today every click inside it bubbles to the
scrim and closes it, including the Diagnostics button. Pressing **Diagnostics** closes About and opens the drawer:
one thing on screen at a time, rather than a modal floating over a dimmed drawer.

## Left to the builder

Class names, where the clamp module lives (a `src/lib/layoutSize.ts` alongside `searchSettings.ts` is the obvious
place), how the pointer capture is written, and the exact wording of the two tooltips — with one exception: what a
handle says about itself is copy like everything else in this window, so it gets reviewed for sense rather than for
length.
