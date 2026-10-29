//! Reconstruct Claude Code's selected history without relying on write order or timestamps.

use super::{
    Class, class_of, content_text, is_command_row, is_prompt_command, truthy, with_command_context,
};
use anyhow::{Context, Result, bail, ensure};
use serde_json::Value;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};

pub(super) struct Reconstruction {
    pub order: Vec<usize>,
    pub on_branch: Vec<bool>,
}

fn text<'a>(row: &'a Value, key: &str) -> Option<&'a str> {
    row.get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
}

fn uuid(row: &Value) -> Option<&str> {
    text(row, "uuid")
}

fn main(row: &Value) -> bool {
    !truthy(row.get("isSidechain"))
}

fn conversational(row: &Value) -> bool {
    matches!(
        row.get("type").and_then(Value::as_str),
        Some("user" | "assistant")
    )
}

fn boundary(row: &Value) -> bool {
    row.get("type").and_then(Value::as_str) == Some("system")
        && row.get("subtype").and_then(Value::as_str) == Some("compact_boundary")
}

fn message_id(row: &Value) -> Option<&str> {
    row.pointer("/message/id")
        .and_then(Value::as_str)
        .filter(|id| !id.trim().is_empty())
}

fn real_prompt(row: &Value) -> bool {
    (matches!(class_of(row), Class::User)
        || (matches!(class_of(row), Class::Results)
            && content_text(row).is_some_and(|text| !text.trim().is_empty())))
        && !truthy(row.get("turnCompanion"))
        && [
            "toolUseResult",
            "sourceToolUseID",
            "sourceToolAssistantUUID",
        ]
        .iter()
        .all(|key| row.get(key).is_none())
        && !content_text(row).is_some_and(|text| {
            let text = text.trim();
            text.starts_with("[Request interrupted by user") && text.ends_with(']')
        })
}

fn expanded_prompt(row: &Value) -> bool {
    row.get("type").and_then(Value::as_str) == Some("user")
        && truthy(row.get("isMeta"))
        && !truthy(row.get("isCompactSummary"))
        && content_text(row).is_some_and(|text| !is_command_row(&text))
}

fn block_ids<'a>(
    row: &'a Value,
    kind: &'static str,
    key: &'static str,
) -> impl Iterator<Item = &'a str> {
    row.pointer("/message/content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(move |block| block.get("type").and_then(Value::as_str) == Some(kind))
        .filter_map(move |block| text(block, key))
}

struct History<'a> {
    rows: &'a [Value],
    by_uuid: HashMap<&'a str, usize>,
}

impl<'a> History<'a> {
    fn new(rows: &'a [Value]) -> Result<Self> {
        let mut by_uuid = HashMap::new();
        for (index, row) in rows.iter().enumerate() {
            if let Some(id) = uuid(row) {
                ensure!(
                    by_uuid.insert(id, index).is_none(),
                    "duplicate Claude Code uuid {id}; the history is ambiguous"
                );
            }
        }
        Ok(Self { rows, by_uuid })
    }

    fn lookup(&self, id: &str) -> Result<usize> {
        self.by_uuid
            .get(id)
            .copied()
            .with_context(|| format!("missing Claude Code parent {id}"))
    }

