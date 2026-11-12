//! Claude Code subagent sessions: `projects/<p>/<parent-id>/subagents/agent-<agentId>.jsonl`,
//! every conversation row `isSidechain`, beside `agent-<agentId>.meta.json`, which names the agent
//! type, a description and the parent's call that started it. A child of a child is filed flat in
//! the root session's folder, its meta.json naming the agent that spawned it (`parentAgentId`).
//!
//! The agent id is unique only inside its parent, so a child's id is `<parent-id>.<agentId>`.

use crate::model::ParentKind;
use anyhow::{Context, Result};
use serde_json::Value;
use std::path::{Path, PathBuf};

/// What a subagent's meta.json says about it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct AgentMeta {
    /// `agentType`: the agent it ran as (`Explore`, `general-purpose`, `in_process_teammate`, …).
    pub agent_type: Option<String>,
    /// `description`: the short task the parent gave it, the name Claude Code shows it under.
    pub description: Option<String>,
    /// `toolUseId`: the parent's `Agent` call. A teammate has none.
    pub tool_use_id: Option<String>,
    /// `parentAgentId`: the agent that spawned it, for a child of a child.
    pub parent_agent_id: Option<String>,
}

/// The qualified id of agent `agent` of session `parent`.
pub(super) fn qualified(parent: &str, agent: &str) -> String {
    format!("{parent}.{agent}")
}

/// A qualified id's parent session id and agent id. A session id has no `.`; an agent id may.
pub(super) fn split(id: &str) -> Option<(&str, &str)> {
    id.split_once('.')
        .filter(|(parent, agent)| !parent.is_empty() && !agent.is_empty())
}

/// The agent id a transcript's file name gives: `agent-<agentId>.jsonl`.
pub(super) fn agent_id_of(transcript: &Path) -> Option<&str> {
    transcript
        .file_name()?
        .to_str()?
        .strip_prefix("agent-")?
        .strip_suffix(".jsonl")
        .filter(|agent| !agent.is_empty())
}

/// Whether `file` is a subagent transcript: `agent-<agentId>.jsonl` in a `subagents` folder.
pub(super) fn is_transcript(file: &Path) -> bool {
    agent_id_of(file).is_some()
        && file
            .parent()
            .and_then(Path::file_name)
            .is_some_and(|folder| folder == "subagents")
}

/// The meta.json beside a transcript. `None` when there is none or it is not a JSON object.
pub(super) fn agent_meta(transcript: &Path) -> Option<AgentMeta> {
    let agent = agent_id_of(transcript)?;
    let file = transcript.with_file_name(format!("agent-{agent}.meta.json"));
    let text = std::fs::read_to_string(file).ok()?;
    let doc: Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()?;
    let text_at = |key: &str| {
        doc.get(key)
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };
    doc.is_object().then(|| AgentMeta {
        agent_type: text_at("agentType"),
        description: text_at("description"),
        tool_use_id: text_at("toolUseId"),
        parent_agent_id: text_at("parentAgentId"),
    })
}

/// Where a child came from: the agent that spawned it, when its meta.json names one, else its
/// root session; and how, with the call and the agent type its meta.json records.
pub(super) fn lineage(session: &str, meta: Option<&AgentMeta>) -> (String, ParentKind) {
    let parent = match meta.and_then(|m| m.parent_agent_id.as_deref()) {
        Some(agent) => qualified(session, agent),
        None => session.to_string(),
    };
    let kind = ParentKind::Spawn {
        call_id: meta.and_then(|m| m.tool_use_id.clone()),
        agent: meta.and_then(|m| m.agent_type.clone()),
    };
    (parent, kind)
}

/// Every subagent transcript under a `projects/` folder, with the id of the session whose folder
/// it is in, in a stable order.
pub(super) fn transcripts(projects: &Path) -> Result<Vec<(PathBuf, String)>> {
    let mut out = Vec::new();
    let read = |dir: &Path| -> Result<Vec<std::fs::DirEntry>> {
        match std::fs::read_dir(dir) {
            Ok(entries) => entries
                .collect::<std::io::Result<Vec<_>>>()
                .with_context(|| format!("read {}", dir.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(e) => Err(e).with_context(|| format!("read {}", dir.display())),
        }
    };
    for project in read(projects)? {
        if !project.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        for session in read(&project.path())? {
            if !session.file_type().is_ok_and(|t| t.is_dir()) {
                continue;
            }
            let Some(parent) = session.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            let folder = session.path().join("subagents");
            if !folder.is_dir() {
                continue;
            }
            for file in read(&folder)? {
                let path = file.path();
                if file.file_type().is_ok_and(|t| t.is_file()) && agent_id_of(&path).is_some() {
                    out.push((path, parent.clone()));
                }
            }
        }
    }
    out.sort();
    Ok(out)
}

/// The transcripts of agent `agent` of session `parent`, matched exactly, in every project.
pub(super) fn find(projects: &Path, parent: &str, agent: &str) -> Vec<PathBuf> {
    let name = format!("agent-{agent}.jsonl");
    let Ok(entries) = std::fs::read_dir(projects) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = entries
        .flatten()
        .map(|project| project.path().join(parent).join("subagents").join(&name))
        .filter(|file| file.is_file())
        .collect();
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_qualified_id_splits_at_its_first_dot() {
        assert_eq!(split("s-1.a1"), Some(("s-1", "a1")));
        assert_eq!(split("s-1.amate.x-1"), Some(("s-1", "amate.x-1")));
        assert_eq!(split("s-1"), None);
        assert_eq!(split(".a1"), None);
        assert_eq!(split("s-1."), None);
    }

    #[test]
    fn only_an_agent_jsonl_in_a_subagents_folder_is_a_transcript() {
        let folder = Path::new("p").join("s").join("subagents");
        assert!(is_transcript(&folder.join("agent-a1.jsonl")));
        assert!(!is_transcript(&folder.join("agent-a1.meta.json")));
        assert!(!is_transcript(&folder.join("agent-a1.forked-skill.json")));
        assert!(!is_transcript(&folder.join("agent-.jsonl")));
        assert!(!is_transcript(&Path::new("p").join("agent-a1.jsonl")));
        assert_eq!(agent_id_of(&folder.join("agent-a1.jsonl")), Some("a1"));
    }

    #[test]
    fn a_child_of_a_child_names_the_agent_that_spawned_it() {
        let meta = AgentMeta {
            agent_type: Some("Explore".into()),
            tool_use_id: Some("toolu_1".into()),
            parent_agent_id: Some("a9".into()),
            ..AgentMeta::default()
        };
        assert_eq!(
            lineage("s-1", Some(&meta)),
            (
                "s-1.a9".to_string(),
                ParentKind::Spawn {
                    call_id: Some("toolu_1".into()),
                    agent: Some("Explore".into()),
                }
            )
        );
        assert_eq!(
            lineage("s-1", None),
            ("s-1".to_string(), ParentKind::spawn())
        );
    }
}
