//! Recognize which harness a store directory belongs to, from the layout its own files make.
//!
//! Detection is evidence, not a verdict: a directory can legitimately hold more than one
//! harness's store (the test homes do), so the answer is every candidate with what was seen. An
//! explicitly stated harness always wins over detection; inference is for a caller that has a
//! directory and nothing else. A candidate needs concrete files — session records, Pi's
//! `models.json`, OpenCode's `opencode.db`: an empty directory tree infers nothing.

use crate::model::HarnessId;
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
    if path.is_file() {
        // Only the name OpenCode itself uses: other harnesses keep SQLite state too (Codex's
        // `state_*.sqlite`), so the magic bytes alone must not decide.
        let named_opencode = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case("opencode.db"));
        return if named_opencode && is_sqlite(path) {
            vec![Detection {
                harness: HarnessId::OpenCode,
                evidence: vec![format!(
                    "{} is a SQLite database (OpenCode's store)",
                    path.display()
                )],
            }]
        } else {
            Vec::new()
        };
    }
    if !path.is_dir() {
        return Vec::new();
    }

    let mut out = Vec::new();

    // Claude Code: projects/<mangled cwd>/<uuid>.jsonl. The directory name alone is too common to
    // be evidence: at least one session file must be there.
    let mut claude = Vec::new();
    let projects = path.join("projects");
    if projects.is_dir() {
        let sessions = count_files(&projects, 2, &|name| name.ends_with(".jsonl"));
        if sessions > 0 {
            claude.push(format!(
                "projects/<project>/ holds {sessions} session .jsonl file(s)"
            ));
        }
    }
    if !claude.is_empty() {
        out.push(Detection {
            harness: HarnessId::ClaudeCode,
            evidence: claude,
        });
    }

    // Codex: sessions/YYYY/MM/DD/rollout-*.jsonl, plus the session catalog.
    let mut codex = Vec::new();
    let sessions = path.join("sessions");
    let rollouts = count_files(&sessions, 4, &|name| {
        name.starts_with("rollout-") && name.ends_with(".jsonl")
    });
    if rollouts > 0 {
        codex.push(format!(
            "sessions/ holds {rollouts} rollout-*.jsonl file(s)"
        ));
    }
    if path.join("session_index.jsonl").is_file() {
        codex.push("session_index.jsonl (Codex's session catalog)".into());
    }
    if !codex.is_empty() {
        out.push(Detection {
            harness: HarnessId::Codex,
            evidence: codex,
        });
    }

    // OpenCode: the SQLite database.
    let mut opencode = Vec::new();
    let db = path.join("opencode.db");
    if db.is_file() && is_sqlite(&db) {
        opencode.push("opencode.db is a SQLite database".into());
    }
    if !opencode.is_empty() {
        out.push(Detection {
            harness: HarnessId::OpenCode,
            evidence: opencode,
        });
    }

    // Pi: sessions/--<mangled cwd>--/<stamp>_<uuid>.jsonl, plus the provider configuration.
    let mut pi = Vec::new();
    let pi_sessions = count_pi_sessions(&sessions);
    if pi_sessions > 0 {
        pi.push(format!(
            "sessions/--<project>--/ holds {pi_sessions} session .jsonl file(s)"
        ));
    }
    if path.join("models.json").is_file() {
        pi.push("models.json (Pi's provider configuration)".into());
    }
    if !pi.is_empty() {
        out.push(Detection {
            harness: HarnessId::Pi,
            evidence: pi,
        });
    }

    out
}

/// Files under `dir` whose name matches, walking at most `depth` levels and a bounded number of
/// entries: detection must stay cheap even on a large store.
fn count_files(dir: &Path, depth: usize, matches: &dyn Fn(&str) -> bool) -> usize {
    fn walk(dir: &Path, depth: usize, matches: &dyn Fn(&str) -> bool, budget: &mut usize) -> usize {
        if depth == 0 || *budget == 0 {
            return 0;
        }
        let mut found = 0;
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                *budget = budget.saturating_sub(1);
                if *budget == 0 {
                    break;
                }
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().into_owned();
                if path.is_dir() {
                    found += walk(&path, depth - 1, matches, budget);
                } else if matches(&name) {
                    found += 1;
                }
            }
        }
        found
    }
    walk(dir, depth, matches, &mut 10_000)
}

/// Pi's project folders are unmistakable: `--<working directory>--`, one `.jsonl` per session.
fn count_pi_sessions(sessions: &Path) -> usize {
    let mut found = 0;
    if let Ok(entries) = std::fs::read_dir(sessions) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if entry.path().is_dir()
                && name.starts_with("--")
                && name.ends_with("--")
                && name.len() > 4
            {
                found += count_files(&entry.path(), 1, &|name| name.ends_with(".jsonl"));
            }
        }
    }
    found
}

/// The 16-byte header every SQLite database starts with.
fn is_sqlite(path: &Path) -> bool {
    use std::io::Read;
    let mut header = [0u8; 16];
    std::fs::File::open(path)
        .and_then(|mut file| file.read_exact(&mut header))
        .is_ok_and(|_| &header == b"SQLite format 3\0")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mukade-detect-{tag}-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn touch(path: &Path, content: &[u8]) {
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
        touch(&dir.join("models.json"), b"{}");
        let found = detect_store(&dir);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].harness, HarnessId::Pi);
        assert_eq!(found[0].evidence.len(), 2, "{:?}", found[0].evidence);
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
    fn an_empty_or_missing_directory_finds_nothing() {
        let dir = scratch("empty");
        assert!(detect_store(&dir).is_empty());
        assert!(detect_store(&dir.join("nonesuch")).is_empty());
    }
}