    /// `last-prompt` is the checkpoint. A later row supersedes it when it continues the
    /// checkpoint, or when a new user prompt starts a branch after that checkpoint.
    fn leaf(&self) -> Result<Option<usize>> {
        let mut checkpoint = None;
        let mut last = None;
        let mut continued = false;
        let mut checkpoint_position = None;
        let mut later_prompts = HashSet::new();
        for (index, row) in self.rows.iter().enumerate() {
            if row.get("type").and_then(Value::as_str) == Some("last-prompt") {
                match row.get("leafUuid") {
                    Some(Value::Null) => {
                        checkpoint = None;
                        checkpoint_position = None;
                        continued = false;
                        later_prompts.clear();
                    }
                    Some(Value::String(id)) if !id.trim().is_empty() => {
                        checkpoint = Some(id.as_str());
                        checkpoint_position = Some(index);
                        continued = false;
                        later_prompts.clear();
                    }
                    _ => {}
                }
            } else if main(row) && conversational(row) && uuid(row).is_some() {
                last = Some(index);
                continued = true;
                if checkpoint.is_some() && real_prompt(row) {
                    later_prompts.insert(index);
                }
            }
        }
        let Some(checkpoint) = checkpoint else {
            return Ok(last);
        };
        let at = self
            .by_uuid
            .get(checkpoint)
            .copied()
            .with_context(|| format!("missing Claude Code checkpoint leaf {checkpoint}"))?;
        ensure!(
            main(&self.rows[at]),
            "Claude Code checkpoint points to a sidechain"
        );
        if continued
            && let Some(last) = last
            && last != at
        {
            let path = self.walk(last)?;
            // A checkpoint flushed before its own leaf is not an ordering barrier for a
            // rewind. Only an already recorded checkpoint leaf establishes that barrier.
            let recorded_checkpoint = checkpoint_position.is_some_and(|position| at < position);
            if path.contains(&at)
                || (recorded_checkpoint && path.iter().any(|index| later_prompts.contains(index)))
            {
                return Ok(Some(last));
            }
        }
        Ok(Some(at))
    }

    fn parent(&self, at: usize) -> Result<Option<usize>> {
        let row = &self.rows[at];
        let id = if boundary(row) {
            match text(row, "logicalParentUuid") {
                Some(link) => Some(self.before_compaction(at, link)?),
                None => text(row, "parentUuid"),
            }
        } else {
            text(row, "parentUuid")
        };
        id.map(|id| self.lookup(id)).transpose()
    }

    /// An automatic compaction can name a reminder written below its own boundary. In that
    /// case the last earlier preserved row, not the reminder, precedes the boundary.
    fn before_compaction(&self, at: usize, link: &'a str) -> Result<&'a str> {
        let link_index = self.lookup(link)?;
        let mut memo = HashMap::new();
        if !self.descends_from_boundary(link_index, at, &mut memo) {
            return Ok(link);
        }
        let kept = self.preserved(at)?.unwrap_or_default();
        let end = kept.iter().position(|id| *id == link).unwrap_or(0);
        kept[..end]
            .iter()
            .rev()
            .copied()
            .find(|id| {
                self.by_uuid
                    .get(id)
                    .is_some_and(|index| !self.descends_from_boundary(*index, at, &mut memo))
            })
            .with_context(|| {
                format!(
                    "Claude Code compaction link {link} descends from its boundary without a preceding preserved row"
                )
            })
    }

    fn descends_from_boundary(
        &self,
        mut at: usize,
        ancestor: usize,
        memo: &mut HashMap<usize, bool>,
    ) -> bool {
        let mut seen = HashSet::new();
        let mut path = Vec::new();
        let result = loop {
            if at == ancestor {
                break true;
            }
            if let Some(result) = memo.get(&at) {
                break *result;
            }
            if !seen.insert(at) {
                break false;
            }
            path.push(at);
            let Some(parent) =
                text(&self.rows[at], "parentUuid").and_then(|id| self.by_uuid.get(id).copied())
            else {
                break false;
            };
            at = parent;
        };
        for index in path {
            memo.insert(index, result);
        }
        result
    }

    /// Resolve the preserved-message list, or walk the segment when no list is present.
    fn preserved(&self, at: usize) -> Result<Option<Vec<&'a str>>> {
        let Some(metadata) = self.rows[at].get("compactMetadata") else {
            return Ok(None);
        };
        if metadata.is_null() {
            return Ok(None);
        }
        ensure!(
            metadata.is_object(),
            "malformed Claude Code compactMetadata"
        );
        if let Some(list) = metadata.get("preservedMessages") {
            ensure!(list.is_object(), "malformed Claude Code preservedMessages");
            let kept = string_list(list.get("uuids"), "preservedMessages.uuids")?;
            let all = match list.get("allUuids") {
                Some(value) => string_list(Some(value), "preservedMessages.allUuids")?,
                None => Vec::new(),
            };
            let kept_set: HashSet<&str> = kept.iter().copied().collect();
            let mut seen = HashSet::new();
            let mut result = Vec::new();
            for id in all
                .into_iter()
                .filter(|id| kept_set.contains(id) || self.by_uuid.contains_key(id))
                .chain(kept)
            {
                if seen.insert(id) {
                    result.push(id);
                }
            }
            if let Some(segment) = metadata.get("preservedSegment") {
                ensure!(
                    segment.is_object(),
                    "malformed Claude Code preservedSegment"
                );
                let head = text(segment, "headUuid")
                    .context("Claude Code preservedSegment has no headUuid")?;
                let tail = text(segment, "tailUuid")
                    .context("Claude Code preservedSegment has no tailUuid")?;
                self.lookup(head)?;
                self.lookup(tail)?;
                let first = result.iter().position(|id| *id == head);
                let last = result.iter().position(|id| *id == tail);
                ensure!(
                    first.zip(last).is_some_and(|(first, last)| first <= last),
                    "Claude Code preservedSegment conflicts with preservedMessages"
                );
            }
            return Ok(Some(result));
        }
        let Some(segment) = metadata.get("preservedSegment") else {
            return Ok(None);
        };
        ensure!(
            segment.is_object(),
            "malformed Claude Code preservedSegment"
        );
        let head =
            text(segment, "headUuid").context("Claude Code preservedSegment has no headUuid")?;
        let tail =
            text(segment, "tailUuid").context("Claude Code preservedSegment has no tailUuid")?;
        let mut at = self.lookup(tail)?;
        let mut seen = HashSet::new();
        let mut result = Vec::new();
        while seen.insert(at) {
            let id = uuid(&self.rows[at]).expect("a UUID index points to a UUID");
            result.push(id);
            if id == head {
                result.reverse();
                return Ok(Some(result));
            }
            let parent = text(&self.rows[at], "parentUuid")
                .context("Claude Code preservedSegment does not resolve to its head")?;
            at = self.lookup(parent)?;
        }
        bail!("cycle in Claude Code preservedSegment")
    }

