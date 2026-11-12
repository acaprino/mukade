//! Listing a store and reading one session, as the view models the interface draws.
//!
//! The library answers (`reader`, `hop::export`); this only shapes. Every per-harness fact comes
//! from there (ADR-0001).

use crate::error::{CmdError, CmdResult};
use crate::locations::Location;
use mukade::hop::SessionRef;
use mukade::model::{Entry, Kind, ParentKind, Session};
use mukade::reader::{self, SessionInfo};
use serde::Serialize;
use serde_json::Value;

/// One line of a listing: what a reader knew about a session before reading it.
#[derive(Debug, Clone, PartialEq, Serialize)]
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

impl From<SessionInfo> for SessionRow {
    fn from(info: SessionInfo) -> Self {
        Self {
            id: info.id,
            title: info.title,
            cwd: info.cwd,
            updated: info.updated,
            archived: info.archived,
            live: info.live,
            unreadable: info.unreadable,
            path: info.path,
            parent: info.parent,
            parent_kind: info.parent_kind,
        }
    }
}

/// Every session a location's store holds, children and forks included, through the harness's own
/// reader.
pub fn list(location: &Location) -> CmdResult<Vec<SessionRow>> {
    let reader = reader::for_harness(location.harness)?;
    let listed = reader.list_all(&location.roots)?;
    Ok(listed.into_iter().map(SessionRow::from).collect())
}

/// One session, read by its native id from the location's store.
pub fn read(location: &Location, id: &str) -> CmdResult<Session> {
    mukade::export(&SessionRef::id(location.harness, id), &location.roots).map_err(CmdError::from)
}

/// What a session says about itself, for the header of the reader.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MetaView {
    pub harness: &'static str,
    pub version: String,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_branch: Option<String>,
    pub models: Vec<String>,
    /// Entries of the active path, markers included.
    pub entries: usize,
    /// Branches the read kept (an all-branches read only).
    pub branches: usize,
}

/// A tool call, shaped for a collapsible line.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ToolView {
    pub name: String,
    pub call_id: String,
    /// The call's input as JSON, one line, cut at [`MAX_INPUT`].
    pub input: String,
    pub input_truncated: bool,
}

/// One drawn line of the conversation. `role` is `user`, `file`, `assistant`, `reasoning`,
/// `tool`, `result` or `compaction`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RowView {
    /// Position of the entry in the session's entries: what a raw read is asked for.
    pub index: usize,
    pub entry_id: String,
    pub role: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time: Option<String>,
    pub text: String,
    /// The text was cut at [`MAX_TEXT`]; `text_chars` is the whole length.
    pub truncated: bool,
    pub text_chars: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<ToolView>,
    pub error: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TranscriptView {
    pub meta: MetaView,
    pub rows: Vec<RowView>,
    pub total_turns: usize,
    /// Earlier user turns left out by the turn limit.
    pub hidden_turns: usize,
}

/// A row's text, and a tool input, are cut here: a session can hold a whole file in one result.
pub const MAX_TEXT: usize = 30_000;
pub const MAX_INPUT: usize = 6_000;

fn clip(text: &str, max: usize) -> (String, bool) {
    match text.char_indices().nth(max) {
        Some((end, _)) => (text[..end].to_string(), true),
        None => (text.to_string(), false),
    }
}

fn row(index: usize, entry: &Entry, role: &'static str, text: &str) -> RowView {
    let (shown, truncated) = clip(text, MAX_TEXT);
    RowView {
        index,
        entry_id: entry.id.clone(),
        role,
        time: entry_time(entry),
        text: shown,
        truncated,
        text_chars: text.chars().count(),
        tool: None,
        error: false,
    }
}

/// The instant the source stamped on the entry, when its first record carries one in the field the
/// common formats share. Read from the verbatim record, never guessed.
fn entry_time(entry: &Entry) -> Option<String> {
    let record = entry.raw.first()?;
    ["timestamp", "time", "created_at", "createdAt"]
        .iter()
        .find_map(|key| record.get(*key).and_then(Value::as_str))
        .map(str::to_string)
}

