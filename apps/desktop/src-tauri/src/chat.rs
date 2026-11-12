//! The chat view's reads (spec §5), through the running chat service only. Never `Bus::open`: it
//! creates and migrates the store, and the service owns that file. Nothing here starts, stops or
//! writes to the service.

use crate::error::{CmdError, CmdResult};
use mukade::chat::client::ChatClient;
use mukade::chat::lifecycle::{self, Paths};
use serde::Serialize;
use serde_json::{Map, Value, json};
use std::future::Future;
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
    let client = timed(ChatClient::connect(&config))
        .await?
        .map_err(|error| CmdError::unreachable(error.to_string()))?;
    let reply = timed(client.call(operation, args)).await;
    client.close().await;
    reply?.map_err(|error| CmdError::failed(format!("{}: {}", error.code, error.message)))
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