    fn walk(&self, leaf: usize) -> Result<Vec<usize>> {
        let mut chain = Vec::new();
        let mut seen = HashSet::new();
        let mut at = Some(leaf);
        while let Some(index) = at {
            ensure!(seen.insert(index), "cycle in Claude Code parent chain");
            ensure!(
                main(&self.rows[index]),
                "Claude Code sidechain on the active parent chain"
            );
            chain.push(index);
            at = self.parent(index)?;
        }
        chain.reverse();
        Ok(chain)
    }

    fn check_compactions(&self, chain: &[usize], ordered: &[usize]) -> Result<()> {
        let mut positions = vec![None; self.rows.len()];
        for (position, &index) in ordered.iter().enumerate() {
            positions[index] = Some(position);
        }
        for &index in chain {
            let row = &self.rows[index];
            if boundary(row) {
                let mut late = false;
                for id in self.preserved(index)?.unwrap_or_default() {
                    let kept = self.lookup(id)?;
                    match positions[kept] {
                        None if conversational(&self.rows[kept]) => {
                            bail!(
                                "Claude Code compaction preserves {id}, which is not on the active branch"
                            )
                        }
                        None => {}
                        Some(position)
                            if position >= positions[index].expect("boundary is ordered") =>
                        {
                            late = true
                        }
                        Some(_) if late => {
                            bail!("Claude Code compaction preserved rows are out of order")
                        }
                        Some(_) => {}
                    }
                }
            }
        }
        let mut preceding = None;
        let mut summarized = HashSet::new();
        for &index in ordered {
            let row = &self.rows[index];
            if boundary(row) {
                preceding = Some(index);
            } else if row.get("type").and_then(Value::as_str) == Some("user")
                && truthy(row.get("isCompactSummary"))
            {
                ensure!(
                    preceding.is_some_and(|previous| boundary(&self.rows[previous])),
                    "Claude Code compaction summary does not follow its boundary on the active branch"
                );
                ensure!(
                    summarized.insert(preceding.expect("a boundary was checked")),
                    "multiple Claude Code summaries for one compaction boundary"
                );
            } else if conversational(row) {
                preceding = Some(index);
            }
        }
        Ok(())
    }
}

