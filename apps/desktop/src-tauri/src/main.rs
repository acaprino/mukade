#![cfg_attr(windows, windows_subsystem = "windows")]

#[tauri::command]
fn app_info() -> serde_json::Value {
    serde_json::json!({ "name": "mukade", "version": env!("CARGO_PKG_VERSION") })
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![app_info])
        .run(tauri::generate_context!())
        .expect("the Mukade window failed to start");
}
