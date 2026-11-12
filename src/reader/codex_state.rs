//! Codex's own record of its threads: `<CODEX_HOME>/state_<n>.sqlite`.
//!
//! The rollouts are the transcripts; this database is the catalogue Codex keeps beside them, and
//! it outlives them: deleting a rollout does not delete its `threads` row. Reading it is how a
//! thread whose rollout is gone is still seen (docs/harnesses/codex.md). It is opened read-only
//! and only ever read, and everything here is best effort: a database that is missing, locked,
//! damaged or laid out by another Codex version is simply an empty catalogue, so the rollouts on
//! disk are listed exactly as they were before this module existed.

use rusqlite::{Connection, OpenFlags, Row};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// One row of Codex's `threads` table, with what mukade lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thread {
    pub id: String,
    /// Where the row says the rollout is (resolved against the Codex home when it is relative).
    /// `None` when the row records no path.
    pub rollout: Option<PathBuf>,
    pub cwd: Option<String>,
    /// The thread's name, else its generated title, else the start of its first message.
    pub title: Option<String>,
    pub updated: Option<String>,
    pub archived: bool,
    /// The thread that spawned this one (`thread_spawn_edges`).
    pub parent: Option<String>,
    /// A thread Codex ran for another (a spawned subagent, a guardian review): not a root.
    pub subagent: bool,
}

/// The catalogue: the database file it came from, and its threads.
#[derive(Debug, Clone, Default)]
pub struct Catalogue {
    pub database: Option<PathBuf>,
    pub threads: Vec<Thread>,
}

impl Catalogue {
    /// The database's file name, for a message.
    pub fn database_name(&self) -> String {
        self.database
            .as_deref()
            .and_then(Path::file_name)
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "state_<n>.sqlite".to_string())
    }
}

/// The newest `state_<n>.sqlite` under `home`: Codex bumps the number when it changes the layout,
/// and leaves the older file behind.
fn newest_database(home: &Path) -> Option<PathBuf> {
    std::fs::read_dir(home)
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let number = name
                .strip_prefix("state_")?
                .strip_suffix(".sqlite")?
                .parse::<u32>()
                .ok()?;
            entry
                .file_type()
                .ok()?
                .is_file()
                .then(|| (number, entry.path()))
        })
        .max_by_key(|(number, _)| *number)
        .map(|(_, path)| path)
}

/// Every thread Codex still has a row for. Empty when there is nothing to read.
pub fn catalogue(home: &Path) -> Catalogue {
    let Some(database) = newest_database(home) else {
        return Catalogue::default();
    };
    let threads = read_threads(&database, home).unwrap_or_default();
    Catalogue {
        database: Some(database),
        threads,
    }
}

