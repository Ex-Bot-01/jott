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

# Jott 0.0.25 — a card that says one thing, one number per measurement, and the score that was printed twice

Three things from the tester's screenshots, in the order they hurt.

## "Which of this text is relevant for the average user?"

That was the question, and the honest answer was: **one sentence and one button**. The consent card had grown into
`models.auto_fetch = false`, a count of missing settings lines, a block of TOML to paste by hand, three buttons
(*Copy these settings*, *Try again*, *Read the note-processor request*) and a bare URL under them. Every line of that
was instructions for a manual job — and since note-processor 0.5.x one press does it through the pipe
(note-processor#7). Worse, there was no button to actually start the download without opening Settings, which is what
the tester asked for in the same message.

So the card now says, at most:

> Meaning search is off: nothing has been allowed to download yet. Allow the download and this folder can be searched
> by meaning as well as by words.

with **Allow the download** beside it — which answers note-processor *and* starts fetching, because that is what 0.0.24
made a yes mean. Two gaps refuse the button on purpose: a settings file np cannot parse (nothing can be written until
somebody edits it, so it gets *Try again* instead), and an embedding engine Jott does not recognise — pressing Allow
there would overwrite a choice this window has no business touching.

Nothing was deleted, only moved to where the question is asked: **Settings → embeddings** still names
`models.auto_fetch`, still says which file np read, still counts what is missing, and now carries *Copy these
settings* (with the same fallback textarea for when the clipboard refuses). One module answers both surfaces, so the
plain sentence and the technical one cannot drift into contradicting each other.

## The orange line was printed against the window frame

*"there is an orange text, but the text starts at the very left side of the gui and is not correctly handled."*
Correct, and visible in the screenshot: every other child of the sidebar brings its own inset — `.tools` pads 0.5rem,
the tree indents further — so a bare `<p class="warn">` between them sat flush against the window edge and read as
part of the chrome rather than as something Jott was saying. It now takes the same inset as the search row above it.

## Two scores, one measurement

*"when I have meaning only … I still see two scores, two identical scores."* Not two measurements — one, printed twice
and mislabelled. Note-processor assigns its `score` field three different ways (`search.rs:1226-1231`):

| Which half ran | What `score` holds | What the row shows |
|---|---|---|
| both (hybrid) | a fused rank of the two lists | the rank figure, plus `meaning 0.83` |
| vectors only (meanings only) | **the cosine itself** | one figure: `meaning 0.83` |
| words only | SQLite's bm25, negated | one figure, tooltip says keyword ranking |

The row used to print `score` and then `meaning <cosine>` unconditionally, so in meanings-only search the same number
appeared twice under a tooltip that called it a fused rank — a description of a measurement nobody took. The label now
comes from what np actually sent (`scoreLabel.ts`: `similarity` / `keyword` / `fused`), and the figures are compared as
np's numbers rather than as their printed forms: `0.8274` and `0.8273` both *read* "0.83" and must stay two figures,
because they really are two measurements.

Tests: **410 front-end** (six new pins on the score rule, plus the card's one-button contract, the copy affordance in
Settings, and that no GitHub URL appears on the card). Rust unchanged at 326.

---

## Jott 0.0.24 — "Allow the download" now downloads

Straight from the release after the last one: *"okay the loading bar is now gone?? why?"*

Because I removed the card and left nothing behind it. Two holes, both mine, neither of them hypothetical — each got
a failing test before it got a fix:

1. **A yes started nothing.** Pressing *Allow the download* asked np to record `embeddings.provider` and
   `embeddings.model`, Jott wrote `models.auto_fetch = true`, and then… nothing moved. No transfer begins until a note
   is saved or *Index now* is pressed. Until 0.0.23 the receipt card papered over that with a sentence about what the
   download would cost; remove the sentence and an accepting person was left staring at a window that looked exactly
   as untouched as before they clicked. So a granted answer now runs the pass itself — `ingest_all`, the whole folder,
   the same rule *Index now* follows — which is what makes np begin fetching, which is what puts bytes on a bar. A
   decline still starts nothing: there is nothing to fetch.
2. **The strip waited for luck.** It reads np's fetch markers, and those are only as fresh as the last poll. The poll
   ran once per opened folder, so a download that began afterwards could run for a while with no bar. Now a
   `fetching` progress line from np — one every second or so during a real transfer — triggers a read of the marker
   immediately, and the strip draws on the first reading.

The offer under the tree (*Set up embeddings*) used to start its own pass right after answering consent; that call is
gone, because both doors now share one path and two passes in a row would be Jott racing itself.

Tests: **404 front-end**, including the two new pins — a yes from the Settings panel reaches `ingest_all`, and a
`fetching` line puts the strip on screen with np's model id on it. Rust unchanged at 326.

---

## Jott 0.0.23 — the consent receipt is gone; the download is the answer

A small change, asked for directly: *"can we remove this message: 'note-processor had already recorded those
settings… The download this allows is about 295.5 MiB. Jott wrote…' — we don't need it anymore, because now the user
accepts it by downloading the model. So only remove this message and the three buttons below the text."*

They were right, and the reason is worth writing down rather than just deleting. That card answered **"what happened
when I clicked?"** — a fair question when the alternative was np's bare state token (`keyword-only:
embeddings.provider is not set`), which read like a refusal of an answer that had just been obeyed. Since 0.0.22 the
answer to that question walks across the top of the window with a byte count on it, so the receipt had become a
second, quieter description of something already visible — with three buttons for copying, retrying and complaining
about a thing that was working.

So: after a **yes that went through**, the card is not drawn at all — no sentences, no *Copy what happened*, no *Try
again*, no *Read the note-processor request*. The rule lives in one tested function, `outcomeNeedsCard`, and what it
still lets through is anything a person might have to act on:

* **np's refusal**, quoted unchanged — a yes np could not carry out is the one thing this window must never soften.
* **A no you pressed** — that gets confirmed, including the model id np remembered in case you change your mind.
* **Jott's own half failing** — it could not write `models.auto_fetch`, or found no settings file to write it into.
  A download you just agreed to may be unguarded; a silent window about that is the flattening this module was built
  to refuse.

What did *not* change is the Settings panel. It keeps saying what np recorded and where, because opening that panel
is somebody asking "what is the state?" — and an empty answer there reads as a read still in flight. One module
answers both surfaces (`consentHelp.ts`), with one deliberate difference, so they cannot drift into contradicting
each other.

Tests: **402 front-end**, including new pins that a clean yes leaves no card and none of those three buttons anywhere
in the window, that np's answer is still readable in Settings, and that a refusal arriving afterwards still gets a
card with np's sentence and never the paste-advice again. Rust unchanged at **326**.

---

## Jott 0.0.22 — a log that grew 48 scans a second, and a download you finally cannot miss

One tester's message contained two complaints. They were the same bug, and finding that out meant reading their log
line by line, which is exactly what the diagnostics journal exists for. Both are fixed; one of them also needed a new
kind of honesty about what a progress bar is allowed to claim.

## The runaway: Jott and np chasing each other's temporary files

The report was *"the file `jott.jsonl` grew rapidly and continuously, even after the GUI showed all files already
embedded."* Their log had **6,885 `scan` lines in 142 seconds** — about 48 a second, for over two minutes.

The cause was mine, shipped in 0.0.21. To stop deleted notes answering searches I taught Jott to ask np to re-index
whenever a path vanished. But note-processor writes its own catalogues by renaming a temporary into place —
`INDEX.md.tmp` → `INDEX.md` (`scan.rs:106`) — on **every** scan, and my filter for "that is np's own file" only knew
names ending `.md`. So:

> np finishes a scan → its temporary disappears → Jott reads that as *a note was deleted outside Jott* → asks for a
> scan → which creates and destroys another temporary → …

Three guards, each written as a failing test first:

* **`could_be_indexed`** (`vault/watch.rs`) — a vanished path only counts as a deleted note if it is a `.md` file that
  is not one of np's catalogues **or their temporaries**. The final-name rule catches the case where a rename beats a
  coalesced file-system event, which is most of the time.
* **A two-second floor** on rescans the *watcher* asks for (`ipc.rs`). Deletes and renames you make inside Jott are
  still immediate — that is a person acting, once per keypress. This only limits the automatic path, so any future
  feedback loop degrades to slow instead of to runaway.
* **A walk that found nothing says nothing** (`ingest/queue.rs`). An all-`unchanged` scan used to announce `Indexed`
  anyway, and every announcement asks the window to re-list the folder. Proven able to fail by putting the old
  unconditional emit back: the new test goes red, so this cannot rot quietly.

np did nothing wrong here, so nothing was filed upstream. Measured against bundled 0.5.1: it renames temporaries
because that is how you avoid leaving a half-written catalogue behind, and it answers `scan` truthfully every time.

## Why the window had gone deaf

The other complaint — *"no indicator if the model is now downloading or not"* — was this same storm's second casualty.
Every scan also emitted an event for the window. The bridge in the shell ran:

```rust
while let Ok(event) = events.recv().await { … }
```

`recv()` reports `Lagged` when a consumer has fallen behind — one slow paint, a dropped row, nothing fatal — and that
is an `Err`, so the loop **ended**. From then on that window could never show progress again, which is precisely when
a 295 MB download started. So: lag now keeps the stream alive (only `Closed` ends it), and one line lands in the
journal — `"event":"events-lagged"` with how many rows were dropped — so "the status line froze" has an answer in the
file somebody can send me instead of only in the source.

## A download strip nobody can overlook

Fixing the pipe makes the little bar in the corner work again, but you asked for something you could not miss, and a
progress *event* only paints while it arrives. So the window now asks np directly.

np keeps a marker file per in-flight fetch and reports them from `models.list`. Measured against bundled 0.5.1 with a
real download running:

```json
{ "id": "embeddinggemma-2", "bytes_done": 184550345, "bytes_total": 309855456,
  "nonce": "3636538-1794853194695092327-0", "started_at": 1794853194, "marker_error": null }
```

That answer comes over the pool's **reader** connection, so it cannot queue behind the embed doing the fetching, and a
poll costs one local pipe round trip: no new child, no workspace walk. While a marker exists the window asks every 1.5
seconds; when there is none, it stops asking.

What you see is a strip across the top of the note pane, in bold, with a determinate bar:

> **downloading embeddinggemma-2 · 137 of 295 MiB** ████████░░░░

The words and numbers are np's. There is no speed and no *2 minutes left*, because nobody measured either and an
estimate that turns out wrong costs more trust than a bar that says nothing. If np did not announce a total, no bar
pretends to know one — you get a moving stripe instead.

### The part that needed care: a marker outlives its process

I killed np halfway through a download and asked again. `models.list` went on answering the same byte count, forever,
until np's own housekeeping runs. So "np has a marker" is **not** "bytes are moving", and a strip that never cleared
would become furniture — the one light meaning *wait for it*, ignored.

Movement decides (`lib/downloadWatch.ts`): an advancing count keeps the strip; two readings with no movement clear it,
and the window keeps asking in case a fetch restarts. The very first reading is shown even though it might be stale,
because hiding first readings would hide real downloads that start and finish between polls. A leftover marker costs a
few seconds of strip; a hidden download costs you staring at a screen wondering. That was the trade I chose, on
purpose, and it is written down next to the code.

## Numbers for this release

* Rust: **326 passing**, with np off `PATH` (CI prints whether it is, because several tests are about it being absent).
  The CI floor goes from 315 to 320 so a disappeared suite fails the build rather than the tester's patience.
* Front end: **401 Vitest tests passing**, exit code checked rather than grepped.
* `cargo clippy -D warnings`, `svelte-check` (0 errors, 0 warnings), and a Windows-target `cargo check` of both crates.
* New pins worth naming: that a walk which changed nothing must not be announced as a change; that `Lagged` keeps the
  stream and `Closed` ends it; that after a lagged read the *next* row still arrives; and that the strip quotes np's
  bytes rather than Jott's arithmetic.

## What to look for

`docs/testing-on-windows.md` §2b-z has the walkthrough: measuring the log file's size ten seconds apart while
indexing, what the strip should say and when it should vanish, and what to send if it never appears — with the three
log lines that answer it (`rescan`, `events-lagged`, `models.list`).
