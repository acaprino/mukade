//! A small per-user cache of what a harness's own binary told us, so the next hop does not ask
//! again: its `--version`, and the OpenCode project a directory belongs to. Starting a harness to
//! ask costs 50 ms to 500 ms (Pi, a Node program, is the slowest); a hop of a few megabytes costs
//! less than that.
//!
//! One JSON object in `probes.json`. It lives in `mukade-data/cache` beside the executable, the
//! one folder mukade keeps anything in (see [`crate::portable`]); where that folder cannot be made
//! there is no cache and nothing is written. A key names the thing asked about together with
//! what would change the answer (a binary's path, size and modification time), so a stale entry
//! is simply never looked up again. Reading or writing it can fail: the cache is then empty, and
//! every caller falls back to asking.

use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

const FILE: &str = "probes.json";
/// Past this many entries the file starts over: it is a cache, not a record.
const MAX_ENTRIES: usize = 256;

/// Where the cache lives: see [`crate::portable::cache_dir`].
pub fn dir() -> Option<PathBuf> {
    crate::portable::cache_dir()
}

/// A table kept whole in its own file, for answers that come many at a time (what a listing read
/// from each of a few hundred session files). Empty when the file is absent, unreadable or not a
/// JSON object: a damaged table is a cold one.
pub fn load_table(dir: &Path, name: &str) -> Map<String, Value> {
    std::fs::read_to_string(dir.join(name))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|value| match value {
            Value::Object(map) => Some(map),
            _ => None,
        })
        .unwrap_or_default()
}

/// Replace a table. Best effort, like every write here: a table that cannot be written is not an
/// error, and a reader never sees half of one.
pub fn store_table(dir: &Path, name: &str, table: &Map<String, Value>) {
    if std::fs::create_dir_all(dir).is_err() {
        return;
    }
    // Written beside the file and renamed over it.
    let temporary = dir.join(format!(".{name}.{}.tmp", std::process::id()));
    let text = Value::Object(table.clone()).to_string();
    if std::fs::write(&temporary, text).is_ok()
        && std::fs::rename(&temporary, dir.join(name)).is_err()
    {
        let _ = std::fs::remove_file(&temporary);
    }
}

/// A file name for a table about `subject` (a folder, say): the same subject always gets the same
/// name, and two subjects get different ones. `<prefix>-<16 hex digits>.json`.
pub fn table_name(prefix: &str, subject: &str) -> String {
    // FNV-1a, 64 bits.
    let hash = subject
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
        });
    format!("{prefix}-{hash:016x}.json")
}

fn load(dir: &Path) -> Map<String, Value> {
    load_table(dir, FILE)
}

/// The cached answer for `key`, if there is one.
pub fn get(dir: &Path, key: &str) -> Option<String> {
    load(dir).get(key)?.as_str().map(str::to_owned)
}

/// Remember `value` for `key`. Best effort: a cache that cannot be written is not an error.
pub fn put(dir: &Path, key: &str, value: &str) {
    let mut entries = load(dir);
    if entries.len() >= MAX_ENTRIES {
        entries.clear();
    }
    entries.insert(key.to_string(), Value::String(value.to_string()));
    store_table(dir, FILE, &entries);
}

/// What a binary's identity is, for a key: its path, size and modification time. `None` when the
/// file cannot be examined (then nothing about it is cached).
pub fn identity(path: &Path) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    let modified = meta
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_nanos();
    Some(format!("{}|{}|{modified}", path.display(), meta.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mukade-cache-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn an_answer_is_remembered_and_a_missing_one_is_not() {
        let dir = scratch("roundtrip");
        assert_eq!(
            get(&dir, "k"),
            None,
            "an absent cache is empty, not an error"
        );
        put(&dir, "k", "v1");
        put(&dir, "other", "v2");
        assert_eq!(get(&dir, "k").as_deref(), Some("v1"));
        assert_eq!(get(&dir, "other").as_deref(), Some("v2"));
        put(&dir, "k", "v3");
        assert_eq!(get(&dir, "k").as_deref(), Some("v3"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_damaged_cache_file_is_an_empty_cache() {
        let dir = scratch("damaged");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(FILE), "{not json").unwrap();
        assert_eq!(get(&dir, "k"), None);
        put(&dir, "k", "v");
        assert_eq!(get(&dir, "k").as_deref(), Some("v"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_changed_file_has_a_different_identity() {
        let dir = scratch("identity");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("tool");
        std::fs::write(&file, "one").unwrap();
        let before = identity(&file).unwrap();
        std::fs::write(&file, "one, changed").unwrap();
        assert_ne!(identity(&file).unwrap(), before);
        assert!(identity(&dir.join("missing")).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
