//! Mukade owns one window. Imago supplies its frame, glass, geometry and recovery policy.
use imago_shell::{
    ProcessFailure, Recovery, RecoveryAction, Rect, Screen, SnapLayouts, WindowState, apply_theme,
    clamp_to_screens, disable_browser_accelerators, drawn_by_page, glass_window_config,
    guard_navigation_with_scheme, install_reload_menu, install_snap_layouts, load_state,
    parse_theme, recovery_action, save_state, save_theme, saved_theme, state_to_save,
    trim_context_menu, watch_process_failures,
};
use std::sync::{
    Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::time::{Duration, Instant};
use tauri::{
    AppHandle, Manager, PhysicalPosition, PhysicalSize, RunEvent, State, WebviewWindow,
    WebviewWindowBuilder, WindowEvent,
};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};

#[derive(Default)]
pub struct Desktop {
    overlay: Mutex<Option<SnapLayouts>>,
    normal: Mutex<Option<WindowState>>,
    recovery: Mutex<Recovery>,
    generation: AtomicU64,
    rebuilding: AtomicBool,
    prompting: AtomicBool,
}

#[tauri::command]
pub fn set_window_theme(
    window: WebviewWindow,
    theme: String,
    look: Option<String>,
    resolved: Option<String>,
) -> Result<(), String> {
    let theme = parse_theme(&theme).ok_or("Unknown window theme")?;
    let (look, resolved) = drawn_by_page(theme, look.as_deref(), resolved.as_deref());
    apply_theme(&window, theme, look, resolved).map_err(|e| e.to_string())?;
    let dir = window
        .app_handle()
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?;
    save_theme(&dir, theme).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn window_caption_maximize(
    desktop: State<'_, Desktop>,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> Result<(), String> {
    let overlay = desktop.overlay.lock().map_err(|e| e.to_string())?;
    overlay
        .as_ref()
        .ok_or("The caption is still being created")?
        .set_rect(x, y, width, height);
    Ok(())
}

fn geometry(window: &WebviewWindow) -> Option<WindowState> {
    let at = window.outer_position().ok()?;
    let size = window.inner_size().ok()?;
    Some(WindowState {
        x: at.x,
        y: at.y,
        width: size.width,
        height: size.height,
        maximized: false,
    })
}

fn remember(window: &WebviewWindow) {
    let app = window.app_handle();
    let desktop = app.state::<Desktop>();
    let normal = *desktop.normal.lock().unwrap();
    let state = state_to_save(
        window.is_maximized().unwrap_or(false),
        window.is_minimized().unwrap_or(false),
        normal,
        || geometry(window),
    );
    if let (Some(state), Ok(dir)) = (state, app.path().app_data_dir()) {
        if let Err(error) = save_state(&dir, &state) {
            eprintln!("Saving the window position: {error}");
        }
    }
}

fn restore(
    window: &WebviewWindow,
    saved: WindowState,
    min_width: f64,
    min_height: f64,
) -> tauri::Result<()> {
    let mut monitors = window.available_monitors()?;
    if let Some(primary) = window.primary_monitor()? {
        monitors.sort_by_key(|monitor| monitor.position() != primary.position());
    }
    let screens: Vec<_> = monitors
        .iter()
        .map(|monitor| {
            let at = monitor.position();
            let size = monitor.size();
            let area = monitor.work_area();
            Screen {
                full: Rect {
                    x: at.x,
                    y: at.y,
                    width: size.width,
                    height: size.height,
                },
                work: Rect {
                    x: area.position.x,
                    y: area.position.y,
                    width: area.size.width,
                    height: area.size.height,
                },
            }
        })
        .collect();
    let target = monitors
        .iter()
        .find(|m| {
            let (at, size) = (m.position(), m.size());
            (0..i64::from(size.width)).contains(&(i64::from(saved.x) - i64::from(at.x)))
                && (0..i64::from(size.height)).contains(&(i64::from(saved.y) - i64::from(at.y)))
        })
        .or(monitors.first());
    let scale = target.map_or(1.0, |m| m.scale_factor());
    let state = clamp_to_screens(
        saved,
        &screens,
        (0, 0),
        (min_width * scale).round() as u32,
        (min_height * scale).round() as u32,
    );
    window.set_position(PhysicalPosition::new(state.x, state.y))?;
    window.set_size(PhysicalSize::new(state.width, state.height))?;
    *window.state::<Desktop>().normal.lock().unwrap() = Some(WindowState {
        maximized: false,
        ..state
    });
    if state.maximized {
        window.maximize()?;
    }
    Ok(())
}

fn failed(window: &WebviewWindow, failure: ProcessFailure, generation: u64) {
    let app = window.app_handle();
    let shell = app.state::<Desktop>();
    if shell.generation.load(Ordering::SeqCst) != generation {
        return;
    }
    let action = recovery_action(
        failure,
        &mut shell.recovery.lock().unwrap(),
        Instant::now(),
        shell.rebuilding.load(Ordering::SeqCst),
    );
    match action {
        RecoveryAction::Reload => {
            std::thread::sleep(failure.reload_delay());
            if shell.generation.load(Ordering::SeqCst) == generation {
                let _ = window.reload();
            }
        }
        RecoveryAction::GiveUp => {
            if shell.prompting.swap(true, Ordering::SeqCst) {
                return;
            }
            if app
                .dialog()
                .message("The Mukade interface stopped responding. Reload it?")
                .buttons(MessageDialogButtons::YesNo)
                .blocking_show()
                && shell.generation.load(Ordering::SeqCst) == generation
            {
                shell.recovery.lock().unwrap().reset();
                let _ = window.reload();
            }
            shell.prompting.store(false, Ordering::SeqCst);
        }
        RecoveryAction::Rebuild => {
            if shell.rebuilding.swap(true, Ordering::SeqCst) {
                return;
            }
            // A delayed report for a previous window must never destroy its replacement.
            if shell.generation.load(Ordering::SeqCst) != generation {
                shell.rebuilding.store(false, Ordering::SeqCst);
                return;
            }
            shell.generation.fetch_add(1, Ordering::SeqCst);
            remember(window);
            let result = window.destroy().and_then(|_| {
                let deadline = Instant::now() + Duration::from_secs(5);
                while app.get_webview_window("main").is_some() {
                    if Instant::now() >= deadline {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::TimedOut,
                            "The old Mukade window is still registered",
                        )
                        .into());
                    }
                    std::thread::sleep(Duration::from_millis(25));
                }
                build_window(app)
            });
            complete_rebuild(&shell.rebuilding, result, |error| {
                // This callback runs off the UI thread. Keep the event loop and any native
                // writer alive even if the last window was destroyed during rollback.
                app.state::<crate::state::AppState>()
                    .drain_writes_for_exit();
                app.dialog()
                    .message(format!(
                        "Could not restore the Mukade window: {error}. Mukade will close."
                    ))
                    .blocking_show();
                app.exit(1);
            });
        }
        RecoveryAction::Ignore => {}
    }
}

