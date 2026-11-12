//! The hop registry: what mukade wrote where (docs/superpowers/specs/2026-11-12-hop-registry-design.md).
//!
//! Each written session is one record under the session it came from: `mukade-data/registry/
//! <source harness>/<hash of the source id>.json`, a JSON array, newest last. A record names the
//! source (harness, id, fingerprint), the destination (harness, store, id, location), and, for each
//! block of the plan that was written, a fingerprint of the block and the ids of the records the
//! writer wrote for it. A later hop of a fork finds its origin's copy there and links to it; a hop
//! of the same unchanged session says it was written before.
//!
//! Like the cache it is best effort: a registry that cannot be read is empty, one that cannot be
//! written is a warning, never a failed hop. `MUKADE_REGISTRY=off` turns it off. A record is used
//! only while its destination session is still where it was written.

use crate::model::HarnessId;
use crate::writer::plan::Plan;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The variable that turns the registry off (`off`, `0` or `false`).
pub const ENV: &str = "MUKADE_REGISTRY";
/// Past this many records for one source session, the oldest go.
const MAX_RECORDS: usize = 32;

/// One written session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    /// When it was written (RFC 3339).
    pub time: String,
    pub source: Source,
    pub dest: Dest,
    /// `session`, or `child` for a subagent session written with its parent.
    #[serde(rename = "as")]
    pub written_as: String,
    /// The planned blocks of the active path, in order.
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    pub harness: String,
    pub id: String,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dest {
    pub harness: String,
    /// The destination store the hop wrote to (its root), empty when the destination's own
    /// default was used.
    pub store: String,
    pub id: String,
    pub location: String,
}

/// One planned block: its fingerprint, and the ids of the records written for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Block {
    #[serde(rename = "h")]
    pub hash: String,
    #[serde(rename = "r", default, skip_serializing_if = "Vec::is_empty")]
    pub records: Vec<String>,
}

/// Whether the registry is on: it is, unless `MUKADE_REGISTRY` says `off`, `0` or `false`.
pub fn enabled() -> bool {
    !is_off(&std::env::var(ENV).unwrap_or_default())
}

fn is_off(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "off" | "0" | "false"
    )
}

/// Where the registry lives: `mukade-data/registry`. `None` where there is no data folder.
pub fn dir() -> Option<PathBuf> {
    crate::portable::data_dir().map(|data| data.join("registry"))
}

/// The fingerprint of each planned block of the active path. Two plans share a block when its
/// fingerprint is the same: the same text, calls, results, in the same place.
pub fn block_hashes(plan: &Plan) -> Vec<String> {
    plan.blocks
        .iter()
        .map(|block| crate::sha256::sha256_hex(format!("{block:?}").as_bytes()))
        .collect()
}

/// How many blocks, from the first, two fingerprint lists share.
pub fn shared_prefix(a: &[String], b: &[Block]) -> usize {
    a.iter()
        .zip(b)
        .take_while(|(hash, block)| **hash == block.hash)
        .count()
}

fn file_of(dir: &Path, harness: &str, id: &str) -> PathBuf {
    dir.join(harness)
        .join(crate::cache::table_name("source", id))
}