fn read_threads(database: &Path, home: &Path) -> rusqlite::Result<Vec<Thread>> {
    let db = Connection::open_with_flags(
        database,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    db.busy_timeout(std::time::Duration::from_secs(2))?;
    let present = columns(&db, "threads")?;
    if !present.contains("id") {
        return Ok(Vec::new());
    }
    // Only the columns this Codex version has; an absent one reads as NULL.
    let wanted = [
        "rollout_path",
        "cwd",
        "title",
        "name",
        "preview",
        "source",
        "thread_source",
        "archived",
        "updated_at_ms",
        "updated_at",
    ];
    let select: Vec<String> = wanted
        .iter()
        .map(|column| {
            if present.contains(*column) {
                format!("\"{column}\"")
            } else {
                "NULL".to_string()
            }
        })
        .collect();
    let mut query = db.prepare(&format!("SELECT id, {} FROM threads", select.join(", ")))?;
    let parents = parent_links(&db);
    let rows = query.query_map([], |row| {
        let text = |index: usize| text_at(row, index);
        let id = text(0).unwrap_or_default();
        let source = text(6).unwrap_or_default();
        let thread_source = text(7).unwrap_or_default();
        let updated_ms = int_at(row, 9).filter(|ms| *ms > 0).or_else(|| {
            int_at(row, 10)
                .filter(|s| *s > 0)
                .map(|s| s.saturating_mul(1000))
        });
        Ok(Thread {
            parent: parents.get(&id).cloned(),
            subagent: parents.contains_key(&id)
                || source.to_ascii_lowercase().contains("subagent")
                || matches!(thread_source.as_str(), "guardian_review" | "subagent"),
            rollout: text(1)
                .filter(|path| !path.is_empty())
                .map(|path| home.join(path)),
            cwd: text(2).filter(|cwd| !cwd.is_empty()),
            title: text(4)
                .filter(|name| !name.trim().is_empty())
                .or_else(|| text(3).filter(|title| !title.trim().is_empty()))
                .or_else(|| text(5).filter(|preview| !preview.trim().is_empty()))
                .map(|title| one_line(&title, TITLE_CHARS)),
            updated: updated_ms
                .and_then(chrono::DateTime::from_timestamp_millis)
                .map(|time| time.to_rfc3339()),
            archived: int_at(row, 8).is_some_and(|archived| archived != 0),
            id,
        })
    })?;
    let mut out = Vec::new();
    for thread in rows {
        let thread = thread?;
        if !thread.id.is_empty() {
            out.push(thread);
        }
    }
    Ok(out)
}

/// How long a title taken from a message may be.
const TITLE_CHARS: usize = 120;

/// `child → parent` for the threads a thread spawned. No table, no links.
fn parent_links(db: &Connection) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let Ok(present) = columns(db, "thread_spawn_edges") else {
        return out;
    };
    if !present.contains("parent_thread_id") || !present.contains("child_thread_id") {
        return out;
    }
    let Ok(mut query) =
        db.prepare("SELECT child_thread_id, parent_thread_id FROM thread_spawn_edges")
    else {
        return out;
    };
    let Ok(rows) = query.query_map([], |row| Ok((text_at(row, 0), text_at(row, 1)))) else {
        return out;
    };
    for (child, parent) in rows.flatten() {
        if let (Some(child), Some(parent)) = (child, parent) {
            out.insert(child, parent);
        }
    }
    out
}

fn columns(db: &Connection, table: &str) -> rusqlite::Result<HashSet<String>> {
    let mut query = db.prepare(&format!("PRAGMA table_info(\"{table}\")"))?;
    let names = query.query_map([], |row| row.get::<_, String>(1))?;
    names.collect()
}

/// A column read as text whatever its declared type; `None` for NULL or a blob.
fn text_at(row: &Row<'_>, index: usize) -> Option<String> {
    use rusqlite::types::ValueRef;
    match row.get_ref(index).ok()? {
        ValueRef::Text(bytes) => Some(String::from_utf8_lossy(bytes).into_owned()),
        ValueRef::Integer(n) => Some(n.to_string()),
        _ => None,
    }
}

/// A column read as an integer whatever its declared type; `None` for NULL or text that is not one.
fn int_at(row: &Row<'_>, index: usize) -> Option<i64> {
    use rusqlite::types::ValueRef;
    match row.get_ref(index).ok()? {
        ValueRef::Integer(n) => Some(n),
        ValueRef::Text(bytes) => std::str::from_utf8(bytes).ok()?.trim().parse().ok(),
        _ => None,
    }
}

/// The first line of `text`, its whitespace collapsed, cut to `max` characters.
fn one_line(text: &str, max: usize) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= max {
        return collapsed;
    }
    let mut cut: String = collapsed.chars().take(max).collect();
    cut.push('…');
    cut
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_title_is_one_line_and_bounded() {
        assert_eq!(one_line("  two\n\nlines\there ", 50), "two lines here");
        let long = "x".repeat(300);
        let cut = one_line(&long, 10);
        assert_eq!(
            cut.chars().count(),
            11,
            "ten characters and the mark: {cut}"
        );
        assert!(cut.ends_with('…'));
        assert_eq!(
            one_line("ünï", 3),
            "ünï",
            "counted in characters, not bytes"
        );
    }

    #[test]
    fn only_a_numbered_state_file_is_the_database() {
        let dir = crate::scratch::Scratch::new("codex-state");
        for name in [
            "state_5.sqlite",
            "state_5.sqlite-wal",
            "state_10.sqlite",
            "state_x.sqlite",
            "state_.sqlite",
            "logs_2.sqlite",
        ] {
            std::fs::write(dir.join(name), b"").unwrap();
        }
        assert_eq!(
            newest_database(&dir).as_deref(),
            Some(dir.join("state_10.sqlite").as_path()),
            "numbered by value (10 after 5), side files and other databases ignored"
        );
    }
}
