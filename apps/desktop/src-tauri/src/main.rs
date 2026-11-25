#![cfg_attr(windows, windows_subsystem = "windows")]
//! The Mukade window: browse every installation's sessions, read them, hop, export, import,
//! resume, and read the chat rooms. The commands' work is in `sessions`, `actions` and `chat`;
//! `commands` only moves it off the window's thread (spec §4.2).

mod actions;
mod chat;
mod commands;
mod error;
mod harnesses;
mod locations;
mod sandbox;
mod sessions;
mod state;
mod terminal;
mod view;

#[cfg(test)]
mod scratch;
#[cfg(test)]
mod testing;

use imago_shell::{DARK_GROUND, LIGHT_GROUND, Material, Options, on_run_event, open_main, plugin};
use tauri::{AppHandle, Emitter, Manager, RunEvent};

/// Whether a reload of the page may go ahead. A reload the person asked for (the item of the window menu) waits while a hop or an
/// import writes, so that the page that comes back finds the write in progress and not a half of it; a reload after the page was
/// lost has nothing to wait for: the write belongs to Rust, and the page asks for it again (`write_status`) when it is back.
fn reload_allowed(writes_running: usize, asked_by_the_person: bool) -> bool {
    !asked_by_the_person || writes_running == 0
}

/// Whether the run loop must let the writes finish before the program ends. `app.exit(1)` is the shell's way out when the window could
/// not be built again; a native writer must not be cut mid-write then. The person's own quit (`commands::quit`, code 0) is theirs: the
/// page has asked them first.
fn drains_writes(code: Option<i32>) -> bool {
    code == Some(1)
}

/// The run loop: the shell's recovery of the window, and the writes kept whole when it fails for good.
fn run_event(app: &AppHandle, event: RunEvent) {
    if let RunEvent::ExitRequested { code, .. } = &event
        && drains_writes(*code)
    {
        app.state::<state::AppState>().drain_writes_for_exit();
    }
    on_run_event(app, event);
}

fn main() {
    let state = state::AppState::new(locations::standard_roots());
    tauri::Builder::default()
        .plugin(plugin())
        .plugin(tauri_plugin_dialog::init())
        .manage(state)
        .setup(|app| {
            // The window is the shell's: hidden until the page has drawn, where it was last time, in the theme and the glass it had. Glass
            // is the person's choice (Settings, or the palette): off at the first start.
            let handle = app.handle().clone();
            let options = Options {
                material: Material::Opaque {
                    dark: DARK_GROUND,
                    light: LIGHT_GROUND,
                },
                reload_label: "Reload interface".into(),
                stopped_message: "The Mukade window stopped responding. Reload it?".into(),
                lost_message: "The Mukade window could not be restored. Mukade will close.".into(),
                before_reload: Box::new(move |failure| {
                    reload_allowed(
                        handle.state::<state::AppState>().writes_running(),
                        failure.is_none(),
                    )
                }),
                ..Options::for_app(app)?
            };
            open_main(app.handle(), options)?;
            Ok(())
        })
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
            commands::session_stamp,
            commands::raw,
            commands::write_status,
            commands::acknowledge_write,
            commands::hop_options,
            commands::hop,
            commands::export,
            commands::import,
            commands::resume,
            commands::work,
            commands::handoff_options,
            commands::handoff,
            commands::pick,
            commands::chat_status,
            commands::chat_rooms,
            commands::chat_members,
            commands::chat_ledger,
            commands::chat_write,
            commands::chat_start,
            commands::chat_stop,
            commands::chat_launch,
            commands::harness_inventory,
            commands::system_roots,
            commands::scan_folder,
            commands::install_system,
            commands::sandboxes,
            commands::sandbox_install,
            commands::sandbox_forget,
            commands::sandbox_restore,
            commands::sandbox_run,
            commands::quit,
        ])
        .build(tauri::generate_context!())
        .expect("the Mukade window failed to start")
        // Without the shell's handler a window that is built again, after its browser process was lost, ends the app.
        .run(run_event);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reload_the_person_asked_for_waits_for_a_write() {
        assert!(reload_allowed(0, true));
        assert!(!reload_allowed(1, true));
    }

    #[test]
    fn a_reload_after_the_page_was_lost_does_not_wait_for_a_write() {
        assert!(reload_allowed(1, false));
        assert!(reload_allowed(0, false));
    }

    #[test]
    fn only_the_shells_failed_exit_waits_for_the_writes() {
        assert!(drains_writes(Some(1)));
        assert!(!drains_writes(Some(0)), "the person confirmed the quit");
        assert!(
            !drains_writes(None),
            "the last window closing: the close was refused while a write ran"
        );
    }
}