fn load(file: &Path) -> Vec<Record> {
    std::fs::read_to_string(file)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Remember `record` in `dir`. The error says why it could not be written.
pub fn add(dir: &Path, record: Record) -> Result<(), String> {
    let file = file_of(dir, &record.source.harness, &record.source.id);
    let folder = file.parent().unwrap_or(dir);
    std::fs::create_dir_all(folder)
        .map_err(|error| format!("cannot make {}: {error}", folder.display()))?;
    let mut records = load(&file);
    records.push(record);
    let excess = records.len().saturating_sub(MAX_RECORDS);
    records.drain(..excess);
    let text = serde_json::to_string(&records).map_err(|error| error.to_string())?;
    // Written beside the file and renamed over it: a reader never sees half of it.
    let temporary = folder.join(format!(
        ".{}.{}.tmp",
        file.file_name().unwrap_or_default().to_string_lossy(),
        uuid::Uuid::new_v4().simple()
    ));
    std::fs::write(&temporary, text)
        .and_then(|()| std::fs::rename(&temporary, &file))
        .map_err(|error| {
            let _ = std::fs::remove_file(&temporary);
            format!("cannot write {}: {error}", file.display())
        })
}

/// The records of the source session `id` of `harness` written to `dest` in the store `store`,
/// newest first, keeping only those whose destination session is still where it was written.
pub fn written_to(
    dir: &Path,
    harness: HarnessId,
    id: &str,
    dest: HarnessId,
    store: &str,
) -> Vec<Record> {
    let mut records: Vec<Record> = load(&file_of(dir, harness.as_str(), id))
        .into_iter()
        .filter(|record| {
            record.source.id == id
                && record.dest.harness == dest.as_str()
                && record.dest.store == store
                && still_there(&record.dest)
        })
        .collect();
    records.reverse();
    records
}

/// Whether a written session is still where it was written: its file, for the destinations that
/// write one. A destination whose sessions live in a database (OpenCode) names the database: it
/// is taken as there while the database is.
fn still_there(dest: &Dest) -> bool {
    Path::new(&dest.location).is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(id: &str, location: &Path, hashes: &[&str]) -> Record {
        Record {
            time: "2026-11-12T12:00:00Z".into(),
            source: Source {
                harness: "claude_code".into(),
                id: id.into(),
                sha256: "s".into(),
            },
            dest: Dest {
                harness: "pi".into(),
                store: "/store".into(),
                id: format!("dest-{id}"),
                location: location.to_string_lossy().into_owned(),
            },
            written_as: "session".into(),
            blocks: hashes
                .iter()
                .map(|h| Block {
                    hash: h.to_string(),
                    records: vec![format!("r-{h}")],
                })
                .collect(),
        }
    }

    /// A record is found by its source, destination and store, newest first, while its
    /// destination file is there; a deleted copy is not found.
    #[test]
    fn a_record_is_found_while_its_copy_is_there() {
        let dir = crate::scratch::Scratch::new("registry");
        let kept = dir.join("kept.jsonl");
        let gone = dir.join("gone.jsonl");
        std::fs::write(&kept, "{}").unwrap();
        add(&dir.join("reg"), record("s1", &gone, &["a"])).unwrap();
        add(&dir.join("reg"), record("s1", &kept, &["a", "b"])).unwrap();
        let found = written_to(
            &dir.join("reg"),
            HarnessId::ClaudeCode,
            "s1",
            HarnessId::Pi,
            "/store",
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].dest.location, kept.to_string_lossy());
        assert!(
            written_to(
                &dir.join("reg"),
                HarnessId::ClaudeCode,
                "s1",
                HarnessId::Pi,
                "/other"
            )
            .is_empty()
        );
        assert!(
            written_to(
                &dir.join("reg"),
                HarnessId::ClaudeCode,
                "s1",
                HarnessId::Codex,
                "/store"
            )
            .is_empty()
        );
        assert!(
            written_to(
                &dir.join("reg"),
                HarnessId::ClaudeCode,
                "s2",
                HarnessId::Pi,
                "/store"
            )
            .is_empty()
        );
    }

    #[test]
    fn the_shared_prefix_stops_at_the_first_different_block() {
        let origin = record("s", Path::new("x"), &["a", "b", "c"]).blocks;
        let hashes = |list: &[&str]| list.iter().map(|h| h.to_string()).collect::<Vec<_>>();
        assert_eq!(shared_prefix(&hashes(&["a", "b", "x", "c"]), &origin), 2);
        assert_eq!(shared_prefix(&hashes(&["x"]), &origin), 0);
        assert_eq!(shared_prefix(&hashes(&["a", "b", "c", "d"]), &origin), 3);
    }

    #[test]
    fn the_variable_turns_it_off() {
        for (value, off) in [
            ("off", true),
            ("0", true),
            (" FALSE ", true),
            ("", false),
            ("on", false),
        ] {
            assert_eq!(is_off(value), off, "{value:?}");
        }
    }
}
