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

**The white — still open, and this paragraph has been wrong once.**

> *What I first wrote:* our config colour `[250, 250, 249, 255]` reached the **window**
> (`tauri-runtime-wry-2.12.3/src/lib.rs:889`) but not the webview, whose colour comes only from
> `webview_attributes.background_color` (same file, 4779) — the attribute wry hands WebView2 as
> `SetDefaultBackgroundColor` (`wry-0.57.2/src/webview2/mod.rs:415`) — so the covering surface kept WebView2's white.
>
> *Why that is false:* I looked in `tauri`, then in `tauri-runtime-wry`, and never in **`tauri-runtime`**, which sits
> between them. Its `src/webview.rs:455` holds `impl From<&WindowConfig> for WebviewAttributes`, and at `:498-500`:
> `if let Some(color) = config.background_color { builder = builder.background_color(color); }`. Tauri builds the
> attributes exactly that way (`tauri-2.12.5/src/webview/mod.rs:436`), so the attribute at runtime-wry:4779 *was* filled
> from config. `git show v0.0.18:apps/shell/tauri.conf.json` shows the field spelled correctly, and `ac120d2` puts it in
> since **0.0.3** (not 0.0.9, as I also wrote). Both layers got the colour; this release therefore changes nothing about
> the first frame's colour, and **the white remains unexplained.**
>
> *Ruled out afterwards:* a GDI white erase — tao paints its own background in `WM_ERASEBKGND`
> (`tao-0.37.2/src/platform_impl/windows/event_loop.rs:1042-1050`) with no class brush (`:641`), and wry's container HWND
> class is likewise NULL-brushed (`wry webview2/mod.rs:222-235`). *Still standing:* WebView2 runtimes lacking
> `ICoreWebView2ControllerOptions3`, where the colour lands later (`wry webview2/mod.rs:409-431`, `:464-468`), and
> WebView2's own surface before its first frame.

The mistake is worth keeping in this file rather than editing out, because it is the same shape as the caret one from
last release: a confident mechanism built on a search that stopped early. "Nothing reads this field" requires searching
every crate in the graph — `tauri`, `tauri-runtime`, `tauri-runtime-wry`, `wry`, `tao` — or it stays a question.

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
3. **The colour stays set, and no longer claims anything.** The paper bytes still go through
   `WebviewWindowBuilder::background_color` — with no config window that is simply the only place left to set them — and
   they remain pinned to `--paper` in `src/app.css`, same drift class as the np installer pin. Removed is the story: by
   the correction above this release makes no first-frame colour change, so nothing in it may be presented as one.
4. **Dark mode stays a guess, and says so.** Before the first window exists nothing in Jott knows whether the desktop
   wants dark — the scheme is read from the page (`src/lib/darkMode.ts`), which needs the page. A dark-mode desktop gets
   light paper for those first frames, exactly as it did before this release: unchanged behaviour, written down because
   "the window was pale" will otherwise be reported against it either way. Not solved with a registry read to win a few
   milliseconds of pre-paint colour.
5. **No show-after-paint trick this time.** The usual fix for a white first frame is to keep the window hidden until
   JavaScript says it painted. That is precisely the arrangement that turns a broken bundle into an app with no window
   at all, and I would be shipping a timing dependency I cannot observe. Since the colour claim fell, this is also no
   longer a "wait and see if it lands" decision: 0.0.20 adds the page-driven reveal *with* a Rust watchdog only if the
   log and the new console say the pale frame is still there — one change at a time, so a report says which one mattered.
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

7. **The jump is reported as conditional, not universal.** The plugin restores only when the label is in its state file
   (`tauri-plugin-window-state-2.5.0/src/lib.rs:198-244`); with no `.window-state.json` it takes the branch at `:245` and
   never calls `set_position`. So this fix covers launches *with* a saved state — which includes an upgrade install,
   since `%APPDATA%\dev.jott.desktop\.window-state.json` survives one. A jump on a genuinely first run is a different
   bug, with post-`show()` DPI handling (`tao-0.37.2/.../event_loop.rs:1837+`) the live candidate. My first draft of the
   release notes asserted the jump explained the reported launch; it cannot have, if that launch was truly the first.

## Gate record

- `crates/jott-core/tests/shell_window.rs`: **six** pins, each shown red before the change (config still had a window,
  `main.rs` had no `.visible(false)`, no colour call, no journal line).
- After review, each hardened pin was shown red **again** with its regression put back by hand: visibility handed back to
  the plugin → red; `show()` wrapped in `if true { … }` → red; a second `.visible(true)` added to the chain → red;
  `features = ["devtools"]` dropped from the manifest → red. Then restored and green again, six of six. A count is not a
  guarantee that a test can fail, which is the sentence this project keeps having to relearn.
- Rust: **315** passing with np off PATH (was 309); clippy `-D warnings` clean.
- Front end: **373** passing, Vitest exit code 0 (untouched by this change; run anyway because CI runs it).
- Windows target: `cargo check --target x86_64-pc-windows-msvc` for `jott-shell`. This gate earned its keep — the first
  run failed on `with_flags`, which does not exist; the plugin's method is `with_state_flags`.
- CI floor raised 300 → 310.

## Upstream — withdrawn

This section proposed filing at `tauri-apps/tauri` that `app.windows[].backgroundColor` "silently misses the webview".
**Do not file it: it is false**, for the reason in the correction above. An upstream report written from a shallow grep
would cost somebody else an afternoon and teach nothing; if the white is ever traced to WebView2 itself, that report gets
written from a measurement instead.

## Review outcome (v0.0.18 → HEAD, before this shipped)

**BLOCK**, on the claims rather than the code — the right thing to block on when the code cannot be watched running:

| Finding | What happened here |
| --- | --- |
| B1 the colour claim is false against the pinned crates | Accepted after re-deriving it personally: `tauri-runtime-2.12.2/src/webview.rs:498`. Four artefacts corrected (code comment, test module doc, release notes, this plan); upstream item withdrawn. |
| P1-1 a jump cannot happen on a first run, yet the notes said it did | Accepted; decision 7 added, §2b-ix rewritten, "both were ours" deleted. |
| P1-2 two of five pins stay green with the regression restored | Accepted — and correct: my flag pin matched a substring and my ordering pin compared two byte offsets. Both hardened, plus a sixth pin for the `main` label and the window sizes. |
| P2 the state-file name is hardcoded in the log line | Fixed: `tauri_plugin_window_state::DEFAULT_FILENAME`. |
| P2 "geometry unreadable at show time" invents a cause | Fixed: tao *panics* rather than returning `Err` on Windows (`windows/window.rs:177-181`), so that arm carries no story at all. |
| P2 nothing pins the `"main"` label or 1200×760 / 720×480 | Fixed by the sixth pin, against `capabilities/default.json` and `v0.0.18`'s own config. |
| P2 §2b-ix promised "nothing moves after it appears" | DPI exception added as item 4. |
| P2 "from a fresh state" named no file | The guide now names `%APPDATA%\dev.jott.desktop\.window-state.json`, and says it is not under `%APPDATA%\jott`. |

The reviewer also re-derived C1 and C3 independently, including the assumption I had never tested: `on_window_ready` runs
synchronously inside `build()` on the main thread (`manager/window.rs:118-129` → `run_on_main_thread` →
`Context::send_user_message` executing inline at `runtime-wry lib.rs:263-273`), so `.visible(false)` really does hide the
move; and geometry still gets saved despite `.run(|_app, _event| {})`, because tauri forwards `RunEvent`s to plugins
itself (`app.rs:2811-2815`) and the plugin saves on `RunEvent::Exit`.
