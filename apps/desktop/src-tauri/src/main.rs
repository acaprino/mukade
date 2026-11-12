#![cfg_attr(windows, windows_subsystem = "windows")]
//! The Mukade window: browse every installation's sessions, read them, hop, export, import,
//! resume, and read the chat rooms. The commands' work is in `sessions`, `actions` and `chat`;
//! `commands` only moves it off the window's thread (spec §4.2).

mod actions;
mod chat;
mod commands;
mod error;
mod locations;
mod sessions;
mod state;
mod terminal;
mod view;

#[cfg(test)]
mod scratch;
#[cfg(test)]
mod testing;

use tauri::{Emitter, Manager};

fn main() {
    let state = state::AppState::new(locations::standard_roots());
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(state)
        .on_window_event(|window, event| {
            // The window refuses to close while a hop or import writes; the page asks first and
            // calls `quit` when the person confirms.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let running = window.state::<state::AppState>().writes_running();
                if running > 0 {
                    api.prevent_close();
                    let _ = window.emit("close-blocked", running);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::startup,
            commands::installations,
            commands::sessions,
            commands::transcript,
            commands::raw,
            commands::hop_options,
            commands::hop,
            commands::export,
            commands::import,
            commands::resume,
            commands::pick,
            commands::chat_status,
            commands::chat_rooms,
            commands::chat_members,
            commands::chat_ledger,
            commands::quit,
        ])
        .run(tauri::generate_context!())
        .expect("the Mukade window failed to start");
}
