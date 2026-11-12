//! Lifting the abandoned branches of a tree-shaped session out of its off-branch records.
//!
//! Claude Code and Pi keep a session as a tree: every record names its parent (`parentUuid`,
//! `parentId`) and the harness resumes the path to the last record. A reader that is not asked
//! for the branches keeps the records of every other path as [`MetaKind::OffBranch`] entries.
//! Asked for them (`--all-branches`), it turns those records into [`Branch`]es: runs of entries
//! parsed with the reader's own conversation parser, each hanging off the entry it left.

use crate::model::{Branch, Entry, Kind, MetaKind, Session};
use anyhow::Result;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// The records of a row that name it and its parent.
fn key<'a>(row: &'a Value, name: &str) -> Option<&'a str> {
    row.get(name)
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
}

/// Move the off-branch conversation records of `session` into `session.branches`.
///
/// A record is a node of the tree when its `id_key` names it; its parent is `parent_key`. A branch
/// is a chain of those records: it starts at a record whose parent is not itself off the active
/// path, and follows the first child at each step; the other children start branches of their
/// own, hung off the entry that holds their parent. `build` turns a chain's records into entries
/// the way the reader turns the records of the resumed path (steps merged, results paired).
pub(crate) fn lift<F>(
    session: &mut Session,
    id_key: &str,
    parent_key: &str,
    mut build: F,
) -> Result<()>
where
    F: FnMut(Vec<Value>) -> Result<Vec<Entry>>,
{
    let is_off = |entry: &Entry| {
        matches!(
            entry.kind,
            Kind::Meta {
                kind: MetaKind::OffBranch
            }
        ) && entry.raw.len() == 1
            && key(&entry.raw[0], id_key).is_some()
    };
    // The abandoned records, in file order, and what hangs below each.
    let off: Vec<usize> = (0..session.entries.len())
        .filter(|&i| is_off(&session.entries[i]))
        .collect();
    if off.is_empty() {
        return Ok(());
    }
    let row_of = |i: usize| &session.entries[i].raw[0];
    let at: HashMap<&str, usize> = off
        .iter()
        .enumerate()
        .filter_map(|(n, &i)| Some((key(row_of(i), id_key)?, n)))
        .collect();
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); off.len()];
    let mut roots: Vec<usize> = Vec::new();
    for (n, &i) in off.iter().enumerate() {
        match key(row_of(i), parent_key).and_then(|parent| at.get(parent)) {
            Some(&parent) if parent != n => children[parent].push(n),
            _ => roots.push(n),
        }
    }

    // Where a record of the path sits: the entry that holds it, and its parent record, so a branch
    // whose parent is a record the entries do not hold (bookkeeping) walks up to one they do.
    let mut holder: HashMap<String, String> = HashMap::new();
    let mut parent_of: HashMap<String, String> = HashMap::new();
    for (i, entry) in session.entries.iter().enumerate() {
        if off.binary_search(&i).is_ok() {
            continue;
        }
        for row in &entry.raw {
            if let Some(id) = key(row, id_key) {
                holder.insert(id.to_string(), entry.id.clone());
                if let Some(parent) = key(row, parent_key) {
                    parent_of.insert(id.to_string(), parent.to_string());
                }
            }
        }
    }
    for &i in &off {
        let row = row_of(i);
        if let (Some(id), Some(parent)) = (key(row, id_key), key(row, parent_key)) {
            parent_of.insert(id.to_string(), parent.to_string());
        }
    }
    let leaves_from = |parent: Option<&str>, holder: &HashMap<String, String>| {
        let mut seen: HashSet<String> = HashSet::new();
        let mut current: Option<String> = parent.map(str::to_owned);
        while let Some(id) = current {
            if let Some(entry) = holder.get(&id) {
                return Some(entry.clone());
            }
            if !seen.insert(id.clone()) {
                return None;
            }
            current = parent_of.get(&id).cloned();
        }
        None
    };

    // Chains, parents before the branches that hang off them.
    let mut pending: std::collections::VecDeque<(usize, Option<String>)> = roots
        .into_iter()
        .map(|n| {
            let parent = key(row_of(off[n]), parent_key);
            (n, leaves_from(parent, &holder))
        })
        .collect();
    let mut branches = Vec::new();
    let mut lifted = HashSet::new();
    while let Some((start, from)) = pending.pop_front() {
        let mut chain = vec![start];
        let mut side: Vec<(usize, usize)> = Vec::new();
        let mut node = start;
        while let Some((first, rest)) = children[node].split_first() {
            side.extend(rest.iter().map(|&other| (node, other)));
            chain.push(*first);
            node = *first;
        }
        let rows: Vec<Value> = chain.iter().map(|&n| row_of(off[n]).clone()).collect();
        let entries = build(rows)?;
        for entry in &entries {
            for row in &entry.raw {
                if let Some(id) = key(row, id_key) {
                    holder.insert(id.to_string(), entry.id.clone());
                }
            }
        }
        lifted.extend(chain.iter().map(|&n| off[n]));
        for (parent, other) in side {
            let parent_id = key(row_of(off[parent]), id_key);
            let from = parent_id.and_then(|id| holder.get(id).cloned());
            pending.push_back((other, from));
        }
        branches.push(Branch { from, entries });
    }
    let mut keep = 0usize;
    session.entries.retain(|_| {
        keep += 1;
        !lifted.contains(&(keep - 1))
    });
    session.branches.extend(branches);
    Ok(())
}
