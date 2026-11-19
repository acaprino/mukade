//! DeepSeek Harness's store: `sessions/{--<project>--,_no-cwd}/<session-id>/session.v<N>.jsonl[.zstd]`.

use super::folder::{Folder, count_files};
use super::layout::StoreLayout;
use crate::model::HarnessId;

pub(crate) struct DeepSeekHarnessLayout;

impl StoreLayout for DeepSeekHarnessLayout {
    fn harness(&self) -> HarnessId {
        HarnessId::DeepSeekHarness
    }

    fn evidence(&self, folder: &Folder) -> Vec<String> {
        let logs = folder
            .folder("sessions")
            .map_or(0, |sessions| count_logs(&Folder::read(&sessions)));
        if logs > 0 {
            vec![format!(
                "sessions/--<project>--/<session>/ holds {logs} session log(s)"
            )]
        } else {
            Vec::new()
        }
    }

    fn session_folders(&self) -> &'static [&'static str] {
        &["sessions"]
    }
}

/// A session log's own name: `session.jsonl` (format 0) or `session.v<N>.jsonl`, either
/// compressed (`.zstd`) or not.
fn is_log(name: &str) -> bool {
    let name = name.strip_suffix(".zstd").unwrap_or(name);
    let Some(stem) = name.strip_suffix(".jsonl") else {
        return false;
    };
    stem == "session"
        || stem.strip_prefix("session.v").is_some_and(|version| {
            !version.is_empty() && version.bytes().all(|b| b.is_ascii_digit())
        })
}

/// The logs one level below the project folders: each session has a folder of its own, unlike
/// Pi's files directly in the project folder.
fn count_logs(sessions: &Folder) -> usize {
    sessions
        .subfolders()
        .into_iter()
        .filter(|(name, _)| {
            *name == "_no-cwd" || (name.starts_with("--") && name.ends_with("--") && name.len() > 4)
        })
        .map(|(_, project)| {
            Folder::read(&project)
                .subfolders()
                .into_iter()
                .map(|(_, session)| count_files(&session, 1, &is_log))
                .sum::<usize>()
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use crate::detect::detect_store;
    use crate::detect::tests::{scratch, touch};
    use crate::model::HarnessId;

    #[test]
    fn a_dsh_home_is_recognized_by_its_session_logs() {
        let dir = scratch("dsh");
        touch(
            &dir.join("sessions/--D-work--/session-1/session.v4.jsonl.zstd"),
            b"\x28\xb5\x2f\xfd",
        );
        touch(
            &dir.join("sessions/_no-cwd/session-2/session.v4.jsonl"),
            b"{}\n",
        );
        let found = detect_store(&dir);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].harness, HarnessId::DeepSeekHarness);
        assert!(found[0].evidence[0].contains("2 session log"), "{found:?}");
    }

    /// Pi keeps its files directly in the project folder, DeepSeek Harness one folder deeper.
    #[test]
    fn pi_and_dsh_project_folders_are_told_apart() {
        let dir = scratch("dsh-not-pi");
        touch(&dir.join("sessions/--pi-project--/stamp_id.jsonl"), b"{}\n");
        let found = detect_store(&dir);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].harness, HarnessId::Pi);

        let dir = scratch("pi-not-dsh");
        touch(
            &dir.join("sessions/--D-work--/session-1/session.v4.jsonl"),
            b"{}\n",
        );
        let found = detect_store(&dir);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].harness, HarnessId::DeepSeekHarness);
    }

    #[test]
    fn only_a_session_log_name_counts() {
        let dir = scratch("dsh-names");
        touch(
            &dir.join("sessions/--D-work--/session-1/notes.jsonl"),
            b"{}\n",
        );
        touch(&dir.join("sessions/--D-work--/session-1/session.lock"), b"");
        assert!(detect_store(&dir).is_empty());
    }
}
