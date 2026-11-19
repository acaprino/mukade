//! A session's subagent sessions, found and read through the source's own reader
//! (docs/superpowers/specs/2026-11-12-subagents-design.md, phase 2).
//!
//! The reader's `list_all` names every child with its `parent`; the children of a session are the
//! sessions whose parent it is, and the children of those, parent before child. Nothing here knows
//! a harness: the readers say what a child is.

use crate::hop::{
    self, ChildrenDone, Compaction, HopError, ImportOpts, Receipt, SessionRef, SourceFacts,
    WriteDetails,
};
use crate::model::{HarnessId, Kind, ParentKind, Session};
use crate::reader::SessionInfo;
use crate::roots::StoreRoots;
use crate::writer::{ChildOf, Subagents, Written};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

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
pub fn children_of(parent: &SessionRef, roots: &StoreRoots) -> Result<Vec<ChildSession>, HopError> {
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
        collect(
            listing,
            ordered(listing, &info.id, None),
            depth + 1,
            seen,
            out,
        );
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
        walk(
            ctx,
            below,
            Some(index),
            reason.as_deref(),
            depth + 1,
            seen,
            out,
        );
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
            map.values()
                .find_map(|inner| linked_call(inner, child, agent))
        }
        Value::Array(items) => items
            .iter()
            .find_map(|inner| linked_call(inner, child, agent)),
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

/// The agent a child ran as and whether it starts from its parent's conversation. The rollout or
/// transcript that was read says it; the listing's row is the fallback (a Codex state database
/// knows a spawn edge, not a fork).
fn spawn_facts(session: &Session, info: &SessionInfo) -> (Option<String>, bool) {
    let spawn = |kind: &Option<ParentKind>| match kind {
        Some(ParentKind::Spawn {
            agent,
            inherits_context,
            ..
        }) => Some((agent.clone(), *inherits_context)),
        _ => None,
    };
    let read = spawn(&session.meta.parent_kind);
    let listed = spawn(&info.parent_kind);
    let agent = read
        .as_ref()
        .and_then(|(agent, _)| agent.clone())
        .or_else(|| listed.as_ref().and_then(|(agent, _)| agent.clone()));
    let inherits =
        read.is_some_and(|(_, inherits)| inherits) || listed.is_some_and(|(_, inherits)| inherits);
    (agent, inherits)
}

/// How a child is written: its parent's compaction when that is deterministic (`native`), else
/// whole. A summary the source's model wrote, or one the caller supplied, belongs to the
/// parent's history alone.
fn child_compaction(opts: &ImportOpts) -> Compaction {
    match opts.native_budget.clone() {
        Some(budget) if opts.compaction == "native" => Compaction::Native(budget),
        _ => Compaction::None,
    }
}

/// Write `selected` into `dest` and, after it, each subagent session of `source` as a session of
/// its own, with the same options and the link the destination has: a parent link for OpenCode,
/// the parent's `subagents/` folder for Claude Code, none for the others. The parent's text-form
/// spawn calls name the destination session of their child. `--dest-id`, `--dest-path`, the
/// session metadata and the resume prompt are the parent's alone.
///
/// A child that cannot be read is a loss in the manifest, and the hop goes on. A child that
/// cannot be written stops the children there: the parent stays written and the error carries its
/// receipt ([`HopError::ChildWriteFailed`]).
pub fn import_with_children(
    dest: HarnessId,
    selected: &Session,
    source: &Session,
    source_roots: &StoreRoots,
    opts: &ImportOpts,
) -> Result<Receipt, HopError> {
    let read = read_children(source, source_roots)?;
    import_children(dest, selected, read, Vec::new(), source_roots, opts)
}

/// The children a dump document holds (`children`, nested as they hang), as [`read_children`]
/// gives a store's: parent before child, each with its depth, the child it hangs from and the
/// call that started it. A child document that cannot be read is in the list with the reason, and
/// so is each child below it.
pub fn children_of_document(doc: &Value) -> Vec<ChildRead> {
    fn walk(
        docs: &[Value],
        parent: Option<usize>,
        lost: Option<&str>,
        depth: usize,
        out: &mut Vec<ChildRead>,
    ) {
        for doc in docs {
            let session = match lost {
                Some(reason) => Err(reason.to_string()),
                None => crate::wire::from_document(doc).map_err(|error| format!("{error:#}")),
            };
            let meta = &doc["meta"];
            let text = |key: &str| meta[key].as_str().map(str::to_owned);
            let parent_kind: Option<ParentKind> =
                serde_json::from_value(meta["parent_kind"].clone()).ok();
            let call_id = match &parent_kind {
                Some(ParentKind::Spawn { call_id, .. }) => call_id.clone(),
                _ => None,
            };
            let info = SessionInfo {
                id: text("id").unwrap_or_default(),
                title: text("title"),
                cwd: text("cwd"),
                updated: text("updated"),
                parent: text("parent"),
                parent_kind,
                ..SessionInfo::default()
            };
            let below = session.as_ref().err().map(|why| {
                format!(
                    "its parent subagent {} was not read: {why}",
                    if info.id.is_empty() { "?" } else { &info.id }
                )
            });
            let index = out.len();
            out.push(ChildRead {
                info,
                depth,
                parent,
                call_id,
                session,
            });
            let kids = doc["children"].as_array().cloned().unwrap_or_default();
            walk(&kids, Some(index), below.as_deref(), depth + 1, out);
        }
    }
    let mut out = Vec::new();
    let docs = doc["children"].as_array().cloned().unwrap_or_default();
    walk(&docs, None, None, 1, &mut out);
    out
}

