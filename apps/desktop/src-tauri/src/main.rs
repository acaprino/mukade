#![cfg_attr(windows, windows_subsystem = "windows")]

mod commands;
mod error;
mod locations;
mod sessions;
mod state;

#[cfg(test)]
mod scratch;
#[cfg(test)]
mod testing;

fn main() {
    let state = state::AppState::new(locations::standard_roots());
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::startup,
            commands::installations,
            commands::sessions,
            commands::transcript,
            commands::raw,
        ])
        .run(tauri::generate_context!())
        .expect("the Mukade window failed to start");
}