/// The conversation as lines, in the source's order, markers left out. With `turns` above zero only
/// the last that many user turns are drawn.
pub fn transcript(session: &Session, turns: usize) -> TranscriptView {
    let entries: Vec<(usize, &Entry)> = session
        .entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| !matches!(entry.kind, Kind::Meta { .. }))
        .collect();
    let user_turns: Vec<usize> = entries
        .iter()
        .enumerate()
        .filter(|(_, (_, entry))| matches!(entry.kind, Kind::User { .. }))
        .map(|(position, _)| position)
        .collect();
    let (start, hidden) = if turns > 0 && user_turns.len() > turns {
        (user_turns[user_turns.len() - turns], user_turns.len() - turns)
    } else {
        (0, 0)
    };
    let mut rows = Vec::new();
    for &(index, entry) in &entries[start..] {
        match &entry.kind {
            Kind::User { text, attachments } => {
                rows.push(row(index, entry, "user", text));
                for attachment in attachments {
                    let path = attachment.path.as_deref().unwrap_or("");
                    rows.push(row(index, entry, "file", &format!("{} {path}", attachment.kind)));
                }
            }
            Kind::Step {
                reasoning,
                text,
                calls,
                ..
            } => {
                for block in reasoning.iter().filter(|b| !b.text.trim().is_empty()) {
                    rows.push(row(index, entry, "reasoning", &block.text));
                }
                for block in text.iter().filter(|b| !b.trim().is_empty()) {
                    rows.push(row(index, entry, "assistant", block));
                }
                for call in calls {
                    let input = serde_json::to_string(&call.input).unwrap_or_default();
                    let (input, input_truncated) = clip(&input, MAX_INPUT);
                    let mut line = row(index, entry, "tool", &call.source_name);
                    line.tool = Some(ToolView {
                        name: call.source_name.clone(),
                        call_id: call.id.clone(),
                        input,
                        input_truncated,
                    });
                    rows.push(line);
                }
            }
            Kind::Results { results } => {
                for result in results {
                    let mut line = row(index, entry, "result", &result.content.text());
                    line.error = result.is_error;
                    rows.push(line);
                }
            }
            Kind::Compaction { summary, .. } => rows.push(row(index, entry, "compaction", summary)),
            Kind::Meta { .. } => {}
        }
    }
    TranscriptView {
        meta: MetaView {
            harness: session.meta.harness.as_str(),
            version: session.meta.harness_version.clone(),
            id: session.meta.id.clone(),
            title: session.meta.title.clone(),
            cwd: session.meta.cwd.clone(),
            created: session.meta.created.clone(),
            updated: session.meta.updated.clone(),
            git_branch: session.meta.git_branch.clone(),
            models: session
                .meta
                .models
                .iter()
                .map(|model| match &model.provider {
                    Some(provider) => format!("{provider}/{}", model.id),
                    None => model.id.clone(),
                })
                .collect(),
            entries: session.entries.len(),
            branches: session.branches.len(),
        },
        rows,
        total_turns: user_turns.len(),
        hidden_turns: hidden,
    }
}

/// One entry's native records, for the raw tab.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RawItem {
    pub index: usize,
    pub entry_id: String,
    /// `user`, `step`, `results`, `compaction`, or `meta: <class>` for a marker.
    pub kind: String,
    /// The verbatim records, or one note when they are past [`MAX_RAW`].
    pub raw: Vec<Value>,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RawPage {
    pub total: usize,
    pub items: Vec<RawItem>,
}

pub const MAX_RAW: usize = 60_000;

fn kind_label(kind: &Kind) -> String {
    match kind {
        Kind::User { .. } => "user".into(),
        Kind::Step { .. } => "step".into(),
        Kind::Results { .. } => "results".into(),
        Kind::Compaction { .. } => "compaction".into(),
        Kind::Meta { kind } => format!(
            "meta: {}",
            serde_json::to_value(kind)
                .ok()
                .and_then(|value| value.as_str().map(str::to_string))
                .unwrap_or_default()
        ),
    }
}