fn string_list<'a>(value: Option<&'a Value>, name: &str) -> Result<Vec<&'a str>> {
    let items = value
        .and_then(Value::as_array)
        .with_context(|| format!("malformed Claude Code {name}"))?;
    items
        .iter()
        .map(|item| {
            item.as_str()
                .filter(|id| !id.trim().is_empty())
                .with_context(|| format!("malformed Claude Code {name}"))
        })
        .collect()
}

/// Recover blocks of active responses and unambiguous answers to their tool calls. A record with
/// no UUID can still be placed when its response ID or tool-use ID establishes its ownership.
fn recover(history: &History<'_>, chain: &[usize], selected: &mut [bool]) -> Result<()> {
    let rows = history.rows;
    let active_messages: HashSet<&str> = chain
        .iter()
        .filter_map(|&index| {
            let row = &rows[index];
            (row.get("type").and_then(Value::as_str) == Some("assistant"))
                .then(|| message_id(row))
                .flatten()
        })
        .collect();
    for (index, row) in rows.iter().enumerate() {
        if main(row)
            && row.get("type").and_then(Value::as_str) == Some("assistant")
            && message_id(row).is_some_and(|id| active_messages.contains(id))
        {
            selected[index] = true;
        }
    }

    let mut calls: HashMap<&str, usize> = HashMap::new();
    for (index, row) in rows
        .iter()
        .enumerate()
        .filter(|(index, _)| selected[*index])
    {
        if row.get("type").and_then(Value::as_str) != Some("assistant") {
            continue;
        }
        for id in block_ids(row, "tool_use", "id") {
            ensure!(
                calls.insert(id, index).is_none(),
                "duplicate Claude Code tool call id {id}"
            );
        }
    }
    let mut settled = HashSet::new();
    for (index, row) in rows.iter().enumerate() {
        if selected[index] && row.get("type").and_then(Value::as_str) == Some("user") {
            settled.extend(block_ids(row, "tool_result", "tool_use_id"));
        }
    }

    let mut candidates: HashMap<&str, Vec<usize>> = HashMap::new();
    for (index, row) in rows.iter().enumerate() {
        if selected[index] || !main(row) || row.get("type").and_then(Value::as_str) != Some("user")
        {
            continue;
        }
        let mut ids = block_ids(row, "tool_result", "tool_use_id").peekable();
        if ids.peek().is_none() {
            continue;
        }
        // Explicit conversational ancestry wins over a tool's ownership hint. The same
        // call can have results on abandoned branches; do not resurrect those results.
        if text(row, "parentUuid")
            .and_then(|id| history.by_uuid.get(id).copied())
            .is_some_and(|parent| !selected[parent] && conversational(&rows[parent]))
        {
            continue;
        }
        let links: Vec<&str> = ["parentUuid", "sourceToolAssistantUUID"]
            .into_iter()
            .filter_map(|key| text(row, key))
            .collect();
        for id in ids {
            let Some(&owner) = calls.get(id) else {
                continue;
            };
            let response = message_id(&rows[owner]);
            let linked = links.is_empty()
                || links.iter().any(|link| {
                    history.by_uuid.get(link).is_some_and(|&parent| {
                        if !selected[parent] {
                            return false;
                        }
                        if parent == owner
                            || (response.is_some() && message_id(&rows[parent]) == response)
                        {
                            return true;
                        }
                        block_ids(&rows[parent], "tool_result", "tool_use_id").any(|previous| {
                            calls.get(previous).is_some_and(|&call| {
                                response.is_some() && message_id(&rows[call]) == response
                            })
                        })
                    })
                });
            if linked && !settled.contains(id) {
                candidates.entry(id).or_default().push(index);
            }
        }
    }
    let mut recovered = HashSet::new();
    for (id, owners) in candidates {
        ensure!(
            owners.len() == 1,
            "ambiguous Claude Code results for tool call {id}"
        );
        recovered.insert(owners[0]);
    }
    for index in recovered {
        let row = &rows[index];
        let ids: Vec<&str> = block_ids(row, "tool_result", "tool_use_id").collect();
        ensure!(
            !ids.is_empty(),
            "Claude Code recovered result has no call id"
        );
        let mut response = None;
        for id in ids {
            ensure!(
                calls.contains_key(id),
                "Claude Code recovered result contains an unrelated tool call {id}"
            );
            ensure!(
                settled.insert(id),
                "duplicate Claude Code result for tool call {id}"
            );
            let owner = message_id(&rows[calls[id]]);
            if response.is_some() && response != owner {
                bail!("Claude Code recovered results belong to different responses");
            }
            response = owner;
        }
        selected[index] = true;
    }
    Ok(())
}

