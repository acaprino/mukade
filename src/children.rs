//! A session's subagent sessions, found and read through the source's own reader
//! (docs/superpowers/specs/2026-11-12-subagents-design.md, phase 2).
//!
//! The reader's `list_all` names every child with its `parent`; the children of a session are the
//! sessions whose parent it is, and the children of those, parent before child. Nothing here knows
//! a harness: the readers say what a child is.

use crate::hop::{self, Compaction, HopError, SessionRef};
use crate::model::{Kind, ParentKind, Session};
use crate::reader::SessionInfo;
use crate::roots::StoreRoots;
use serde_json::{Value, json};
use std::collections::HashSet;

/// A subagent session of a source session, with how deep it is: 1 for the session's own children.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChildSession {
    pub info: SessionInfo,
    pub depth: usize,
}

/// A child read from the store: where it hangs, the call that started it, and the session, or
/// why it could not be read.
#[derive(Debug, Clone)]
pub struct ChildRead {
    pub info: SessionInfo,
    pub depth: usize,
    /// The index (in the same list) of the child this one hangs from; `None` for a child of the
    /// session the walk started from.
    pub parent: Option<usize>,
    /// The parent's tool call that started it, where the source says which.
    pub call_id: Option<String>,
    /// The session, or the reason it was not read: a session the listing already knows is
    /// unreadable, a read that failed, or a parent child that was not read.
    pub session: Result<Session, String>,
}

/// Every subagent session of `parent`, depth 1 and deeper, parent before child. The children of
/// a session are in the order of the calls that started them in it (a child whose call is unknown
/// comes after those that have one, by update time); those of a child are ordered without
/// reading it, by update time. The session is read once, the store listed once.
pub fn children_of(
    parent: &SessionRef,
    roots: &StoreRoots,
) -> Result<Vec<ChildSession>, HopError> {
    let session = hop::export(parent, roots)?;
    let listing = list_all(&session, roots)?;
    let mut out = Vec::new();
    let mut seen = HashSet::from([session.meta.id.clone()]);
    collect(
        &listing,
        ordered(&listing, &session.meta.id, Some(&session)),
        1,
        &mut seen,
        &mut out,
    );
    Ok(out)
}

fn collect(
    listing: &[SessionInfo],
    kids: Vec<(&SessionInfo, Option<String>)>,
    depth: usize,
    seen: &mut HashSet<String>,
    out: &mut Vec<ChildSession>,
) {
    for (info, _) in kids {
        if !seen.insert(info.id.clone()) {
            continue;
        }
        out.push(ChildSession {
            info: info.clone(),
            depth,
        });
        collect(listing, ordered(listing, &info.id, None), depth + 1, seen, out);
    }
}

/// Read every subagent session of `parent`: [`children_of`] with each child read, children of
/// children ordered by their parent's own calls. A child that cannot be read is in the list with
/// the reason, and so is each child below it (its parent was not read).
pub fn read_children(parent: &Session, roots: &StoreRoots) -> Result<Vec<ChildRead>, HopError> {
    let listing = list_all(parent, roots)?;
    let mut out = Vec::new();
    let mut seen = HashSet::from([parent.meta.id.clone()]);
    let kids = ordered(&listing, &parent.meta.id, Some(parent));
    walk(
        &Walk {
            listing: &listing,
            harness: parent.meta.harness,
            roots,
        },
        kids,
        None,
        None,
        1,
        &mut seen,
        &mut out,
    );
    Ok(out)
}

struct Walk<'a> {
    listing: &'a [SessionInfo],
    harness: crate::model::HarnessId,
    roots: &'a StoreRoots,
}

fn walk(
    ctx: &Walk<'_>,
    kids: Vec<(&SessionInfo, Option<String>)>,
    parent: Option<usize>,
    lost: Option<&str>,
    depth: usize,
    seen: &mut HashSet<String>,
    out: &mut Vec<ChildRead>,
) {
    for (info, call_id) in kids {
        if !seen.insert(info.id.clone()) {
            continue;
        }
        let session = match (lost, info.unreadable.as_ref()) {
            (Some(reason), _) => Err(reason.to_string()),
            (None, Some(why)) => Err(why.clone()),
            (None, None) => hop::export(&SessionRef::id(ctx.harness, info.id.clone()), ctx.roots)
                .map_err(|error| error.to_string()),
        };
        let below = ordered(ctx.listing, &info.id, session.as_ref().ok());
        let reason = session
            .as_ref()
            .err()
            .map(|why| format!("its parent subagent {} was not read: {why}", info.id));
        let index = out.len();
        out.push(ChildRead {
            info: info.clone(),
            depth,
            parent,
            call_id,
            session,
        });
        walk(ctx, below, Some(index), reason.as_deref(), depth + 1, seen, out);
    }
}

fn list_all(parent: &Session, roots: &StoreRoots) -> Result<Vec<SessionInfo>, HopError> {
    let reader = crate::reader::for_harness(parent.meta.harness)
        .map_err(|error| HopError::Unsupported(format!("{error:#}")))?;
    let _span = crate::timing::span("list subagents");
    reader.list_all(roots).map_err(hop::classify_read)
}

