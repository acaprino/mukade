#![cfg_attr(windows, windows_subsystem = "windows")]
//! The Mukade window: browse every installation's sessions, read them, hop, export, import,
//! resume, and read the chat rooms. The commands' work is in `commands`; here it is only moved off
//! the window's thread (spec §4.2).

mod chat;
mod commands;
mod error;
mod location;
#[cfg(test)]
mod scratch;
mod terminal;
mod view;

use commands::{Busy, Shared};
use error::{CmdError, CmdResult};
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tauri::{Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

type Shared_<'a> = State<'a, Arc<Shared>>;

/// Run `job` on the blocking pool: a library call reads stores and may run a harness.
async fn blocking<T: Send + 'static>(
    job: impl FnOnce() -> CmdResult<T> + Send + 'static,
) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(job)
        .await
        .map_err(|error| CmdError::failed(format!("the job stopped: {error}")))?
}

#[tauri::command]
fn app_info(shared: Shared_<'_>) -> Value {
    json!({
        "name": "mukade",
        "version": env!("CARGO_PKG_VERSION"),
        "turns": shared.args.turns,
        "scan": shared.args.scan,
        "platform": std::env::consts::OS,
    })
}

#[tauri::command]
async fn installations(shared: Shared_<'_>, scan: Vec<String>) -> CmdResult<commands::Inventory> {
    let shared = shared.inner().clone();
    blocking(move || Ok(commands::inventory(&shared, &scan))).await
}

#[tauri::command]
async fn scan(shared: Shared_<'_>, dir: String) -> CmdResult<commands::Inventory> {
    let shared = shared.inner().clone();
    blocking(move || Ok(commands::inventory(&shared, &[dir]))).await
}

#[tauri::command]
async fn sessions(shared: Shared_<'_>, installation: usize) -> CmdResult<Vec<view::SessionRow>> {
    let shared = shared.inner().clone();
    blocking(move || commands::sessions(&shared, installation)).await
}

#[tauri::command]
async fn transcript(
    shared: Shared_<'_>,
    installation: usize,
    id: String,
    turns: usize,
) -> CmdResult<view::Transcript> {
    let shared = shared.inner().clone();
    blocking(move || commands::transcript(&shared, installation, &id, turns)).await
}

#[tauri::command]
async fn raw(
    shared: Shared_<'_>,
    installation: usize,
    id: String,
    entry_id: String,
) -> CmdResult<Vec<Value>> {
    let shared = shared.inner().clone();
    blocking(move || commands::raw(&shared, installation, &id, &entry_id)).await
}

#[tauri::command]
async fn hop_options(
    shared: Shared_<'_>,
    source: usize,
    id: String,
) -> CmdResult<commands::HopOptions> {
    let shared = shared.inner().clone();
    blocking(move || commands::hop_options(&shared, source, &id)).await
}

#[tauri::command]
async fn hop(shared: Shared_<'_>, args: commands::HopArgs) -> CmdResult<view::ReceiptView> {
    let shared = shared.inner().clone();
    blocking(move || {
        let _busy = Busy::new(&shared);
        commands::hop(&shared, &args)
    })
    .await
}

#[tauri::command]
async fn export(
    shared: Shared_<'_>,
    installation: usize,
    id: String,
    to: Option<String>,
) -> CmdResult<commands::Exported> {
    let shared = shared.inner().clone();
    blocking(move || commands::export(&shared, installation, &id, to.as_deref())).await
}

#[tauri::command]
async fn import(
    shared: Shared_<'_>,
    document: String,
    into: usize,
    prompt: Option<String>,
) -> CmdResult<view::ReceiptView> {
    let shared = shared.inner().clone();
    blocking(move || {
        let _busy = Busy::new(&shared);
        commands::import(&shared, &document, into, prompt.as_deref())
    })
    .await
}

#[tauri::command]
async fn resume(
    shared: Shared_<'_>,
    installation: usize,
    id: String,
) -> CmdResult<commands::Resumed> {
    let shared = shared.inner().clone();
    blocking(move || commands::resume(&shared, installation, &id)).await
}

/// A native dialog: `open` a JSON document, `save` one (with `name` proposed), or a `folder`.
/// `None` when the person cancels.
#[tauri::command]
async fn pick(
    app: tauri::AppHandle,
    kind: String,
    name: Option<String>,
) -> CmdResult<Option<String>> {
    blocking(move || {
        let dialog = app.dialog().file();
        let picked = match kind.as_str() {
            "open" => dialog
                .add_filter("Mukade document", &["json"])
                .blocking_pick_file(),
            "save" => dialog
                .add_filter("Mukade document", &["json"])
                .set_file_name(name.unwrap_or_else(|| "session.json".to_string()))
                .blocking_save_file(),
            "folder" => dialog.blocking_pick_folder(),
            other => return Err(CmdError::refused(format!("unknown dialog {other}"))),
        };
        Ok(picked
            .and_then(|path| path.into_path().ok())
            .map(|path| path.display().to_string()))
    })
    .await
}

#[tauri::command]
async fn chat_status() -> chat::ChatStatus {
    chat::status().await
}

#[tauri::command]
async fn chat_rooms(cursor: Option<String>) -> CmdResult<Value> {
    chat::call("rooms", chat::page_args(json!({}), &[("cursor", cursor)])).await
}

#[tauri::command]
async fn chat_members(room_id: String) -> CmdResult<Value> {
    chat::call(
        "members",
        chat::page_args(json!({ "room_id": room_id }), &[]),
    )
    .await
}

#[tauri::command]
async fn chat_ledger(
    room_id: String,
    cursor: Option<String>,
    since: Option<String>,
    text: Option<String>,
) -> CmdResult<Value> {
    chat::call(
        "ledger",
        chat::page_args(
            json!({ "room_id": room_id }),
            &[("cursor", cursor), ("since", since), ("text", text)],
        ),
    )
    .await
}

/// The window asked to close while a hop or import ran, and the person confirmed.
#[tauri::command]
fn quit(app: tauri::AppHandle) {
    app.exit(0);
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let shared = Arc::new(Shared::new(
        commands::resolve_roots(),
        commands::Args::parse(&args),
    ));
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(shared)
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let busy = window.state::<Arc<Shared>>().busy.load(Ordering::SeqCst);
                if busy > 0 {
                    api.prevent_close();
                    let _ = window.emit("close-blocked", busy);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            app_info,
            installations,
            scan,
            sessions,
            transcript,
            raw,
            hop_options,
            hop,
            export,
            import,
            resume,
            pick,
            chat_status,
            chat_rooms,
            chat_members,
            chat_ledger,
            quit,
        ])
        .run(tauri::generate_context!())
        .expect("the Mukade window failed to start");
}
