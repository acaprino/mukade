//! `mukade chat connect <harness>`: write the chat service into a harness's own MCP
//! configuration, and take it out again.
//!
//! This is the one place mukade writes a harness's configuration, and only when asked. It
//! writes one entry, named `mukade`, in the user-scope file; everything else in the file stays.
//! The credential is written there and never printed. What was written is remembered beside the
//! chat database, so `disconnect` removes only an entry mukade wrote and nobody changed since.

use super::security;
use crate::bus::ChatError;
use serde_json::{Map, Value, json};
use std::path::{Path, PathBuf};

/// The name of the entry in every harness's configuration.
pub const ENTRY: &str = "mukade";
/// What stands for the credential wherever an entry is shown.
pub const REDACTED: &str = "<credential>";
/// How many earlier copies of one harness's configuration are kept.
const BACKUPS: usize = 5;

/// The running service, as a client has to name it.
pub struct Server {
    pub endpoint: String,
    pub token: String,
    pub service_id: String,
}

impl Server {
    /// The two headers every request carries.
    pub fn headers(&self) -> Value {
        json!({
            "Authorization": format!("Bearer {}", self.token),
            "X-Mukade-Service-Id": self.service_id,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Json,
    Toml,
}

/// Where one harness keeps its MCP servers, and the entry that names the chat service there.
/// Each harness's adapter answers this (`Harness::chat_mcp_target`).
#[derive(Debug, Clone, PartialEq)]
pub struct Target {
    pub path: PathBuf,
    pub format: Format,
    /// The tables that lead to the entry, outermost first, without the entry's own name.
    pub tables: Vec<&'static str>,
    pub entry: Value,
}

fn refused(code: &str, message: impl Into<String>) -> ChatError {
    ChatError {
        code: code.into(),
        message: message.into(),
        retryable: false,
        ack_batch_id: None,
    }
}

fn io_failure(what: &str, path: &Path) -> ChatError {
    refused(
        "configuration_failure",
        format!("{what} {}", path.display()),
    )
}

/// The target of `harness`, or the reason it has none.
pub fn target_of(
    harness: crate::HarnessId,
    vars: &crate::catalog::Vars,
    server: &Server,
) -> Result<Target, ChatError> {
    crate::harnesses::get(harness)
        .chat_mcp_target(vars, server)
        .ok_or_else(|| {
            refused(
                "unsupported",
                format!("{harness} has no MCP configuration mukade can write, or its folder cannot be determined"),
            )
        })
}

/// The entry with its credential replaced, safe to print.
pub fn redacted(entry: &Value) -> Value {
    match entry {
        Value::String(text) if text.starts_with("Bearer ") => json!(format!("Bearer {REDACTED}")),
        Value::Object(map) => map.iter().map(|(k, v)| (k.clone(), redacted(v))).collect(),
        other => other.clone(),
    }
}

fn fingerprint(entry: &Value) -> String {
    crate::sha256::sha256_hex(entry.to_string().as_bytes())
}

// ---------------------------------------------------------------------------------------------
// one document, two formats

enum Document {
    Json(Value),
    Toml(toml_edit::DocumentMut),
}

fn toml_to_json(item: &toml_edit::Item) -> Option<Value> {
    fn value(v: &toml_edit::Value) -> Value {
        match v {
            toml_edit::Value::String(s) => json!(s.value()),
            toml_edit::Value::Integer(i) => json!(i.value()),
            toml_edit::Value::Float(f) => json!(f.value()),
            toml_edit::Value::Boolean(b) => json!(b.value()),
            toml_edit::Value::Datetime(d) => json!(d.value().to_string()),
            toml_edit::Value::Array(a) => Value::Array(a.iter().map(value).collect()),
            toml_edit::Value::InlineTable(t) => {
                Value::Object(t.iter().map(|(k, v)| (k.to_string(), value(v))).collect())
            }
        }
    }
    match item {
        toml_edit::Item::None => None,
        toml_edit::Item::Value(v) => Some(value(v)),
        toml_edit::Item::Table(t) => Some(Value::Object(
            t.iter()
                .filter_map(|(k, v)| Some((k.to_string(), toml_to_json(v)?)))
                .collect(),
        )),
        toml_edit::Item::ArrayOfTables(a) => Some(Value::Array(
            a.iter()
                .map(|t| {
                    Value::Object(
                        t.iter()
                            .filter_map(|(k, v)| Some((k.to_string(), toml_to_json(v)?)))
                            .collect(),
                    )
                })
                .collect(),
        )),
    }
}

fn json_to_toml(value: &Value) -> Option<toml_edit::Value> {
    Some(match value {
        Value::String(s) => s.as_str().into(),
        Value::Bool(b) => (*b).into(),
        Value::Number(n) => match n.as_i64() {
            Some(i) => i.into(),
            None => n.as_f64()?.into(),
        },
        Value::Array(items) => {
            let mut array = toml_edit::Array::new();
            for item in items {
                array.push(json_to_toml(item)?);
            }
            array.into()
        }
        Value::Object(map) => {
            let mut table = toml_edit::InlineTable::new();
            for (key, item) in map {
                table.insert(key, json_to_toml(item)?);
            }
            table.into()
        }
        Value::Null => return None,
    })
}

impl Document {
    fn parse(format: Format, text: &str, path: &Path) -> Result<Self, ChatError> {
        let unreadable = || {
            refused(
                "configuration_failure",
                format!(
                    "{} is not plain {}: mukade does not rewrite it (comments or a syntax error); add the entry by hand, see docs/usage.md",
                    path.display(),
                    if format == Format::Json {
                        "JSON"
                    } else {
                        "TOML"
                    }
                ),
            )
        };
        Ok(match format {
            Format::Json if text.trim().is_empty() => Document::Json(json!({})),
            Format::Json => {
                let value: Value = serde_json::from_str(text).map_err(|_| unreadable())?;
                if !value.is_object() {
                    return Err(unreadable());
                }
                Document::Json(value)
            }
            Format::Toml => Document::Toml(text.parse().map_err(|_| unreadable())?),
        })
    }

    fn entry(&self, tables: &[&str]) -> Option<Value> {
        match self {
            Document::Json(root) => {
                let mut at = root;
                for table in tables {
                    at = at.get(table)?;
                }
                at.get(ENTRY).cloned()
            }
            Document::Toml(doc) => {
                let mut at = doc.as_item();
                for table in tables {
                    at = at.get(table)?;
                }
                toml_to_json(at.get(ENTRY)?)
            }
        }
    }

    /// Put the entry in place (`Some`) or take it out (`None`). A table on the way that is
    /// something else than a table is a conflict.
    fn set(
        &mut self,
        tables: &[&str],
        entry: Option<&Value>,
        path: &Path,
    ) -> Result<(), ChatError> {
        let conflict = || {
            refused(
                "configuration_conflict",
                format!(
                    "{} holds something unexpected where its MCP servers go",
                    path.display()
                ),
            )
        };
        match self {
            Document::Json(root) => {
                let mut at = root;
                for table in tables {
                    let map = at.as_object_mut().ok_or_else(conflict)?;
                    if entry.is_none() && !map.contains_key(*table) {
                        return Ok(());
                    }
                    at = map
                        .entry(table.to_string())
                        .or_insert_with(|| Value::Object(Map::new()));
                }
                let map = at.as_object_mut().ok_or_else(conflict)?;
                match entry {
                    Some(entry) => {
                        map.insert(ENTRY.into(), entry.clone());
                    }
                    None => {
                        map.shift_remove(ENTRY);
                    }
                }
            }
            Document::Toml(doc) => {
                let mut at = doc.as_table_mut() as &mut dyn toml_edit::TableLike;
                for table in tables {
                    if entry.is_none() && !at.contains_key(table) {
                        return Ok(());
                    }
                    let next = at.entry(table).or_insert_with(|| {
                        let mut made = toml_edit::Table::new();
                        made.set_implicit(true);
                        toml_edit::Item::Table(made)
                    });
                    at = next.as_table_like_mut().ok_or_else(conflict)?;
                }
                match entry {
                    Some(entry) => {
                        let mut table = toml_edit::Table::new();
                        for (key, value) in entry.as_object().ok_or_else(conflict)? {
                            table.insert(
                                key,
                                toml_edit::Item::Value(json_to_toml(value).ok_or_else(conflict)?),
                            );
                        }
                        at.insert(ENTRY, toml_edit::Item::Table(table));
                    }
                    None => {
                        at.remove(ENTRY);
                    }
                }
            }
        }
        Ok(())
    }

    fn text(&self) -> String {
        match self {
            Document::Json(root) => {
                let mut text = serde_json::to_string_pretty(root).unwrap_or_default();
                text.push('\n');
                text
            }
            Document::Toml(doc) => doc.to_string(),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// what mukade remembers it wrote

fn records_path(bus_dir: &Path) -> PathBuf {
    bus_dir.join("connections.json")
}

fn records(bus_dir: &Path) -> Result<Map<String, Value>, ChatError> {
    match security::read_private(&records_path(bus_dir)) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|_| io_failure("Invalid connection record", &records_path(bus_dir))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Map::new()),
        Err(_) => Err(io_failure("Cannot read", &records_path(bus_dir))),
    }
}

fn save_records(bus_dir: &Path, records: &Map<String, Value>) -> Result<(), ChatError> {
    let bytes = serde_json::to_vec(records).unwrap_or_default();
    security::atomic_write(&records_path(bus_dir), &bytes)
        .map_err(|_| io_failure("Cannot write", &records_path(bus_dir)))
}

/// Whether the entry found in the file is the one mukade wrote there.
fn ours(records: &Map<String, Value>, harness: &str, target: &Target, found: &Value) -> bool {
    records.get(harness).is_some_and(|record| {
        record["path"].as_str() == Some(&target.path.to_string_lossy())
            && record["entry_sha256"].as_str() == Some(&fingerprint(found))
    })
}

/// Keep the file as it was, in the chat service's own protected folder, and drop the oldest
/// copies of this harness beyond [`BACKUPS`].
fn backup(bus_dir: &Path, harness: &str, original: &[u8]) -> Result<(), ChatError> {
    let folder = bus_dir.join("backups");
    let failed = || io_failure("Cannot keep a copy of the configuration in", &folder);
    security::create_private_directory(&folder).map_err(|_| failed())?;
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%S%3fZ");
    let prefix = format!("connect-{harness}-");
    let copy = folder.join(format!(
        "{prefix}{stamp}-{}.bak",
        uuid::Uuid::new_v4().simple()
    ));
    {
        use std::io::Write;
        let mut file = security::create_private_file(&copy).map_err(|_| failed())?;
        file.write_all(original).map_err(|_| failed())?;
        file.sync_all().map_err(|_| failed())?;
    }
    let mut copies: Vec<PathBuf> = std::fs::read_dir(&folder)
        .map_err(|_| failed())?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with(&prefix))
        })
        .collect();
    copies.sort();
    for old in copies.iter().rev().skip(BACKUPS) {
        let _ = std::fs::remove_file(old);
    }
    Ok(())
}

/// Replace the file with `text`, unless it changed since `original` was read.
fn publish(path: &Path, original: Option<&[u8]>, text: &str) -> Result<(), ChatError> {
    let now = match std::fs::read(path) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(_) => return Err(io_failure("Cannot read", path)),
    };
    if now.as_deref() != original {
        return Err(refused(
            "configuration_conflict",
            format!(
                "{} changed while it was being edited; nothing was written, try again",
                path.display()
            ),
        ));
    }
    let folder = path
        .parent()
        .ok_or_else(|| io_failure("No folder for", path))?;
    std::fs::create_dir_all(folder).map_err(|_| io_failure("Cannot create the folder of", path))?;
    let temporary = folder.join(format!(
        ".mukade-connect-{}.tmp",
        uuid::Uuid::new_v4().simple()
    ));
    let written = (|| {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temporary, path)
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&temporary);
        return Err(io_failure("Cannot write", path));
    }
    Ok(())
}

