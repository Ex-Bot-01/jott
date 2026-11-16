# A window that opens white and then moves — plan for 0.0.19

Reported from a fresh install on Windows:

> When I open the freshly installed app at first it opens as white window then it seems like the window jumped to the
> right about 5 cm on the screen and then open correctly.

Two symptoms, and they are not the same bug. Written down before touching anything, because both of my previous
"fixed the caret" claims turned out to be about elements that did not exist, and the way to stop doing that is to say
what was measured and where.

## What was measured, in the crates we compile against

**The move.** `tauri-2.12.5/src/app.rs:2687-2698`: `fn setup()` walks `config.app.windows` and builds each one
(line 2691) *before* it calls our `setup` closure. A window named in the config is therefore created — and shown,
because `visible` defaults to true — at whatever position Windows chose. The geometry somebody saved on the last exit
is applied by `tauri-plugin-window-state-2.5.0` from its `on_window_ready` hook (lib.rs:433), which runs inside that
same creation, and `restore_state` does it with plain `set_position` / `set_size` calls (lib.rs:213, 230) on a window
that is already on screen. So the launch is: appear here, then be moved to where you belong. On the very first launch
there is no state file and nothing moves — which matches "at first" being when it was noticed, and predicts it happens
on every later launch too.

**The white.** Our config had `"backgroundColor": [250, 250, 249, 255]` since 0.0.9. Half of that colour arrives:
`tauri-runtime-wry-2.12.3/src/lib.rs:889` reads `config.background_color` and sets it on the **window**. The webview's
own colour is taken from `webview_attributes.background_color` alone (same file, line 4779), and that attribute — never
set by any config path — is what wry hands to WebView2 as `SetDefaultBackgroundColor`
(`wry-0.57.2/src/webview2/mod.rs:415`). The surface that actually covers the client area before the page paints kept
WebView2's default, which is white. Tauri's builder method sets both halves
(`tauri-2.12.5/src/webview/webview_window.rs:1248-1250`); the config field only reaches one of them.

I first wrote to myself "nothing reads `WindowConfig::background_color`". That was wrong, and it was wrong because I
grep'd `tauri` and stopped. The reader is two crates down, in the runtime. What survives checking is narrower: the
config colour never reaches the **webview**, which is the layer that paints over it.

## Decisions

1. **Build the window in code, hidden, and show it ourselves.** `app.windows` becomes empty; `setup` builds
   `main` with `.visible(false)`, the plugin restores geometry while nobody can see it, then `show()` + `set_focus()`.
   One appearance, in the right place. Pinned by `crates/jott-core/tests/shell_window.rs`, which also pins that a config
   window never comes back — because its behaviour is invisible from inside the app.
2. **The plugin no longer owns visibility.** `StateFlags::all().difference(StateFlags::VISIBLE)`. With `VISIBLE` in
   the set, `restore_state` shows the window only if the state *file* says it was visible (lib.rs:279), and the file is
   written on exit — an MSI upgrade that kills Jott mid-run can leave a file saying "hidden", and a window nobody can
   see cannot be closed, inspected, or sent a log from. This is the same failure I ruled out in 0.0.9 (never hide a
   window on somebody's behalf) and it is not worth the risk of an invisible app to save one line of code.
3. **Colour where it is read.** The paper bytes go through `WebviewWindowBuilder::background_color`, so both the window
   and WebView2 get them. They are pinned to `--paper` in `src/app.css`, same drift class as the np installer pin.
4. **Dark mode stays a guess, and says so.** Before the first window exists nothing in Jott knows whether the desktop
   wants dark — the scheme is read from the page (`src/lib/darkMode.ts`), which needs the page. A dark-mode user will
   now see light paper for the first frames instead of white: a change, not obviously worse, and nobody here has seen
   either. Not solved with a registry read to win a few milliseconds of pre-paint colour.
5. **No show-after-paint trick this time.** The usual fix for a white first frame is to keep the window hidden until
   JavaScript says it painted. That is precisely the arrangement that turns a broken bundle into an app with no window
   at all, and I would be shipping a timing dependency I cannot observe. If paper does not land, 0.0.20 can add the
   page-driven reveal *with* a Rust watchdog — staged, one change each, so a report tells us which one mattered.
6. **Instrument instead of guessing.** Each launch writes one journal line, `event: "window"`, with the size it came up
   at, the top-left position, and whether a state file was there. Numbers and one word about a file — no paths, no note
   text, and no new keys in `LocalRecord`'s closed set. If this fix does not fix it, their log answers before I ask.

## What could not be tested here, and what was done instead

The operator installed computer use for this session, and it does not close the gap:

- **This model takes no image input.** `screenshot` answers "tool image omitted: model does not support images", and
  both models configured on this box (`~/.pi/agent/models.json`) are text-only Qwen GGUF checkpoints. I can capture a
  window; I cannot look at it.
- **There is no Windows desktop reachable from here.** No `virsh`, `virt-viewer`, `qemu`, `xfreerdp`, `rdesktop`, and
  nothing listening but SSH. The only display is this Linux X session, whose entire window list is one xterm.
- **Jott's GUI cannot run on this box at all.** It needs `libwebkit2gtk-4.0/4.1`; `ldconfig -p` finds neither, and there
  is no root to install it — the same wall that keeps `apps/shell` out of the local workspace.

So the visual claim in 0.0.19 rests on: source evidence for the mechanism, a Windows-target `cargo check` for
compilation, four structural pins, and one log line written at show time. §2b-ix of `docs/testing-on-windows.md` asks a
person to look, and tells them what to send back. Anything stronger would be theatre — the kind of test that passes
because nothing in it can fail.

## Gate record

- `crates/jott-core/tests/shell_window.rs`: four pins, each shown red before the change (config still had a window,
  `main.rs` had no `.visible(false)`, no colour call, no journal line).
- Rust: **313** passing with np off PATH (was 309); clippy `-D warnings` clean.
- Front end: **373** passing, Vitest exit code 0 (untouched by this change; run anyway because CI runs it).
- Windows target: `cargo check --target x86_64-pc-windows-msvc` for `jott-shell`. This gate earned its keep — the first
  run failed on `with_flags`, which does not exist; the plugin's method is `with_state_flags`.
- CI floor raised 300 → 310.

## Upstream, worth filing (not done yet)

`app.windows[].backgroundColor` in `tauri.conf.json` sets the window colour and silently not the webview's, on the one
platform where the webview composites over the window. A documented option that half-works is a trap; Jott now avoids it
by building its own window. Filing at `tauri-apps/tauri` needs the operator's go-ahead — it is somebody else's tracker.
