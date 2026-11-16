# Jott 0.0.19 — the window stops moving (and one gap left honestly open)

A fresh install on Windows reported two things in one sentence: *it opens as a white window, then it seems like the
window jumped to the right about 5 cm, then it opens correctly.*

One of those is fixed with its mechanism measured. The other is **not** fixed, and this release says so rather than
claiming otherwise: I had a story for the white that read convincingly, and a reviewer took it apart by reading one
crate further than I did. The difference between the two halves is the interesting part, so it is written down below.

The backend inside is unchanged: **note-processor 0.5.1**, protocol 1. Nothing about your notes, your index or your
search changes in this release.

## The jump — fixed

Any window named in the config is created — and shown, because visible-by-default — before anything can move it
(`tauri-2.12.5/src/app.rs:2691` builds every config window, then runs our setup code). The saved position from your last
exit is applied inside that same creation by the window-state plugin, with an ordinary "move this window" call on a
window already on screen (`tauri-plugin-window-state-2.5.0/src/lib.rs:213`). So Jott appeared where Windows put it and
then moved to where it belonged.

One condition, stated because my first version of these notes got it wrong: **the move needs a saved state to restore.**
With no `.window-state.json` the plugin never calls `set_position` at all. That does not make your report mistaken — an
*upgrade* install keeps that file (`%APPDATA%\dev.jott.desktop\.window-state.json`) while still counting as freshly
installed, which is the likely case here. But a jump on a genuinely first run would be a different bug, and DPI
renegotiation on a multi-monitor setup — which moves windows after they appear — is the remaining candidate. The new log
line says which one you hit.

Jott now builds its own window, hidden; the plugin restores size and position while nobody can watch; and only then does
the window appear — once, in the right place. The plugin was also told to stop deciding whether Jott is visible: with
that flag on, it shows the window only if a file says the last run left it visible, and an upgrade that kills Jott
mid-run can leave that file lying. An app you cannot see is not something you can close, inspect, or send a log from.

## The white — still open, and now labelled as open

I wrote a paragraph here explaining that our paper colour `[250, 250, 249, 255]` reached the *window* but never the
*webview*, so WebView2 painted its own default white until the page caught up. It was a clean story. **It was wrong.**
`tauri-runtime-2.12.2/src/webview.rs:498` takes that same config field and fills the exact webview attribute I claimed no
config path ever touches, and `git show v0.0.18:apps/shell/tauri.conf.json` confirms the colour has been spelled
correctly since 0.0.3. It reached both layers all along — so this release changes nothing about the colour of the first
frame, and the white you saw is still unexplained.

What has been ruled out since: a GDI white erase behind our back. tao paints its own background in `WM_ERASEBKGND`
(`tao-0.37.2/.../event_loop.rs:1042`) with no class brush, and wry's container window is the same
(`wry-0.57.2/src/webview2/mod.rs:222`). What is left, in order of likelihood: WebView2 runtimes lacking the newer
controller-options interface, where the colour lands later than the first frame (`wry webview2/mod.rs:409-431`), and
WebView2's own surface before its first painted frame — neither of which Jott can set from where it stands.

So this release ships two instruments instead of a claim: the launch geometry now goes to the log, and the installed app
finally has a console (below). If the white is still there in 0.0.19, §2b-ix says what to look at — and next time the
answer starts from something you can paste rather than something you have to describe.

The lesson, being the second one this week: I grep'd `tauri`, found nothing, and stopped. Twice now the reader has been
two crates down. "Nothing reads this field" needs a search of every crate in the graph, or it stays a question.

## The installed app never had a console. It does now

While checking whether this fix could be *seen*, one more config setting turned out to be fiction — and this one was
checked both ways before being written down, unlike the colour above. `tauri.conf.json` also said `"devtools": true`, and
the released Jott ignored it. wry turns devtools off in any build that is not a debug
build (`wry-0.57.2/src/lib.rs:873-875`), and Tauri passes its own setting along only when the `devtools` feature is
compiled in (`tauri-runtime-wry/src/lib.rs:5146`) — a feature Jott never asked for. So **Ctrl+Shift+I has done nothing in
every installed build since 0.0.4**, including in the instructions this guide has been giving since then: "press it and
paste this", "that is a new bug and I want the console output". If you ever tried one and no window appeared, nothing
was wrong with your machine.

Jott now builds with tauri's `devtools` feature, so an installed copy has a real console: **Ctrl+Shift+I**, or right-click
→ Inspect. The reason is not tidiness — for two weeks the only instrument between a report and a fix has been your
description of what you saw, and half of them have pointed at the wrong thing.

## One thing we can read from your log

Nobody who builds Jott has ever seen it run: this crate needs WebView2 or webkit even to compile, and no machine in the
loop has one with a display attached. So each launch now writes a line to the log — size, top-left position, and whether
a saved state file was there:

```json
{"event":"window","detail":"1200x760 with its top-left at +312+180; state file present","at_ms":1731...}
```

Numbers and one word about a file: no paths, no note text. If the window still misbehaves after this release, that line
says what happened without anybody having to describe a movement in centimetres.

## What this release cannot claim

The jump's mechanism is measured in dependency source, the geometry is journalled, and six tests pin the structure so it
cannot quietly revert. **Nobody has seen the result**, and the white frame has no explanation on offer at all — which is
better than the false one that was here an hour ago. The usual trick for a white first frame — keep the window hidden
until JavaScript reports it painted — was deliberately not used: it turns a broken bundle into an app with no window at
all, and I am not shipping a timing dependency I cannot observe. If 0.0.19 still opens pale, that is the next step, taken
with a watchdog so Jott can never start invisible.

§2b-ix of `docs/testing-on-windows.md` is what to look for, and what to send back if it is still wrong.

## Checks

315 Rust tests (was 309), 373 front-end tests, clippy clean at `-D warnings`, and a Windows-target type-check of the
shell — which failed its first run and caught a plugin method that does not exist (`with_flags`; it is
`with_state_flags`).

The six new tests were each shown failing before the change, and then shown failing again **after** it, with the
regression put back by hand: visibility returned to the plugin → red; the show wrapped in an `if` → red; a second
`.visible(true)` added to the chain → red; the devtools feature dropped from the manifest → red. A reviewer had said two
of my five pins could stay green with the bug restored, and was right about both.

That review also blocked this release once, on the false colour claim above, and its verdict on what remains is carried
in the plan: `docs/superpowers/plans/2026-11-17-a-white-window-that-moved.md`.