fn report(status: &str, harness: &str, target: &Target, entry: Option<&Value>) -> Value {
    json!({
        "status": status,
        "harness": harness,
        "scope": "user",
        "path": target.path,
        "entry": entry.map(redacted),
    })
}

/// Write the entry. `dry_run` reports what would happen and writes nothing.
///
/// `unchanged`: the entry is already there. `connected`: written, or would be. Refused when an
/// entry of that name is there that mukade did not write, or that someone changed since.
pub fn connect(
    target: &Target,
    bus_dir: &Path,
    harness: &str,
    dry_run: bool,
) -> Result<Value, ChatError> {
    let original = match std::fs::read(&target.path) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(_) => return Err(io_failure("Cannot read", &target.path)),
    };
    let text = String::from_utf8(original.clone().unwrap_or_default())
        .map_err(|_| io_failure("Not UTF-8 text:", &target.path))?;
    let mut document = Document::parse(target.format, &text, &target.path)?;
    let mut known = records(bus_dir)?;
    let found = document.entry(&target.tables);
    let record = json!({"path": target.path, "entry_sha256": fingerprint(&target.entry)});
    if found.as_ref() == Some(&target.entry) {
        if !dry_run && known.get(harness) != Some(&record) {
            known.insert(harness.into(), record);
            save_records(bus_dir, &known)?;
        }
        return Ok(report("unchanged", harness, target, Some(&target.entry)));
    }
    if let Some(found) = &found
        && !ours(&known, harness, target, found)
    {
        return Err(refused(
            "configuration_conflict",
            format!(
                "{} already has a server named {ENTRY} that mukade did not write, or that was changed since; remove or rename it first",
                target.path.display()
            ),
        ));
    }
    document.set(&target.tables, Some(&target.entry), &target.path)?;
    if dry_run {
        return Ok(report(
            "would_connect",
            harness,
            target,
            Some(&target.entry),
        ));
    }
    if let Some(original) = &original {
        backup(bus_dir, harness, original)?;
    }
    publish(&target.path, original.as_deref(), &document.text())?;
    known.insert(harness.into(), record);
    save_records(bus_dir, &known)?;
    Ok(report("connected", harness, target, Some(&target.entry)))
}

