//! Conformance tests for the OpenCode 2.1.4 reader (docs/session-interchange-spec.md Â§9).
//!
//! Requirements exercised: L4 (`compaction` + `metadata.keepID`), L5/L8 (one assistant row is one
//! step, holding reasoning, text and both parallel calls with their results), L6 (`native.signature`),
//! L9 (an assistant row with no `finish` is a killed step â€” and `session export` omits it).
//!
//! Two fixtures: the store dump (`rows.json`, what the database holds) and the interchange export
//! (`session-export.json`, what `opencode session export` writes).

use mukade::model::{
    CanonicalTool, HarnessId, Kind, MetaKind, Protocol, ResultContent, Session, StepEnd, Trigger,
};
use mukade::reader::opencode;
use std::path::PathBuf;

mod support;

fn source(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/opencode/source")
        .join(name)
}

fn read_rows() -> Session {
    opencode::read_file(&source("rows.json")).expect("the store dump must read")
}

fn read_export() -> Session {
    opencode::read_file(&source("session-export.json")).expect("the export must read")
}

fn conversation(s: &Session) -> Vec<&mukade::model::Entry> {
    s.conversation().collect()
}

#[test]
fn meta_comes_from_the_session_record() {
    let s = read_rows();
    assert_eq!(s.meta.harness, HarnessId::OpenCode);
    assert_eq!(s.meta.harness_version, "2.1.4");
    assert_eq!(s.meta.id, "ses_e82023514ffe02y4T8OoUu99VZ");
    assert_eq!(
        s.meta.cwd.as_deref(),
        Some("C:/Users/alfio/AppData/Local/shinatama-verify/H27/opencode/work")
    );
    assert_eq!(s.meta.title.as_deref(), Some("H27-opencode-source"));
    assert_eq!(s.meta.models.len(), 1);
    assert_eq!(s.meta.models[0].provider.as_deref(), Some("anthropic"));
    assert_eq!(s.meta.models[0].id, "h27-mock");
    assert_eq!(s.meta.created.as_deref(), Some("2026-10-27T15:32:27.395Z"));
    assert_eq!(s.meta.updated.as_deref(), Some("2026-10-27T15:32:37.925Z"));
}

#[test]
fn every_store_row_is_kept_exactly_once() {
    let doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(source("rows.json")).unwrap()).unwrap();
    let rows = doc["session_message"].as_array().unwrap();
    let s = read_rows();
    let mut source: Vec<String> = rows.iter().map(|r| r.to_string()).collect();
    let mut kept: Vec<String> = s
        .entries
        .iter()
        .flat_map(|e| e.raw.iter())
        .map(|r| r.to_string())
        .collect();
    source.sort();
    kept.sort();
    assert_eq!(kept, source, "every row is kept verbatim, and only once");
}

#[test]
fn conversation_is_the_expected_entry_sequence() {
    let s = read_rows();
    let kinds: Vec<&str> = conversation(&s)
        .iter()
        .map(|e| support::kind_name(&e.kind))
        .collect();
    assert_eq!(
        kinds,
        vec![
            "user",    // T1
            "step",    // reasoning + text + two parallel read calls, results in the same row
            "results", // derived from that row
            "step",    // H27-FINAL
            "user",    // T2
            "step",    // H27-REPLY
            "compaction",
            "user", // T3
            "step", // T3 reply
            "user", // T4
            "step", // killed: empty text, no finish
        ]
    );
    assert_eq!(
        s.entries.len(),
        15,
        "14 rows become 15 entries (one row holds a step and its results)"
    );
}

#[test]
fn one_assistant_row_is_one_step_with_both_calls() {
    let s = read_rows();
    let conv = conversation(&s);
    let Kind::Step {
        model,
        reasoning,
        text,
        calls,
        end,
    } = &conv[1].kind
    else {
        panic!("expected a Step")
    };
    assert_eq!(model.as_ref().map(|m| m.id.as_str()), Some("h27-mock"));
    assert_eq!(text, &vec!["H27-PRE: reading the files.".to_string()]);
    assert_eq!(calls.len(), 2, "both parallel calls stay in one step");
    assert_eq!(calls[0].id, "toolu_h27call1x0");
    assert_eq!(calls[0].source_name, "read");
    assert_eq!(calls[0].canonical, Some(CanonicalTool::Read));
    assert!(
        calls[0].input["path"]
            .as_str()
            .unwrap_or_default()
            .ends_with("alpha.txt")
    );
    assert_eq!(*end, StepEnd::ToolUse);

    let o = reasoning[0]
        .opaque
        .as_ref()
        .expect("OpenCode stores native.signature");
    assert_eq!(o.protocol, Protocol::AnthropicMessages);
    assert_eq!(o.signature.as_deref(), Some("h27-mock-signature"));
    assert_eq!(o.model.as_deref(), Some("h27-mock"));

    let Kind::Results { results } = &conv[2].kind else {
        panic!("expected Results")
    };
    assert_eq!(results.len(), 2, "the row's tool parts carry both results");
    assert_eq!(results[0].call_id, calls[0].id);
    assert_eq!(results[1].call_id, calls[1].id);
    assert_eq!(
        results[0].content,
        ResultContent::Text("Read file C:\\Users\\alfio\\AppData\\Local\\shinatama-verify\\H27\\opencode\\work\\alpha.txt, lines 1-1\n1: ALPHA-CONTENT-42".into()),
        "the result text travels verbatim"
    );
}

