# Jott 0.0.19 — the window stops moving, and stops arriving white

A fresh install on Windows reported two things in one sentence: *it opens as a white window, then it seems like the
window jumped to the right about 5 cm, then it opens correctly.* That is two bugs wearing one coat, and both of them
were ours — specifically, both came from letting `tauri.conf.json` create the window.

The backend inside is unchanged: **note-processor 0.5.1**, protocol 1. Nothing about your notes, your index or your
search changes in this release.

## The jump

Any window named in the config is created — and shown, because visible-by-default — before anything can move it
(`tauri-2.12.5/src/app.rs:2691` builds every config window, then runs our setup code). The saved position from your last
exit is applied afterwards by the window-state plugin, with an ordinary "move this window" call on a window already on
screen (`tauri-plugin-window-state-2.5.0/src/lib.rs:213`). So Jott appeared where Windows put it and then slid to where
it belonged. That is why nobody sees it on the very first launch, and everybody sees it after that.

Jott now builds its own window, hidden; the plugin restores size and position while nobody can watch; and only then does
the window appear — once, in the right place. The plugin was also told to stop deciding whether Jott is visible: with
that flag on, it shows the window only if a file says the last run left it visible, and an upgrade that kills Jott
mid-run can leave that file lying. An app you cannot see is not something you can close, inspect, or send a log from.

## The white

Our config has named paper since 0.0.9 — `[250, 250, 249, 255]`, the colour of `--paper` in the stylesheet. Half of it
was arriving. A config colour is applied to the **window** (`tauri-runtime-wry/src/lib.rs:889`); the webview's colour
comes only from a builder attribute that no config path ever sets (same file, line 4779), and that attribute is what
wry hands to WebView2 as its default background. So the surface actually covering the window in the frames before the
page paints kept WebView2's own default — white — however our config was spelled. Setting the colour through the
builder reaches both halves, which is why it now has to be set in code.

On a dark-mode desktop you will now see light paper for those first frames instead of white. That is still a guess:
before the first window exists, nothing in Jott knows whether your desktop wants dark — that question is asked of the
page, which does not exist yet. Trading one wrong colour for another is not a fix pretending to be one, and it is said
plainly here rather than fixed with a registry read to win a few milliseconds.

## The installed app never had a console. It does now

While checking whether this fix could be *seen*, one more thing turned out to be fiction: `tauri.conf.json` also said
`"devtools": true`, and the released Jott ignored that too. wry turns devtools off in any build that is not a debug
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

The mechanism is measured in dependency source, the geometry is now journalled, and four tests pin the structure so it
cannot quietly revert. **Nobody has seen the result.** The usual trick for a white first frame — keep the window hidden
until JavaScript reports it painted — was deliberately not used: it turns a broken bundle into an app with no window at
all, and I am not shipping a timing dependency I cannot observe. If paper does not land, that is the next step, taken
with a watchdog so Jott can never start invisible.

§2b-ix of `docs/testing-on-windows.md` is what to look for, and what to send back if it is still wrong.

## Checks

313 Rust tests (was 309), 373 front-end tests, clippy clean at `-D warnings`, and a Windows-target type-check of the
shell — which failed its first run and caught a plugin method that does not exist (`with_flags`; it is
`with_state_flags`). The four new tests were each shown failing before the change.