// Keep suppressing the runtime's automatic last-window exit throughout the fatal path.
fn complete_rebuild(
    rebuilding: &AtomicBool,
    result: tauri::Result<()>,
    exit: impl FnOnce(tauri::Error),
) {
    match result {
        Ok(()) => rebuilding.store(false, Ordering::SeqCst),
        Err(error) => exit(error),
    }
}

// Once build() registers a hidden window, every failure up to and including show() must
// either remove it or report why rollback also failed.
fn finish_registered_window(
    result: tauri::Result<()>,
    rollback: impl FnOnce() -> tauri::Result<()>,
) -> tauri::Result<()> {
    if let Err(error) = result {
        if let Err(cleanup) = rollback() {
            return Err(std::io::Error::other(format!(
                "{error}; removing the incomplete window also failed: {cleanup}"
            ))
            .into());
        }
        return Err(error);
    }
    Ok(())
}

pub fn build_window(app: &AppHandle) -> tauri::Result<()> {
    let dir = app.path().app_data_dir()?;
    let base = app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == "main")
        .cloned()
        .ok_or(tauri::Error::WindowNotFound)?;
    let mut config = glass_window_config(base, saved_theme(&dir));
    config.visible = false;
    let dev = app
        .config()
        .build
        .dev_url
        .as_ref()
        .filter(|_| tauri::is_dev())
        .map(|url| url.origin().ascii_serialization());
    let origins: Vec<_> = dev.as_deref().into_iter().collect();
    let window = guard_navigation_with_scheme(
        WebviewWindowBuilder::from_config(app, &config)?,
        &origins,
        config.use_https_scheme,
    )
    .build()?;
    let shell = app.state::<Desktop>();
    let generation = shell.generation.fetch_add(1, Ordering::SeqCst) + 1;
    let result = configure_and_show(
        &window,
        load_state(&dir),
        (
            config.min_width.unwrap_or(500.0),
            config.min_height.unwrap_or(0.0),
        ),
        generation,
    );
    finish_registered_window(result, || {
        shell.generation.fetch_add(1, Ordering::SeqCst);
        *shell.overlay.lock().unwrap() = None;
        window.destroy()
    })
}

