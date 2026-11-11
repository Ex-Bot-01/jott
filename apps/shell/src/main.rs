#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Task 10 registers the vault commands here. Until then this is a window with no
    // commands, which is enough for the spike hand-off: does a window appear on
    // Windows, and does an event from Rust reach the webview promptly?
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running Jott");
}
