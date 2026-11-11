//! Conformance tests for the Codex CLI 0.164.0 reader (docs/session-interchange-spec.md Â§9).
//!
//! Requirements exercised: L4 (the `compacted` record becomes one `Compaction` entry, and the
//! assistant record that wrote the summary is not conversation), L5 (the conversation lives in
//! `response_item` records only), L6 (reasoning keeps `encrypted_content`), L9 (a graceful
//! `<turn_aborted>` marker and a hard-killed last turn).

use mukade::model::{
    CanonicalTool, HarnessId, Kind, MetaKind, Protocol, ResultContent, Session, StepEnd, Trigger,
};
use mukade::reader::codex;
use serde_json::Value;
use std::path::PathBuf;

mod support;

fn fixture() -> PathBuf {
    support::source_path(HarnessId::Codex)
}

fn read() -> Session {
    codex::read_file(&fixture()).expect("the fixture must read")
}

fn conversation(s: &Session) -> Vec<&mukade::model::Entry> {
    s.conversation().collect()
}

#[test]
fn meta_comes_from_the_session_header() {
    let s = read();
    assert_eq!(s.meta.harness, HarnessId::Codex);
    assert_eq!(s.meta.harness_version, "0.164.0");
    assert_eq!(s.meta.id, "01a17e07-93f7-7a73-85c7-9f0205836600");
    assert_eq!(
        s.meta.cwd.as_deref(),
        Some(r"C:\Users\alfio\AppData\Local\shinatama-verify\H27\codex\work")
    );
    assert_eq!(s.meta.models.len(), 1);
    assert_eq!(s.meta.models[0].provider.as_deref(), Some("h27"));
    assert_eq!(s.meta.models[0].id, "h27-mock");
    assert_eq!(s.meta.created.as_deref(), Some("2026-10-27T15:43:08.286Z"));
    assert_eq!(s.meta.updated.as_deref(), Some("2026-10-27T15:43:27.631Z"));
}

#[test]
fn listing_exposes_the_fixture_working_directory() {
    use mukade::reader::Reader;

    let scratch = support::scratch(HarnessId::Codex, "list-cwd");
    support::stage_fixture(HarnessId::Codex, &scratch);
    let listed = codex::Codex.list(&support::roots(&scratch)).unwrap();

    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, support::source_id(HarnessId::Codex));
    assert_eq!(
        listed[0].cwd.as_deref(),
        Some(r"C:\Users\alfio\AppData\Local\shinatama-verify\H27\codex\work")
    );
}

#[test]
fn listing_keeps_sessions_with_missing_or_damaged_headers() {
    use mukade::reader::Reader;

    let scratch = support::scratch(HarnessId::Codex, "list-bad-header");
    support::stage_fixture(HarnessId::Codex, &scratch);
    let roots = support::roots(&scratch);
    let file = codex::Codex.list(&roots).unwrap().remove(0).path;
    let headers: &[&[u8]] = &[
        b"",
        b"{unfinished\n",
        b"\xff\n",
        b"{\"type\":\"session_meta\",\"payload\":{}}\n",
        b"{\"type\":\"session_meta\",\"payload\":{\"cwd\":42}}\n",
        b"{\"type\":\"turn_context\",\"payload\":{\"cwd\":\"wrong\"}}\n",
    ];
    for header in headers {
        std::fs::write(&file, header).unwrap();
        let listed = codex::Codex.list(&roots).unwrap();
        assert_eq!(listed.len(), 1, "header: {header:?}");
        assert_eq!(listed[0].id, support::source_id(HarnessId::Codex));
        assert_eq!(listed[0].cwd, None, "header: {header:?}");
    }
}

#[test]
fn listing_reads_a_bom_header_without_decoding_the_transcript() {
    use mukade::reader::Reader;

    let scratch = support::scratch(HarnessId::Codex, "list-header-only");
    support::stage_fixture(HarnessId::Codex, &scratch);
    let roots = support::roots(&scratch);
    let file = codex::Codex.list(&roots).unwrap().remove(0).path;
    std::fs::write(
        &file,
        b"\xef\xbb\xbf{\"type\":\"session_meta\",\"payload\":{\"cwd\":\"C:\\\\work\"}}\n\xff\n",
    )
    .unwrap();

    let listed = codex::Codex.list(&roots).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].cwd.as_deref(), Some(r"C:\work"));
}

#[test]
fn listing_ignores_headers_over_one_mebibyte() {
    use mukade::reader::Reader;

    let scratch = support::scratch(HarnessId::Codex, "list-large-header");
    support::stage_fixture(HarnessId::Codex, &scratch);
    let roots = support::roots(&scratch);
    let file = codex::Codex.list(&roots).unwrap().remove(0).path;
    let header = serde_json::json!({"type": "session_meta", "payload": {"cwd": "large"}});
    std::fs::write(&file, format!("{header}{}\n", " ".repeat(1024 * 1024))).unwrap();

    let listed = codex::Codex.list(&roots).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, support::source_id(HarnessId::Codex));
    assert_eq!(listed[0].cwd, None);
}

/// Every source record is kept verbatim, exactly once. Codex interleaves events with the model's
/// records, so a step's rows are not adjacent in the file: the order checked is that of each
/// entry's *first* row, and the multiset of rows is compared whole.
#[test]
fn every_source_record_is_kept_exactly_once() {
    let text = std::fs::read_to_string(fixture()).unwrap();
    let rows: Vec<Value> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let s = read();

    let mut source_rows: Vec<String> = rows.iter().map(|r| r.to_string()).collect();
    let mut kept: Vec<String> = s
        .entries
        .iter()
        .flat_map(|e| e.raw.iter())
        .map(|r| r.to_string())
        .collect();
    source_rows.sort();
    kept.sort();
    assert_eq!(
        kept, source_rows,
        "rows are kept verbatim, none lost and none duplicated"
    );

    let position = |r: &Value| rows.iter().position(|x| x == r);
    let firsts: Vec<usize> = s
        .entries
        .iter()
        .filter_map(|e| e.raw.first().and_then(position))
        .collect();
    assert!(
        firsts.windows(2).all(|w| w[0] <= w[1]),
        "entries follow the native order: {firsts:?}"
    );
}

#[test]
fn conversation_is_the_expected_entry_sequence() {
    let s = read();
    let kinds: Vec<&str> = conversation(&s)
        .iter()
        .map(|e| support::kind_name(&e.kind))
        .collect();
    assert_eq!(
        kinds,
        vec![
            "user",       // T1
            "step",       // reasoning + text + two parallel exec_command calls
            "results",    // both function_call_outputs
            "step",       // H27-FINAL
            "user",       // T2
            "step",       // H27-REPLY
            "compaction", // compacted
            "user",       // T3
            "step",       // T3 reply
            "user",       // T4a, later aborted
            "user",       // T4b, killed while hanging
        ]
    );
    assert_eq!(s.entries.len(), 70, "75 rows become 70 entries");
}

