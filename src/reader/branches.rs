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
/// path. A record that `attaches(child, parent)` to its parent (another block of the same response,
/// the result of a call its parent made) belongs with it and never starts a branch; of the others
/// below a chain, the first in the file goes on with it and each further one starts a branch of its
/// own. Every branch hangs off the conversation entry that holds its nearest ancestor record.
/// `build` turns a chain's records, in file order, into entries the way the reader turns the
/// records of the resumed path (steps merged, results paired).
pub(crate) fn lift<A, F>(
    session: &mut Session,
    id_key: &str,
    parent_key: &str,
    attaches: A,
    mut build: F,
) -> Result<()>
where
    A: Fn(&Value, &Value) -> bool,
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
    // Which conversation entry holds a record, and every record's parent, so a branch whose parent
    // is a record no conversation entry holds (bookkeeping, a merged block) walks up to one that
    // does.
    let conversation = |entry: &Entry| !matches!(entry.kind, Kind::Meta { .. });
    let mut holder: HashMap<String, String> = HashMap::new();
    let mut parent_of: HashMap<String, String> = HashMap::new();
    for entry in &session.entries {
        for row in &entry.raw {
            let Some(id) = key(row, id_key) else {
                continue;
            };
            if let Some(parent) = key(row, parent_key) {
                parent_of.insert(id.to_string(), parent.to_string());
            }
            if conversation(entry) {
                holder.insert(id.to_string(), entry.id.clone());
            }
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
    let parent_record = |n: usize| key(row_of(off[n]), parent_key);
    // The tree of the abandoned records. A bookkeeping record between two of them (an attachment
    // row, kept as Meta of its own kind) is passed through: the record below it hangs from the
    // nearest record above that is abandoned or on the path.
    let tree_parent = |n: usize| {
        let mut seen: HashSet<&str> = HashSet::new();
        let mut current = parent_record(n);
        while let Some(id) = current {
            if at.contains_key(id) || holder.contains_key(id) || !seen.insert(id) {
                return Some(id);
            }
            current = parent_of.get(id).map(String::as_str);
        }
        None
    };
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); off.len()];
    let mut roots: Vec<usize> = Vec::new();
    for n in 0..off.len() {
        match tree_parent(n).and_then(|parent| at.get(parent)) {
            Some(&parent) if parent != n => children[parent].push(n),
            _ => roots.push(n),
        }
    }

    // Chains, parents before the branches that hang off them.
    let mut pending: std::collections::VecDeque<(usize, Option<String>)> = roots
        .into_iter()
        .map(|n| (n, leaves_from(parent_record(n), &holder)))
        .collect();
    let mut branches = Vec::new();
    let mut lifted = HashSet::new();
    while let Some((start, from)) = pending.pop_front() {
        let mut chain: Vec<usize> = Vec::new();
        let mut side: Vec<usize> = Vec::new();
        let mut head = Some(start);
        while let Some(first) = head.take() {
            // `first` and every record that attaches below it, then what goes on from them.
            let mut group = vec![first];
            let mut onward: Vec<usize> = Vec::new();
            let mut k = 0;
            while k < group.len() {
                let node = group[k];
                k += 1;
                for &child in &children[node] {
                    if attaches(row_of(off[child]), row_of(off[node])) {
                        group.push(child);
                    } else {
                        onward.push(child);
                    }
                }
            }
            chain.extend(group);
            onward.sort_unstable();
            let mut onward = onward.into_iter();
            head = onward.next();
            side.extend(onward);
        }
        chain.sort_unstable();
        let rows: Vec<Value> = chain.iter().map(|&n| row_of(off[n]).clone()).collect();
        let entries = build(rows)?;
        for entry in entries.iter().filter(|entry| conversation(entry)) {
            for row in &entry.raw {
                if let Some(id) = key(row, id_key) {
                    holder.insert(id.to_string(), entry.id.clone());
                }
            }
        }
        lifted.extend(chain.iter().map(|&n| off[n]));
        for other in side {
            pending.push_back((other, leaves_from(parent_record(other), &holder)));
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
