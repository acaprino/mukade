//! The chats the Codex app hosts on ChatGPT: `<CODEX_HOME>/sqlite/codex.db`.
//!
//! The app lists chats whose conversation lives on ChatGPT beside the local ones. Only their
//! catalogue row is on this machine (title, time, the project they belong to), never the
//! messages. A project that the app backs with a ChatGPT project has a local folder, kept in
//! `state_<n>.sqlite`; those are the chats returned here, with that folder. Both databases are
//! opened read-only and only ever read, and everything is best effort: a database that is
//! missing, locked, damaged or laid out by another Codex version gives no hosted chats.

use super::codex_state::{columns, newest_database, text_at};
use rusqlite::{Connection, OpenFlags, Row};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// The reason a hosted chat cannot be read.
pub(crate) const HOSTED: &str =
    "hosted by ChatGPT: the conversation is not on this machine; the work it left is in its folder";

/// One hosted chat whose project has a local folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HostedChat {
    pub id: String,
    pub title: Option<String>,
    /// RFC 3339, from `source_updated_at` (epoch seconds, fractional).
    pub updated: Option<String>,
    /// The project's root at position 0.
    pub folder: PathBuf,
    /// `<home>/sqlite/codex.db`.
    pub database: PathBuf,
}

/// A catalogue row of a hosted chat that belongs to a project.
struct Entry {
    id: String,
    title: Option<String>,
    updated: Option<String>,
    project: String,
}

/// The hosted chats of a Codex home that have a local folder. Empty when there is nothing to read.
pub(crate) fn hosted_chats(home: &Path) -> Vec<HostedChat> {
    let database = home.join("sqlite").join("codex.db");
    let entries = read_catalogue(&database).unwrap_or_default();
    if entries.is_empty() {
        return Vec::new();
    }
    let Some(state) = newest_database(home) else {
        return Vec::new();
    };
    let folders = project_folders(&state).unwrap_or_default();
    entries
        .into_iter()
        .filter_map(|entry| {
            let folder = folders.get(&entry.project)?.clone();
            Some(HostedChat {
                id: entry.id,
                title: entry.title,
                updated: entry.updated,
                folder,
                database: database.clone(),
            })
        })
        .collect()
}