/// A summary without its own UUID still belongs to a boundary when it names that boundary
/// directly and no other summary occupies it. It must be inserted before the following turn.
fn recover_summaries(history: &History<'_>, chain: &[usize], selected: &mut [bool]) -> Result<()> {
    let rows = history.rows;
    let mut assigned = HashSet::new();
    let mut current_boundary = None;
    for &index in chain {
        let row = &rows[index];
        if boundary(row) {
            current_boundary = Some(index);
        } else if matches!(class_of(row), Class::Summary) {
            if let Some(parent) = current_boundary {
                assigned.insert(parent);
            }
        } else if conversational(row) {
            current_boundary = None;
        }
    }
    for (index, row) in rows.iter().enumerate() {
        if !main(row) || uuid(row).is_some() || !matches!(class_of(row), Class::Summary) {
            continue;
        }
        let Some(parent) = text(row, "parentUuid").and_then(|id| history.by_uuid.get(id).copied())
        else {
            continue;
        };
        if selected[parent] && boundary(&rows[parent]) {
            ensure!(
                assigned.insert(parent),
                "multiple Claude Code summaries for one compaction boundary"
            );
            selected[index] = true;
        }
    }
    Ok(())
}

/// An expanded slash-command prompt without a UUID is placeable when it directly names the
/// command row on the selected chain and there is no competing expansion.
fn recover_expansions(history: &History<'_>, chain: &[usize], selected: &mut [bool]) -> Result<()> {
    let rows = history.rows;
    let on_chain: HashSet<usize> = chain.iter().copied().collect();
    let mut assigned = HashSet::new();
    for &index in chain {
        if expanded_prompt(&rows[index])
            && let Some(parent) =
                text(&rows[index], "parentUuid").and_then(|id| history.by_uuid.get(id).copied())
        {
            assigned.insert(parent);
        }
    }
    for (index, row) in rows.iter().enumerate() {
        if !main(row) || uuid(row).is_some() || !expanded_prompt(row) {
            continue;
        }
        let Some(parent) = text(row, "parentUuid").and_then(|id| history.by_uuid.get(id).copied())
        else {
            continue;
        };
        if on_chain.contains(&parent)
            && matches!(
                class_of(&rows[parent]),
                Class::Meta(super::MetaKind::Command)
            )
            && content_text(&rows[parent])
                .as_deref()
                .is_some_and(is_prompt_command)
        {
            ensure!(
                !super::has_tool_result(row),
                "Claude Code slash-command expansion also contains a tool result"
            );
            ensure!(
                assigned.insert(parent),
                "multiple Claude Code expansions for one slash command"
            );
            selected[index] = true;
        }
    }
    Ok(())
}

/// Preserve bookkeeping that causally connects a recovered response to the active chain. A
/// missing parent or a conversational ancestor on another branch cannot be guessed away.
fn recover_ancestors(history: &History<'_>, chain: &[usize], selected: &mut [bool]) -> Result<()> {
    let mut on_chain = vec![false; selected.len()];
    for &index in chain {
        on_chain[index] = true;
    }
    let mut pending: Vec<usize> = selected
        .iter()
        .enumerate()
        .filter_map(|(index, &on)| (on && !on_chain[index]).then_some(index))
        .collect();
    while let Some(index) = pending.pop() {
        let Some(parent) = text(&history.rows[index], "parentUuid") else {
            continue;
        };
        let parent = history.lookup(parent)?;
        ensure!(
            main(&history.rows[parent]),
            "Claude Code recovered record belongs to a sidechain"
        );
        if selected[parent] {
            continue;
        }
        ensure!(
            !conversational(&history.rows[parent]) && !boundary(&history.rows[parent]),
            "Claude Code recovered record depends on another conversation branch"
        );
        selected[parent] = true;
        pending.push(parent);
    }
    Ok(())
}

