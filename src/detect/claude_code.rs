//! Claude Code's store: `projects/<mangled cwd>/<uuid>.jsonl`.

use super::folder::{Folder, count_files};
use super::layout::StoreLayout;
use crate::model::HarnessId;

pub(crate) struct ClaudeCodeLayout;

impl StoreLayout for ClaudeCodeLayout {
    fn harness(&self) -> HarnessId {
        HarnessId::ClaudeCode
    }

    /// The folder name alone is too common to be evidence: at least one session file must be
    /// there.
    fn evidence(&self, folder: &Folder) -> Vec<String> {
        let Some(projects) = folder.folder("projects") else {
            return Vec::new();
        };
        match count_files(&projects, 2, &|name| name.ends_with(".jsonl")) {
            0 => Vec::new(),
            sessions => vec![format!(
                "projects/<project>/ holds {sessions} session .jsonl file(s)"
            )],
        }
    }

    fn session_folders(&self) -> &'static [&'static str] {
        &["projects"]
    }
}
