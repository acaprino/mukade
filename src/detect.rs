//! Recognize harness stores and installations from the layouts their own files make.
//!
//! Detection is evidence, not a verdict: a directory can legitimately hold more than one
//! harness's store (the test homes do), so the answer is every candidate with what was seen. An
//! explicitly stated harness always wins over detection; inference is for a caller that has a
//! directory and nothing else. A candidate needs concrete files — session records, Pi's
//! `models.json`, OpenCode's `opencode.db`: an empty directory tree infers nothing.
//!
//! - [`detect_store`] answers for one folder (or OpenCode's database file);
//! - [`scan_installations`] walks a tree and reports every installation in it, portable and
//!   custom ones included, with the executable kept beside each store
//!   ([`engine_beside_store`]);
//! - [`locate_installation`] resolves a path a command was handed to one installation.
//!
//! Each harness's layout is an adapter (`claude_code`, `codex`, `opencode`, `pi`) behind one
//! contract (`layout::StoreLayout`), so detection and the scan apply the same rules. The CLI and
//! the terminal browser use these functions and recognize nothing themselves (ADR-0001).

mod claude_code;
mod codex;
mod engine;
mod folder;
mod layout;
mod locate;
mod opencode;
mod pi;
mod scan;

pub use engine::engine_beside_store;
pub use locate::{Located, locate_installation};
pub use scan::{DEFAULT_SCAN_DEPTH, Installation, Scan, scan_installations};

use crate::model::HarnessId;
use folder::Folder;
use layout::layouts;
use std::path::Path;

/// One harness a directory may belong to, with what was seen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Detection {
    pub harness: HarnessId,
    pub evidence: Vec<String>,
}

/// Recognize the harness stores `path` holds. A file is accepted too: OpenCode's store is the
/// SQLite database itself, not a directory. An empty answer means nothing was recognized.
pub fn detect_store(path: &Path) -> Vec<Detection> {
    let detection = |harness: HarnessId, evidence: Vec<String>| {
        (!evidence.is_empty()).then_some(Detection { harness, evidence })
    };
    if path.is_file() {
        return layouts()
            .into_iter()
            .filter_map(|layout| detection(layout.harness(), layout.file_evidence(path)))
            .collect();
    }
    if !path.is_dir() {
        return Vec::new();
    }
    let folder = Folder::read(path);
    layouts()
        .into_iter()
        .filter_map(|layout| detection(layout.harness(), layout.evidence(&folder)))
        .collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::path::PathBuf;

    pub(crate) fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mukade-detect-{tag}-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    pub(crate) fn touch(path: &Path, content: &[u8]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    #[test]
    fn a_pi_home_is_recognized() {
        let dir = scratch("pi");
        touch(
            &dir.join("sessions/--pi-project--/2026-10-27T16-00-39-689Z_id.jsonl"),
            b"{}\n",
        );
        touch(&dir.join("models.json"), br#"{"providers":{}}"#);
        let found = detect_store(&dir);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].harness, HarnessId::Pi);
        assert_eq!(found[0].evidence.len(), 2, "{:?}", found[0].evidence);
    }

    #[test]
    fn another_tools_models_json_is_not_pis() {
        // OpenCode-style provider lists and other tools' files share the name: Pi's own file
        // lists its `providers`.
        let dir = scratch("not-pi");
        touch(&dir.join("models.json"), br#"{"provider":{"gw":{}}}"#);
        assert!(detect_store(&dir).is_empty());
        touch(&dir.join("models.json"), b"not json");
        assert!(detect_store(&dir).is_empty());
    }

    #[test]
    fn a_codex_home_is_recognized_by_its_dated_rollouts() {
        let dir = scratch("codex");
        touch(
            &dir.join("sessions/2026/10/27/rollout-2026-10-27T16-43-08-id.jsonl"),
            b"{}\n",
        );
        let found = detect_store(&dir);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].harness, HarnessId::Codex);
        // A dated rollout is Codex's, not Pi's: no `--project--` folder exists.
        assert!(!found.iter().any(|d| d.harness == HarnessId::Pi));
    }

    #[test]
    fn an_opencode_database_is_recognized_as_a_file_and_beside_it() {
        let dir = scratch("opencode");
        let db = dir.join("opencode.db");
        touch(&db, b"SQLite format 3\0rest of the file");
        assert_eq!(detect_store(&dir)[0].harness, HarnessId::OpenCode);
        assert_eq!(detect_store(&db)[0].harness, HarnessId::OpenCode);
        // A file that merely has the extension is not enough.
        let fake = dir.join("fake.db");
        touch(&fake, b"not a database");
        assert!(detect_store(&fake).is_empty());
    }

    #[test]
    fn a_shared_home_reports_every_harness_it_holds() {
        let dir = scratch("shared");
        touch(&dir.join("projects/claude-project/id.jsonl"), b"{}\n");
        touch(&dir.join("sessions/--pi-project--/stamp_id.jsonl"), b"{}\n");
        let found = detect_store(&dir);
        let harnesses: Vec<HarnessId> = found.iter().map(|d| d.harness).collect();
        assert_eq!(harnesses, vec![HarnessId::ClaudeCode, HarnessId::Pi]);
    }

    #[test]
    fn a_sqlite_file_is_opencode_only_under_its_own_name() {
        let dir = scratch("sqlite-names");
        // Codex keeps SQLite state too: the magic bytes alone must not name a harness.
        let codex_state = dir.join("state_5.sqlite");
        touch(&codex_state, b"SQLite format 3\0rest of the file");
        assert!(detect_store(&codex_state).is_empty());
    }

    #[test]
    fn an_empty_projects_directory_is_not_a_claude_store() {
        let dir = scratch("empty-projects");
        std::fs::create_dir_all(dir.join("projects")).unwrap();
        assert!(detect_store(&dir).is_empty());
    }

    #[test]
    fn a_folder_named_projects_with_another_case_is_not_claude_code() {
        // `D:\Projects\<repo>\notes.jsonl` is not Claude Code's lowercase `projects/` store.
        let root = scratch("scan-case");
        touch(&root.join("Projects/repo/notes.jsonl"), b"{}\n");
        assert!(detect_store(&root).is_empty());
    }

    #[test]
    fn an_empty_or_missing_directory_finds_nothing() {
        let dir = scratch("empty");
        assert!(detect_store(&dir).is_empty());
        assert!(detect_store(&dir.join("nonesuch")).is_empty());
    }
}
