//! The handoff note: what to give another harness so it can continue a session whose conversation
//! cannot be read but whose folder holds its work.
//!
//! The note is made from a template, `continue.md`. The default is embedded in the binary and
//! copied, once, to a folder beside the executable so it can be edited. This module composes the
//! note and the one-line first prompt that points at it; the command that starts the harness comes
//! from the catalog (`catalog::interactive_start`). It names no harness.

use crate::catalog;
use crate::model::HarnessId;
use crate::reader::SessionInfo;
use crate::work::{Work, lines};
use std::io::Write;
use std::path::{Path, PathBuf};

/// The folder, beside the executable, that holds the editable template.
pub const FOLDER: &str = "handoff-prompts";

/// The template file in [`FOLDER`].
const TEMPLATE_FILE: &str = "continue.md";

/// The template that ships with mukade.
pub const DEFAULT_TEMPLATE: &str = include_str!("../assets/handoff-prompts/continue.md");

/// The last line of the work: it is a remark, not an item.
const REMOTE_LINE: &str = "(remote state as of the last fetch)";

/// A template read from a checkout or a file with Windows line endings is the same template.
fn normalize(text: &str) -> String {
    text.replace("\r\n", "\n")
}

/// `<dir>/continue.md` when it exists and is not empty, else the default. `dir` is the folder
/// beside the executable; the default is written there once, best effort, so it can be edited.
pub fn template(dir: Option<&Path>) -> String {
    let default = normalize(DEFAULT_TEMPLATE);
    let Some(dir) = dir else {
        return default;
    };
    let path = dir.join(TEMPLATE_FILE);
    match std::fs::read_to_string(&path) {
        Ok(text) if !text.trim().is_empty() => normalize(&text),
        Ok(_) => default,
        Err(_) => {
            // Best effort: a folder that cannot be written only means the default is not editable.
            let _ = write_default(dir, &path, &default);
            default
        }
    }
}

/// Create `dir` and write the default into `path`, never replacing a file that is there.
fn write_default(dir: &Path, path: &Path, text: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(text.as_bytes())
}

/// The folder of the session, when it has one.
fn folder_of(info: &SessionInfo) -> Option<&str> {
    info.cwd
        .as_deref()
        .filter(|folder| !folder.trim().is_empty())
}

