//! Codex CLI's store: `sessions/YYYY/MM/DD/rollout-*.jsonl`, plus its session catalog.

use super::folder::{Folder, count_files};
use super::layout::StoreLayout;
use crate::model::HarnessId;

pub(crate) struct CodexLayout;

impl StoreLayout for CodexLayout {
    fn harness(&self) -> HarnessId {
        HarnessId::Codex
    }

    fn evidence(&self, folder: &Folder) -> Vec<String> {
        let mut evidence = Vec::new();
        let rollouts = folder.folder("sessions").map_or(0, |sessions| {
            count_files(&sessions, 4, &|name| {
                name.starts_with("rollout-") && name.ends_with(".jsonl")
            })
        });
        if rollouts > 0 {
            evidence.push(format!(
                "sessions/ holds {rollouts} rollout-*.jsonl file(s)"
            ));
        }
        if folder.file("session_index.jsonl").is_some() {
            evidence.push("session_index.jsonl (Codex's session catalog)".into());
        }
        evidence
    }

    fn session_folders(&self) -> &'static [&'static str] {
        &["sessions"]
    }
}
