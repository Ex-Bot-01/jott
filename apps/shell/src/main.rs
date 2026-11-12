#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ipc;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(ipc::App::startup())
        .invoke_handler(tauri::generate_handler![
            ipc::registry_info,
            ipc::np_status,
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
            ipc::flush_on_close
        ])
        .setup(|app| {
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