/// The work as list lines: a dash for each item, the sub-lines indented, the remark as it is.
fn work_text(work: &Work) -> String {
    if work.repositories.is_empty() {
        return work
            .note
            .clone()
            .unwrap_or_else(|| "nothing was found".to_string());
    }
    lines(work)
        .iter()
        .map(|line| {
            if let Some(sub) = line.strip_prefix("  ") {
                format!("  - {sub}")
            } else if line == REMOTE_LINE {
                line.clone()
            } else {
                format!("- {line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Replace each token of `values` in `template` in one pass, so a value that holds a token
/// (a title with `{id}` in it) is left as it is.
fn fill(template: &str, values: &[(&str, String)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open..];
        match values.iter().find(|(token, _)| after.starts_with(token)) {
            Some((token, value)) => {
                out.push_str(value);
                rest = &after[token.len()..];
            }
            None => {
                out.push('{');
                rest = &after[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// The note, from the template: `{title}`, `{harness}`, `{id}`, `{updated}`, `{reason}`, `{folder}`,
/// `{work}`.
pub fn note(template: &str, harness: HarnessId, info: &SessionInfo, work: &Work) -> String {
    let title = info
        .title
        .clone()
        .filter(|title| !title.trim().is_empty())
        .unwrap_or_else(|| info.id.clone());
    let updated = info
        .updated
        .clone()
        .unwrap_or_else(|| "unknown".to_string());
    let reason = info
        .unreadable
        .clone()
        .unwrap_or_else(|| "it was not read".to_string());
    let folder = match folder_of(info) {
        Some(folder) => folder.to_string(),
        None => work.folder.display().to_string(),
    };
    fill(
        template,
        &[
            ("{title}", title),
            ("{harness}", catalog::entry(harness).display.clone()),
            ("{id}", info.id.clone()),
            ("{updated}", updated),
            ("{reason}", reason),
            ("{folder}", folder),
            ("{work}", work_text(work)),
        ],
    )
}

/// `<data_dir>/handoffs/<harness>-<id>.md`, the id reduced to `[A-Za-z0-9._-]` (others become `_`).
pub fn note_path(data_dir: &Path, harness: HarnessId, id: &str) -> PathBuf {
    let safe: String = id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect();
    data_dir
        .join("handoffs")
        .join(format!("{}-{safe}.md", harness.as_str()))
}

/// Writes the note, creating its folder.
pub fn save(text: &str, path: &Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, text)
}

/// `Read <path> and continue from it.`
pub fn first_prompt(note_file: &Path) -> String {
    format!("Read {} and continue from it.", note_file.display())
}

/// Why a session cannot be handed off: it has no folder. `None`: it can.
pub fn refusal(info: &SessionInfo) -> Option<String> {
    if folder_of(info).is_some() {
        return None;
    }
    Some(
        "this session has no folder on this machine: there is nothing local to continue from"
            .to_string(),
    )
}

/// The harnesses a handoff can start, in `HarnessId::ALL` order.
pub fn destinations() -> Vec<HarnessId> {
    HarnessId::ALL
        .into_iter()
        .filter(|harness| catalog::interactive_start(*harness, "", "").is_some())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detect::tests::scratch;
    use crate::work::{Branch, Repository, Worktree};

    fn info() -> SessionInfo {
        SessionInfo {
            id: "6b00-ab".to_string(),
            title: Some("Port the parser".to_string()),
            cwd: Some("/work".to_string()),
            updated: Some("2026-11-01T10:00:00Z".to_string()),
            unreadable: Some("its conversation lives on the ChatGPT servers".to_string()),
            ..SessionInfo::default()
        }
    }

    fn work() -> Work {
        Work {
            folder: PathBuf::from("/work"),
            repositories: vec![Repository {
                path: PathBuf::from("/work/app"),
                worktrees: vec![Worktree {
                    path: PathBuf::from("/work/app"),
                    branch: Some("main".to_string()),
                    head: "abc1234".to_string(),
                    uncommitted: 2,
                }],
                branches: vec![
                    Branch {
                        name: "main".to_string(),
                        upstream: Some("origin/main".to_string()),
                        ahead: Some(0),
                        unpushed: 0,
                    },
                    Branch {
                        name: "side".to_string(),
                        upstream: None,
                        ahead: None,
                        unpushed: 3,
                    },
                ],
                error: None,
            }],
            note: None,
        }
    }

    #[test]
    fn the_note_names_the_session_the_reason_the_folder_and_the_work() {
        let work = work();
        let text = note(
            &normalize(DEFAULT_TEMPLATE),
            HarnessId::Codex,
            &info(),
            &work,
        );
        assert!(
            text.contains(
                "\"Port the parser\" (Codex CLI, id 6b00-ab, last active 2026-11-01T10:00:00Z)"
            ),
            "{text}"
        );
        assert!(
            text.contains(
                "Its conversation is not available here: its conversation lives on the ChatGPT servers."
            ),
            "{text}"
        );
        assert!(text.contains("Work it left in /work:\n"), "{text}");
        for line in lines(&work) {
            assert!(text.contains(line.trim()), "{line} is not in {text}");
        }
        assert!(
            text.contains(
                "- app: branch main, upstream origin/main, in sync, 2 uncommitted paths\n  - branch side, no upstream, 3 commits on no remote\n(remote state as of the last fetch)\n"
            ),
            "{text}"
        );
        assert!(!text.contains('{'), "{text}");
    }

    #[test]
    fn a_session_without_title_or_time_is_still_described() {
        let info = SessionInfo {
            id: "x1".to_string(),
            ..SessionInfo::default()
        };
        let work = Work {
            folder: PathBuf::from("/gone"),
            repositories: Vec::new(),
            note: None,
        };
        let text = note(
            &normalize(DEFAULT_TEMPLATE),
            HarnessId::ClaudeCode,
            &info,
            &work,
        );
        assert!(
            text.contains("\"x1\" (Claude Code, id x1, last active unknown)"),
            "{text}"
        );
        assert!(
            text.contains("not available here: it was not read."),
            "{text}"
        );
        assert!(
            text.contains("Work it left in /gone:\nnothing was found\n"),
            "{text}"
        );
    }

    #[test]
    fn work_without_repositories_is_its_note() {
        let work = Work {
            folder: PathBuf::from("/gone"),
            repositories: Vec::new(),
            note: Some("/gone does not exist".to_string()),
        };
        let text = note("{work}", HarnessId::Codex, &info(), &work);
        assert_eq!(text, "/gone does not exist");
    }

    #[test]
    fn a_value_that_holds_a_token_is_not_filled_again() {
        let mut info = info();
        info.title = Some("{id} and {work}".to_string());
        let text = note("{title}|{id}|{missing}", HarnessId::Codex, &info, &work());
        assert_eq!(text, "{id} and {work}|6b00-ab|{missing}");
    }

    #[test]
    fn a_session_without_a_cwd_shows_the_folder_of_its_work() {
        let mut info = info();
        info.cwd = None;
        let text = note("{folder}", HarnessId::Codex, &info, &work());
        assert_eq!(text, PathBuf::from("/work").display().to_string());
    }

    #[test]
    fn the_note_file_is_in_the_data_folder_named_by_harness_and_id() {
        assert_eq!(
            note_path(Path::new("D"), HarnessId::Codex, "6b00-ab"),
            Path::new("D").join("handoffs").join("codex-6b00-ab.md")
        );
    }

    #[test]
    fn the_note_file_name_keeps_only_safe_characters() {
        assert_eq!(
            note_path(Path::new("D"), HarnessId::Codex, "a/b\\c:d..e"),
            Path::new("D").join("handoffs").join("codex-a_b_c_d..e.md")
        );
    }

    #[test]
    fn the_note_is_saved_in_a_folder_made_for_it() {
        let dir = scratch("handoff-save");
        let path = note_path(&dir, HarnessId::Codex, "x");
        save("text", &path).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "text");
    }

    #[test]
    fn the_first_prompt_is_one_line_naming_the_file() {
        assert_eq!(
            first_prompt(Path::new("D/handoffs/codex-x.md")),
            format!(
                "Read {} and continue from it.",
                Path::new("D/handoffs/codex-x.md").display()
            )
        );
    }

    #[test]
    fn an_edited_template_replaces_the_default_and_an_empty_one_does_not() {
        let dir = scratch("handoff-edited");
        let dir: &Path = &dir;
        std::fs::write(dir.join(TEMPLATE_FILE), "custom {id}").unwrap();
        assert_eq!(template(Some(dir)), "custom {id}");
        for empty in ["", "  \n\t\n"] {
            std::fs::write(dir.join(TEMPLATE_FILE), empty).unwrap();
            assert_eq!(template(Some(dir)), template(None));
            // An empty file is the owner's to fix: it is not written over.
            assert_eq!(
                std::fs::read_to_string(dir.join(TEMPLATE_FILE)).unwrap(),
                empty
            );
        }
    }

    #[test]
    fn the_default_template_is_written_once_for_editing() {
        let base = scratch("handoff-once");
        let dir = base.join(FOLDER);
        assert_eq!(template(Some(dir.as_path())), template(None));
        let file = dir.join(TEMPLATE_FILE);
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            normalize(DEFAULT_TEMPLATE)
        );
        std::fs::write(&file, "mine").unwrap();
        assert_eq!(template(Some(dir.as_path())), "mine");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "mine");
    }

    #[test]
    fn the_template_does_not_depend_on_line_endings() {
        assert_eq!(normalize("a\r\nb {id}\r\n"), "a\nb {id}\n");
        assert!(!template(None).contains('\r'));
        let dir = scratch("handoff-crlf");
        let dir: &Path = &dir;
        std::fs::write(dir.join(TEMPLATE_FILE), "a\r\nb {id}\r\n").unwrap();
        assert_eq!(template(Some(dir)), "a\nb {id}\n");
    }

    #[test]
    fn a_session_without_a_folder_is_refused() {
        let mut info = info();
        assert_eq!(refusal(&info), None);
        info.cwd = None;
        let said = refusal(&info).expect("a refusal");
        assert!(said.contains("no folder"), "{said}");
        info.cwd = Some("  ".to_string());
        assert!(refusal(&info).is_some());
    }

    #[test]
    fn the_destinations_are_claude_code_and_codex() {
        assert_eq!(
            destinations(),
            vec![HarnessId::ClaudeCode, HarnessId::Codex]
        );
    }
}