#[test]
fn parallel_calls_and_reasoning_stay_in_one_step() {
    let s = read();
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
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].id, "call_h27call1x0");
    assert_eq!(calls[0].source_name, "exec_command");
    assert_eq!(
        calls[0].canonical,
        Some(CanonicalTool::PowerShell),
        "the session's <environment_context> says <shell>powershell</shell>, and the call names none"
    );
    assert!(
        calls[0].input["cmd"]
            .as_str()
            .unwrap_or_default()
            .contains("alpha.txt"),
        "the arguments string is decoded: {:?}",
        calls[0].input
    );
    assert_eq!(*end, StepEnd::ToolUse);

    let r = &reasoning[0];
    assert_eq!(
        r.text,
        "H27-REASONING: the user wants two files; read them in parallel."
    );
    let o = r.opaque.as_ref().expect("Codex stores encrypted_content");
    assert_eq!(o.protocol, Protocol::OpenaiResponses);
    assert_eq!(o.encrypted.as_deref(), Some("h27-mock-encrypted-reasoning"));
    assert_eq!(o.signature, None);
    assert_eq!(o.model.as_deref(), Some("h27-mock"));
}

#[test]
fn results_are_paired_by_call_id() {
    let s = read();
    let conv = conversation(&s);
    let Kind::Results { results } = &conv[2].kind else {
        panic!("expected Results")
    };
    let ids: Vec<&str> = results.iter().map(|r| r.call_id.as_str()).collect();
    assert_eq!(ids, vec!["call_h27call1x0", "call_h27call1x1"]);
    let Kind::Step { calls, .. } = &conv[1].kind else {
        panic!()
    };
    assert_eq!(calls.len(), results.len());
    for r in results {
        assert!(calls.iter().any(|c| c.id == r.call_id));
        assert!(
            r.content.text().starts_with("Chunk ID:"),
            "the output travels verbatim"
        );
        let ResultContent::Text(_) = r.content else {
            panic!("expected text")
        };
    }
}

/// L4: `compacted` becomes one entry holding the source summary; the assistant record that wrote
/// that summary is the compaction's own turn, so it is folded in rather than carried.
#[test]
fn compaction_keeps_the_summary_and_folds_the_summary_turn() {
    let s = read();
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
    assert!(
        summary.contains("H27-SUMMARY: the user asked to read alpha.txt"),
        "{summary}"
    );
    assert!(summary.starts_with("Another language model started to solve this problem"));
    assert_eq!(*trigger, Trigger::Unknown, "Codex records no trigger field");
    assert_eq!(*tokens_before, Some(100));
    assert_eq!(
        kept_from.as_deref(),
        Some(conv[7].id.as_str()),
        "the tail starts at T3"
    );

    // The summary the model wrote is not conversation: no step carries that turn's text.
    assert!(
        !s.entries.iter().any(|e| matches!(&e.kind, Kind::Step { text, .. }
            if text.iter().any(|t| t.contains("Another language model started to solve this problem")))),
        "the compaction turn is not a step"
    );
    // ...but it is still in the record, inside the compaction entry.
    let compaction = s
        .entries
        .iter()
        .find(|e| matches!(e.kind, Kind::Compaction { .. }))
        .unwrap();
    assert_eq!(
        compaction.raw.len(),
        2,
        "the summary record and the compacted record"
    );
    assert!(compaction.raw[0].to_string().contains("H27-SUMMARY"));
}

/// L9: the graceful `<turn_aborted>` travels as a marker, never as user text; the hard kill
/// leaves a user turn with no reply.
#[test]
fn aborts_are_markers_and_the_killed_turn_has_no_reply() {
    let s = read();
    let aborted: Vec<&MetaKind> = s
        .entries
        .iter()
        .filter_map(|e| match &e.kind {
            Kind::Meta { kind } if *kind == MetaKind::TurnAborted => Some(kind),
            _ => None,
        })
        .collect();
    assert_eq!(
        aborted.len(),
        2,
        "the <turn_aborted> record and the turn_aborted event"
    );
    assert!(
        !s.entries
            .iter()
            .any(|e| matches!(&e.kind, Kind::User { text, .. } if text.contains("<turn_aborted>"))),
        "the marker must not become user text"
    );

    assert!(s.interrupted_last_turn());
    let conv = conversation(&s);
    let Kind::User { text, .. } = &conv[10].kind else {
        panic!("expected the killed turn last")
    };
    assert_eq!(text, "H27-HANG T4b");
    let Kind::User {
        text: aborted_text, ..
    } = &conv[9].kind
    else {
        panic!()
    };
    assert_eq!(aborted_text, "H27-HANG T4a");
}

#[test]
fn internal_records_are_meta() {
    let s = read();
    let metas: Vec<MetaKind> = s
        .entries
        .iter()
        .filter_map(|e| match &e.kind {
            Kind::Meta { kind } => Some(*kind),
            _ => None,
        })
        .collect();
    let count = |k: MetaKind| metas.iter().filter(|m| **m == k).count();
    assert_eq!(count(MetaKind::Idle), 10, "task_started and task_complete");
    assert_eq!(count(MetaKind::ModelChange), 10, "thread_settings_applied");
    assert_eq!(
        count(MetaKind::SystemPrompt),
        2,
        "developer instructions and <environment_context>"
    );
    assert_eq!(
        count(MetaKind::Internal),
        35,
        "session_meta, token_count, item_completed, turn_context, token_usage_record, world_state"
    );
}

#[test]
fn incomplete_last_turn_still_works_with_no_conversation_at_all() {
    // A rollout with no conversation (only the header) must read, not fail.
    let rows = vec![serde_json::json!({
        "timestamp": "2026-10-27T15:43:08.360Z",
        "ordinal": 0,
        "type": "session_meta",
        "payload": {"session_id": "abc", "id": "abc", "cwd": r"C:\x", "cli_version": "0.164.0"}
    })];
    let s = codex::parse(&rows, None).unwrap();
    assert_eq!(s.meta.id, "abc");
    assert_eq!(s.entries.len(), 1);
    assert!(conversation(&s).is_empty());
    assert!(!s.interrupted_last_turn());
}