#[test]
fn compaction_keeps_the_summary_and_the_native_boundary() {
    let s = read_rows();
    let conv = conversation(&s);
    let Kind::Compaction {
        summary,
        kept_from,
        trigger,
        tokens_before,
    } = &conv[6].kind
    else {
        panic!("expected the Compaction entry at index 6")
    };
    assert!(summary.starts_with("## Objective"), "{summary}");
    assert!(summary.contains("H27-SUMMARY: the user asked to read alpha.txt"));
    assert_eq!(*trigger, Trigger::Manual);
    assert_eq!(*tokens_before, Some(100));
    assert_eq!(
        kept_from.as_deref(),
        Some(conv[4].id.as_str()),
        "metadata.keepID names the T2 user row, the first message kept"
    );
}

/// L9: the row with no `finish` is a killed step. `session export` leaves it out, so the same
/// session read from the export ends on a user turn instead.
#[test]
fn a_row_without_finish_is_a_killed_step_and_the_export_omits_it() {
    let s = read_rows();
    let conv = conversation(&s);
    let last = conv.last().copied().unwrap();
    let Kind::Step {
        text, calls, end, ..
    } = &last.kind
    else {
        panic!("expected the killed assistant row last")
    };
    assert_eq!(*end, StepEnd::Killed);
    assert!(text.is_empty(), "the killed row holds an empty text block");
    assert!(calls.is_empty());
    assert!(
        s.interrupted_last_turn(),
        "L9: a killed step after the last user turn leaves that turn unanswered"
    );

    let exported = read_export();
    assert_eq!(
        exported.entries.len(),
        14,
        "the export omits the killed row"
    );
    let last = conversation(&exported).last().copied().unwrap();
    let Kind::User { text, .. } = &last.kind else {
        panic!("expected the T4 user turn last")
    };
    assert_eq!(text, "H27-HANG");
    assert!(exported.interrupted_last_turn());
}

#[test]
fn idle_markers_are_meta() {
    let s = read_rows();
    let idle = s
        .entries
        .iter()
        .filter(|e| {
            matches!(
                e.kind,
                Kind::Meta {
                    kind: MetaKind::Idle
                }
            )
        })
        .count();
    assert_eq!(idle, 4, "an idle record is a turn marker, not conversation");
}

fn export_doc() -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(source("session-export.json")).unwrap()).unwrap()
}

