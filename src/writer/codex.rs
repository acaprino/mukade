//! The Codex CLI 0.164.0 writer.
//!
//! Shape it must produce (docs/session-interchange-spec.md §2.2, L7):
//!
//! - `sessions\YYYY\MM\DD\rollout-<local ts>-<uuid>.jsonl`, one JSON object per line with
//!   `{timestamp, ordinal, type, payload}`;
//! - `session_meta` carrying **Codex's own fields**: `originator`, the **destination's**
//!   `cli_version` and the destination config's `model_provider` — the probe wrote the source's
//!   `cli_version` and `model_provider: "openai"`, which is the bug L7 exists for;
//! - no `state_5.sqlite` row: `threads.rollout_path` wins over the file on disk, and a row pointing
//!   at a missing file fails hard (P5). This writer never opens that database, and a fresh id
//!   cannot have a stale row. Codex rebuilds its SQLite stores from the rollout by itself;
//! - the conversation lives in `response_item` records; `turn_context` starts a turn.

use super::plan::{self, PlannedBlock, PlannedResult};
use super::{WriteOpts, Writer, Written, write_jsonl};
use crate::model::{HarnessId, Reasoning, Session};
use crate::policy::{Clock, ReasoningPlan};
use anyhow::{Context, Result};
use chrono::DateTime;
use serde_json::{Map, Value, json};
use std::path::PathBuf;

/// The Codex CLI writer.
#[derive(Debug, Clone, Copy, Default)]
pub struct Codex;

impl Writer for Codex {
    fn harness(&self) -> HarnessId {
        HarnessId::Codex
    }

    fn write_planned(
        &self,
        session: &Session,
        plan: &plan::Plan,
        opts: &WriteOpts,
    ) -> Result<Written> {
        super::validate_options(HarnessId::Codex, opts)?;
        let root = opts
            .roots
            .codex_home
            .clone()
            .context("CODEX_HOME is not set: nowhere to write the Codex rollout")?;
        let cwd = opts
            .cwd
            .clone()
            .or_else(|| session.meta.cwd.clone())
            .context("a working directory is required")?;
        let id = opts.dest_id.clone().unwrap_or_else(crate::policy::new_session_id);
        let mut clock = Clock::new(opts.now.as_deref());
        let stamp = clock.base_millis();
        let (date, file_stamp) = rollout_stamp(stamp);
        // Codex refuses a rollout whose `session_meta` has no `cli_version` ([V] 2026-10-28: the
        // same file resumes as soon as the key is there), so the destination's version is required,
        // not optional.
        let version = opts
            .dest_version
            .clone()
            .context("Codex needs the destination's cli_version (`--dest-version`): it refuses a rollout whose session_meta has none")?;

        let dir = root.join("sessions").join(date);
        std::fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
        let file = opts.dest_path.clone().unwrap_or_else(|| dir.join(format!("rollout-{file_stamp}-{id}.jsonl")));

        let rows = rows(plan, session, &id, &cwd, opts, &version, &mut clock);
        write_jsonl(&file, &rows)?;

        Ok(Written {
            id: id.clone(),
            location: file.to_string_lossy().into_owned(),
            resume: vec![
                opts.exe.clone().unwrap_or_else(|| "codex".into()),
                "exec".into(),
                "resume".into(),
                id,
            ],
        })
    }
}

/// The date folder and the file-name stamp for a rollout, both derived from one instant.
fn rollout_stamp(millis: i64) -> (PathBuf, String) {
    let Some(dt) = DateTime::from_timestamp_millis(millis) else {
        return (PathBuf::from("1970/01/01"), "1970-01-01T00-00-00".into());
    };
    let date = PathBuf::from(dt.format("%Y").to_string())
        .join(dt.format("%m").to_string())
        .join(dt.format("%d").to_string());
    (date, dt.format("%Y-%m-%dT%H-%M-%S").to_string())
}

