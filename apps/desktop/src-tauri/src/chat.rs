//! The chat view's reads and writes, through the running chat service only. Never `Bus::open`: it
//! creates and migrates the store, and the service owns that file. The window writes as the
//! room's host (`docs/usage.md`, The host): a room's administration, and its own member's joining,
//! leaving and messages. It starts and stops the service, and launches an agent in a terminal,
//! with the command line beside it, as `mukade chat` does.

use crate::error::{CmdError, CmdResult};
use mukade::chat::client::{ChatClient, ClientConfig};
use mukade::chat::lifecycle::{self, Paths};
use serde::Serialize;
use serde_json::{Map, Value, json};
use std::ffi::OsString;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Each call to the service, connection included, gives up after this.
const TIMEOUT: Duration = Duration::from_secs(5);

/// The most rows one page asks for: the service's own limit.
const PAGE: u64 = 500;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ChatStatus {
    /// `running`, `stopped`, `unreachable`.
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_dir: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

async fn timed<T>(future: impl Future<Output = T>) -> CmdResult<T> {
    tokio::time::timeout(TIMEOUT, future)
        .await
        .map_err(|_| CmdError::unreachable("the chat service did not answer in 5 s"))
}

fn paths() -> CmdResult<Paths> {
    Paths::from_env().map_err(|error| CmdError::unreachable(error.to_string()))
}

/// What the service's status says, as the status bar shows it.
pub fn status_from(value: &Value) -> ChatStatus {
    let text = |key: &str| value.get(key).and_then(Value::as_str).map(str::to_string);
    ChatStatus {
        state: text("status").unwrap_or_else(|| "unreachable".to_string()),
        endpoint: text("endpoint"),
        data_dir: value
            .get("data_dir")
            .and_then(Value::as_str)
            .map(str::to_string),
        message: None,
    }
}

pub async fn status() -> ChatStatus {
    let unreachable = |message: String| ChatStatus {
        state: "unreachable".to_string(),
        endpoint: None,
        data_dir: None,
        message: Some(message),
    };
    let paths = match paths() {
        Ok(paths) => paths,
        Err(error) => return unreachable(error.message),
    };
    match timed(lifecycle::status(&paths)).await {
        Ok(Ok(value)) => status_from(&value),
        Ok(Err(error)) => unreachable(error.to_string()),
        Err(error) => unreachable(error.message),
    }
}

/// One read-only call (`rooms`, `members`, `ledger`) on a fresh connection.
pub async fn call(operation: &str, args: Value) -> CmdResult<Value> {
    let paths = paths()?;
    let config = lifecycle::client_config(&paths)
        .map_err(|error| CmdError::unreachable(error.to_string()))?;
    request(&config, operation, args).await
}

/// What the window writes: a room's administration, and its own member's. Never another
/// member's role, handle or binding.
pub const WRITES: [&str; 8] = [
    "open", "close", "reopen", "delete", "join", "rejoin", "leave", "send",
];

/// One write on a fresh connection, with the host's credential: the member the window joins
/// (`host: true`) is the host's, so what it sends reaches the agents as the user's.
pub async fn write(operation: &str, args: Value) -> CmdResult<Value> {
    if !WRITES.contains(&operation) {
        return Err(CmdError::refused(format!(
            "{operation} is not a write the window makes"
        )));
    }
    let paths = paths()?;
    let config = lifecycle::host_client_config(&paths)
        .map_err(|error| CmdError::unreachable(error.to_string()))?;
    request(&config, operation, args).await
}

async fn request(config: &ClientConfig, operation: &str, args: Value) -> CmdResult<Value> {
    let client = timed(ChatClient::connect(config))
        .await?
        .map_err(|error| CmdError::unreachable(error.to_string()))?;
    let reply = timed(client.call(operation, args)).await;
    client.close().await;
    reply?.map_err(|error| CmdError::failed(format!("{}: {}", error.code, error.message)))
}

/// The command line beside this program: it serves the chat and runs a launched agent.
fn command_line() -> CmdResult<PathBuf> {
    let exe = std::env::current_exe().map_err(|error| CmdError::failed(error.to_string()))?;
    let cli = exe.with_file_name(if cfg!(windows) { "mukade.exe" } else { "mukade" });
    if cli.is_file() {
        Ok(cli)
    } else {
        Err(CmdError::not_found(format!(
            "{} is not beside this program; run `mukade chat start` in a terminal",
            cli.display()
        )))
    }
}

/// Start the service with the command line beside this program, as `mukade chat start` does. It
/// blocks while the service comes up: the command runs it off the async threads.
pub async fn start() -> CmdResult<Value> {
    let paths = paths()?;
    let cli = command_line()?;
    lifecycle::start_with_executable(&paths, None, &cli)
        .await
        .map_err(|error| CmdError::failed(error.to_string()))
}

/// Stop the service; its rooms and messages stay.
pub async fn stop() -> CmdResult<Value> {
    let paths = paths()?;
    lifecycle::stop(&paths)
        .await
        .map_err(|error| CmdError::failed(error.to_string()))
}

/// The harnesses `mukade chat launch` runs.
pub const LAUNCHABLE: [&str; 4] = ["claude_code", "codex", "opencode", "pi"];
const ROLES: [&str; 4] = ["peer", "lead", "worker", "observer"];
/// The longest a launched agent listens between turns, as the command line allows.
const MAX_LISTEN: u64 = 3600;

/// The command a launch runs: the command line's `chat launch`, which finds or opens the room.
pub fn launch_command(
    cli: &Path,
    harness: &str,
    room: &str,
    role: &str,
    listen: u64,
) -> CmdResult<Vec<String>> {
    if !LAUNCHABLE.contains(&harness) {
        return Err(CmdError::refused(format!(
            "{harness} cannot be launched into a room"
        )));
    }
    if !ROLES.contains(&role) {
        return Err(CmdError::refused(format!("unknown role {role}")));
    }
    if listen > MAX_LISTEN {
        return Err(CmdError::refused("an agent listens 3600 seconds at most"));
    }
    if room.is_empty() {
        return Err(CmdError::refused("a launch needs a room"));
    }
    Ok(vec![
        cli.display().to_string(),
        "chat".into(),
        "launch".into(),
        harness.into(),
        "--room".into(),
        room.into(),
        "--role".into(),
        role.into(),
        "--listen".into(),
        listen.to_string(),
    ])
}

/// Run an agent in a room, in a new terminal in `cwd`; returns the command it runs. The window
/// does not join for it: the agent's member appears at a poll.
pub fn launch(
    harness: &str,
    room: &str,
    role: &str,
    listen: u64,
    cwd: Option<&str>,
) -> CmdResult<Vec<String>> {
    let argv = launch_command(&command_line()?, harness, room, role, listen)?;
    let cwd = cwd.map(Path::new).filter(|cwd| cwd.is_dir());
    // The launched command line must find this window's data folder, so its service.
    let env: Vec<(OsString, Option<OsString>)> = std::env::var_os("MUKADE_DATA_DIR")
        .map(|dir| (OsString::from("MUKADE_DATA_DIR"), Some(dir)))
        .into_iter()
        .collect();
    crate::terminal::launch(&argv, cwd, &env).map_err(CmdError::failed)?;
    Ok(argv)
}

/// The arguments of a page: `fixed`, the page size, and each optional value that is set.
pub fn page_args(fixed: Value, optional: &[(&str, Option<String>)]) -> Value {
    let mut args = match fixed {
        Value::Object(map) => map,
        _ => Map::new(),
    };
    args.insert("limit".into(), json!(PAGE));
    for (key, value) in optional {
        if let Some(value) = value.as_ref().filter(|value| !value.is_empty()) {
            args.insert((*key).into(), json!(value));
        }
    }
    Value::Object(args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_status_reply_becomes_the_status_bar() {
        let running = status_from(
            &json!({"status": "running", "endpoint": "http://127.0.0.1:4000/mcp", "data_dir": "/d"}),
        );
        assert_eq!(running.state, "running");
        assert_eq!(
            running.endpoint.as_deref(),
            Some("http://127.0.0.1:4000/mcp")
        );
        let stopped = status_from(&json!({"status": "stopped", "data_dir": "/d"}));
        assert_eq!(
            (stopped.state.as_str(), stopped.endpoint),
            ("stopped", None)
        );
        assert_eq!(status_from(&json!({})).state, "unreachable");
    }

    #[test]
    fn a_launch_runs_the_command_line_and_refuses_what_it_cannot_run() {
        let cli = Path::new("mukade");
        assert_eq!(
            launch_command(cli, "codex", "design", "worker", 300).unwrap(),
            [
                "mukade", "chat", "launch", "codex", "--room", "design", "--role", "worker",
                "--listen", "300"
            ]
        );
        assert!(launch_command(cli, "hermes", "design", "peer", 0).is_err());
        assert!(launch_command(cli, "codex", "design", "host", 0).is_err());
        assert!(launch_command(cli, "codex", "design", "peer", 3601).is_err());
        assert!(launch_command(cli, "codex", "", "peer", 0).is_err());
    }

    #[test]
    fn a_page_asks_for_the_service_limit_and_only_the_values_that_are_set() {
        let args = page_args(
            json!({"room_id": "r"}),
            &[
                ("cursor", None),
                ("text", Some("hop".into())),
                ("since", Some(String::new())),
            ],
        );
        assert_eq!(args, json!({"room_id": "r", "limit": 500, "text": "hop"}));
    }
}
