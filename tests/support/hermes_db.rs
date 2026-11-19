//! A synthetic Hermes `state.db`, built from the shape Hermes v0.23.1 declares (`SCHEMA_SQL` in
//! `hermes_state_common.py`, without the FTS tables and the columns no reader needs). Nothing in
//! it is copied from a real store.

use rusqlite::{Connection, ToSql, params};
use serde_json::Value;
use std::path::{Path, PathBuf};

const SCHEMA: &str = "
CREATE TABLE schema_version (version INTEGER NOT NULL);
CREATE TABLE sessions (
    id TEXT PRIMARY KEY,
    runtime_revision INTEGER NOT NULL DEFAULT 0,
    source TEXT NOT NULL,
    created_source TEXT,
    model TEXT,
    model_config TEXT,
    system_prompt TEXT,
    parent_session_id TEXT REFERENCES sessions(id),
    started_at REAL NOT NULL,
    ended_at REAL,
    end_reason TEXT,
    message_count INTEGER DEFAULT 0,
    tool_call_count INTEGER DEFAULT 0,
    cwd TEXT,
    git_branch TEXT,
    billing_provider TEXT,
    title TEXT,
    last_activity_at REAL,
    archived INTEGER NOT NULL DEFAULT 0,
    auto_archived INTEGER NOT NULL DEFAULT 0,
    pinned INTEGER NOT NULL DEFAULT 0,
    hidden INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions(id),
    role TEXT NOT NULL,
    content TEXT,
    tool_call_id TEXT,
    tool_calls TEXT,
    tool_name TEXT,
    effect_disposition TEXT,
    timestamp REAL NOT NULL,
    token_count INTEGER,
    finish_reason TEXT,
    reasoning TEXT,
    reasoning_content TEXT,
    reasoning_details TEXT,
    codex_reasoning_items TEXT,
    codex_message_items TEXT,
    platform_message_id TEXT,
    observed INTEGER DEFAULT 0,
    _compressed_summary INTEGER NOT NULL DEFAULT 0,
    active INTEGER NOT NULL DEFAULT 1,
    compacted INTEGER NOT NULL DEFAULT 0,
    api_content TEXT,
    display_kind TEXT,
    display_metadata TEXT,
    display_identity BLOB,
    display_order INTEGER,
    message_uid TEXT,
    absorbed_message_uids TEXT,
    tool_call_uids TEXT,
    tool_call_uid TEXT
);
CREATE INDEX idx_messages_session ON messages(session_id, id);
";

pub struct HermesDb {
    pub conn: Connection,
    pub path: PathBuf,
}

/// A new `state.db` at schema 33 in `home`.
pub fn create(home: &Path) -> HermesDb {
    create_at(home, 33, SCHEMA)
}

/// A new `state.db` with the given schema number and DDL (a store of another shape).
pub fn create_at(home: &Path, version: i64, schema: &str) -> HermesDb {
    std::fs::create_dir_all(home).expect("create the Hermes home");
    let path = home.join("state.db");
    let _ = std::fs::remove_file(&path);
    let conn = Connection::open(&path).expect("create state.db");
    conn.execute_batch(schema).expect("create the schema");
    conn.execute("INSERT INTO schema_version VALUES (?1)", [version])
        .expect("stamp the schema");
    HermesDb { conn, path }
}

/// The schema this builder uses, for a test that needs it with a column removed.
pub fn schema() -> &'static str {
    SCHEMA
}

impl HermesDb {
    /// A session row; `started` is epoch seconds.
    pub fn session(&self, id: &str, source: &str, started: f64) -> &Self {
        self.conn
            .execute(
                "INSERT INTO sessions (id, source, started_at) VALUES (?1, ?2, ?3)",
                params![id, source, started],
            )
            .expect("insert a session");
        self
    }

    /// Set one column of a session.
    pub fn set_session(&self, id: &str, column: &str, value: &dyn ToSql) -> &Self {
        self.conn
            .execute(
                &format!("UPDATE sessions SET {column} = ?1 WHERE id = ?2"),
                params![value, id],
            )
            .expect("update a session");
        self
    }