fn configure_and_show(
    window: &WebviewWindow,
    saved: Option<WindowState>,
    minimum: (f64, f64),
    generation: u64,
) -> tauri::Result<()> {
    if let Some(state) = saved {
        restore(window, state, minimum.0, minimum.1)?;
    }
    if !tauri::is_dev() {
        disable_browser_accelerators(window)?;
        trim_context_menu(window)?;
    }
    *window.state::<Desktop>().overlay.lock().unwrap() = Some(install_snap_layouts(window)?);
    let watched = window.clone();
    watch_process_failures(window, move |failure| failed(&watched, failure, generation))?;
    let reloaded = window.clone();
    install_reload_menu(window, "Reload interface", move || {
        if reloaded.state::<crate::state::AppState>().writes_running() == 0 {
            let _ = reloaded.reload();
        }
    })?;
    let moved = window.clone();
    window.on_window_event(move |event| {
        let shell = moved.state::<Desktop>();
        if shell.rebuilding.load(Ordering::SeqCst) {
            return;
        }
        match event {
            WindowEvent::Moved(_) | WindowEvent::Resized(_)
                if !moved.is_maximized().unwrap_or(true)
                    && !moved.is_minimized().unwrap_or(true) =>
            {
                if let Some(state) = geometry(&moved) {
                    *shell.normal.lock().unwrap() = Some(state);
                }
            }
            WindowEvent::CloseRequested { .. } => remember(&moved),
            _ => {}
        }
    });
    window.show()
}

pub fn on_run_event(app: &AppHandle, event: RunEvent) {
    if let RunEvent::ExitRequested {
        api, code: None, ..
    } = event
    {
        if app.state::<Desktop>().rebuilding.load(Ordering::SeqCst) {
            api.prevent_exit();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn failed_setup_removes_the_registered_window_and_exits() {
        let registered = Cell::new(true);
        let rebuilding = AtomicBool::new(true);
        let exited = Cell::new(false);
        let result = finish_registered_window(
            Err(std::io::Error::other("injected show failure").into()),
            || {
                registered.set(false);
                Ok(())
            },
        );
        complete_rebuild(&rebuilding, result, |error| {
            assert!(error.to_string().contains("injected show failure"));
            assert!(!registered.get());
            assert!(rebuilding.load(Ordering::SeqCst));
            exited.set(true);
        });
        assert!(exited.get());
    }

    #[test]
    fn a_failed_destroy_cannot_keep_an_incomplete_window_alive() {
        let rebuilding = AtomicBool::new(true);
        let exited = Cell::new(false);
        let result = finish_registered_window(
            Err(std::io::Error::other("injected setup failure").into()),
            || Err(std::io::Error::other("injected destroy failure").into()),
        );
        complete_rebuild(&rebuilding, result, |error| {
            let message = error.to_string();
            assert!(message.contains("injected setup failure"));
            assert!(message.contains("injected destroy failure"));
            assert!(rebuilding.load(Ordering::SeqCst));
            exited.set(true);
        });
        assert!(exited.get());
    }

    #[test]
    fn a_fully_configured_and_shown_window_completes_recovery() {
        let rebuilding = AtomicBool::new(true);
        let result = finish_registered_window(Ok(()), || panic!("destroyed a usable window"));
        complete_rebuild(&rebuilding, result, |_| {
            panic!("exited with a usable window")
        });
        assert!(!rebuilding.load(Ordering::SeqCst));
    }
}