/// A prompt that only mentions a marker tag is the user's own words.
#[test]
fn a_prompt_that_mentions_a_marker_tag_is_a_user_turn() {
    let mut rows: Vec<serde_json::Value> = std::fs::read_to_string(fixture())
        .unwrap()
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    rows.push(serde_json::json!({
        "timestamp": "2026-10-27T16:50:00.000Z", "type": "response_item",
        "payload": {"type": "message", "role": "user", "content": [
            {"type": "input_text", "text": "What is the <environment_context> block in my rollout? LITERAL-USER"}]},
    }));
    let s = codex::parse(&rows, None).unwrap();
    let last = s.conversation().last().unwrap();
    let Kind::User { text, .. } = &last.kind else {
        panic!("{:?}", last.kind)
    };
    assert!(text.contains("LITERAL-USER"), "{text}");
}

fn rows_with(extra: Vec<serde_json::Value>) -> Session {
    let mut rows: Vec<serde_json::Value> = std::fs::read_to_string(fixture())
        .unwrap()
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    for payload in extra {
        rows.push(serde_json::json!({
            "timestamp": "2026-10-27T16:50:00.000Z", "type": "response_item", "payload": payload,
        }));
    }
    codex::parse(&rows, None).unwrap()
}

fn calls_of(s: &Session) -> Vec<mukade::model::ToolCall> {
    s.conversation()
        .filter_map(|e| match &e.kind {
            Kind::Step { calls, .. } => Some(calls.clone()),
            _ => None,
        })
        .flatten()
        .collect()
}

/// A `local_shell_call` has no `name` or `arguments`: its command is in `action`.
#[test]
fn a_local_shell_call_keeps_its_command() {
    let s = rows_with(vec![serde_json::json!({
        "type": "local_shell_call", "call_id": "call_ls1", "status": "completed",
        "action": {"type": "exec", "command": ["ls", "-la"]},
    })]);
    let call = calls_of(&s)
        .into_iter()
        .find(|c| c.id == "call_ls1")
        .unwrap();
    assert_eq!(call.source_name, "local_shell");
    assert_eq!(call.input["command"], serde_json::json!(["ls", "-la"]));
}

/// A web search the model ran is part of its step, not an internal record.
#[test]
fn a_web_search_call_is_part_of_the_step() {
    let s = rows_with(vec![serde_json::json!({
        "type": "web_search_call", "id": "ws_1", "status": "completed",
        "action": {"type": "search", "query": "WEBSEARCH-QUERY"},
    })]);
    let call = calls_of(&s).into_iter().find(|c| c.id == "ws_1").unwrap();
    assert_eq!(call.source_name, "web_search");
    assert_eq!(call.input["query"], "WEBSEARCH-QUERY");
}

/// A tool output given as content items stays structured: text as text, an image as an image.
#[test]
fn a_structured_tool_output_stays_structured() {
    let s = rows_with(vec![
        serde_json::json!({"type": "function_call", "call_id": "call_img", "name": "view_image", "arguments": "{}"}),
        serde_json::json!({"type": "function_call_output", "call_id": "call_img", "output": [
            {"type": "input_text", "text": "IMG-CAPTION"},
            {"type": "input_image", "image_url": "data:image/png;base64,iVBORw0KGgo="},
        ]}),
    ]);
    let result = s
        .conversation()
        .filter_map(|e| match &e.kind {
            Kind::Results { results } => Some(results.clone()),
            _ => None,
        })
        .flatten()
        .find(|r| r.call_id == "call_img")
        .unwrap();
    assert_eq!(
        result.content,
        ResultContent::Blocks(vec![
            serde_json::json!({"type": "text", "text": "IMG-CAPTION"}),
            serde_json::json!({"type": "input_image", "image_url": "data:image/png;base64,iVBORw0KGgo="}),
        ])
    );
}

