//! What the window is sent: plain records the TypeScript side mirrors by hand (`src/types.ts`).
//! Every conversion here is pure, so the tests run without a window or a store.

use crate::location::Location;
use mukade::model::{Kind, ParentKind, Session};
use mukade::reader::SessionInfo;
use serde::Serialize;
use serde_json::Value;

/// One place sessions live (spec §6 `installations`).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Installation {
    /// The location's index, stable while the window runs: scans only append.
    pub key: usize,
    /// `HarnessId::as_str`, for matching a chat member's harness.
    pub harness: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub engine: Option<String>,
    pub scanned: bool,
    /// Whether mukade writes into this harness: a hop destination.
    pub writable: bool,
    pub count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Installation {
    pub fn of(key: usize, location: &Location, count: usize, error: Option<String>) -> Self {
        Self {
            key,
            harness: location.harness.as_str().to_string(),
            label: location.label.clone(),
            store: location
                .store
                .as_ref()
                .map(|store| store.display().to_string()),
            engine: location.store.as_ref().map(|_| {
                location
                    .exe
                    .clone()
                    .unwrap_or_else(|| "standard resolution".to_string())
            }),
            scanned: location.store.is_some(),
            writable: mukade::harnesses::get(location.harness)
                .destination()
                .is_some(),
            count,
            error,
        }
    }
}

/// One listed session (spec §6 `sessions`). The tree, the folders and the children counts are
/// built from these on the TypeScript side.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SessionRow {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated: Option<String>,
    pub archived: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub live: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unreadable: Option<String>,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_kind: Option<ParentKind>,
}

impl From<&SessionInfo> for SessionRow {
    fn from(info: &SessionInfo) -> Self {
        Self {
            id: info.id.clone(),
            title: info.title.clone().filter(|title| !title.trim().is_empty()),
            cwd: info.cwd.clone().filter(|cwd| !cwd.trim().is_empty()),
            updated: info.updated.clone(),
            archived: info.archived,
            live: info.live,
            unreadable: info.unreadable.clone(),
            path: info.path.clone(),
            parent: info.parent.clone(),
            parent_kind: info.parent.as_ref().and(info.parent_kind.clone()),
        }
    }
}

/// The longest tool result or compaction summary sent whole; the Raw tab holds the rest.
pub const TEXT_CAP: usize = 20_000;

/// One block of a transcript entry, as the reader draws it.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Block {
    /// `user`, `assistant`, `reasoning`, `tool`, `result`, `error`, `attachment`, `compaction`.
    pub role: &'static str,
    pub text: String,
    /// The tool's name, for `tool` (its input is `text`, as JSON).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The text was cut at [`TEXT_CAP`] characters.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub cut: bool,
}

/// One conversation entry (spec §6 `transcript`), in native order.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct EntryView {
    pub id: String,
    /// `user`, `step`, `results`, `compaction`.
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time: Option<String>,
    pub blocks: Vec<Block>,
    /// How many native records it was read from (the Raw tab's count).
    pub records: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Transcript {
    pub title: String,
    pub entries: Vec<EntryView>,
    /// User turns left out by the turn limit.
    pub hidden_turns: usize,
    pub total_turns: usize,
}

/// The conversation of `session`, its last `turns` user turns only (0: all). Meta entries are not
/// conversation and are left out, as the TUI does.
pub fn transcript(session: &Session, fallback_title: &str, turns: usize) -> Transcript {
    let entries: Vec<&mukade::Entry> = session.conversation().collect();
    let user_turns: Vec<usize> = entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| matches!(entry.kind, Kind::User { .. }))
        .map(|(index, _)| index)
        .collect();
    let (start, hidden) = if turns > 0 && user_turns.len() > turns {
        (
            user_turns[user_turns.len() - turns],
            user_turns.len() - turns,
        )
    } else {
        (0, 0)
    };
    let title = session
        .meta
        .title
        .clone()
        .filter(|title| !title.trim().is_empty())
        .unwrap_or_else(|| fallback_title.to_string());
    Transcript {
        title,
        entries: entries[start..]
            .iter()
            .map(|entry| entry_view(entry))
            .collect(),
        hidden_turns: hidden,
        total_turns: user_turns.len(),
    }
}

