//! A read-only census of Codex Desktop code mode (spec L10) over the rollouts of a real Codex home.
//!
//! Ignored by default; it reads `CODEX_HOME` and never writes:
//! `CODEX_HOME=<home> cargo test --test codex_code_mode_census -- --ignored --nocapture`.

use mukade::model::{HarnessId, Kind};
use mukade::reader::codex;
use mukade::tools::{Shell, translate};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn rollouts(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rollouts(&path, out);
        } else if path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with("rollout-") && n.ends_with(".jsonl"))
        {
            out.push(path);
        }
    }
}

#[test]
#[ignore = "reads a real Codex home named by CODEX_HOME"]
fn census_of_code_mode_over_a_real_codex_home() {
    let Some(home) = std::env::var_os("CODEX_HOME") else {
        eprintln!("CODEX_HOME is not set: nothing to read");
        return;
    };
    let home = PathBuf::from(home);
    let mut files = Vec::new();
    rollouts(&home.join("sessions"), &mut files);
    rollouts(&home.join("archived_sessions"), &mut files);

    let (mut unreadable, mut execs, mut whole, mut split_scripts) = (0, 0, 0, 0);
    let mut by_tool: BTreeMap<String, usize> = BTreeMap::new();
    let mut native: BTreeMap<&str, usize> = BTreeMap::new();
    let mut versions: BTreeMap<String, usize> = BTreeMap::new();
    for file in &files {
        let Ok(session) = codex::read_file(file) else {
            unreadable += 1;
            continue;
        };
        *versions
            .entry(session.meta.harness_version.clone())
            .or_default() += 1;
        let calls: Vec<_> = session
            .entries
            .iter()
            .filter_map(|e| match &e.kind {
                Kind::Step { calls, .. } => Some(calls.clone()),
                _ => None,
            })
            .flatten()
            .collect();
        let results: Vec<_> = session
            .entries
            .iter()
            .filter_map(|e| match &e.kind {
                Kind::Results { results } => Some(results.clone()),
                _ => None,
            })
            .flatten()
            .collect();
        let exec_ids: Vec<String> = session
            .entries
            .iter()
            .flat_map(|e| e.raw.iter())
            .filter(|row| {
                row.pointer("/payload/type").and_then(|v| v.as_str()) == Some("custom_tool_call")
                    && row.pointer("/payload/name").and_then(|v| v.as_str()) == Some("exec")
            })
            .filter_map(|row| row.pointer("/payload/call_id")?.as_str().map(String::from))
            .collect();
        for id in exec_ids {
            execs += 1;
            if calls.iter().any(|c| c.id == id && c.source_name == "exec") {
                whole += 1;
                continue;
            }
            split_scripts += 1;
            let made: Vec<_> = calls
                .iter()
                .filter(|c| c.id == id || c.id.starts_with(&format!("{id}_")))
                .collect();
            assert!(!made.is_empty(), "{}: exec {id} vanished", file.display());
            for call in made {
                assert!(!call.id.contains('#'), "{}", call.id);
                let answers = results.iter().filter(|r| r.call_id == call.id).count();
                assert_eq!(
                    answers,
                    1,
                    "{}: {} has {answers} results",
                    file.display(),
                    call.id
                );
                *by_tool.entry(call.source_name.clone()).or_default() += 1;
                for (label, dest, shell) in [
                    ("claude", HarnessId::ClaudeCode, None),
                    (
                        "claude+powershell",
                        HarnessId::ClaudeCode,
                        Some(Shell::PowerShell),
                    ),
                    ("pi", HarnessId::Pi, None),
                    ("pi+powershell", HarnessId::Pi, Some(Shell::PowerShell)),
                    ("codex", HarnessId::Codex, None),
                    (
                        "codex+powershell",
                        HarnessId::Codex,
                        Some(Shell::PowerShell),
                    ),
                ] {
                    *native.entry(label).or_default() += 0;
                    if translate(call, dest, shell).is_call() {
                        *native.entry(label).or_default() += 1;
                    }
                }
            }
        }
    }
    println!("rollouts: {} (unreadable: {unreadable})", files.len());
    println!("codex versions: {versions:?}");
    println!("exec calls: {execs}; split: {split_scripts}; kept whole: {whole}");
    println!("calls made by split scripts, by tool: {by_tool:?}");
    println!(
        "of the {} calls made by split scripts, written as a native call: {native:?}",
        by_tool.values().sum::<usize>()
    );
}
