//! The commands the interface calls. Each is a thin wrapper: the work is a plain function in a
//! module of its own (`sessions`, `actions`, `chat`), run off the UI thread, so the unit tests
//! need no window.

use crate::actions::{self, Exported, HopArgs, HopOptions, Resumed};
use crate::chat;
use crate::error::{CmdError, CmdResult};
use crate::locations::{self, InstallationView, ScanNote};
use crate::sessions::{self, RawPage, SessionRow, TranscriptView};
use crate::state::AppState;
use crate::view::ReceiptView;
use serde::Serialize;
use serde_json::{Value, json};
use std::path::PathBuf;
use tauri::State;
use tauri_plugin_dialog::DialogExt;

/// Run a blocking library call off the UI thread.
pub async fn blocking<T, F>(work: F) -> CmdResult<T>
where
    T: Send + 'static,
    F: FnOnce() -> CmdResult<T> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|error| CmdError::failed(format!("the worker stopped: {error}")))?
}

/// What the command line asked for: `--turns N` and each `--scan <dir>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Startup {
    pub turns: usize,
    pub scan: Vec<String>,
    pub version: &'static str,
}

/// The user turns a transcript opens with.
pub const DEFAULT_TURNS: usize = 50;

pub fn parse_args(args: &[String]) -> Startup {
    let mut turns = DEFAULT_TURNS;
    let mut scan = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if arg == "--turns" {
            if let Some(value) = args.get(index + 1) {
                turns = value.parse().unwrap_or(DEFAULT_TURNS);
                index += 1;
            }
        } else if let Some(value) = arg.strip_prefix("--turns=") {
            turns = value.parse().unwrap_or(DEFAULT_TURNS);
        } else if arg == "--scan" {
            if let Some(folder) = args.get(index + 1) {
                scan.push(folder.clone());
                index += 1;
            }
        } else if let Some(folder) = arg.strip_prefix("--scan=") {
            scan.push(folder.to_string());
        }
        index += 1;
    }
    Startup {
        turns,
        scan,
        version: env!("CARGO_PKG_VERSION"),
    }
}

#[tauri::command]
pub fn startup() -> Startup {
    parse_args(&std::env::args().collect::<Vec<_>>())
}

#[derive(Debug, Clone, Serialize)]
pub struct InstallationList {
    pub installations: Vec<InstallationView>,
    pub scans: Vec<ScanNote>,
}

/// The standard store of each harness, then every installation the folders in `scan` hold, each
/// listed once (its count, or why its store could not be read). One listing at a time: a reload
/// during a scan waits for it instead of interleaving.
pub fn installations_for(state: &AppState, scan: &[String]) -> InstallationList {
    let _one_at_a_time = state.begin_inventory();
    let mut found = locations::standard_locations(state.standard());
    let folders: Vec<PathBuf> = scan.iter().map(PathBuf::from).collect();
    let scans = locations::scan_folders(&mut found, &folders, state.standard());
    let installations = found
        .iter()
        .map(|location| {
            let mut view = InstallationView::from(location);
            match sessions::list_info(location) {
                Ok(listed) => {
                    view.count = listed.len();
                    state.set_listed(&location.key(), listed);
                }
                Err(error) => {
                    view.error = Some(error.message);
                    state.set_listed(&location.key(), Vec::new());
                }
            }
            view
        })
        .collect();
    state.set_locations(found);
    InstallationList {
        installations,
        scans,
    }
}

#[tauri::command]
pub async fn installations(
    state: State<'_, AppState>,
    scan: Vec<String>,
) -> CmdResult<InstallationList> {
    let state = state.inner().clone();
    blocking(move || Ok(installations_for(&state, &scan))).await
}