/// The tool parts of the export, mutable, in order.
fn tool_parts(doc: &mut serde_json::Value) -> Vec<&mut serde_json::Value> {
    doc["messages"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .filter_map(|m| {
            m.get_mut("content")
                .and_then(serde_json::Value::as_array_mut)
        })
        .flatten()
        .filter(|part| part["type"] == "tool")
        .collect()
}

fn results_of(s: &Session) -> Vec<mukade::model::ToolResult> {
    s.conversation()
        .filter_map(|e| match &e.kind {
            Kind::Results { results } => Some(results.clone()),
            _ => None,
        })
        .flatten()
        .collect()
}

/// L9: a tool still `pending` or `running` when OpenCode stopped has no result. The call stays
/// unanswered instead of "succeeding" with an empty output.
#[test]
fn a_tool_that_never_finished_has_no_result() {
    let mut doc = export_doc();
    let id = {
        let mut parts = tool_parts(&mut doc);
        let part = &mut parts[0];
        part["state"] =
            serde_json::json!({"status": "running", "input": part["state"]["input"].clone()});
        part["id"].as_str().unwrap().to_string()
    };
    let s = opencode::parse(&doc, None).unwrap();
    assert!(
        !results_of(&s).iter().any(|r| r.call_id == id),
        "{:?}",
        results_of(&s)
    );
}

/// A failed call keeps OpenCode's error text, which is where OpenCode records it.
#[test]
fn a_failed_tool_keeps_its_error_message() {
    let mut doc = export_doc();
    let id = {
        let mut parts = tool_parts(&mut doc);
        let part = &mut parts[0];
        part["state"] = serde_json::json!({
            "status": "error", "input": part["state"]["input"].clone(),
            "error": {"type": "tool.execution", "message": "ENOENT: no such file"},
        });
        part["id"].as_str().unwrap().to_string()
    };
    let s = opencode::parse(&doc, None).unwrap();
    let result = results_of(&s)
        .into_iter()
        .find(|r| r.call_id == id)
        .unwrap();
    assert!(result.is_error);
    assert_eq!(
        result.content,
        ResultContent::Text("ENOENT: no such file".into())
    );
}

/// A compaction OpenCode marked `failed` was never applied: it is not the source's compaction.
#[test]
fn a_failed_compaction_is_not_a_compaction() {
    let mut doc = export_doc();
    let messages = doc["messages"].as_array_mut().unwrap();
    let mut failed = messages
        .iter()
        .find(|m| m["type"] == "compaction")
        .unwrap()
        .clone();
    failed["id"] = "msg_failed_compaction".into();
    failed["status"] = "failed".into();
    failed["summary"] = "Compaction summary did not match the required template".into();
    messages.push(failed);
    let s = opencode::parse(&doc, None).unwrap();
    let last = s.last_compaction().unwrap();
    assert_ne!(last.id, "msg_failed_compaction");
    assert_eq!(
        s.conversation()
            .filter(|e| matches!(e.kind, Kind::Compaction { .. }))
            .count(),
        1
    );
}

/// Every model the session used, in first-seen order.
#[test]
fn every_model_is_listed_in_first_seen_order() {
    let mut doc = export_doc();
    let last_assistant = doc["messages"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .rev()
        .find(|m| m["type"] == "assistant")
        .unwrap();
    last_assistant["model"]["id"] = "model-b".into();
    let s = opencode::parse(&doc, None).unwrap();
    let ids: Vec<&str> = s.meta.models.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, ["h27-mock", "model-b"]);
}

/// Native order in the store is `seq`, not the order rows came back in.
#[test]
fn store_rows_are_read_in_seq_order() {
    let path = source("rows.json");
    let mut doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let before: Vec<String> = conversation(&opencode::parse(&doc, None).unwrap())
        .iter()
        .map(|e| e.id.clone())
        .collect();
    doc["session_message"].as_array_mut().unwrap().reverse();
    let after: Vec<String> = conversation(&opencode::parse(&doc, None).unwrap())
        .iter()
        .map(|e| e.id.clone())
        .collect();
    assert_eq!(after, before);
}

/// A reasoning part OpenCode stored through OpenRouter keeps its payload in
/// `native.reasoningDetails` (the shape of a live export, 2026-10-28): its signature or encrypted
/// data is the block's opaque part, and its format names the protocol.
#[test]
fn reasoning_details_are_read_as_the_opaque_payload() {
    let doc = serde_json::json!({
        "info": {"id": "ses_x", "directory": "D:/w", "time": {"created": 1, "updated": 2}},
        "messages": [
            {"id": "msg_u", "type": "user", "text": "17 times 23?", "time": {"created": 1}},
            {"id": "msg_a", "type": "assistant", "finish": "stop", "time": {"created": 2},
             "model": {"id": "anthropic/claude-haiku-5.5", "providerID": "openrouter"},
             "content": [
                {"type": "reasoning", "text": "",
                 "native": {"reasoningDetails": [{"type": "reasoning.text",
                    "format": "anthropic-claude-v1", "index": 0, "signature": "SIG"}]}},
                {"type": "reasoning", "text": "",
                 "native": {"reasoningDetails": [{"type": "reasoning.encrypted",
                    "format": "anthropic-claude-v1", "index": 1, "data": "DATA"}]}},
                {"type": "text", "text": "391"}]}
        ]
    });
    let s = mukade::reader::opencode::parse(&doc, None).unwrap();
    let reasoning = s
        .conversation()
        .find_map(|e| match &e.kind {
            Kind::Step { reasoning, .. } => Some(reasoning.clone()),
            _ => None,
        })
        .unwrap();
    let signed = reasoning[0].opaque.as_ref().unwrap();
    assert_eq!(signed.signature.as_deref(), Some("SIG"));
    assert_eq!(signed.protocol, mukade::model::Protocol::AnthropicMessages);
    let redacted = reasoning[1].opaque.as_ref().unwrap();
    assert_eq!(redacted.encrypted.as_deref(), Some("DATA"));
    assert_eq!(redacted.signature, None);
    assert_eq!(
        redacted.protocol,
        mukade::model::Protocol::AnthropicMessages
    );
}

/// A child session names the session that spawned it: `info.parentID` in an export (OpenCode
/// 2.2.5 `session/info.ts`), `parent_id` in a store row. A fork names its source in `info.fork`.
#[test]
fn a_child_names_its_parent_in_either_form_and_a_fork_its_source() {
    use mukade::model::ParentKind;
    let mut doc = export_doc();
    doc["info"]["parentID"] = "ses_parent".into();
    let child = opencode::parse(&doc, None).unwrap().meta;
    assert_eq!(child.parent.as_deref(), Some("ses_parent"));
    assert_eq!(child.parent_kind, Some(ParentKind::spawn()));

    let mut doc = export_doc();
    let boundary = serde_json::json!({"type": "through", "messageID": "msg_1"});
    doc["info"]["fork"] = serde_json::json!({"sessionID": "ses_source", "boundary": boundary});
    let fork = opencode::parse(&doc, None).unwrap().meta;
    assert_eq!(fork.parent.as_deref(), Some("ses_source"));
    assert_eq!(
        fork.parent_kind,
        Some(ParentKind::Fork {
            at: Some("msg_1".into())
        })
    );

    let mut rows: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(source("rows.json")).unwrap()).unwrap();
    rows["session_v2"][0]["parent_id"] = "ses_parent".into();
    let row = opencode::parse(&rows, None).unwrap().meta;
    assert_eq!(row.parent.as_deref(), Some("ses_parent"));
    assert_eq!(row.parent_kind, Some(ParentKind::spawn()));

    rows["session_v2"][0]["parent_id"] = serde_json::Value::Null;
    rows["session_v2"][0]["fork_session_id"] = "ses_source".into();
    rows["session_v2"][0]["fork_boundary"] =
        serde_json::json!({"type": "through", "messageID": "msg_1"});
    let forked_row = opencode::parse(&rows, None).unwrap().meta;
    assert_eq!(forked_row.parent.as_deref(), Some("ses_source"));
    assert_eq!(
        forked_row.parent_kind,
        Some(ParentKind::Fork {
            at: Some("msg_1".into())
        })
    );

    let root = read_export().meta;
    assert_eq!((root.parent, root.parent_kind), (None, None));
}