fn open(database: &Path) -> rusqlite::Result<Connection> {
    let db = Connection::open_with_flags(
        database,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    db.busy_timeout(std::time::Duration::from_secs(2))?;
    Ok(db)
}

/// Whether `table` has every column of `needed`.
fn has_columns(db: &Connection, table: &str, needed: &[&str]) -> rusqlite::Result<bool> {
    let present = columns(db, table)?;
    Ok(needed.iter().all(|column| present.contains(*column)))
}

/// The rows of the hosts of kind `chatgpt` that name a project, newest first.
fn read_catalogue(database: &Path) -> rusqlite::Result<Vec<Entry>> {
    let db = open(database)?;
    let catalogue = [
        "host_id",
        "thread_id",
        "display_title",
        "source_updated_at",
        "project_id",
    ];
    if !has_columns(&db, "local_thread_catalog", &catalogue)?
        || !has_columns(&db, "local_thread_catalog_hosts", &["host_id", "host_kind"])?
    {
        return Ok(Vec::new());
    }
    let mut query = db.prepare(
        "SELECT c.thread_id, c.display_title, c.source_updated_at, c.project_id \
         FROM local_thread_catalog c \
         JOIN local_thread_catalog_hosts h ON h.host_id = c.host_id \
         WHERE h.host_kind = 'chatgpt' AND c.project_id IS NOT NULL \
         ORDER BY c.source_updated_at DESC, c.thread_id",
    )?;
    let rows = query.query_map([], |row| {
        Ok(Entry {
            id: text_at(row, 0).unwrap_or_default(),
            title: text_at(row, 1).filter(|title| !title.trim().is_empty()),
            updated: seconds_at(row, 2).and_then(rfc3339),
            project: text_at(row, 3).unwrap_or_default(),
        })
    })?;
    let mut out = Vec::new();
    for entry in rows {
        let entry = entry?;
        if !entry.id.is_empty() && !entry.project.is_empty() {
            out.push(entry);
        }
    }
    Ok(out)
}

/// `ChatGPT project id → the folder at position 0`, for the projects the app backs with one.
fn project_folders(database: &Path) -> rusqlite::Result<HashMap<String, PathBuf>> {
    let db = open(database)?;
    if !has_columns(&db, "projects", &["id", "metadata"])?
        || !has_columns(&db, "project_roots", &["project_id", "position", "path"])?
    {
        return Ok(HashMap::new());
    }
    let mut query = db.prepare(
        "SELECT p.metadata, r.path \
         FROM projects p JOIN project_roots r ON r.project_id = p.id \
         WHERE r.position = 0 ORDER BY p.id",
    )?;
    let rows = query.query_map([], |row| Ok((text_at(row, 0), text_at(row, 1))))?;
    let mut out = HashMap::new();
    for row in rows {
        if let (Some(metadata), Some(path)) = row? {
            if path.is_empty() {
                continue;
            }
            if let Some(project) = backing_project(&metadata) {
                out.entry(project).or_insert_with(|| PathBuf::from(path));
            }
        }
    }
    Ok(out)
}

/// The ChatGPT project id in a project's metadata: `codex.chatGptBacking` is a JSON string that
/// holds, once parsed again, its `projectId`.
fn backing_project(metadata: &str) -> Option<String> {
    let metadata: serde_json::Value = serde_json::from_str(metadata).ok()?;
    let backing: serde_json::Value =
        serde_json::from_str(metadata.get("codex.chatGptBacking")?.as_str()?).ok()?;
    backing
        .get("projectId")?
        .as_str()
        .filter(|id| !id.is_empty())
        .map(str::to_string)
}

/// A column read as seconds whatever its declared type; `None` for NULL or text that is not a number.
fn seconds_at(row: &Row<'_>, index: usize) -> Option<f64> {
    use rusqlite::types::ValueRef;
    match row.get_ref(index).ok()? {
        ValueRef::Real(seconds) => Some(seconds),
        ValueRef::Integer(n) => Some(n as f64),
        ValueRef::Text(bytes) => std::str::from_utf8(bytes).ok()?.trim().parse().ok(),
        _ => None,
    }
}

/// Epoch seconds, fractional, as RFC 3339 to the millisecond.
fn rfc3339(seconds: f64) -> Option<String> {
    if !seconds.is_finite() || seconds <= 0.0 {
        return None;
    }
    chrono::DateTime::from_timestamp_millis((seconds * 1000.0).round() as i64)
        .map(|time| time.to_rfc3339())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    const CATALOGUE_DDL: &str = r#"
        CREATE TABLE "local_thread_catalog" (
          host_id TEXT NOT NULL,
          thread_id TEXT NOT NULL,
          display_title TEXT NOT NULL,
          source_created_at REAL NOT NULL,
          source_updated_at REAL NOT NULL,
          cwd TEXT,
          source_kind TEXT NOT NULL,
          source_detail TEXT,
          model_provider TEXT,
          git_branch TEXT,
          observation_sequence INTEGER NOT NULL,
          missing_candidate INTEGER NOT NULL DEFAULT 0 CHECK (missing_candidate IN (0, 1)),
          thread_source TEXT,
          source_recency_at REAL NOT NULL DEFAULT 0,
          pending_observed_title INTEGER NOT NULL DEFAULT 0,
          project_id TEXT,
          conversation_origin TEXT, trial_conversation_type TEXT, chatgpt_async_status INTEGER,
          PRIMARY KEY (host_id, thread_id)
        );
        CREATE TABLE "local_thread_catalog_hosts" (
          host_id TEXT PRIMARY KEY,
          host_kind TEXT NOT NULL CHECK (host_kind IN ('local', 'ssh', 'wsl', 'remote-control', 'chatgpt'))
        );
        INSERT INTO local_thread_catalog_hosts VALUES ('chatgpt:acc:user-1', 'chatgpt');
        INSERT INTO local_thread_catalog_hosts VALUES ('local', 'local');
    "#;

    const STATE_DDL: &str = r#"
        CREATE TABLE projects (
          id TEXT PRIMARY KEY,
          name TEXT NOT NULL,
          metadata TEXT NOT NULL DEFAULT '{}',
          position INTEGER NOT NULL,
          created_at_ms INTEGER NOT NULL,
          updated_at_ms INTEGER NOT NULL
        );
        CREATE TABLE project_roots (
          project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
          position INTEGER NOT NULL,
          path TEXT NOT NULL,
          PRIMARY KEY (project_id, position)
        );
    "#;

    const CHATGPT_HOST: &str = "chatgpt:acc:user-1";

    fn catalogue_db(home: &Path) -> Connection {
        std::fs::create_dir_all(home.join("sqlite")).unwrap();
        let db = Connection::open(home.join("sqlite").join("codex.db")).unwrap();
        db.execute_batch(CATALOGUE_DDL).unwrap();
        db
    }

    fn state_db(home: &Path) -> Connection {
        let db = Connection::open(home.join("state_5.sqlite")).unwrap();
        db.execute_batch(STATE_DDL).unwrap();
        db
    }

    fn add_chat(
        db: &Connection,
        host: &str,
        thread: &str,
        title: &str,
        updated: f64,
        project: Option<&str>,
        missing: i64,
    ) {
        db.execute(
            "INSERT INTO local_thread_catalog (host_id, thread_id, display_title, \
             source_created_at, source_updated_at, source_kind, observation_sequence, \
             missing_candidate, project_id) VALUES (?1, ?2, ?3, 1.0, ?4, 'chatgpt', 1, ?5, ?6)",
            params![host, thread, title, updated, missing, project],
        )
        .unwrap();
    }

    fn add_project_with_metadata(
        db: &Connection,
        id: &str,
        metadata: &str,
        roots: &[(i64, &Path)],
    ) {
        db.execute(
            "INSERT INTO projects (id, name, metadata, position, created_at_ms, updated_at_ms) \
             VALUES (?1, ?1, ?2, 0, 0, 0)",
            params![id, metadata],
        )
        .unwrap();
        for (position, path) in roots {
            db.execute(
                "INSERT INTO project_roots (project_id, position, path) VALUES (?1, ?2, ?3)",
                params![id, position, path.to_string_lossy()],
            )
            .unwrap();
        }
    }

    /// A project the app backs with the ChatGPT project `backing`.
    fn add_project(db: &Connection, id: &str, backing: &str, roots: &[(i64, &Path)]) {
        let inner = serde_json::json!({
            "accountId": "acc",
            "userId": "user-1",
            "projectId": backing,
        })
        .to_string();
        let metadata = serde_json::json!({
            "codex.chatGptBacking": inner,
            "codex.localProjectId": id,
        })
        .to_string();
        add_project_with_metadata(db, id, &metadata, roots);
    }

    /// One hosted chat `t1` of the ChatGPT project `g-p-abc`, whose root is `folder`.
    fn one_chat(home: &Path, folder: &Path) {
        let catalogue = catalogue_db(home);
        add_chat(
            &catalogue,
            CHATGPT_HOST,
            "t1",
            "Capire lo stack di Daodan",
            1795254778.231,
            Some("g-p-abc"),
            0,
        );
        let state = state_db(home);
        add_project(&state, "local-1", "g-p-abc", &[(0, folder)]);
    }

    #[test]
    fn a_hosted_chat_of_a_project_with_a_folder_is_listed() {
        let scratch = crate::detect::tests::scratch("hosted");
        let home = scratch.to_path_buf();
        one_chat(&home, &scratch.join("Daodan"));

        let chats = hosted_chats(&home);
        assert_eq!(chats.len(), 1);
        assert_eq!(chats[0].id, "t1");
        assert_eq!(chats[0].title.as_deref(), Some("Capire lo stack di Daodan"));
        assert_eq!(chats[0].folder, scratch.join("Daodan"));
        let expected =
            chrono::DateTime::from_timestamp_millis((1795254778.231_f64 * 1000.0).round() as i64)
                .unwrap()
                .to_rfc3339();
        assert!(expected.ends_with(".231+00:00"), "{expected}");
        assert_eq!(chats[0].updated.as_deref(), Some(expected.as_str()));
        assert_eq!(chats[0].database, home.join("sqlite").join("codex.db"));
    }

    #[test]
    fn a_chat_without_a_project_or_whose_project_has_no_root_is_left_out() {
        let scratch = crate::detect::tests::scratch("hosted");
        let home = scratch.to_path_buf();
        let catalogue = catalogue_db(&home);
        add_chat(
            &catalogue,
            CHATGPT_HOST,
            "plain",
            "No project",
            10.0,
            None,
            0,
        );
        add_chat(
            &catalogue,
            CHATGPT_HOST,
            "rootless",
            "Project without a folder",
            11.0,
            Some("g-p-empty"),
            0,
        );
        let state = state_db(&home);
        add_project(&state, "local-1", "g-p-empty", &[]);
        drop((catalogue, state));

        assert!(hosted_chats(&home).is_empty());
    }

    #[test]
    fn a_row_of_another_host_kind_is_left_out() {
        let scratch = crate::detect::tests::scratch("hosted");
        let home = scratch.to_path_buf();
        let catalogue = catalogue_db(&home);
        add_chat(&catalogue, "local", "t1", "Local", 10.0, Some("g-p-abc"), 0);
        let state = state_db(&home);
        add_project(
            &state,
            "local-1",
            "g-p-abc",
            &[(0, &scratch.join("Daodan"))],
        );
        drop((catalogue, state));

        assert!(hosted_chats(&home).is_empty());
    }

    #[test]
    fn the_root_at_position_zero_is_the_folder() {
        let scratch = crate::detect::tests::scratch("hosted");
        let home = scratch.to_path_buf();
        let catalogue = catalogue_db(&home);
        add_chat(
            &catalogue,
            CHATGPT_HOST,
            "t1",
            "Two roots",
            10.0,
            Some("g-p-abc"),
            0,
        );
        let state = state_db(&home);
        let second = scratch.join("second");
        let first = scratch.join("first");
        add_project(&state, "local-1", "g-p-abc", &[(1, &second), (0, &first)]);
        drop((catalogue, state));

        let chats = hosted_chats(&home);
        assert_eq!(chats.len(), 1);
        assert_eq!(chats[0].folder, first);
    }

    #[test]
    fn a_missing_candidate_row_is_listed_like_the_others() {
        let scratch = crate::detect::tests::scratch("hosted");
        let home = scratch.to_path_buf();
        let catalogue = catalogue_db(&home);
        add_chat(
            &catalogue,
            CHATGPT_HOST,
            "t1",
            "Seen",
            10.0,
            Some("g-p-abc"),
            0,
        );
        add_chat(
            &catalogue,
            CHATGPT_HOST,
            "t2",
            "Missing",
            20.0,
            Some("g-p-abc"),
            1,
        );
        let state = state_db(&home);
        add_project(
            &state,
            "local-1",
            "g-p-abc",
            &[(0, &scratch.join("Daodan"))],
        );
        drop((catalogue, state));

        let chats = hosted_chats(&home);
        let ids: Vec<&str> = chats.iter().map(|chat| chat.id.as_str()).collect();
        assert_eq!(ids, ["t2", "t1"], "both, the newest first");
    }

    #[test]
    fn a_locked_or_foreign_database_gives_no_hosted_chats() {
        // (a) no sqlite folder at all.
        let scratch = crate::detect::tests::scratch("hosted");
        let home = scratch.to_path_buf();
        let state = state_db(&home);
        add_project(
            &state,
            "local-1",
            "g-p-abc",
            &[(0, &scratch.join("Daodan"))],
        );
        drop(state);
        assert!(hosted_chats(&home).is_empty(), "no sqlite folder");

        // (b) codex.db is not a database.
        std::fs::create_dir_all(home.join("sqlite")).unwrap();
        std::fs::write(home.join("sqlite").join("codex.db"), "this is a text file").unwrap();
        assert!(hosted_chats(&home).is_empty(), "a text file");

        // (c) the catalogue lacks `project_id`.
        let scratch = crate::detect::tests::scratch("hosted");
        let home = scratch.to_path_buf();
        std::fs::create_dir_all(home.join("sqlite")).unwrap();
        let catalogue = Connection::open(home.join("sqlite").join("codex.db")).unwrap();
        catalogue
            .execute_batch(
                "CREATE TABLE local_thread_catalog (host_id TEXT NOT NULL, thread_id TEXT NOT NULL, \
                 display_title TEXT NOT NULL, source_updated_at REAL NOT NULL); \
                 CREATE TABLE local_thread_catalog_hosts (host_id TEXT PRIMARY KEY, host_kind TEXT NOT NULL); \
                 INSERT INTO local_thread_catalog_hosts VALUES ('chatgpt:acc:user-1', 'chatgpt'); \
                 INSERT INTO local_thread_catalog VALUES ('chatgpt:acc:user-1', 't1', 'Title', 10.0);",
            )
            .unwrap();
        let state = state_db(&home);
        add_project(
            &state,
            "local-1",
            "g-p-abc",
            &[(0, &scratch.join("Daodan"))],
        );
        drop((catalogue, state));
        assert!(hosted_chats(&home).is_empty(), "no project_id column");

        // (d) the project's metadata is not JSON.
        let scratch = crate::detect::tests::scratch("hosted");
        let home = scratch.to_path_buf();
        let catalogue = catalogue_db(&home);
        add_chat(
            &catalogue,
            CHATGPT_HOST,
            "t1",
            "Title",
            10.0,
            Some("g-p-abc"),
            0,
        );
        let state = state_db(&home);
        add_project_with_metadata(
            &state,
            "local-1",
            "not json",
            &[(0, &scratch.join("Daodan"))],
        );
        drop((catalogue, state));
        assert!(
            hosted_chats(&home).is_empty(),
            "metadata that does not parse"
        );
    }

    #[test]
    fn reading_leaves_both_databases_byte_identical() {
        let scratch = crate::detect::tests::scratch("hosted");
        let home = scratch.to_path_buf();
        one_chat(&home, &scratch.join("Daodan"));
        let catalogue = home.join("sqlite").join("codex.db");
        let state = home.join("state_5.sqlite");
        let before = (
            std::fs::read(&catalogue).unwrap(),
            std::fs::read(&state).unwrap(),
        );

        assert_eq!(hosted_chats(&home).len(), 1);

        assert!(
            before.0 == std::fs::read(&catalogue).unwrap(),
            "codex.db changed"
        );
        assert!(
            before.1 == std::fs::read(&state).unwrap(),
            "state_5.sqlite changed"
        );
    }
}
