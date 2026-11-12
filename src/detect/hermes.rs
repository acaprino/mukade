//! Hermes Agent's store: the SQLite database `state.db` in its home folder.

use super::folder::{Folder, is_sqlite};
use super::layout::StoreLayout;
use crate::model::HarnessId;
use std::path::Path;

const DATABASE: &str = "state.db";

/// The tables that make a `state.db` Hermes's: other tools keep a database of that name.
const TABLES: [&str; 3] = ["schema_version", "sessions", "messages"];

pub(crate) struct HermesLayout;

impl StoreLayout for HermesLayout {
    fn harness(&self) -> HarnessId {
        HarnessId::Hermes
    }

    fn evidence(&self, folder: &Folder) -> Vec<String> {
        match folder.file(DATABASE) {
            Some(db) if is_sqlite(&db) && has_tables(&db) => {
                vec!["state.db is a SQLite database with Hermes's sessions and messages".into()]
            }
            _ => Vec::new(),
        }
    }
}

/// Whether the database holds every one of [`TABLES`], read without writing anything.
fn has_tables(db: &Path) -> bool {
    use rusqlite::{Connection, OpenFlags};
    let Ok(connection) = Connection::open_with_flags(
        db,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) else {
        return false;
    };
    TABLES.iter().all(|table| {
        connection
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [table],
                |_| Ok(()),
            )
            .is_ok()
    })
}

#[cfg(test)]
mod tests {
    use crate::detect::detect_store;
    use crate::detect::tests::{scratch, touch};
    use crate::model::HarnessId;

    fn database(path: &std::path::Path, tables: &[&str]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let db = rusqlite::Connection::open(path).unwrap();
        for table in tables {
            db.execute_batch(&format!("CREATE TABLE {table} (id TEXT);"))
                .unwrap();
        }
    }

    #[test]
    fn a_hermes_home_is_recognized_by_its_state_database() {
        let dir = scratch("hermes");
        database(
            &dir.join("state.db"),
            &["schema_version", "sessions", "messages"],
        );
        let found = detect_store(&dir);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].harness, HarnessId::Hermes);
    }

    #[test]
    fn another_state_database_is_not_hermess() {
        let dir = scratch("not-hermes");
        database(&dir.join("state.db"), &["sessions"]);
        assert!(detect_store(&dir).is_empty());
        touch(&dir.join("other/state.db"), b"not a database");
        assert!(detect_store(&dir.join("other")).is_empty());
    }
}
