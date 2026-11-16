# Jott 0.0.24 — "Allow the download" now downloads

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