#[tauri::command]
pub async fn sessions(
    state: State<'_, AppState>,
    installation: String,
) -> CmdResult<Vec<SessionRow>> {
    let location = state.location(&installation)?;
    let state = state.inner().clone();
    blocking(move || {
        // The listing `installations` read, else the store's (and kept for the actions).
        let listed = match state.listed(&installation) {
            Some(listed) => listed,
            None => {
                let listed = sessions::list_info(&location)?;
                state.set_listed(&installation, listed.clone());
                listed
            }
        };
        Ok(listed.into_iter().map(SessionRow::from).collect())
    })
    .await
}

#[tauri::command]
pub async fn transcript(
    state: State<'_, AppState>,
    installation: String,
    id: String,
    turns: usize,
) -> CmdResult<TranscriptView> {
    let location = state.location(&installation)?;
    let state = state.inner().clone();
    blocking(move || {
        // A change of turn limit draws the open session again without reading it again.
        let session = match state.cached_session(&installation, &id) {
            Some(session) => session,
            None => state.cache_session(&installation, &id, sessions::read(&location, &id)?),
        };
        Ok(sessions::transcript(&session, turns))
    })
    .await
}

#[tauri::command]
pub async fn raw(
    state: State<'_, AppState>,
    installation: String,
    id: String,
    offset: usize,
    limit: usize,
) -> CmdResult<RawPage> {
    let location = state.location(&installation)?;
    let state = state.inner().clone();
    blocking(move || {
        let session = match state.cached_session(&installation, &id) {
            Some(session) => session,
            None => {
                let read = sessions::read(&location, &id)?;
                state.cache_session(&installation, &id, read)
            }
        };
        Ok(sessions::raw_page(&session, offset, limit.clamp(1, 200)))
    })
    .await
}

#[tauri::command]
pub async fn hop_options(
    state: State<'_, AppState>,
    source: String,
    id: String,
) -> CmdResult<HopOptions> {
    let state = state.inner().clone();
    blocking(move || actions::hop_options(&state, &source, &id)).await
}

#[tauri::command]
pub async fn hop(state: State<'_, AppState>, args: HopArgs) -> CmdResult<ReceiptView> {
    let state = state.inner().clone();
    blocking(move || {
        let _write = state.begin_write();
        actions::hop(&state, &args)
    })
    .await
}

#[tauri::command]
pub async fn export(
    state: State<'_, AppState>,
    installation: String,
    id: String,
    to: Option<String>,
) -> CmdResult<Exported> {
    let state = state.inner().clone();
    blocking(move || actions::export(&state, &installation, &id, to.as_deref())).await
}

#[tauri::command]
pub async fn import(
    state: State<'_, AppState>,
    document: String,
    into: String,
    prompt: Option<String>,
) -> CmdResult<ReceiptView> {
    let state = state.inner().clone();
    blocking(move || {
        let _write = state.begin_write();
        actions::import(&state, &document, &into, prompt.as_deref())
    })
    .await
}

#[tauri::command]
pub async fn resume(
    state: State<'_, AppState>,
    installation: String,
    id: String,
) -> CmdResult<Resumed> {
    let state = state.inner().clone();
    blocking(move || actions::resume(&state, &installation, &id)).await
}

