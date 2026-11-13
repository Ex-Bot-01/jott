#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ipc;

use std::path::PathBuf;
use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
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