fn rows(
    plan: &plan::Plan,
    session: &Session,
    id: &str,
    cwd: &str,
    opts: &WriteOpts,
    version: &str,
    clock: &mut Clock,
) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    let mut ordinal = 0u64;
    let mut turn = 0u64;

    let push =
        |out: &mut Vec<Value>, ordinal: &mut u64, kind: &str, payload: Value, clock: &mut Clock| {
            let mut row = Map::new();
            row.insert("timestamp".into(), Value::String(clock.next_iso()));
            row.insert("ordinal".into(), Value::from(*ordinal));
            row.insert("type".into(), Value::String(kind.to_string()));
            row.insert("payload".into(), payload);
            *ordinal += 1;
            out.push(Value::Object(row));
        };

    // L7: the destination's own session metadata. The key set is the one Codex itself accepts for
    // a hand-written rollout: `base_instructions` (null — we do not invent a system prompt) and
    // **no `session_id`** ([V]: with the probe's nine rollouts, and re-probed on 0.164.0 on
    // 2026-10-28 — the file is refused with `does not start with session metadata` without it).
    let mut meta = Map::new();
    meta.insert("base_instructions".into(), Value::Null);
    meta.insert("id".into(), Value::String(id.to_string()));
    meta.insert("timestamp".into(), Value::String(clock.next_iso()));
    meta.insert("cwd".into(), Value::String(cwd.to_string()));
    meta.insert("originator".into(), Value::String("codex_cli_rs".into()));
    meta.insert("source".into(), Value::String("cli".into()));
    meta.insert("cli_version".into(), Value::String(version.to_string()));
    if let Some(provider) = opts.model_provider.as_ref() {
        meta.insert("model_provider".into(), Value::String(provider.clone()));
    }
    if let Some(branch) = session.meta.git_branch.as_ref() {
        meta.insert("git".into(), json!({"branch": branch}));
    }
    push(
        &mut out,
        &mut ordinal,
        "session_meta",
        Value::Object(meta),
        clock,
    );

    // The environment block a native rollout carries at the top of the conversation: it states the
    // working directory and **the shell commands run with**, which is what lets a later read know
    // the dialect of this session's calls (L13).
    let mut environment = format!("<environment_context>\n  <cwd>{cwd}</cwd>\n");
    if let Some(shell) = opts.dest_shell {
        let name = match shell {
            crate::tools::Shell::Posix => "bash",
            crate::tools::Shell::PowerShell => "powershell",
        };
        environment.push_str(&format!("  <shell>{name}</shell>\n"));
    }
    environment.push_str("</environment_context>");
    push(
        &mut out,
        &mut ordinal,
        "response_item",
        json!({
            "type": "message",
            "id": format!("msg_{}", uuid::Uuid::new_v4().simple()),
            "role": "user",
            "content": [{"type": "input_text", "text": environment}],
        }),
        clock,
    );

    for block in &plan.blocks {
        match block {
            PlannedBlock::User { text, attachments } => {
                turn += 1;
                push(&mut out, &mut ordinal, "event_msg", json!({"type":"task_started", "turn_id":format!("turn-{turn}")}), clock);
                let mut content = vec![json!({"type": "input_text", "text": text})];
                content.extend(
                    attachments
                        .iter()
                        .filter_map(|a| (!a.raw.is_null()).then_some(a.raw.clone())),
                );
                push(
                    &mut out,
                    &mut ordinal,
                    "response_item",
                    json!({"type": "message", "id": format!("msg_{}", uuid::Uuid::new_v4().simple()), "role": "user", "content": content}),
                    clock,
                );
            }
            PlannedBlock::Compaction { summary: text } => {
                push(
                    &mut out,
                    &mut ordinal,
                    "response_item",
                    user_message(text),
                    clock,
                );
            }
            PlannedBlock::Step(step) => {
                let turn_id = format!("turn-{turn}");
                let model = plan::model_id(step, session, opts);
                let mut context = Map::new();
                context.insert("turn_id".into(), Value::String(turn_id.clone()));
                if let Some(model) = model.as_ref() {
                    context.insert("model".into(), Value::String(model.clone()));
                }
                context.insert("cwd".into(), Value::String(cwd.to_string()));
                push(
                    &mut out,
                    &mut ordinal,
                    "turn_context",
                    Value::Object(context),
                    clock,
                );

                for reasoning in &step.reasoning {
                    if let ReasoningPlan::Native(Reasoning { text, opaque }) = reasoning {
                        push(
                            &mut out,
                            &mut ordinal,
                            "response_item",
                            json!({
                                "type": "reasoning",
                                "summary": [{"type": "summary_text", "text": text}],
                                "content": null,
                                "encrypted_content": opaque.as_ref().and_then(|o| o.encrypted.clone()),
                            }),
                            clock,
                        );
                    }
                }
                if !step.text.is_empty() {
                    let content: Vec<Value> = step
                        .text
                        .iter()
                        .map(|t| json!({"type": "output_text", "text": t}))
                        .collect();
                    push(
                        &mut out,
                        &mut ordinal,
                        "response_item",
                        json!({"type": "message", "role": "assistant", "phase": if matches!(step.end, crate::model::StepEnd::Completed) { "final_answer" } else { "commentary" }, "content": content, "id": assistant_message_id()}),
                        clock,
                    );
                }
                for call in &step.calls {
                    push(
                        &mut out,
                        &mut ordinal,
                        "response_item",
                        json!({
                            "type": "function_call",
                            "name": call.name,
                            "arguments": serde_json::to_string(&call.input).unwrap_or_else(|_| "{}".into()),
                            "call_id": call.id,
                        }),
                        clock,
                    );
                }
                if matches!(step.end, crate::model::StepEnd::Completed) {
                    push(&mut out, &mut ordinal, "event_msg", json!({"type":"task_complete", "turn_id":turn_id, "last_agent_message":step.text.join("\n")}), clock);
                }
            }
            PlannedBlock::Results { results } => {
                for result in results {
                    match result {
                        PlannedResult::Native {
                            call_id, content, ..
                        } => push(
                            &mut out,
                            &mut ordinal,
                            "response_item",
                            json!({"type": "function_call_output", "call_id": call_id, "output": content.text()}),
                            clock,
                        ),
                        // A call that travelled as text answers with text, in the same place.
                        PlannedResult::Text(t) => push(
                            &mut out,
                            &mut ordinal,
                            "response_item",
                            user_message(t),
                            clock,
                        ),
                    }
                }
            }
        }
    }
    out
}

fn user_message(text: &str) -> Value {
    json!({
        "type": "message",
        "role": "user",
        "content": [{"type": "input_text", "text": text}],
    })
}

fn assistant_message_id() -> String {
    format!("msg_{}", uuid::Uuid::new_v4().simple())
}