fn edge(edges: &mut Vec<(usize, usize)>, before: usize, after: usize) {
    edges.push((before, after));
}

/// Topological order with file position only as a deterministic tie-breaker for independent rows.
fn topo(selected: &[bool], edges: impl IntoIterator<Item = (usize, usize)>) -> Option<Vec<usize>> {
    let mut outgoing = vec![Vec::new(); selected.len()];
    let mut degree = vec![0usize; selected.len()];
    let mut unique = HashSet::new();
    for (before, after) in edges {
        if unique.insert((before, after)) {
            outgoing[before].push(after);
            degree[after] += 1;
        }
    }
    let mut ready = BinaryHeap::new();
    for (index, &included) in selected.iter().enumerate() {
        if included && degree[index] == 0 {
            ready.push(Reverse(index));
        }
    }
    let mut ordered = Vec::new();
    while let Some(Reverse(index)) = ready.pop() {
        ordered.push(index);
        for &next in &outgoing[index] {
            degree[next] -= 1;
            if degree[next] == 0 {
                ready.push(Reverse(next));
            }
        }
    }
    (ordered.len() == selected.iter().filter(|on| **on).count()).then_some(ordered)
}

fn order(history: &History<'_>, chain: &[usize], selected: &[bool]) -> Result<Vec<usize>> {
    let rows = history.rows;
    let mut hard = Vec::new();
    for pair in chain.windows(2) {
        edge(&mut hard, pair[0], pair[1]);
    }
    let mut calls = HashMap::new();
    let mut responses: HashMap<&str, Vec<usize>> = HashMap::new();
    for (index, row) in rows
        .iter()
        .enumerate()
        .filter(|(index, _)| selected[*index])
    {
        if !boundary(row)
            && let Some(parent) =
                text(row, "parentUuid").and_then(|id| history.by_uuid.get(id).copied())
            && selected[parent]
        {
            edge(&mut hard, parent, index);
        }
        if row.get("type").and_then(Value::as_str) == Some("assistant") {
            if let Some(id) = message_id(row) {
                responses.entry(id).or_default().push(index);
            }
            for id in block_ids(row, "tool_use", "id") {
                calls.insert(id, index);
            }
        }
    }
    let mut results: HashMap<&str, Vec<usize>> = HashMap::new();
    for (index, row) in rows
        .iter()
        .enumerate()
        .filter(|(index, _)| selected[*index])
    {
        if row.get("type").and_then(Value::as_str) != Some("user") {
            continue;
        }
        for id in block_ids(row, "tool_result", "tool_use_id") {
            if let Some(&call) = calls.get(id) {
                edge(&mut hard, call, index);
                if let Some(message) = message_id(&rows[call]) {
                    results.entry(message).or_default().push(index);
                }
            }
        }
    }
    let mut positions = vec![None; rows.len()];
    for (position, &index) in chain.iter().enumerate() {
        positions[index] = Some(position);
    }
    for (index, row) in rows
        .iter()
        .enumerate()
        .filter(|(index, _)| selected[*index])
    {
        if positions[index].is_some()
            || !(matches!(class_of(row), Class::Summary) || expanded_prompt(row))
        {
            continue;
        }
        let Some(parent) = text(row, "parentUuid").and_then(|id| history.by_uuid.get(id).copied())
        else {
            continue;
        };
        let Some(at) = positions[parent] else {
            continue;
        };
        if let Some(&following) = chain[at + 1..]
            .iter()
            .find(|&&next| conversational(&rows[next]) || boundary(&rows[next]))
        {
            edge(&mut hard, index, following);
        }
    }
    let mut following = vec![None; chain.len()];
    let mut next = None;
    for (position, &index) in chain.iter().enumerate().rev() {
        following[position] = next;
        if matches!(
            class_of(&rows[index]),
            Class::Assistant(_) | Class::User | Class::Boundary
        ) {
            next = Some(index);
        }
    }
    let mut soft = Vec::new();
    for (message, members) in responses {
        let anchors: Vec<usize> = members
            .iter()
            .filter_map(|&index| positions[index])
            .collect();
        let Some(&first) = anchors.iter().min() else {
            continue;
        };
        let last = *anchors.iter().max().expect("an anchor exists");
        let before = first.checked_sub(1).map(|at| chain[at]);
        let after = following[last];
        for &member in &members {
            if let Some(before) = before
                && (positions[before].is_none() || positions[member].is_none())
            {
                edge(&mut soft, before, member);
            }
            if let Some(after) = after
                && (positions[member].is_none() || positions[after].is_none())
            {
                edge(&mut soft, member, after);
            }
        }
        if let Some(after) = after {
            for &result in results.get(message).into_iter().flatten() {
                if positions[result].is_none() || positions[after].is_none() {
                    edge(&mut soft, result, after);
                }
            }
        }
    }
    topo(selected, hard.iter().chain(&soft).copied())
        .or_else(|| topo(selected, hard.iter().copied()))
        .context("cycle in reconstructed Claude Code history")
}

