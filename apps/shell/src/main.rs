#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ipc;

use std::path::PathBuf;
use tauri::window::Color;
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_window_state::StateFlags;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        // Only for one line: a model finished downloading while the window was not focused. The window's
        // own banner says the same thing, so a refusal or an absent notifier changes nothing (§4).
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        // Restores size/position and saves them on exit, in its own state file. Kept out of the registry:
        // this is window decoration, not a fact about somebody's notes.
        //
        // Every flag but `VISIBLE`. The plugin restores geometry from its `on_window_ready` hook, which runs while
        // our window is still hidden below — that is the whole point, and it is why the launch no longer visibly
        // jumps. With `VISIBLE` in the set the plugin also decides *whether to show the window*, from what a file
        // says about a previous run; an MSI upgrade that kills Jott mid-run can leave that file saying the window
        // was hidden, and a window nobody can see cannot be closed, diagnosed, or sent a log from. Visibility
        // stays ours, decided once, in `setup`, without consulting the past.
        .plugin(
            tauri_plugin_window_state::Builder::new()
                .with_state_flags(StateFlags::all().difference(StateFlags::VISIBLE))
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            ipc::registry_info,
            ipc::np_status,
            ipc::app_info,
            ipc::list_vault,
            ipc::open_vault,
            ipc::read_note,
            ipc::save_note,
            ipc::create_note,
            ipc::create_folder,
            ipc::rename_entry,
            ipc::move_entry,
            ipc::delete_entry,
            ipc::mirror_buffer,
            ipc::discard_mirror,
            ipc::search_vault,
            ipc::ingest_all,
            ipc::set_consent,
            ipc::embeddings_offer,
            ipc::embeddings_help,
            ipc::embeddings_status,
            ipc::diagnostics,
            ipc::flush_on_close
        ])
        .setup(|app| {
            // Where the installer put np: `$RESOURCES/np`, beside the other files we ship. A source build
            // that never ran `scripts/fetch-np` has no such folder, and Jott falls back to PATH and says so.
            let candidate = app
                .path()
                .resource_dir()
                .map(|dir| dir.join("np"))
                .unwrap_or_default();
            // An empty path means "no bundled np", and resolution falls back to PATH. Checking `is_dir`
            // here rather than passing the hope along keeps `resolve_np`'s error message honest about where
            // it looked.
            let np_bundle = if candidate.is_dir() { candidate } else { PathBuf::new() };
            app.manage(ipc::App::startup(np_bundle));

            // The window is built here rather than named in `tauri.conf.json` for one reason, measured in the crates
            // we build against and pinned by `crates/jott-core/tests/shell_window.rs`.
            //
            // Tauri creates a config window (`tauri-2.12.5/src/app.rs:2691`) and shows it — `visible` defaults to true
            // (`tauri-utils:2072`, applied at `tauri-runtime-wry:857`, made visible in tao's `init`) — before our
            // setup closure runs. The window-state plugin restores the saved position inside that same creation:
            // `on_window_ready` (plugin `lib.rs:433`) → `restore_state` → plain `set_position`/`set_size` (`:213`,
            // `:230`). So a launch with a saved state showed the window where Windows put it and then moved it.
            // Built hidden, restored during `build()`, shown once below: nobody watches the move.
            //
            // The precondition matters, because the report did not say it: with no `.window-state.json` there is
            // nothing to restore (`restore_state` takes its else-branch at `:245` and never calls `set_position`), so
            // a jump on somebody's genuinely first launch is a different bug — DPI renegotiation moves windows after
            // `show()` too (`tao-0.37.2/.../event_loop.rs:1837+`). The `event: "window"` line says which case it was.
            //
            // The colour below is not a second fix, and said so here after a review caught me claiming otherwise: a
            // config colour reaches both layers as well (`tauri-runtime-2.12.2/src/webview.rs:498` fills the very
            // `webview_attributes.background_color` that `tauri-runtime-wry:4779` hands to wry), so moving it from
            // config to code changes nothing about the white first frame. It is here because, with no config window,
            // this is now the only place it can be set. The white stays unexplained.
            let window = WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                .title("Jott")
                .inner_size(1200.0, 760.0)
                .min_inner_size(720.0, 480.0)
                // The light paper of `src/app.css` and `index.html`, pinned to it by a test. A dark-mode desktop
                // still sees this light paper before the page paints: nothing in Jott knows the scheme before there
                // is a page to ask, and no machine that runs our tests has a display.
                .background_color(Color(250, 250, 249, 255))
                // Redundant with tauri's `devtools` feature (`tauri-runtime-wry:5146` forwards
                // `devtools.unwrap_or(true)`), kept because the line says out loud what this build is for.
                .devtools(true)
                .visible(false)
                .build()?;

            // Shown unconditionally, in the same breath as it is built: no timer, no dependence on the front end
            // running, nothing that can leave Jott running with no window on screen.
            let _ = window.show();
            let _ = window.set_focus();
            ipc::note_window(&app.handle(), &window);

            // Ask np once, in the background, and tell the window if it is not there (§1, §2). The webview
            // may not have listeners yet — which is why `np_status` answers the same question on demand.
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move { ipc::announce_np(&handle).await });

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building Jott")
        .run(|_app, _event| {});
}