/// The subagent sessions whose parent is `parent_id`, each with the call that started it, in the
/// order of the calls in `parent` when it is at hand. A fork is not a subagent.
fn ordered<'a>(
    listing: &'a [SessionInfo],
    parent_id: &str,
    parent: Option<&Session>,
) -> Vec<(&'a SessionInfo, Option<String>)> {
    let position = |call: &str| {
        parent.and_then(|session| {
            session
                .entries
                .iter()
                .flat_map(|entry| match &entry.kind {
                    Kind::Step { calls, .. } => calls.iter().map(|call| call.id.as_str()).collect(),
                    _ => Vec::new(),
                })
                .position(|id| id == call)
        })
    };
    let mut kids: Vec<(&SessionInfo, Option<String>)> = listing
        .iter()
        .filter(|info| {
            info.parent.as_deref() == Some(parent_id)
                && info.id != parent_id
                && matches!(info.parent_kind, Some(ParentKind::Spawn { .. }))
        })
        .map(|info| (info, parent.and_then(|session| call_of(session, info))))
        .collect();
    kids.sort_by(|(a, a_call), (b, b_call)| {
        let key = |info: &SessionInfo, call: &Option<String>| {
            (
                call.as_deref().and_then(position).unwrap_or(usize::MAX),
                info.updated.clone(),
                info.id.clone(),
            )
        };
        key(a, a_call).cmp(&key(b, b_call))
    });
    kids
}

/// The call of `parent` that started `child`: the one its source names (Claude's `toolUseId`),
/// else the one a record of the parent links to the child's id (Codex's `SubAgentActivity`), else
/// the one whose result names it (an OpenCode `subagent` result's `sessionID`).
fn call_of(parent: &Session, child: &SessionInfo) -> Option<String> {
    if let Some(ParentKind::Spawn {
        call_id: Some(call),
        ..
    }) = &child.parent_kind
    {
        return Some(call.clone());
    }
    // A qualified id (`<parent>.<agent>`) is named by its agent part in the parent's records.
    let agent = child.id.split_once('.').map(|(_, agent)| agent);
    let names = |text: &str| text.contains(&child.id) || agent.is_some_and(|a| text.contains(a));
    for entry in &parent.entries {
        for raw in &entry.raw {
            if let Some(call) = linked_call(raw, &child.id, agent) {
                return Some(call);
            }
        }
    }
    parent.entries.iter().find_map(|entry| match &entry.kind {
        Kind::Results { results } => results
            .iter()
            .find(|result| names(&result.content.text()))
            .map(|result| result.call_id.clone()),
        _ => None,
    })
}

/// An object of a record that says `agent_thread_id` is the child, with the id of the call beside it.
fn linked_call(value: &Value, child: &str, agent: Option<&str>) -> Option<String> {
    match value {
        Value::Object(map) => {
            let thread = map.get("agent_thread_id").and_then(Value::as_str);
            let call = map.get("id").and_then(Value::as_str);
            if let (Some(thread), Some(call)) = (thread, call)
                && (thread == child || Some(thread) == agent)
            {
                return Some(call.to_string());
            }
            map.values().find_map(|inner| linked_call(inner, child, agent))
        }
        Value::Array(items) => items.iter().find_map(|inner| linked_call(inner, child, agent)),
        _ => None,
    }
}

/// The dump documents of the children `read` holds, nested as they hang: the children of the
/// session itself, each with its own `children`. What cannot be dumped is named in the second
/// value, one `<id>: <reason>` per child.
pub fn child_documents(
    read: &[ChildRead],
    roots: &StoreRoots,
) -> Result<(Vec<Value>, Vec<String>), HopError> {
    fn build(
        read: &[ChildRead],
        parent: Option<usize>,
        roots: &StoreRoots,
        lost: &mut Vec<String>,
    ) -> Result<Vec<Value>, HopError> {
        let mut docs = Vec::new();
        for (index, child) in read.iter().enumerate().filter(|(_, c)| c.parent == parent) {
            match &child.session {
                Ok(session) => {
                    let (selection, _) = hop::compact(session, &Compaction::None, roots)?;
                    let mut doc = hop::document(&selection, None);
                    doc["children"] = json!(build(read, Some(index), roots, lost)?);
                    docs.push(doc);
                }
                Err(reason) => {
                    lost.push(format!("{}: {reason}", child.info.id));
                    // What hangs below an unread child is named too, never silently gone.
                    build(read, Some(index), roots, lost)?;
                }
            }
        }
        Ok(docs)
    }
    let mut lost = Vec::new();
    let docs = build(read, None, roots, &mut lost)?;
    Ok((docs, lost))
}

/// The dump document of `session` ([`hop::document`]) with its subagent sessions in `children`,
/// each a full document of its own, nested as they hang. A child that could not be read is
/// named in `children_lost`, one `<id>: <reason>` each (the key is absent when none was lost).
pub fn document(
    session: &Session,
    summary: Option<&crate::summary::SummaryFacts>,
    roots: &StoreRoots,
) -> Result<Value, HopError> {
    let mut doc = hop::document(session, summary);
    let (children, lost) = child_documents(&read_children(session, roots)?, roots)?;
    doc["children"] = json!(children);
    if !lost.is_empty() {
        doc["children_lost"] = json!(lost);
    }
    Ok(doc)
}