fn validate(row: &Value) -> Result<()> {
    match class_of(row) {
        Class::Assistant(_) => {
            ensure!(
                message_id(row).is_some(),
                "Claude Code assistant has no message.id"
            );
            let blocks = row
                .pointer("/message/content")
                .and_then(Value::as_array)
                .context("Claude Code assistant content is not a list")?;
            for block in blocks {
                match block.get("type").and_then(Value::as_str) {
                    Some("text") if block.get("text").is_some_and(Value::is_string) => {}
                    Some("thinking") if block.get("thinking").is_some_and(Value::is_string) => {}
                    Some("redacted_thinking")
                        if block.get("data").is_some_and(Value::is_string) => {}
                    Some("tool_use")
                        if text(block, "id").is_some()
                            && text(block, "name").is_some()
                            && block.get("input").is_some_and(Value::is_object) => {}
                    _ => bail!("malformed or unsupported Claude Code assistant content block"),
                }
            }
        }
        Class::User | Class::Results | Class::Summary => {
            let content = row
                .pointer("/message/content")
                .context("Claude Code user has no content")?;
            ensure!(
                content.is_string() || content.is_array(),
                "malformed Claude Code user content"
            );
            if let Some(blocks) = content.as_array() {
                for block in blocks {
                    let kind =
                        text(block, "type").context("malformed Claude Code user content block")?;
                    if kind == "tool_result" {
                        ensure!(
                            text(block, "tool_use_id").is_some(),
                            "Claude Code tool result has no call id"
                        );
                    }
                }
            }
        }
        Class::Boundary | Class::Meta(_) => {}
    }
    Ok(())
}

fn validate_responses(rows: &[Value], ordered: &[usize], check_results: bool) -> Result<()> {
    let mut seen = HashSet::new();
    let mut current: Option<&str> = None;
    let mut after_prompt_command = false;
    let mut has_prompt = false;
    let mut calls = HashMap::new();
    if check_results {
        for &index in ordered {
            let row = &rows[index];
            if let Some(response) = message_id(row) {
                for id in block_ids(row, "tool_use", "id") {
                    calls.insert(id, response);
                }
            }
        }
    }
    for &index in ordered {
        let row = &rows[index];
        let class = class_of(row);
        let text = matches!(class, Class::Meta(super::MetaKind::Command))
            .then(|| content_text(row))
            .flatten();
        let class = with_command_context(row, class, text.as_deref(), &mut after_prompt_command);
        match class {
            Class::Assistant(_) => {
                ensure!(
                    !check_results || has_prompt,
                    "Claude Code assistant response precedes the first prompt"
                );
                let Some(id) = message_id(&rows[index]) else {
                    continue;
                };
                if current != Some(id) {
                    ensure!(
                        seen.insert(id),
                        "Claude Code response {id} is split around another message"
                    );
                    current = Some(id);
                }
            }
            Class::User => {
                has_prompt = true;
                current = None;
            }
            Class::Summary => {
                has_prompt |= content_text(row).is_some_and(|text| !text.trim().is_empty());
                current = None;
            }
            Class::Boundary => current = None,
            Class::Results => {
                ensure!(
                    !check_results || has_prompt,
                    "Claude Code tool result precedes the first prompt"
                );
                for id in block_ids(&rows[index], "tool_result", "tool_use_id") {
                    if let Some(&response) = calls.get(id) {
                        ensure!(
                            current == Some(response),
                            "Claude Code result {id} crosses another conversation message"
                        );
                    }
                }
                if content_text(&rows[index]).is_some() {
                    current = None;
                }
            }
            Class::Meta(_) => {}
        }
    }
    Ok(())
}