/// A native dialog: `open` a JSON document, `save` one (with `name` proposed), or a `folder`.
/// `None` when the person cancels.
#[tauri::command]
pub async fn pick(
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
pub async fn chat_status() -> chat::ChatStatus {
    chat::status().await
}

#[tauri::command]
pub async fn chat_rooms(cursor: Option<String>) -> CmdResult<Value> {
    chat::call("rooms", chat::page_args(json!({}), &[("cursor", cursor)])).await
}

#[tauri::command]
pub async fn chat_members(room_id: String) -> CmdResult<Value> {
    chat::call(
        "members",
        chat::page_args(json!({ "room_id": room_id }), &[]),
    )
    .await
}

#[tauri::command]
pub async fn chat_ledger(
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
pub fn quit(app: tauri::AppHandle) {
    app.exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scratch::Scratch;
    use crate::testing::{CLAUDE_FIXTURE, claude_store};

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|arg| arg.to_string()).collect()
    }

    #[test]
    fn the_command_line_sets_the_turn_limit_and_the_folders_to_scan() {
        assert_eq!(parse_args(&args(&["mukade"])).turns, DEFAULT_TURNS);
        let startup = parse_args(&args(&[
            "mukade",
            "--turns",
            "0",
            "--scan",
            "D:\\apps",
            "--scan=E:\\more",
        ]));
        assert_eq!(startup.turns, 0, "0 means every turn");
        assert_eq!(startup.scan, vec!["D:\\apps", "E:\\more"]);
        assert_eq!(parse_args(&args(&["mukade", "--turns=7"])).turns, 7);
        assert_eq!(
            parse_args(&args(&["mukade", "--turns", "many"])).turns,
            DEFAULT_TURNS
        );
        assert!(parse_args(&args(&["mukade", "--scan"])).scan.is_empty());
    }

    #[test]
    fn the_installation_list_is_the_six_standard_stores_and_what_a_scan_adds() {
        let scratch = Scratch::new("installations");
        let state = AppState::new(claude_store(&scratch));
        let none = installations_for(&state, &[]);
        assert_eq!(
            none.installations.len(),
            mukade::model::HarnessId::ALL.len()
        );
        assert!(none.installations.iter().all(|i| !i.scanned));
        assert!(none.scans.is_empty());
        // An empty folder holds no installation: one note, nothing added.
        let empty = scratch.join("empty");
        std::fs::create_dir_all(&empty).unwrap();
        let scanned = installations_for(&state, &[empty.display().to_string()]);
        assert_eq!(scanned.installations.len(), none.installations.len());
        assert_eq!(scanned.scans.len(), 1);
        assert_eq!(scanned.scans[0].added, 0);
        assert!(!scanned.scans[0].stopped_early);
        // The state now answers by key, and knows what each store listed.
        assert!(state.location("claude_code").is_ok());
        let claude = none
            .installations
            .iter()
            .find(|i| i.key == "claude_code")
            .unwrap();
        assert_eq!(claude.count, 1);
        assert!(claude.error.is_none());
        assert!(state.listed_session("claude_code", CLAUDE_FIXTURE).is_ok());
    }

    #[test]
    fn a_scan_adds_only_installations_the_state_can_resolve_by_key() {
        let scratch = Scratch::new("scan-pi");
        let app = scratch.join("PortablePi");
        let sessions = app.join("agent").join("sessions").join("proj");
        std::fs::create_dir_all(&sessions).unwrap();
        let fixture = crate::testing::fixtures()
            .join("pi/source")
            .read_dir()
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        std::fs::copy(fixture, sessions.join("one.jsonl")).unwrap();
        let state = AppState::new(claude_store(&scratch));
        let listed = installations_for(&state, &[app.display().to_string()]);
        let added = listed.scans[0].added;
        // What counts as a Pi store is the detector's call; the test only holds that whatever it
        // found became a keyed, scanned installation the state can resolve.
        let scanned: Vec<&InstallationView> =
            listed.installations.iter().filter(|i| i.scanned).collect();
        assert_eq!(scanned.len(), added);
        for installation in scanned {
            assert!(state.location(&installation.key).is_ok());
            assert!(installation.store.is_some());
        }
    }

    #[test]
    fn the_transcript_and_raw_commands_share_the_session_the_reader_opened() {
        let scratch = Scratch::new("read-cache");
        let state = AppState::new(claude_store(&scratch));
        let location = state.location("claude_code").unwrap();
        let session = sessions::read(&location, CLAUDE_FIXTURE).unwrap();
        let cached = state.cache_session("claude_code", CLAUDE_FIXTURE, session);
        let page = sessions::raw_page(&cached, 0, 5);
        assert!(page.total >= page.items.len());
        assert!(
            state
                .cached_session("claude_code", CLAUDE_FIXTURE)
                .is_some()
        );
    }
}