/// Write a dump document's session into `dest` and, after it, the children it holds, as
/// [`import_with_children`] writes a store's ([`crate::hop::import_document`] for the session).
/// The children `dump` could not read (`children_lost`) are losses of this write too.
pub fn import_document_with_children(
    dest: HarnessId,
    doc: &Value,
    session: &Session,
    opts: &ImportOpts,
) -> Result<Receipt, HopError> {
    let opts = hop::document_opts(doc, opts);
    let lost: Vec<String> = doc["children_lost"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(|line| format!("child {line}"))
        .collect();
    import_children(
        dest,
        session,
        children_of_document(doc),
        lost,
        &StoreRoots::default(),
        &opts,
    )
}

/// [`import_with_children`] for children already read: `read` as [`read_children`] gives them,
/// `lost` the losses known before (children the source could not hand over).
fn import_children(
    dest: HarnessId,
    selected: &Session,
    read: Vec<ChildRead>,
    lost: Vec<String>,
    source_roots: &StoreRoots,
    opts: &ImportOpts,
) -> Result<Receipt, HopError> {
    let destination = crate::harnesses::destination(dest)?;
    let writer = destination.writer();
    let compaction = child_compaction(opts);

    // Every id is chosen before the parent is written: its text names its children, and a Claude
    // child's id holds the parent's.
    let root_id = match (&opts.write.dest_id, writer.child_ids_hold_root()) {
        (Some(id), _) => Some(id.clone()),
        (None, true) => Some(writer.new_id()),
        (None, false) => None,
    };
    let ids: Vec<Option<String>> = read
        .iter()
        .map(|child| {
            child
                .session
                .is_ok()
                .then(|| writer.child_id(root_id.as_deref().unwrap_or_default()))
        })
        .collect();
    let spawned = |parent: Option<usize>| -> Vec<(String, String)> {
        read.iter()
            .zip(&ids)
            .filter(|(child, _)| child.parent == parent)
            .filter_map(|(child, id)| Some((child.call_id.clone()?, id.clone()?)))
            .collect()
    };
    let mut parent_opts = opts.clone();
    if ids.iter().any(Option::is_some) {
        parent_opts.write.dest_id = root_id.clone();
    }

    let mut step = |written: &Written, calls: &HashMap<String, String>| -> ChildrenDone {
        let mut done = ChildrenDone {
            losses: lost.clone(),
            ..ChildrenDone::default()
        };
        // Per child: its destination session and the destination call each of its own spawn
        // calls became.
        let mut landed: Vec<Option<(String, HashMap<String, String>)>> = vec![None; read.len()];
        for (index, child) in read.iter().enumerate() {
            let source_id = &child.info.id;
            let session = match &child.session {
                Ok(session) => session,
                Err(reason) => {
                    done.losses.push(format!("child {source_id}: {reason}"));
                    continue;
                }
            };
            if done.failure.is_some() {
                done.losses.push(format!(
                    "child {source_id}: not written, an earlier child failed"
                ));
                continue;
            }
            let (parent_id, parent_calls) = match child.parent {
                None => (written.id.clone(), calls),
                Some(parent) => match &landed[parent] {
                    Some((id, parent_calls)) => (id.clone(), parent_calls),
                    // Its parent was not written, which stopped the children before this one.
                    None => continue,
                },
            };
            let id = ids[index].clone().expect("a readable child has an id");
            let spawn = spawn_facts(session, &child.info);
            let of = ChildOf {
                root_id: written.id.clone(),
                root_location: written.location.clone(),
                parent_id,
                depth: child.depth,
                agent: spawn.0.clone(),
                description: session.meta.title.clone().or(child.info.title.clone()),
                tool_use_id: child
                    .call_id
                    .as_ref()
                    .and_then(|call| parent_calls.get(call))
                    .cloned(),
            };
            let child_opts = ImportOpts {
                cwd: opts.cwd.clone(),
                roots: opts.roots.clone(),
                reasoning: opts.reasoning,
                write: WriteDetails {
                    dest_id: Some(id.clone()),
                    dest_path: None,
                    metadata: None,
                    ..opts.write.clone()
                },
                resume: None,
                source_facts: SourceFacts::of(session),
                compaction: compaction.name().to_string(),
                summary: None,
                native_budget: match &compaction {
                    Compaction::Native(budget) => Some(budget.clone()),
                    _ => None,
                },
            };
            let result =
                hop::compact(session, &compaction, source_roots).and_then(|(selection, _)| {
                    hop::import_inner(
                        dest,
                        &selection,
                        &child_opts,
                        &Subagents {
                            spawned: spawned(Some(index)),
                            of: Some(of),
                            assigned_id: true,
                        },
                        None,
                    )
                });
            match result {
                Ok(imported) => {
                    let mut entry = json!({
                        "source_id": source_id,
                        "dest_id": imported.written.id,
                        "dest_path": imported.written.location,
                        "link": if destination.links_children() { "native" } else { "none" },
                        "depth": child.depth,
                        "compaction": compaction.name(),
                    });
                    if spawn.1 {
                        entry["inherits_context"] = json!(true);
                    }
                    done.entries.push(entry);
                    for loss in imported.receipt.manifest["losses"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                    {
                        done.losses.push(format!("child {source_id}: {loss}"));
                    }
                    landed[index] = Some((imported.written.id, imported.spawn_calls));
                }
                Err(error) => {
                    done.losses
                        .push(format!("child {source_id}: not written: {error}"));
                    done.failure = Some(format!("child {source_id}: {error}"));
                }
            }
        }
        done
    };
    hop::import_inner(
        dest,
        selected,
        &parent_opts,
        &Subagents {
            spawned: spawned(None),
            of: None,
            assigned_id: opts.write.dest_id.is_none(),
        },
        Some(&mut step),
    )
    .map(|imported| imported.receipt)
}
