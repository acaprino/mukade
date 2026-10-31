//! OpenCode's store: the SQLite database `opencode.db`, recognized beside it or as the file.

use super::folder::{Folder, is_sqlite};
use super::layout::StoreLayout;
use crate::model::HarnessId;
use std::path::{Path, PathBuf};

const DATABASE: &str = "opencode.db";

pub(crate) struct OpenCodeLayout;

impl StoreLayout for OpenCodeLayout {
    fn harness(&self) -> HarnessId {
        HarnessId::OpenCode
    }

    fn evidence(&self, folder: &Folder) -> Vec<String> {
        match folder.file(DATABASE) {
            Some(db) if is_sqlite(&db) => vec!["opencode.db is a SQLite database".into()],
            _ => Vec::new(),
        }
    }

    /// Only the name OpenCode itself uses: other harnesses keep SQLite state too (Codex's
    /// `state_*.sqlite`), so the magic bytes alone must not decide.
    fn file_evidence(&self, file: &Path) -> Vec<String> {
        let named = file
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case(DATABASE));
        if named && is_sqlite(file) {
            vec![format!(
                "{} is a SQLite database (OpenCode's store)",
                file.display()
            )]
        } else {
            Vec::new()
        }
    }

    /// `OPENCODE_DB` names the database file, not its folder.
    fn store(&self, folder: &Path) -> PathBuf {
        folder.join(DATABASE)
    }
}
