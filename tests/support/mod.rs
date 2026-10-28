//! Shared scaffolding for the conformance tests.
//!
//! Two rules hold for every test here: the source session is the fixture under `fixtures/`, and
//! everything written lands in a scratch directory under the system temporary folder — never a
//! real store, and never a harness's own home.
#![allow(dead_code)]

use mukade::model::{HarnessId, Kind, Protocol, Session};
use mukade::policy::ReasoningPolicy;
use mukade::reader::{claude_code, codex, opencode, pi};
use mukade::roots::StoreRoots;
use mukade::tools::Shell;
use mukade::writer::WriteOpts;
use std::path::{Path, PathBuf};

/// A fixed instant, so a written session is reproducible.
pub const NOW: &str = "2026-10-27T16:00:00.000Z";

const FIXTURES: &str = "fixtures";

/// The native source session each harness produced, recorded as a fixture.
pub fn source_path(harness: HarnessId) -> PathBuf {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(FIXTURES);
    match harness {
        HarnessId::ClaudeCode => base.join("claude_code/source/c45db7bc-542c-4201-89de-b590c6fdb797.jsonl"),
        HarnessId::Codex => {
            base.join("codex/source/rollout-2026-10-27T16-43-08-01a17e07-93f7-7a73-85c7-9f0205836600.jsonl")
        }
        HarnessId::OpenCode => base.join("opencode/source/rows.json"),
        HarnessId::Pi => base.join("pi/source/2026-10-27T16-00-39-689Z_01a17e17-9f07-743d-8072-8b3b13d65ab5.jsonl"),
    }
}

/// Read a harness's own source session with our reader.
pub fn read_source(harness: HarnessId) -> Session {
    let path = source_path(harness);
    match harness {
        HarnessId::ClaudeCode => claude_code::read_file(&path),
        HarnessId::Codex => codex::read_file(&path),
        HarnessId::OpenCode => opencode::read_file(&path),
        HarnessId::Pi => pi::read_file(&path),
    }
    .expect("the fixture must read")
}

/// A fresh scratch directory for one test, under the harness's own scratch area. The counter makes
/// every call unique: two tests that ask for the same tag (a shared helper, say) must not delete
/// each other's store while the other is still running.
pub fn scratch(harness: HarnessId, tag: &str) -> PathBuf {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let unique = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("mukade-tests")
        .join(harness.as_str())
        .join(format!("{tag}-{unique}"));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(base.join("home")).expect("create the scratch home");
    base
}

/// The working directory a hop files the session under: the destination's own project folder.
pub fn cwd_for(harness: HarnessId) -> String {
    std::env::temp_dir().join(format!("mukade-{}-work", harness.as_str())).display().to_string()
}

/// Write options aimed at a scratch root, with the destination's own facts stated.
pub fn opts(harness: HarnessId, scratch: &Path) -> WriteOpts {
    let home = scratch.join("home");
    let roots = StoreRoots {
        claude_config_dir: Some(home.clone()),
        codex_home: Some(home.clone()),
        opencode_db: Some(home.join("opencode.db")),
        pi_agent_dir: Some(home),
    };
    WriteOpts {
        roots,
        cwd: Some(cwd_for(harness)),
        dest_version: Some(version(harness).to_string()),
        dest_protocol: Some(protocol(harness)),
        dest_shell: Some(shell(harness)),
        offered: None,
        model_provider: Some("h27".to_string()),
        model: None,
        project_id: None,
        reasoning: ReasoningPolicy::DropForeign,
        title: Some("interchange test".to_string()),
        now: Some(NOW.to_string()),
        opencode_exe: Some("opencode".to_string()),
    }
}

fn version(harness: HarnessId) -> &'static str {
    match harness {
        HarnessId::ClaudeCode => "2.1.311",
        HarnessId::Codex => "0.164.0",
        HarnessId::OpenCode => "2.1.4",
        HarnessId::Pi => "1.2.0",
    }
}

/// The protocol the destination speaks; the bench's OpenCode and Pi both talked to an Anthropic
/// provider, which is what their own stores record.
pub fn protocol(harness: HarnessId) -> Protocol {
    match harness {
        HarnessId::ClaudeCode | HarnessId::OpenCode | HarnessId::Pi => Protocol::AnthropicMessages,
        HarnessId::Codex => Protocol::OpenaiResponses,
    }
}

fn shell(harness: HarnessId) -> Shell {
    match harness {
        HarnessId::Codex => Shell::PowerShell,
        _ => Shell::Posix,
    }
}

/// The conversation as a script, so two sessions can be compared independently of the metadata
/// each harness writes around it. Inputs are left out on purpose: a hop may rename arguments and
/// convert units, and that is not a loss.
pub fn script(session: &Session) -> Vec<String> {
    session
        .conversation()
        .map(|e| match &e.kind {
            Kind::User { text, .. } => format!("user|{text}"),
            Kind::Step { text, calls, end, .. } => {
                let calls: Vec<String> = calls
                    .iter()
                    .map(|c| format!("{}={:?}", c.id, c.canonical))
                    .collect();
                format!("step|{}|{}|{end:?}", text.join("\u{1}"), calls.join(","))
            }
            Kind::Results { results } => {
                let r: Vec<String> = results
                    .iter()
                    .map(|r| format!("{}={}={}", r.call_id, r.content.text(), r.is_error))
                    .collect();
                format!("results|{}", r.join(","))
            }
            // A writer renders a compaction as one user message, so that is what a round trip
            // must show.
            Kind::Compaction { summary, .. } => format!("user|{summary}"),
            Kind::Meta { .. } => "meta".to_string(),
        })
        .collect()
}

/// The conversation of a session with `Meta` entries removed, for comparisons.
pub fn conversation_script(session: &Session) -> Vec<String> {
    script(session).into_iter().filter(|s| s != "meta").collect()
}

/// The conversation as a round trip must show it: a step that lost **everything** (an assistant
/// turn with no text and no call, like OpenCode's killed row) is not written as an empty message —
/// every wire dialect refuses one — so it is not expected back. The plan records it in
/// `dropped_empty_steps` instead of dropping it silently.
pub fn roundtrip_script(session: &Session) -> Vec<String> {
    conversation_script(session).into_iter().filter(|s| !s.starts_with("step|||")).collect()
}

/// Read a whole JSON Lines file into values.
pub fn read_jsonl(path: &Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("each line is JSON"))
        .collect()
}