pub(super) fn reconstruct(rows: &[Value]) -> Result<Reconstruction> {
    let history = History::new(rows)?;
    let Some(leaf) = history.leaf()? else {
        ensure!(
            !rows.iter().any(|row| main(row)
                && conversational(row)
                && !matches!(class_of(row), Class::Meta(_))),
            "Claude Code conversation has no reconstructible UUID chain"
        );
        return Ok(Reconstruction {
            order: (0..rows.len()).collect(),
            on_branch: vec![true; rows.len()],
        });
    };
    let chain = history.walk(leaf)?;
    let mut selected = vec![false; rows.len()];
    for &index in &chain {
        selected[index] = true;
    }
    recover(&history, &chain, &mut selected)?;
    recover_summaries(&history, &chain, &mut selected)?;
    recover_expansions(&history, &chain, &mut selected)?;
    recover_ancestors(&history, &chain, &mut selected)?;
    let mut unchained_off_branch = vec![false; rows.len()];
    for (index, row) in rows.iter().enumerate() {
        if main(row)
            && uuid(row).is_none()
            && conversational(row)
            && !selected[index]
            && !matches!(class_of(row), Class::Meta(_))
        {
            let links: Vec<&str> = ["parentUuid", "sourceToolAssistantUUID"]
                .into_iter()
                .filter_map(|key| text(row, key))
                .collect();
            if !links.is_empty()
                && links.iter().all(|id| {
                    history
                        .by_uuid
                        .get(id)
                        .is_some_and(|&parent| !selected[parent])
                })
            {
                unchained_off_branch[index] = true;
            } else {
                bail!(
                    "cannot place unchained Claude Code conversation record {}",
                    index + 1
                );
            }
        }
    }
    let ordered = order(&history, &chain, &selected)?;
    history.check_compactions(&chain, &ordered)?;
    let physical: Vec<usize> = selected
        .iter()
        .enumerate()
        .filter_map(|(index, &on)| on.then_some(index))
        .collect();
    let reordered = ordered != physical;
    let recovered = selected.iter().filter(|on| **on).count() != chain.len();
    validate_responses(rows, &ordered, reordered || recovered)?;
    if reordered || recovered {
        for &index in &ordered {
            validate(&rows[index]).with_context(|| format!("Claude Code record {}", index + 1))?;
        }
        // An unchained expanded prompt cannot be assigned from adjacency alone. A distinct
        // linked expansion can establish that an adjacent metadata row is not the prompt.
        let expansion_parents: HashSet<&str> = rows
            .iter()
            .enumerate()
            .filter_map(|(index, row)| {
                (selected[index] && expanded_prompt(row))
                    .then(|| text(row, "parentUuid"))
                    .flatten()
            })
            .collect();
        if rows.iter().enumerate().any(|(index, row)| {
            index > 0
                && !selected[index]
                && uuid(row).is_none()
                && truthy(row.get("isMeta"))
                && content_text(row)
                    .as_deref()
                    .is_some_and(|text| !is_command_row(text))
                && content_text(&rows[index - 1])
                    .as_deref()
                    .is_some_and(is_prompt_command)
                && !uuid(&rows[index - 1]).is_some_and(|id| expansion_parents.contains(id))
        }) {
            bail!("cannot place an unchained Claude Code slash-command expansion safely");
        }
    }
    let mut ordered = ordered.into_iter();
    let order = (0..rows.len())
        .map(|index| {
            if selected[index] {
                ordered.next().expect("one selected row per slot")
            } else {
                index
            }
        })
        .collect();
    let on_branch = rows
        .iter()
        .enumerate()
        .map(|(index, row)| {
            selected[index]
                || !main(row)
                || (uuid(row).is_none()
                    && !unchained_off_branch[index]
                    && !((reordered || recovered)
                        && matches!(class_of(row), Class::Meta(super::MetaKind::Command))))
        })
        .collect();
    Ok(Reconstruction { order, on_branch })
}