/// A subagent's report reaches the parent as a `synthetic` message (`metadata.source:
/// "subagent"`): the parent's model received it, so it is a user turn in place. Other synthetic
/// messages (a shell job's) stay records.
#[test]
fn a_subagents_report_is_a_user_turn_in_its_parent() {
    let mut doc = export_doc();
    let report = serde_json::json!({
        "id": "msg_report", "type": "synthetic", "time": {"created": 1793115157000_i64},
        "text": "<subagent sessionID=\"ses_child\" state=\"completed\">The tests pass.</subagent>",
        "description": "Run the tests",
        "metadata": {"source": "subagent", "childID": "ses_child", "agent": "general", "state": "completed"},
    });
    let shell = serde_json::json!({
        "id": "msg_shell", "type": "synthetic", "time": {"created": 1793115157001_i64},
        "text": "<shell>done</shell>", "description": "build",
        "metadata": {"source": "shell", "jobID": "j1", "shellID": "s1", "state": "completed"},
    });
    let messages = doc["messages"].as_array_mut().unwrap();
    messages.push(report);
    messages.push(shell);

    let session = opencode::parse(&doc, None).unwrap();

    let last_user = session
        .conversation()
        .filter_map(|entry| match &entry.kind {
            Kind::User { text, .. } => Some((entry.id.as_str(), text.as_str())),
            _ => None,
        })
        .last();
    assert_eq!(
        last_user,
        Some((
            "msg_report",
            "<subagent sessionID=\"ses_child\" state=\"completed\">The tests pass.</subagent>"
        ))
    );
    let shell = session.entry("msg_shell").unwrap();
    assert!(matches!(shell.kind, Kind::Meta { .. }), "{:?}", shell.kind);
}

/// A subagent report with no text gives the parent's model nothing: it stays a record, not an
/// empty user turn.
#[test]
fn an_empty_subagent_report_stays_a_record() {
    let mut doc = export_doc();
    doc["messages"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "id": "msg_empty_report", "type": "synthetic", "time": {"created": 1793115157000_i64},
            "metadata": {"source": "subagent", "childID": "ses_child", "state": "completed"},
        }));
    let session = opencode::parse(&doc, None).unwrap();
    let report = session.entry("msg_empty_report").unwrap();
    assert!(
        matches!(report.kind, Kind::Meta { .. }),
        "{:?}",
        report.kind
    );
}

/// A parent whose subagent reported into it hops into every file destination with the report.
#[test]
fn a_parent_with_a_report_hops_into_every_file_destination() {
    let mut doc = export_doc();
    doc["messages"].as_array_mut().unwrap().push(serde_json::json!({
        "id": "msg_report", "type": "synthetic", "time": {"created": 1793115157000_i64},
        "text": "<subagent sessionID=\"ses_child\" state=\"completed\">The tests pass.</subagent>",
        "metadata": {"source": "subagent", "childID": "ses_child", "agent": "general", "state": "completed"},
    }));
    let session = opencode::parse(&doc, None).unwrap();
    support::assert_hops_keeping(&session, "The tests pass.", "oc-report-hop");
}