    /// A message row; returns its id. The timestamp follows the row id.
    pub fn message(&self, session: &str, role: &str, content: Option<&str>) -> i64 {
        self.conn
            .execute(
                "INSERT INTO messages (session_id, role, content, timestamp) \
                 VALUES (?1, ?2, ?3, 1794480000.0 + (SELECT COALESCE(MAX(id), 0) FROM messages))",
                params![session, role, content],
            )
            .expect("insert a message");
        self.conn.last_insert_rowid()
    }

    /// Set one column of a message.
    pub fn set(&self, id: i64, column: &str, value: &dyn ToSql) -> &Self {
        self.conn
            .execute(
                &format!("UPDATE messages SET {column} = ?1 WHERE id = ?2"),
                params![value, id],
            )
            .expect("update a message");
        self
    }

    /// An assistant row that calls tools, with the calls in the OpenAI shape Hermes stores.
    pub fn calls(&self, session: &str, text: &str, calls: &[(&str, &str, Value)]) -> i64 {
        let id = self.message(session, "assistant", Some(text));
        let array: Vec<Value> = calls
            .iter()
            .map(|(call_id, name, arguments)| {
                serde_json::json!({
                    "id": call_id,
                    "call_id": call_id,
                    "response_item_id": format!("fc_{call_id}"),
                    "type": "function",
                    "function": {"name": name, "arguments": arguments.to_string()},
                })
            })
            .collect();
        self.set(id, "tool_calls", &Value::Array(array).to_string());
        self.set(id, "finish_reason", &"tool_calls");
        id
    }

    /// A tool row answering `call_id`.
    pub fn result(&self, session: &str, call_id: &str, tool: &str, content: &str) -> i64 {
        let id = self.message(session, "tool", Some(content));
        self.set(id, "tool_call_id", &call_id);
        self.set(id, "tool_name", &tool);
        id
    }

    /// Load the sessions of a `hermes sessions export` file into the tables, as Hermes stores
    /// them: `content` with the JSON prefix for a list, the JSON columns as text.
    pub fn load_export(&self, export: &Path) {
        let text = std::fs::read_to_string(export).expect("read the export");
        for line in text.lines().filter(|line| !line.trim().is_empty()) {
            let mut session: serde_json::Map<String, Value> =
                serde_json::from_str(line).expect("an export line");
            let messages = session.remove("messages").expect("messages");
            session.remove("timings");
            self.insert_row("sessions", &session, false);
            for message in messages.as_array().expect("an array") {
                self.insert_row("messages", message.as_object().expect("an object"), true);
            }
        }
    }

    fn insert_row(&self, table: &str, row: &serde_json::Map<String, Value>, message: bool) {
        let have: Vec<String> = {
            let mut query = self
                .conn
                .prepare(&format!("SELECT name FROM pragma_table_info('{table}')"))
                .unwrap();
            query
                .query_map([], |r| r.get::<_, String>(0))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap()
        };
        let mut columns = Vec::new();
        let mut values: Vec<rusqlite::types::Value> = Vec::new();
        for (name, value) in row.iter().filter(|(name, _)| have.contains(name)) {
            columns.push(name.clone());
            values.push(match value {
                Value::Null => rusqlite::types::Value::Null,
                Value::Number(n) if n.is_i64() => {
                    rusqlite::types::Value::Integer(n.as_i64().unwrap())
                }
                Value::Number(n) => rusqlite::types::Value::Real(n.as_f64().unwrap()),
                Value::String(s) => rusqlite::types::Value::Text(s.clone()),
                Value::Array(_) | Value::Object(_) if message && name == "content" => {
                    rusqlite::types::Value::Text(format!("\u{0}json:{value}"))
                }
                other => rusqlite::types::Value::Text(other.to_string()),
            });
        }
        let marks: Vec<String> = (1..=columns.len()).map(|n| format!("?{n}")).collect();
        self.conn
            .execute(
                &format!(
                    "INSERT INTO {table} ({}) VALUES ({})",
                    columns.join(", "),
                    marks.join(", ")
                ),
                rusqlite::params_from_iter(values),
            )
            .expect("insert an export row");
    }
}