/// Native order is the rollout's own order, never the timestamps.
#[test]
fn the_rollout_order_wins_over_timestamps() {
    let mut rows: Vec<serde_json::Value> = std::fs::read_to_string(fixture())
        .unwrap()
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let texts = |s: &Session| -> Vec<String> {
        s.conversation()
            .filter_map(|e| match &e.kind {
                Kind::User { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    };
    let before = texts(&codex::parse(&rows, None).unwrap());
    let n = rows.len();
    for (i, row) in rows.iter_mut().enumerate() {
        row["timestamp"] = format!("2026-10-27T15:{:02}:00.000Z", 59 - (i * 59 / n)).into();
    }
    assert_eq!(texts(&codex::parse(&rows, None).unwrap()), before);
}

// --- Paginated threads (Codex 0.165) ---------------------------------------------------------

const THREAD: &str = "01a181df-cfb6-79e2-8977-37b2cfde0b79";
const PAGE: &str = "01a1827c-9cd7-7180-8a25-b3d7e423149a";

fn record(ordinal: u64, kind: &str, payload: serde_json::Value) -> String {
    serde_json::json!({
        "timestamp": format!("2026-10-28T10:{:02}:00.000Z", ordinal), "ordinal": ordinal,
        "type": kind, "payload": payload,
    })
    .to_string()
        + "\n"
}

fn say(ordinal: u64, role: &str, text: &str) -> String {
    let block = if role == "user" {
        "input_text"
    } else {
        "output_text"
    };
    record(
        ordinal,
        "response_item",
        serde_json::json!({"type": "message", "role": role, "content": [{"type": block, "text": text}]}),
    )
}

/// A Codex home holding one thread over two pages: the base file, then a page that continues the
/// thread from the base's first three records (`history_base`), as Codex 0.165 writes a thread
/// whose later turns were redone. Returns the home, the base file and the page file.
fn paginated_home(tag: &str, offset_shift: i64) -> (PathBuf, PathBuf, PathBuf) {
    let home = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("mukade-tests")
        .join("codex-paginated")
        .join(format!("{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let day = home.join("sessions/2026/10/28");
    std::fs::create_dir_all(&day).unwrap();
    let meta = serde_json::json!({"id": THREAD, "session_id": THREAD, "cwd": r"D:\Projects\daodan",
                                  "cli_version": "0.165.0-alpha.1.2", "history_mode": "paginated"});
    let kept = record(0, "session_meta", meta.clone())
        + &say(1, "user", "first")
        + &say(2, "assistant", "one");
    let base_text =
        kept.clone() + &say(3, "user", "abandoned") + &say(4, "assistant", "abandoned reply");
    let base = day.join(format!("rollout-2026-10-28T10-38-11-{THREAD}.jsonl"));
    std::fs::write(&base, &base_text).unwrap();
    let mut page_meta = meta;
    page_meta["history_base"] = serde_json::json!({
        "thread_id": THREAD, "end_ordinal_exclusive": 3,
        "end_byte_offset": kept.len() as i64 + offset_shift,
    });
    let page_text = record(3, "session_meta", page_meta)
        + &say(4, "user", "redo")
        + &say(5, "assistant", "two");
    let page = day.join(format!("rollout-2026-10-28T13-29-27-{THREAD}_{PAGE}.jsonl"));
    std::fs::write(&page, &page_text).unwrap();
    (home, base, page)
}

fn texts(s: &Session) -> Vec<String> {
    s.conversation()
        .filter_map(|e| match &e.kind {
            Kind::User { text, .. } => Some(text.clone()),
            Kind::Step { text, .. } => Some(text.join("")),
            _ => None,
        })
        .collect()
}

fn roots(home: &std::path::Path) -> mukade::roots::StoreRoots {
    mukade::roots::StoreRoots {
        codex_home: Some(home.to_path_buf()),
        ..Default::default()
    }
}

/// Codex 0.165 keeps a thread whose later turns were redone over two files: the active page
/// starts from the base file's first records (`history_base`) and carries on in its own file,
/// whose name ends `<thread>_<page>`. The thread is that prefix and that page; the base's records
/// past the prefix are a branch the thread left, counted, not carried.
#[test]
fn a_paginated_thread_is_its_base_prefix_and_its_latest_page() {
    use mukade::reader::Reader;
    let (home, base, page) = paginated_home("read", 0);
    let s = codex::Codex
        .read(THREAD, &roots(&home))
        .expect("the thread reads");
    assert_eq!(texts(&s), ["first", "one", "redo", "two"]);
    assert_eq!(s.meta.id, THREAD);
    let off_branch: Vec<String> = s
        .entries
        .iter()
        .filter(|e| {
            matches!(
                e.kind,
                Kind::Meta {
                    kind: MetaKind::OffBranch
                }
            )
        })
        .map(|e| e.raw[0].to_string())
        .collect();
    assert_eq!(off_branch.len(), 2, "{off_branch:?}");
    assert!(
        off_branch.iter().all(|r| r.contains("abandoned")),
        "{off_branch:?}"
    );
    // The fingerprint is that of the bytes read: the base's prefix, then the page.
    let base_bytes = std::fs::read(&base).unwrap();
    let page_bytes = std::fs::read(&page).unwrap();
    let offset = std::str::from_utf8(&base_bytes)
        .unwrap()
        .match_indices('\n')
        .nth(2)
        .unwrap()
        .0
        + 1;
    let mut read_bytes = base_bytes[..offset].to_vec();
    read_bytes.extend_from_slice(&page_bytes);
    assert_eq!(
        s.meta.source_sha256.as_deref(),
        Some(mukade::sha256::sha256_hex(&read_bytes).as_str())
    );

    // The page's own file, read directly, is the same thread.
    let direct = codex::read_file(&page).expect("the page reads with its base");
    assert_eq!(texts(&direct), ["first", "one", "redo", "two"]);

    // Listed once, under the thread's id, at its active page.
    let listed = codex::Codex.list(&roots(&home)).unwrap();
    assert_eq!(listed.len(), 1, "{listed:?}");
    assert_eq!(listed[0].id, THREAD);
    assert_eq!(std::path::Path::new(&listed[0].path), page.as_path());
}

/// A page whose base is not there, or whose recorded byte offset does not fall where its ordinal
/// says, is refused with a reason: a guess would carry the wrong history.
#[test]
fn a_page_without_its_base_or_with_a_wrong_offset_is_refused() {
    let (home, base, page) = paginated_home("no-base", 0);
    std::fs::remove_file(&base).unwrap();
    let error = codex::read_file(&page).unwrap_err().to_string();
    assert!(error.contains("history_base"), "{error}");
    let _ = std::fs::remove_dir_all(&home);

    let (_, _, page) = paginated_home("bad-offset", 5);
    let error = codex::read_file(&page).unwrap_err().to_string();
    assert!(error.contains("end_byte_offset"), "{error}");
}

/// `Branches::All` reads a paginated thread whole: the base file, its abandoned turns included,
/// then the page's own records. Nothing is left behind, so nothing is counted as off-branch.
#[test]
fn every_branch_of_a_paginated_thread_reads_with_branches_all() {
    let (home, base, page) = paginated_home("all", 0);
    let s =
        codex::read_with(THREAD, &roots(&home), codex::Branches::All).expect("the thread reads");
    assert_eq!(
        texts(&s),
        [
            "first",
            "one",
            "abandoned",
            "abandoned reply",
            "redo",
            "two"
        ]
    );
    assert!(
        !s.entries.iter().any(|e| matches!(
            e.kind,
            Kind::Meta {
                kind: MetaKind::OffBranch
            }
        )),
        "nothing left behind"
    );
    let mut read_bytes = std::fs::read(&base).unwrap();
    read_bytes.extend_from_slice(&std::fs::read(&page).unwrap());
    assert_eq!(
        s.meta.source_sha256.as_deref(),
        Some(mukade::sha256::sha256_hex(&read_bytes).as_str())
    );
    let direct = codex::read_file_with(&page, codex::Branches::All).unwrap();
    assert_eq!(texts(&direct), texts(&s));
    // The default is the active part only.
    let active = codex::read_with(THREAD, &roots(&home), codex::Branches::Active).unwrap();
    assert_eq!(texts(&active), ["first", "one", "redo", "two"]);
}

/// A thread redone twice: base → page 1 → page 2, each page continuing from a prefix of the file
/// before it, with the thread's ordinals running on across files (as the one page seen does).
/// The latest version is read: the base's prefix, page 1's prefix, then page 2. What each earlier
/// file holds past its cut is left behind and counted; `Branches::All` carries every file whole.
#[test]
fn a_chain_of_pages_reads_as_its_latest_version() {
    use mukade::reader::Reader;
    let home = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("mukade-tests")
        .join("codex-paginated")
        .join(format!("chain-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let day = home.join("sessions/2026/10/28");
    std::fs::create_dir_all(&day).unwrap();
    let meta = serde_json::json!({"id": THREAD, "session_id": THREAD, "cwd": r"D:\Projects\daodan",
                                  "history_mode": "paginated"});
    let page_meta = |ordinal: u64, end: u64, bytes: usize| {
        let mut m = meta.clone();
        m["history_base"] = serde_json::json!({"thread_id": THREAD, "end_ordinal_exclusive": end,
                                               "end_byte_offset": bytes});
        record(ordinal, "session_meta", m)
    };
    // base: ordinals 0-4; page 1 continues from its first 3 records.
    let base_kept = record(0, "session_meta", meta.clone())
        + &say(1, "user", "first")
        + &say(2, "assistant", "one");
    let base_text =
        base_kept.clone() + &say(3, "user", "abandoned") + &say(4, "assistant", "abandoned reply");
    // page 1: ordinals 3-7; page 2 continues from the thread's first 6 records (3 of page 1's).
    let page1_kept =
        page_meta(3, 3, base_kept.len()) + &say(4, "user", "redo") + &say(5, "assistant", "two");
    let page1_text = page1_kept.clone()
        + &say(6, "user", "abandoned again")
        + &say(7, "assistant", "reply again");
    // page 2: ordinals 6-8.
    let page2_text = page_meta(6, 6, page1_kept.len())
        + &say(7, "user", "again")
        + &say(8, "assistant", "three");
    let base = day.join(format!("rollout-2026-10-28T10-38-11-{THREAD}.jsonl"));
    let page1 = day.join(format!("rollout-2026-10-28T13-29-27-{THREAD}_{PAGE}.jsonl"));
    let page2 = day.join(format!(
        "rollout-2026-10-28T15-20-00-{THREAD}_01a1830a-0000-7000-8000-000000000002.jsonl"
    ));
    std::fs::write(&base, &base_text).unwrap();
    std::fs::write(&page1, &page1_text).unwrap();
    std::fs::write(&page2, &page2_text).unwrap();

    let s = codex::Codex
        .read(THREAD, &roots(&home))
        .expect("the thread reads");
    assert_eq!(texts(&s), ["first", "one", "redo", "two", "again", "three"]);
    let left: Vec<String> = s
        .entries
        .iter()
        .filter(|e| {
            matches!(
                e.kind,
                Kind::Meta {
                    kind: MetaKind::OffBranch
                }
            )
        })
        .map(|e| e.raw[0].to_string())
        .collect();
    assert_eq!(left.len(), 4, "{left:?}");
    let read_bytes = [
        base_kept.as_bytes(),
        page1_kept.as_bytes(),
        page2_text.as_bytes(),
    ]
    .concat();
    assert_eq!(
        s.meta.source_sha256.as_deref(),
        Some(mukade::sha256::sha256_hex(&read_bytes).as_str())
    );

    let all = codex::read_with(THREAD, &roots(&home), codex::Branches::All).unwrap();
    assert_eq!(
        texts(&all),
        [
            "first",
            "one",
            "abandoned",
            "abandoned reply",
            "redo",
            "two",
            "abandoned again",
            "reply again",
            "again",
            "three"
        ]
    );
    let every_byte = [
        base_text.as_bytes(),
        page1_text.as_bytes(),
        page2_text.as_bytes(),
    ]
    .concat();
    assert_eq!(
        all.meta.source_sha256.as_deref(),
        Some(mukade::sha256::sha256_hex(&every_byte).as_str())
    );

    let listed = codex::Codex.list(&roots(&home)).unwrap();
    assert_eq!(listed.len(), 1, "{listed:?}");
    assert_eq!(std::path::Path::new(&listed[0].path), page2.as_path());
}

/// A subagent's rollout names its own thread in `id` and the thread that spawned it in
/// `session_id` (Codex Desktop, seen on 971 of 982 rollouts on 2026-10-28; Codex's own
/// `state_5.sqlite` keys the subagent's thread by `id`). The session is the rollout's own thread.
#[test]
fn a_subagent_rollout_is_its_own_thread_not_the_one_that_spawned_it() {
    let child = "01a164f1-6c9e-7173-8fea-c06263ae5eca";
    let root = "01a164f0-97a1-7433-b313-b15481e862d4";
    let header = |payload: serde_json::Value| {
        vec![
            serde_json::json!({"timestamp": "2026-10-22T19:48:26.000Z", "ordinal": 0,
                               "type": "session_meta", "payload": payload}),
            serde_json::json!({"timestamp": "2026-10-22T19:48:27.000Z", "ordinal": 1,
                               "type": "response_item", "payload": {"type": "message", "role": "user",
                               "content": [{"type": "input_text", "text": "task"}]}}),
        ]
    };
    let subagent = header(serde_json::json!({
        "id": child, "session_id": root, "cwd": r"D:\Projects\daodan", "cli_version": "0.165.0",
        "thread_source": "subagent",
        "source": {"subagent": {"thread_spawn": {"parent_thread_id": root, "depth": 1}}},
    }));
    assert_eq!(codex::parse(&subagent, None).unwrap().meta.id, child);

    // A hand-written rollout may carry only one of the two.
    let only_session_id = header(serde_json::json!({"session_id": root, "cwd": r"C:\x"}));
    assert_eq!(codex::parse(&only_session_id, None).unwrap().meta.id, root);
    let only_id = header(serde_json::json!({"id": child, "cwd": r"C:\x"}));
    assert_eq!(codex::parse(&only_id, None).unwrap().meta.id, child);
}

/// Codex Desktop 0.165 sends the project's AGENTS.md as a user message
/// (`# AGENTS.md instructions for <dir>`), and records app events such as
/// `<external_codex_apps_open_page>` as user messages too. Neither is the user's prompt; the
/// user's answer to a question (`<send_user_message_question_reply>`) is.
#[test]
fn injected_instructions_and_app_events_are_not_user_prompts() {
    let s = rows_with(vec![
        serde_json::json!({"type": "message", "role": "user", "content": [{"type": "input_text",
            "text": "# AGENTS.md instructions for D:\\Projects\\daodan\n\n<INSTRUCTIONS>\nrules\n</INSTRUCTIONS>"}]}),
        serde_json::json!({"type": "message", "role": "user", "content": [{"type": "input_text",
            "text": "<external_codex_apps_open_page>{\"page_id\":null}</external_codex_apps_open_page>"}]}),
        serde_json::json!({"type": "message", "role": "user", "content": [{"type": "input_text",
            "text": "<send_user_message_question_reply> [{\"answer\":\"yes\"}]"}]}),
    ]);
    let users: Vec<String> = s
        .conversation()
        .filter_map(|e| match &e.kind {
            Kind::User { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect();
    assert!(
        !users.iter().any(|t| t.contains("AGENTS.md instructions")),
        "{users:?}"
    );
    assert!(
        !users
            .iter()
            .any(|t| t.contains("external_codex_apps_open_page")),
        "{users:?}"
    );
    assert!(
        users
            .iter()
            .any(|t| t.contains("send_user_message_question_reply")),
        "{users:?}"
    );
    let metas: Vec<&MetaKind> = s
        .entries
        .iter()
        .filter_map(|e| match &e.kind {
            Kind::Meta { kind } => Some(kind),
            _ => None,
        })
        .collect();
    assert!(metas.contains(&&MetaKind::SystemPrompt));
}

/// A thread's name is in `<CODEX_HOME>/session_index.jsonl`, an append-only catalogue of
/// `{id, thread_name, updated_at}` where the newest entry wins; it is the name the Codex app shows.
#[test]
fn the_thread_name_comes_from_the_session_index() {
    use mukade::reader::Reader;
    let (home, _, page) = paginated_home("name", 0);
    let index = [
        serde_json::json!({"id": THREAD, "thread_name": "First name", "updated_at": "2026-10-28T09:38:20Z"}),
        serde_json::json!({"id": "someone-else", "thread_name": "Other", "updated_at": "2026-10-28T11:00:00Z"}),
        serde_json::json!({"id": THREAD, "thread_name": "Renamed", "updated_at": "2026-10-28T10:00:00Z"}),
    ]
    .iter()
    .map(|v| v.to_string() + "\n")
    .collect::<String>()
        + "not json\n";
    std::fs::write(home.join("session_index.jsonl"), index).unwrap();
    let s = codex::Codex.read(THREAD, &roots(&home)).unwrap();
    assert_eq!(s.meta.title.as_deref(), Some("Renamed"));
    assert_eq!(
        codex::read_file(&page).unwrap().meta.title.as_deref(),
        Some("Renamed")
    );
    std::fs::remove_file(home.join("session_index.jsonl")).unwrap();
    assert_eq!(
        codex::Codex.read(THREAD, &roots(&home)).unwrap().meta.title,
        None
    );
}

/// The list carries the same name, read from the catalogue once per `list` and never from the
/// rollouts (docs/superpowers/specs/2026-10-30-mukade-ui-design.md §7.1): the explorer shows the
/// thread's name instead of its uuid.
#[test]
fn listing_shows_the_thread_name_from_the_session_index() {
    use mukade::reader::Reader;
    let (home, _, _) = paginated_home("list-name", 0);
    let index = [
        serde_json::json!({"id": THREAD, "thread_name": "First name", "updated_at": "2026-10-28T09:38:20Z"}),
        serde_json::json!({"id": "someone-else", "thread_name": "Other", "updated_at": "2026-10-28T11:00:00Z"}),
        serde_json::json!({"id": THREAD, "thread_name": "Renamed", "updated_at": "2026-10-28T10:00:00Z"}),
    ]
    .iter()
    .map(|v| v.to_string() + "\n")
    .collect::<String>()
        + "not json\n";
    std::fs::write(home.join("session_index.jsonl"), index).unwrap();
    let listed = codex::Codex.list(&roots(&home)).unwrap();
    assert_eq!(listed.len(), 1, "{listed:?}");
    assert_eq!(listed[0].id, THREAD);
    assert_eq!(listed[0].title.as_deref(), Some("Renamed"));
    std::fs::remove_file(home.join("session_index.jsonl")).unwrap();
    assert_eq!(codex::Codex.list(&roots(&home)).unwrap()[0].title, None);
}

fn rollout_record(ordinal: u64, kind: &str, payload: Value) -> Value {
    serde_json::json!({
        "timestamp": format!("2026-11-10T10:00:{ordinal:02}.000Z"),
        "ordinal": ordinal, "type": kind, "payload": payload,
    })
}

fn header() -> Value {
    rollout_record(
        0,
        "session_meta",
        serde_json::json!({"session_id": "abc", "id": "abc", "cwd": r"C:\x", "cli_version": "0.166.0"}),
    )
}

fn prompt(ordinal: u64, text: &str) -> Value {
    rollout_record(
        ordinal,
        "response_item",
        serde_json::json!({"type": "message", "id": format!("msg_u{ordinal}"), "role": "user",
            "content": [{"type": "input_text", "text": text}]}),
    )
}

fn answer(ordinal: u64, turn: &str, text: &str) -> Value {
    rollout_record(
        ordinal,
        "response_item",
        serde_json::json!({"type": "message", "id": format!("msg_a{ordinal}"), "role": "assistant",
            "content": [{"type": "output_text", "text": text}],
            "internal_chat_message_metadata_passthrough": {"turn_id": turn}}),
    )
}

/// The two records a graceful interrupt writes: the marker message and the event.
fn abort(ordinal: u64, turn: &str) -> [Value; 2] {
    [
        rollout_record(
            ordinal,
            "response_item",
            serde_json::json!({"type": "message", "id": format!("msg_x{ordinal}"), "role": "user",
                "content": [{"type": "input_text", "text": "<turn_aborted>\nThe user interrupted the previous turn on purpose.\n</turn_aborted>"}],
                "internal_chat_message_metadata_passthrough": {"turn_id": turn}}),
        ),
        rollout_record(
            ordinal + 1,
            "event_msg",
            serde_json::json!({"type": "turn_aborted", "turn_id": turn, "reason": "interrupted"}),
        ),
    ]
}

/// L9: what the model had written when the user interrupted is kept, and the step says it was
/// aborted: a destination must not take a cut-off reply for a finished one.
#[test]
fn a_reply_cut_by_an_abort_is_an_aborted_step() {
    let mut rows = vec![
        header(),
        prompt(1, "PROMPT-1"),
        answer(2, "turn-A", "PARTIAL-1"),
    ];
    rows.extend(abort(3, "turn-A"));
    let s = codex::parse(&rows, None).unwrap();
    let conv = conversation(&s);
    assert_eq!(conv.len(), 2, "{conv:?}");
    let Kind::Step { text, end, .. } = &conv[1].kind else {
        panic!("{:?}", conv[1].kind)
    };
    assert_eq!(text, &["PARTIAL-1".to_string()]);
    assert_eq!(*end, StepEnd::Aborted);
    assert!(s.interrupted_last_turn());
}

/// An abort names its turn: a reply that an earlier turn finished is not the one it cut.
#[test]
fn an_abort_of_another_turn_leaves_a_finished_reply_alone() {
    let mut rows = vec![header(), prompt(1, "PROMPT-1"), answer(2, "turn-A", "DONE-1")];
    rows.extend(abort(3, "turn-B"));
    let s = codex::parse(&rows, None).unwrap();
    let conv = conversation(&s);
    let Kind::Step { end, .. } = &conv[1].kind else {
        panic!("{:?}", conv[1].kind)
    };
    assert_eq!(*end, StepEnd::Completed);
}

/// L10: in a Codex Desktop rollout a code-mode `exec` script that is exactly one shell call reads
/// as that call and reaches Claude Code as its `Bash` tool; a script of several calls stays `exec`
/// and travels as text. Both records are kept verbatim.
#[test]
fn a_one_call_code_mode_script_reads_as_the_call_it_wraps() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("mukade-tests")
        .join("codex-code-mode")
        .join(std::process::id().to_string());
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let one = r#"text(await tools.exec_command({cmd: "git status", yield_time_ms: 1000}));"#;
    let many = r#"const r = await Promise.allSettled([tools.exec_command({cmd: "a"}), tools.exec_command({cmd: "b"})]); text(r);"#;
    let exec = |ordinal, id: &str, script: &str| {
        record(ordinal, "response_item", serde_json::json!({
            "type": "custom_tool_call", "call_id": id, "name": "exec", "input": script,
        }))
    };
    let output = |ordinal, id: &str| {
        record(ordinal, "response_item", serde_json::json!({
            "type": "custom_tool_call_output", "call_id": id,
            "output": [
                {"type": "input_text", "text": "Script completed
Wall time 0.1 seconds
Output:
"},
                {"type": "input_text", "text": "On branch main"},
            ],
        }))
    };
    let text = record(0, "session_meta", serde_json::json!({
            "id": THREAD, "cwd": "/work", "cli_version": "0.166.0", "originator": "codex_work_desktop",
        }))
        + &say(1, "user", "status?")
        + &exec(2, "call_one", one)
        + &output(3, "call_one")
        + &exec(4, "call_many", many)
        + &output(5, "call_many")
        + &say(6, "assistant", "clean");
    let file = dir.join(format!("rollout-2026-10-28T10-38-11-{THREAD}.jsonl"));
    std::fs::write(&file, &text).unwrap();

    let session = codex::read_file(&file).expect("the rollout reads");
    let calls: Vec<_> = session
        .entries
        .iter()
        .filter_map(|entry| match &entry.kind {
            Kind::Step { calls, .. } => Some(calls.iter()),
            _ => None,
        })
        .flatten()
        .collect();
    assert_eq!(calls.len(), 2, "{calls:?}");
    assert_eq!(calls[0].source_name, "exec_command");
    assert_eq!(calls[0].input["cmd"], "git status");
    assert_eq!(calls[0].canonical, Some(CanonicalTool::Bash));
    assert_eq!(calls[1].source_name, "exec");
    assert_eq!(calls[1].input, Value::String(many.into()));
    let raw = serde_json::to_string(&session.entries.iter().flat_map(|e| e.raw.iter()).collect::<Vec<_>>()).unwrap();
    assert!(raw.contains("tools.exec_command"), "the scripts stay in the raw records");

    let scratch = support::scratch(HarnessId::ClaudeCode, "codex-code-mode");
    let opts = support::opts(HarnessId::ClaudeCode, &scratch);
    let plan = mukade::writer::plan::plan(&session, HarnessId::ClaudeCode, &opts);
    let planned: Vec<_> = plan
        .blocks
        .iter()
        .filter_map(|block| match block {
            mukade::writer::plan::PlannedBlock::Step(step) => Some(step),
            _ => None,
        })
        .collect();
    let names: Vec<&str> = planned
        .iter()
        .flat_map(|step| step.calls.iter().map(|call| call.name.as_str()))
        .collect();
    assert_eq!(names, ["Bash"], "the one-call script is a Bash call; the other is text");
    assert!(
        planned.iter().flat_map(|step| step.text.iter()).any(|t| t.contains("Promise.allSettled")),
        "the several-call script travels as text"
    );
}

/// A subagent's rollout names the thread that spawned it in `session_meta.session_id`; a root
/// thread's `session_id` is its own id and names no parent.
#[test]
fn a_subagent_rollout_names_its_parent_thread() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("mukade-tests")
        .join("codex-subagent-parent")
        .join(std::process::id().to_string());
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let parent = "01a181df-0000-7000-8000-000000000001";
    let write = |name: &str, session_id: &str| {
        let text = record(0, "session_meta", serde_json::json!({
                "id": THREAD, "session_id": session_id, "cwd": "/work", "cli_version": "0.166.0",
            }))
            + &say(1, "user", "go")
            + &say(2, "assistant", "done");
        let file = dir.join(format!("rollout-2026-10-28T10-38-11-{name}.jsonl"));
        std::fs::write(&file, text).unwrap();
        codex::read_file(&file).expect("the rollout reads")
    };
    assert_eq!(write("sub", parent).meta.parent.as_deref(), Some(parent));
    assert_eq!(write("root", THREAD).meta.parent, None);
}

// ------------------------------------------------------------------------- L10: code mode

const HEADER: &str = "Script completed\nWall time 0.1 seconds\nOutput:\n";

fn text_item(text: &str) -> Value {
    serde_json::json!({"type": "input_text", "text": text})
}

/// A Codex Desktop `exec` call with its script and the output items Codex recorded.
fn exec_rows(script: &str, output: Value) -> Session {
    rows_with(vec![
        serde_json::json!({"type": "custom_tool_call", "status": "completed", "call_id": "call_x",
            "name": "exec", "input": script}),
        serde_json::json!({"type": "custom_tool_call_output", "call_id": "call_x", "output": output}),
    ])
}

fn exec_output(header: &str, items: &[&str]) -> Value {
    let mut out = vec![text_item(header)];
    out.extend(items.iter().map(|t| text_item(t)));
    Value::Array(out)
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

fn split_ids(s: &Session) -> Vec<String> {
    calls_of(s)
        .into_iter()
        .filter(|c| c.id.starts_with("call_x"))
        .map(|c| c.id)
        .collect()
}

/// L10: a script of one literal call is that call, with the script's output as its result.
#[test]
fn a_one_call_script_reads_as_that_call() {
    let s = exec_rows(
        "text(await tools.exec_command({cmd:\"CMD-1\"}));",
        exec_output(HEADER, &["OUT-1"]),
    );
    let call = calls_of(&s).into_iter().find(|c| c.id == "call_x").unwrap();
    assert_eq!(call.source_name, "exec_command");
    assert_eq!(call.input, serde_json::json!({"cmd": "CMD-1"}));
    // The fixture's environment block declares PowerShell: the dialect rule (L13) still applies.
    assert_eq!(call.canonical, Some(CanonicalTool::PowerShell));
    assert!(calls_of(&s).iter().all(|c| c.source_name != "exec"));
    let result = results_of(&s)
        .into_iter()
        .find(|r| r.call_id == "call_x")
        .unwrap();
    assert_eq!(result.content, ResultContent::Text("OUT-1".into()));
    assert!(!result.is_error);
}

/// L10, widened: statements in sequence are calls in sequence, each with its own output item.
#[test]
fn a_sequence_reads_as_calls_with_their_own_results() {
    let s = exec_rows(
        "text(await tools.exec_command({cmd:\"CMD-1\"}));\n\
         text(await tools.write_stdin({session_id:7, chars:\"\"}));\n\
         text(await tools.apply_patch(\"PATCH-1\"));\n",
        exec_output(HEADER, &["OUT-1", "OUT-2", "OUT-3"]),
    );
    assert_eq!(split_ids(&s), ["call_x", "call_x_2", "call_x_3"]);
    let names: Vec<String> = calls_of(&s)
        .into_iter()
        .filter(|c| c.id.starts_with("call_x"))
        .map(|c| c.source_name)
        .collect();
    assert_eq!(names, ["exec_command", "write_stdin", "apply_patch"]);
    let step_holding_them = s
        .conversation()
        .filter(|e| match &e.kind {
            Kind::Step { calls, .. } => calls.iter().any(|c| c.id.starts_with("call_x")),
            _ => false,
        })
        .count();
    assert_eq!(step_holding_them, 1, "the calls stay in one step");
    let results: Vec<(String, String)> = results_of(&s)
        .into_iter()
        .filter(|r| r.call_id.starts_with("call_x"))
        .map(|r| (r.call_id, r.content.text()))
        .collect();
    assert_eq!(
        results,
        [
            ("call_x".to_string(), "OUT-1".to_string()),
            ("call_x_2".to_string(), "OUT-2".to_string()),
            ("call_x_3".to_string(), "OUT-3".to_string()),
        ]
    );
}

/// A failed one-call script is that call with an error result; with several calls it is not
/// known which one failed, so the script stays whole.
#[test]
fn a_failed_one_call_script_is_an_error_result() {
    let failed = "Script failed\nWall time 0.1 seconds\nOutput:\n";
    let s = exec_rows(
        "text(await tools.exec_command({cmd:\"CMD-1\"}));",
        exec_output(failed, &["ERR-1"]),
    );
    let result = results_of(&s)
        .into_iter()
        .find(|r| r.call_id == "call_x")
        .unwrap();
    assert!(result.is_error);
    assert_eq!(result.content.text(), "ERR-1");

    let s = exec_rows(
        "text(await tools.exec_command({cmd:\"CMD-1\"}));\ntext(await tools.exec_command({cmd:\"CMD-2\"}));",
        exec_output(failed, &["ERR-1", "ERR-2"]),
    );
    assert_whole(&s);
}

/// The script as today's reader leaves it: one opaque `exec` call, one result.
fn assert_whole(s: &Session) {
    let calls: Vec<_> = calls_of(s)
        .into_iter()
        .filter(|c| c.id.starts_with("call_x"))
        .collect();
    assert_eq!(calls.len(), 1, "{calls:?}");
    assert_eq!(calls[0].source_name, "exec");
    assert_eq!(
        calls[0].canonical,
        Some(CanonicalTool::Other("exec".into()))
    );
    assert!(calls[0].input.is_string(), "the script stays verbatim");
    let results: Vec<_> = results_of(s)
        .into_iter()
        .filter(|r| r.call_id.starts_with("call_x"))
        .collect();
    assert!(results.len() <= 1, "{results:?}");
}

#[test]
fn an_item_count_mismatch_keeps_the_script_whole() {
    let s = exec_rows(
        "text(await tools.exec_command({cmd:\"CMD-1\"}));\ntext(await tools.exec_command({cmd:\"CMD-2\"}));",
        exec_output(HEADER, &["OUT-1"]),
    );
    assert_whole(&s);
    let s = exec_rows(
        "text(await tools.exec_command({cmd:\"CMD-1\"}));",
        exec_output(HEADER, &["OUT-1", "OUT-2"]),
    );
    assert_whole(&s);
}

#[test]
fn an_exec_without_output_stays_whole() {
    let s = rows_with(vec![
        serde_json::json!({"type": "custom_tool_call", "status": "completed",
        "call_id": "call_x", "name": "exec", "input": "text(await tools.exec_command({cmd:\"CMD-1\"}));"}),
    ]);
    assert_whole(&s);
}

#[test]
fn a_running_script_stays_whole() {
    let s = exec_rows(
        "text(await tools.exec_command({cmd:\"CMD-1\"}));",
        exec_output(
            "Script running with cell ID 3\nWall time 0.7 seconds\nOutput:\n",
            &["OUT-1"],
        ),
    );
    assert_whole(&s);
}

#[test]
fn a_string_output_stays_whole() {
    let s = exec_rows(
        "text(await tools.exec_command({cmd:\"CMD-1\"}));",
        Value::String(format!("{HEADER}OUT-1")),
    );
    assert_whole(&s);
}

#[test]
fn an_image_output_stays_whole() {
    let s = exec_rows(
        "text(await tools.exec_command({cmd:\"CMD-1\"}));",
        serde_json::json!([text_item(HEADER), {"type": "input_image", "image_url": "data:image/png;base64,AA=="}]),
    );
    assert_whole(&s);
}

#[test]
fn a_complex_script_stays_whole() {
    let s = exec_rows(
        "const r = await Promise.allSettled([tools.exec_command({cmd:\"CMD-1\"})]);\ntext(r);",
        exec_output(HEADER, &["OUT-1"]),
    );
    assert_whole(&s);
}

/// The split is a reading: every source record is still kept, once, in native order.
#[test]
fn the_split_keeps_every_source_record() {
    let s = exec_rows(
        "text(await tools.exec_command({cmd:\"CMD-1\"}));\ntext(await tools.exec_command({cmd:\"CMD-2\"}));",
        exec_output(HEADER, &["OUT-1", "OUT-2"]),
    );
    assert_eq!(split_ids(&s), ["call_x", "call_x_2"]);
    let raw: Vec<&Value> = s.entries.iter().flat_map(|e| e.raw.iter()).collect();
    let exec_rows_kept = raw
        .iter()
        .filter(|r| r["payload"]["call_id"] == "call_x")
        .count();
    assert_eq!(
        exec_rows_kept, 2,
        "the exec call and its output, verbatim, once each"
    );
}

/// A split call keeps the shell in effect where the script ran, not one declared later on.
#[test]
fn a_split_call_takes_the_shell_declared_before_it() {
    let s = rows_with(vec![
        serde_json::json!({"type": "custom_tool_call", "status": "completed", "call_id": "call_x",
            "name": "exec", "input": "text(await tools.exec_command({cmd:\"CMD-1\"}));"}),
        serde_json::json!({"type": "custom_tool_call_output", "call_id": "call_x",
            "output": exec_output(HEADER, &["OUT-1"])}),
        serde_json::json!({"type": "message", "role": "user", "content": [{"type": "input_text",
            "text": "<environment_context>
  <shell>bash</shell>
</environment_context>"}]}),
    ]);
    let call = calls_of(&s).into_iter().find(|c| c.id == "call_x").unwrap();
    assert_eq!(call.source_name, "exec_command");
    assert_eq!(call.canonical, Some(CanonicalTool::PowerShell));
}
