//! Claude Code's `history.jsonl`: the prompts the user typed, across every session.
//!
//! It is kept apart from the transcripts and outlives them: deleting a session's file does not
//! delete its prompts. Each line is `{display, pastedContents, project, sessionId, timestamp}`
//! (`timestamp` in milliseconds). Reading it is how a session whose transcript is gone, and the
//! project it ran in, are still seen. A line that is not that (damaged, from an older Claude Code
//! that wrote no `sessionId`) is skipped: a prompt without a session is not a session.

use serde::Deserialize;
use std::collections::HashMap;
use std::io::BufRead;
use std::path::{Path, PathBuf};

/// A session as `history.jsonl` knows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistorySession {
    pub id: String,
    pub project: String,
    /// The first prompt that is not a slash command, on one line.
    pub title: Option<String>,
    /// The time of the last prompt, in milliseconds since 1970.
    pub last_ms: i64,
}

/// Where the file is.
pub fn path(config_dir: &Path) -> PathBuf {
    config_dir.join("history.jsonl")
}

#[derive(Deserialize)]
struct Prompt {
    display: Option<String>,
    project: Option<String>,
    #[serde(rename = "sessionId")]
    session_id: Option<String>,
    timestamp: Option<i64>,
}

/// How long a title taken from a prompt may be.
const TITLE_CHARS: usize = 120;

/// Every session `history.jsonl` names, in the order they first appear. Empty when there is no
/// file or nothing in it can be read.
pub fn sessions(config_dir: &Path) -> Vec<HistorySession> {
    let Ok(file) = std::fs::File::open(path(config_dir)) else {
        return Vec::new();
    };
    let mut order: Vec<String> = Vec::new();
    let mut known: HashMap<String, (HistorySession, bool)> = HashMap::new();
    for line in std::io::BufReader::new(file).split(b'\n') {
        let Ok(line) = line else { break };
        let Ok(prompt) = serde_json::from_slice::<Prompt>(&line) else {
            continue;
        };
        let (Some(id), Some(project)) = (
            prompt.session_id.filter(|id| !id.is_empty()),
            prompt.project.filter(|project| !project.is_empty()),
        ) else {
            continue;
        };
        let at = prompt.timestamp.unwrap_or(0);
        let text = prompt.display.as_deref().map(str::trim).unwrap_or("");
        let entry = known.entry(id.clone()).or_insert_with(|| {
            order.push(id.clone());
            (
                HistorySession {
                    id,
                    project,
                    title: None,
                    last_ms: at,
                },
                false,
            )
        });
        entry.0.last_ms = entry.0.last_ms.max(at);
        if text.is_empty() {
            continue;
        }
        let command = text.starts_with('/');
        // The first prompt is the title, unless it is a slash command and a real prompt follows.
        if entry.0.title.is_none() || (!command && !entry.1) {
            entry.0.title = Some(one_line(text, TITLE_CHARS));
            entry.1 = !command;
        }
    }
    order
        .into_iter()
        .filter_map(|id| known.remove(&id).map(|(session, _)| session))
        .collect()
}

/// `text` on one line, its whitespace collapsed, cut to `max` characters.
fn one_line(text: &str, max: usize) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= max {
        return collapsed;
    }
    let mut cut: String = collapsed.chars().take(max).collect();
    cut.push('…');
    cut
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home_with(history: &str, tag: &str) -> crate::scratch::Scratch {
        let dir = crate::scratch::Scratch::new(&format!("history-{tag}"));
        std::fs::write(dir.join("history.jsonl"), history).unwrap();
        dir
    }

    #[test]
    fn a_session_is_its_prompts_with_the_last_time_and_the_first_real_prompt() {
        let dir = home_with(
            concat!(
                r#"{"display":"/model","project":"P","sessionId":"s1","timestamp":10}"#,
                "\n",
                r#"{"display":"  real\n  one ","project":"P","sessionId":"s1","timestamp":30}"#,
                "\n",
                r#"{"display":"later","project":"P","sessionId":"s1","timestamp":20}"#,
                "\n",
                r#"{"display":"/only-commands","project":"Q","sessionId":"s2","timestamp":5}"#,
                "\n",
            ),
            "aggregate",
        );
        let found = sessions(&dir);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].id, "s1");
        assert_eq!(found[0].title.as_deref(), Some("real one"));
        assert_eq!(found[0].last_ms, 30, "the latest time, whatever the order");
        assert_eq!(
            found[1].title.as_deref(),
            Some("/only-commands"),
            "better than none"
        );
    }
}
