//! Pi's store: `sessions/--<mangled cwd>--/<stamp>_<uuid>.jsonl`, plus its provider
//! configuration.

use super::folder::{Folder, count_files};
use super::layout::StoreLayout;
use crate::model::HarnessId;
use std::path::Path;

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
        if folder
            .file("models.json")
            .is_some_and(|models| lists_providers(&models))
        {
            evidence.push("models.json (Pi's provider configuration)".into());
        }
        evidence
    }

    fn session_folders(&self) -> &'static [&'static str] {
        &["sessions"]
    }
}

/// The most of `models.json` read: Pi's own file is a few kilobytes.
const MODELS_LIMIT: u64 = 1024 * 1024;

/// Pi's `models.json` is an object listing its `providers`. Other tools keep a file of that name
/// (an OpenCode-style `provider` map, say): the name alone is not Pi's.
fn lists_providers(models: &Path) -> bool {
    use std::io::Read;
    let mut text = String::new();
    std::fs::File::open(models)
        .and_then(|file| file.take(MODELS_LIMIT).read_to_string(&mut text))
        .is_ok()
        && serde_json::from_str::<serde_json::Value>(&text)
            .is_ok_and(|value| value.get("providers").is_some_and(|p| p.is_object()))
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