fn entry_view(entry: &mukade::Entry) -> EntryView {
    let mut blocks = Vec::new();
    let kind = match &entry.kind {
        Kind::User { text, attachments } => {
            blocks.push(block("user", text, None));
            for attachment in attachments {
                let path = attachment.path.as_deref().unwrap_or("");
                blocks.push(block(
                    "attachment",
                    format!("{} {path}", attachment.kind).trim(),
                    None,
                ));
            }
            "user"
        }
        Kind::Step {
            reasoning,
            text,
            calls,
            ..
        } => {
            for part in reasoning.iter().filter(|part| !part.text.trim().is_empty()) {
                blocks.push(block("reasoning", &part.text, None));
            }
            for part in text.iter().filter(|part| !part.trim().is_empty()) {
                blocks.push(block("assistant", part, None));
            }
            for call in calls {
                let input = serde_json::to_string(&call.input).unwrap_or_default();
                blocks.push(block("tool", &input, Some(call.source_name.clone())));
            }
            "step"
        }
        Kind::Results { results } => {
            for result in results {
                let role = if result.is_error { "error" } else { "result" };
                blocks.push(block(role, &result.content.text(), None));
            }
            "results"
        }
        Kind::Compaction { summary, .. } => {
            blocks.push(block("compaction", summary, None));
            "compaction"
        }
        // `conversation()` leaves Meta out; a future kind shows as an empty entry, not a panic.
        _ => "meta",
    };
    EntryView {
        id: entry.id.clone(),
        kind,
        time: entry.raw.iter().find_map(record_time),
        blocks,
        records: entry.raw.len(),
    }
}

fn block(role: &'static str, text: &str, name: Option<String>) -> Block {
    let cut = text.chars().count() > TEXT_CAP;
    Block {
        role,
        text: if cut {
            text.chars().take(TEXT_CAP).collect()
        } else {
            text.to_string()
        },
        name,
        cut,
    }
}

/// When a native record says it was made: the common field names, as the records carry them (an
/// RFC 3339 string, or epoch milliseconds). `None` when the record names no time.
fn record_time(record: &Value) -> Option<String> {
    let text = |value: &Value| match value {
        Value::String(text) if !text.is_empty() => Some(text.clone()),
        Value::Number(number) => number.as_i64().and_then(|millis| {
            chrono::DateTime::from_timestamp_millis(millis).map(|time| time.to_rfc3339())
        }),
        _ => None,
    };
    ["timestamp", "created_at", "createdAt", "time"]
        .iter()
        .find_map(|key| match record.get(key)? {
            Value::Object(times) => times.get("created").and_then(text),
            other => text(other),
        })
}

/// A hop's or an import's result, as the receipt dialog shows it (spec F6).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ReceiptView {
    pub dest_id: String,
    pub dest_label: String,
    /// The destination installation's key, to show the new session in the tree.
    pub dest_key: usize,
    pub location: String,
    pub headless: Vec<String>,
    /// The command that opens it in the destination's own interface; empty with `refusal`.
    pub interactive: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refusal: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest_path: Option<String>,
    pub losses: Vec<String>,
    pub children: Vec<ChildLine>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply: Option<String>,
    /// The session was written but something after it failed (the resume prompt, a subagent).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ChildLine {
    pub source_id: String,
    pub dest_id: String,
    pub linked: bool,
}

pub fn receipt(
    receipt: &mukade::Receipt,
    dest_key: usize,
    dest: &Location,
    manifest_path: Option<String>,
    warning: Option<String>,
) -> ReceiptView {
    let info = SessionInfo {
        id: receipt.dest_id.clone(),
        path: receipt.location.clone(),
        ..Default::default()
    };
    let refusal = mukade::harnesses::get(dest.harness).resume_refusal(&info);
    let interactive = if refusal.is_some() {
        Vec::new()
    } else {
        mukade::catalog::interactive_resume(
            dest.harness,
            &dest.engine(),
            &info.id,
            Some(std::path::Path::new(&info.path)),
        )
    };
    let losses = receipt
        .manifest
        .get("losses")
        .and_then(Value::as_array)
        .map(|losses| {
            losses
                .iter()
                .map(|loss| {
                    loss.as_str()
                        .map(str::to_string)
                        .unwrap_or_else(|| serde_json::to_string(loss).unwrap_or_default())
                })
                .collect()
        })
        .unwrap_or_default();
    ReceiptView {
        dest_id: receipt.dest_id.clone(),
        dest_label: dest.label.clone(),
        dest_key,
        location: receipt.location.clone(),
        headless: receipt.resume_command.clone(),
        interactive,
        refusal,
        manifest_path,
        losses,
        children: child_lines(&receipt.manifest),
        reply: receipt.resume_output.clone(),
        warning,
    }
}

