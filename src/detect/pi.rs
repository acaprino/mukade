//! Pi's store: `sessions/--<mangled cwd>--/<stamp>_<uuid>.jsonl`, plus its provider
//! configuration.

use super::folder::{Folder, count_files};
use super::layout::StoreLayout;
use crate::model::HarnessId;

pub(crate) struct PiLayout;

impl StoreLayout for PiLayout {
    fn harness(&self) -> HarnessId {
        HarnessId::Pi
    }

    fn evidence(&self, folder: &Folder) -> Vec<String> {
        let mut evidence = Vec::new();
        let sessions = folder
            .folder("sessions")
            .map_or(0, |sessions| count_sessions(&Folder::read(&sessions)));
        if sessions > 0 {
            evidence.push(format!(
                "sessions/--<project>--/ holds {sessions} session .jsonl file(s)"
            ));
        }
        if folder.file("models.json").is_some() {
            evidence.push("models.json (Pi's provider configuration)".into());
        }
        evidence
    }

    fn session_folders(&self) -> &'static [&'static str] {
        &["sessions"]
    }
}

/// Pi's project folders are unmistakable: `--<working directory>--`, one `.jsonl` per session.
fn count_sessions(sessions: &Folder) -> usize {
    sessions
        .subfolders()
        .into_iter()
        .filter(|(name, _)| name.starts_with("--") && name.ends_with("--") && name.len() > 4)
        .map(|(_, project)| count_files(&project, 1, &|name| name.ends_with(".jsonl")))
        .sum()
}