/// `limit` entries of the session from `offset`, markers included, with their source records.
pub fn raw_page(session: &Session, offset: usize, limit: usize) -> RawPage {
    let items = session
        .entries
        .iter()
        .enumerate()
        .skip(offset)
        .take(limit)
        .map(|(index, entry)| {
            let size = serde_json::to_string(&entry.raw).map_or(0, |text| text.len());
            let truncated = size > MAX_RAW;
            RawItem {
                index,
                entry_id: entry.id.clone(),
                kind: kind_label(&entry.kind),
                raw: if truncated {
                    vec![serde_json::json!({
                        "note": "the native records of this entry are too large to show here",
                        "bytes": size,
                    })]
                } else {
                    entry.raw.clone()
                },
                truncated,
            }
        })
        .collect();
    RawPage {
        total: session.entries.len(),
        items,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scratch::Scratch;
    use crate::testing::{CLAUDE_FIXTURE, claude, claude_store};
    use mukade::roots::StoreRoots;

    #[test]
    fn a_listing_names_the_fixture_session_with_its_path() {
        let scratch = Scratch::new("listing");
        let roots = claude_store(&scratch);
        let rows = list(&claude(&roots)).expect("the store lists");
        let row = rows.iter().find(|row| row.id == CLAUDE_FIXTURE).expect("listed");
        assert!(row.path.ends_with(".jsonl"), "{}", row.path);
        assert!(!row.archived);
        assert!(row.parent.is_none());
    }

    #[test]
    fn a_store_that_is_not_there_lists_nothing_or_says_why() {
        let scratch = Scratch::new("no-store");
        let roots = StoreRoots {
            claude_config_dir: Some(scratch.join("missing")),
            ..Default::default()
        };
        // Either an empty listing or an error with the library's words: never a panic.
        match list(&claude(&roots)) {
            Ok(rows) => assert!(rows.is_empty()),
            Err(error) => assert!(!error.message.is_empty()),
        }
    }

    #[test]
    fn the_transcript_draws_conversation_in_order_and_leaves_markers_out() {
        let scratch = Scratch::new("transcript");
        let roots = claude_store(&scratch);
        let session = read(&claude(&roots), CLAUDE_FIXTURE).expect("reads");
        let view = transcript(&session, 0);
        assert_eq!(view.hidden_turns, 0);
        assert!(view.total_turns > 0);
        assert_eq!(view.rows.first().map(|row| row.role), Some("user"));
        assert!(view.rows.iter().any(|row| row.role == "assistant"));
        let conversation: usize = session.conversation().count();
        assert!(!view.rows.is_empty() && view.rows.len() >= conversation / 2);
        // Row indexes follow the entries and never decrease.
        assert!(view.rows.windows(2).all(|pair| pair[0].index <= pair[1].index));
        assert_eq!(view.meta.id, session.meta.id);
        assert_eq!(view.meta.harness, "claude_code");
    }

    #[test]
    fn a_turn_limit_hides_the_earlier_turns_and_says_how_many() {
        let scratch = Scratch::new("turns");
        let roots = claude_store(&scratch);
        let session = read(&claude(&roots), CLAUDE_FIXTURE).unwrap();
        let all = transcript(&session, 0);
        assert!(all.total_turns >= 2, "the fixture has more than one turn");
        let last = transcript(&session, 1);
        assert_eq!(last.hidden_turns, all.total_turns - 1);
        assert_eq!(last.rows.iter().filter(|row| row.role == "user").count(), 1);
        assert!(last.rows.len() < all.rows.len());
        let roomy = transcript(&session, all.total_turns + 5);
        assert_eq!(roomy.hidden_turns, 0);
        assert_eq!(roomy.rows.len(), all.rows.len());
    }

    #[test]
    fn a_tool_call_carries_its_name_and_a_bounded_input() {
        let scratch = Scratch::new("tools");
        let roots = claude_store(&scratch);
        let session = read(&claude(&roots), CLAUDE_FIXTURE).unwrap();
        let view = transcript(&session, 0);
        if let Some(call) = view.rows.iter().find(|row| row.role == "tool") {
            let tool = call.tool.as_ref().expect("a tool row has its call");
            assert_eq!(tool.name, call.text);
            assert!(tool.input.chars().count() <= MAX_INPUT);
        }
    }

    #[test]
    fn text_is_cut_on_a_character_boundary_and_flagged() {
        let (text, cut) = clip("héllo wörld", 4);
        assert_eq!(text, "héll");
        assert!(cut);
        let (text, cut) = clip("short", 50);
        assert_eq!((text.as_str(), cut), ("short", false));
        let long = "x".repeat(MAX_TEXT + 5);
        let scratch = Scratch::new("cut");
        let roots = claude_store(&scratch);
        let mut session = read(&claude(&roots), CLAUDE_FIXTURE).unwrap();
        session.entries = vec![Entry {
            id: "u1".into(),
            kind: Kind::User {
                text: long,
                attachments: Vec::new(),
            },
            raw: Vec::new(),
        }];
        let view = transcript(&session, 0);
        assert!(view.rows[0].truncated);
        assert_eq!(view.rows[0].text.chars().count(), MAX_TEXT);
        assert_eq!(view.rows[0].text_chars, MAX_TEXT + 5);
        session.entries.clear();
        assert!(transcript(&session, 0).rows.is_empty());
    }

    #[test]
    fn the_raw_page_pages_through_every_entry_with_its_native_records() {
        let scratch = Scratch::new("raw");
        let roots = claude_store(&scratch);
        let session = read(&claude(&roots), CLAUDE_FIXTURE).unwrap();
        let total = session.entries.len();
        let first = raw_page(&session, 0, 3);
        assert_eq!(first.total, total);
        assert_eq!(first.items.len(), 3.min(total));
        assert!(first.items.iter().any(|item| !item.raw.is_empty()));
        let past = raw_page(&session, total + 10, 5);
        assert!(past.items.is_empty());
        let all = raw_page(&session, 0, total);
        let kinds: Vec<&str> = all.items.iter().map(|i| i.kind.as_str()).collect();
        assert!(kinds.iter().any(|kind| *kind == "user"));
    }
}