/// What became of each subagent session a hop carried (the manifest's `children`).
pub fn child_lines(manifest: &Value) -> Vec<ChildLine> {
    manifest
        .get("children")
        .and_then(Value::as_array)
        .map(|children| {
            children
                .iter()
                .map(|child| {
                    let text = |key: &str| {
                        child
                            .get(key)
                            .and_then(Value::as_str)
                            .unwrap_or("?")
                            .to_string()
                    };
                    ChildLine {
                        source_id: text("source_id"),
                        dest_id: text("dest_id"),
                        linked: child.get("link").and_then(Value::as_str) == Some("native"),
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mukade::model::{HarnessId, Meta};
    use serde_json::json;

    fn session(entries: Vec<mukade::Entry>) -> Session {
        Session {
            meta: Meta {
                harness: HarnessId::ALL[0],
                harness_version: "1".into(),
                id: "s".into(),
                cwd: None,
                title: None,
                created: None,
                updated: None,
                models: Vec::new(),
                git_branch: None,
                parent: None,
                parent_kind: None,
                source_path: None,
                source_sha256: None,
            },
            entries,
            branches: Vec::new(),
        }
    }

    fn user(id: &str, text: &str) -> mukade::Entry {
        mukade::Entry {
            id: id.into(),
            kind: Kind::User {
                text: text.into(),
                attachments: Vec::new(),
            },
            raw: vec![json!({"timestamp": "2026-11-12T12:00:00Z"})],
        }
    }

    #[test]
    fn the_turn_limit_keeps_the_last_user_turns_and_counts_the_rest() {
        let session = session(vec![user("a", "one"), user("b", "two"), user("c", "three")]);
        let view = transcript(&session, "fallback", 2);
        let ids: Vec<&str> = view.entries.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, ["b", "c"]);
        assert_eq!((view.hidden_turns, view.total_turns), (1, 3));
        assert_eq!(view.title, "fallback");
        assert_eq!(
            view.entries[0].time.as_deref(),
            Some("2026-11-12T12:00:00Z")
        );
        assert_eq!(transcript(&session, "f", 0).entries.len(), 3);
    }

    #[test]
    fn a_long_text_is_cut_and_says_so() {
        let long = "x".repeat(TEXT_CAP + 5);
        let view = transcript(&session(vec![user("a", &long)]), "f", 0);
        let block = &view.entries[0].blocks[0];
        assert!(block.cut);
        assert_eq!(block.text.chars().count(), TEXT_CAP);
    }

    #[test]
    fn a_record_time_in_epoch_milliseconds_reads_as_rfc3339() {
        let time = record_time(&json!({"time": {"created": 1_700_000_000_000_i64}}));
        assert_eq!(time.as_deref(), Some("2023-11-14T22:13:20+00:00"));
        assert_eq!(record_time(&json!({"other": 1})), None);
    }

    #[test]
    fn child_lines_say_whether_the_destination_linked_each_child() {
        let manifest = json!({"children": [
            {"source_id": "a", "dest_id": "x", "link": "native"},
            {"source_id": "b", "dest_id": "y", "link": "none"},
        ]});
        assert_eq!(
            child_lines(&manifest),
            [
                ChildLine {
                    source_id: "a".into(),
                    dest_id: "x".into(),
                    linked: true
                },
                ChildLine {
                    source_id: "b".into(),
                    dest_id: "y".into(),
                    linked: false
                },
            ]
        );
        assert!(child_lines(&json!({})).is_empty());
    }

    #[test]
    fn a_row_drops_an_empty_title_and_a_kind_without_a_parent() {
        let info = SessionInfo {
            id: "s".into(),
            title: Some("  ".into()),
            parent_kind: Some(ParentKind::fork()),
            ..Default::default()
        };
        let row = SessionRow::from(&info);
        assert_eq!(row.title, None);
        assert_eq!(row.parent_kind, None);
        let json = serde_json::to_value(SessionRow::from(&SessionInfo {
            parent: Some("p".into()),
            parent_kind: Some(ParentKind::Spawn {
                call_id: None,
                agent: Some("Explore".into()),
                inherits_context: false,
            }),
            ..info
        }))
        .unwrap();
        assert_eq!(
            json["parent_kind"],
            json!({"kind": "spawn", "agent": "Explore"})
        );
    }
}