/// Take the entry out, when it is the one mukade wrote.
pub fn disconnect(
    target: &Target,
    bus_dir: &Path,
    harness: &str,
    dry_run: bool,
) -> Result<Value, ChatError> {
    let original = match std::fs::read(&target.path) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(_) => return Err(io_failure("Cannot read", &target.path)),
    };
    let text = String::from_utf8(original.clone().unwrap_or_default())
        .map_err(|_| io_failure("Not UTF-8 text:", &target.path))?;
    let mut document = Document::parse(target.format, &text, &target.path)?;
    let mut known = records(bus_dir)?;
    let Some(found) = document.entry(&target.tables) else {
        if !dry_run && known.remove(harness).is_some() {
            save_records(bus_dir, &known)?;
        }
        return Ok(report("not_connected", harness, target, None));
    };
    if !ours(&known, harness, target, &found) {
        return Err(refused(
            "configuration_conflict",
            format!(
                "the server named {ENTRY} in {} is not the one mukade wrote, or was changed since; it is left as it is",
                target.path.display()
            ),
        ));
    }
    document.set(&target.tables, None, &target.path)?;
    if dry_run {
        return Ok(report("would_disconnect", harness, target, None));
    }
    if let Some(original) = &original {
        backup(bus_dir, harness, original)?;
    }
    publish(&target.path, original.as_deref(), &document.text())?;
    known.remove(harness);
    save_records(bus_dir, &known)?;
    Ok(report("disconnected", harness, target, None))
}
