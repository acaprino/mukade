mod support;
use mukade::bus::view::{ViewKind, build};
use mukade::model::{
    Attachment, CanonicalTool, Reasoning, ResultContent, StepEnd, ToolCall, ToolResult,
};
use mukade::{Entry, HarnessId, Kind, Session};
use serde_json::json;
fn session(entries: Vec<Entry>) -> Session {
    let mut s = support::read_source(HarnessId::Pi);
    s.entries = entries;
    s
}
fn prompt(text: &str) -> Entry {
    Entry {
        id: "user-1".into(),
        kind: Kind::User {
            text: text.into(),
            attachments: vec![],
        },
        raw: vec![json!({"type":"message","message":{"role":"user"}})],
    }
}
fn tool(name: &str, canonical: CanonicalTool, input: serde_json::Value) -> Entry {
    Entry {
        id: "step".into(),
        kind: Kind::Step {
            model: None,
            reasoning: vec![],
            text: vec![],
            calls: vec![ToolCall {
                id: "call".into(),
                source_name: name.into(),
                canonical: Some(canonical),
                input,
            }],
            end: StepEnd::ToolUse,
        },
        raw: vec![],
    }
}
fn output(e: Entry) -> String {
    serde_json::to_string(&build(&session(vec![e]))).unwrap()
}

#[test]
fn fingerprints_include_full_permitted_fields_and_stable_metadata() {
    let prefix = "p".repeat(1300);
    let a = build(&session(vec![prompt(&format!("{prefix}a"))]));
    let b = build(&session(vec![prompt(&format!("{prefix}b"))]));
    assert_eq!(a[0].text, b[0].text);
    assert_ne!(a[0].fingerprint, b[0].fingerprint);
    let prefix = "path".repeat(50);
    let a = build(&session(vec![tool(
        "Read",
        CanonicalTool::Read,
        json!({"file_path":format!("{prefix}a")}),
    )]));
    let b = build(&session(vec![tool(
        "Read",
        CanonicalTool::Read,
        json!({"file_path":format!("{prefix}b")}),
    )]));
    assert_eq!(a[0].text, b[0].text);
    assert_ne!(a[0].fingerprint, b[0].fingerprint);
}

#[test]
fn excluded_tool_bodies_and_masked_secrets_do_not_change_fingerprints() {
    let a = build(&session(vec![tool(
        "Write",
        CanonicalTool::Write,
        json!({"file_path":"safe.txt","content":"private A"}),
    )]));
    let b = build(&session(vec![tool(
        "Write",
        CanonicalTool::Write,
        json!({"file_path":"safe.txt","content":"private B"}),
    )]));
    assert_eq!(a[0].fingerprint, b[0].fingerprint);
    let a = build(&session(vec![prompt(
        "API_KEY=sk-live-aaaaaaaaaaaaaaaaaaaa",
    )]));
    let b = build(&session(vec![prompt(
        "API_KEY=sk-live-bbbbbbbbbbbbbbbbbbbb",
    )]));
    assert_eq!(a[0].fingerprint, b[0].fingerprint);
}
#[test]
fn shell_wrappers_do_not_expose_inline_programs() {
    for command in [
        r"git -c alias.demo=!echo\ PRIVATEBODY demo",
        "npm exec -- node --eval=PRIVATEBODY",
        "pnpm exec node -e PRIVATEBODY",
        "git commit -m PRIVATEBODY",
        "cargo --config alias.demo=PRIVATEBODY demo",
    ] {
        let out = output(tool(
            "Bash",
            CanonicalTool::Bash,
            json!({"command": command}),
        ));
        assert!(!out.contains("PRIVATEBODY"), "{out}");
    }
}
#[test]
fn pi_bash_execution_shows_the_command_never_the_output() {
    let mut e = prompt("ls\nAPI_KEY=sk-live-aaaaaaaaaaaaaaaaaaaa");
    e.raw = vec![
        json!({"type":"message","message":{"role":"bashExecution","command":"ls","output":"API_KEY=sk-live-aaaaaaaaaaaaaaaaaaaa"}}),
    ];
    let out = output(e);
    assert!(out.contains("ls"));
    assert!(!out.contains("API_KEY"));
    assert!(!out.contains("aaaa"));
}
#[test]
fn inline_script_and_heredoc_bodies_are_not_shown() {
    for cmd in [
        "python -c 'print(\"PRIVATEBODY\")'",
        "bash -c 'PRIVATEBODY\nmore'",
        "cat <<EOF\nPRIVATEBODY\nEOF",
        "powershell -Command PRIVATEBODY",
        "echo PRIVATEBODY > file",
    ] {
        let out = output(tool("Bash", CanonicalTool::Bash, json!({"command":cmd})));
        assert!(!out.contains("PRIVATEBODY"), "{out}");
    }
}
#[test]
fn a_patch_in_a_command_is_not_shown() {
    assert!(
        !output(tool(
            "Bash",
            CanonicalTool::Bash,
            json!({"command":"apply_patch '*** Begin Patch\nPRIVATEBODY\n*** End Patch'"})
        ))
        .contains("PRIVATEBODY")
    );
}
#[test]
fn edit_and_write_show_path_not_content() {
    for k in [
        CanonicalTool::Edit,
        CanonicalTool::Write,
        CanonicalTool::MultiEdit,
    ] {
        let out = output(tool(
            "write",
            k,
            json!({"path":"src/lib.rs","content":"PRIVATEBODY","old_string":"PRIVATEBODY"}),
        ));
        assert!(out.contains("src/lib.rs"));
        assert!(!out.contains("PRIVATEBODY"));
    }
}

#[test]
fn write_size_is_safe_explicit_and_changes_fingerprint_only_with_size() {
    let project = |body: &str| {
        build(&session(vec![tool(
            "Write",
            CanonicalTool::Write,
            json!({"file_path":"safe.txt","content":body}),
        )]))
        .remove(0)
    };
    let first = project("\u{754c}a");
    assert!(first.text.contains("safe.txt"));
    assert!(first.text.contains("4 UTF-8 bytes"));
    assert!(first.text.contains("2 Unicode scalars"));
    assert!(!first.text.contains("\u{754c}a"));
    assert_eq!(first.fingerprint, project("\u{4eba}b").fingerprint);
    assert_ne!(first.fingerprint, project("PRIVATEBODY").fingerprint);
    assert!(!project("PRIVATEBODY").text.contains("PRIVATEBODY"));
}

#[test]
fn edits_show_old_new_sizes_for_supported_spellings_without_bodies() {
    for (old, new) in [
        ("old_string", "new_string"),
        ("oldString", "newString"),
        ("oldText", "newText"),
    ] {
        let mut input = json!({"path":"safe.txt"});
        input[old] = json!("\u{754c}");
        input[new] = json!("PRIVATEBODY");
        let first = build(&session(vec![tool(
            "Edit",
            CanonicalTool::Edit,
            input.clone(),
        )]))
        .remove(0);
        assert!(
            first
                .text
                .contains("old: 3 UTF-8 bytes / 1 Unicode scalars"),
            "{}",
            first.text
        );
        assert!(
            first
                .text
                .contains("new: 11 UTF-8 bytes / 11 Unicode scalars"),
            "{}",
            first.text
        );
        assert!(!first.text.contains("PRIVATEBODY"));
        assert!(!first.text.contains('\u{754c}'));
        input[new] = json!("OTHERSECRET");
        let same = build(&session(vec![tool(
            "Edit",
            CanonicalTool::Edit,
            input.clone(),
        )]))
        .remove(0);
        assert_eq!(first.fingerprint, same.fingerprint);
        input[new] = json!("different-size");
        let changed = build(&session(vec![tool("Edit", CanonicalTool::Edit, input)])).remove(0);
        assert_ne!(first.fingerprint, changed.fingerprint);
    }
}

#[test]
fn multiedits_report_counts_and_known_sizes_without_guessing_missing_fields() {
    for kind in [CanonicalTool::Edit, CanonicalTool::MultiEdit] {
        let mut input = json!({"file_path":"safe.txt","edits":[{"oldText":"\u{754c}","newText":"a"},{"old_string":"x","new_string":"bb"}]});
        let item = build(&session(vec![tool(
            "MultiEdit",
            kind.clone(),
            input.clone(),
        )]))
        .remove(0);
        assert!(item.text.contains("2 edits"));
        assert!(
            item.text.contains("old: 4 UTF-8 bytes / 2 Unicode scalars"),
            "{}",
            item.text
        );
        assert!(
            item.text.contains("new: 3 UTF-8 bytes / 3 Unicode scalars"),
            "{}",
            item.text
        );
        assert!(!item.text.contains('\u{754c}'));
        input["edits"][0]["oldText"] = json!("\u{4eba}");
        input["edits"][1]["new_string"] = json!("cc");
        let same = build(&session(vec![tool(
            "MultiEdit",
            kind.clone(),
            input.clone(),
        )]))
        .remove(0);
        assert_eq!(item.fingerprint, same.fingerprint);
        input["edits"]
            .as_array_mut()
            .unwrap()
            .push(json!({"old_string":"","new_string":""}));
        let different_count = build(&session(vec![tool("MultiEdit", kind, input)])).remove(0);
        assert_ne!(item.fingerprint, different_count.fingerprint);
    }
    let input = json!({"path":"safe.txt","edits":[{"old_string":"PRIVATEBODY","new_string":""},{"old_string":null}]});
    let item = build(&session(vec![tool(
        "MultiEdit",
        CanonicalTool::MultiEdit,
        input,
    )]))
    .remove(0);
    assert!(item.text.contains("2 edits"));
    assert!(item.text.contains("1/2 known"));
    assert!(!item.text.contains("PRIVATEBODY"));
}
#[test]
fn read_result_shows_only_ok_and_size() {
    let e = Entry {
        id: "result".into(),
        kind: Kind::Results {
            results: vec![ToolResult {
                call_id: "call".into(),
                content: ResultContent::Text("PRIVATEBODY".into()),
                is_error: false,
                truncated_by_source: false,
            }],
        },
        raw: vec![],
    };
    let view = build(&session(vec![e]));
    assert_eq!(view[0].kind, ViewKind::ToolOutcome);
    assert!(view[0].text.contains("ok"));
    assert!(view[0].text.contains("11"));
    assert!(!view[0].text.contains("PRIVATEBODY"));
}
#[test]
fn unknown_tool_shows_name_only() {
    let out = output(tool(
        "strange",
        CanonicalTool::Other("strange".into()),
        json!({"command":"PRIVATEBODY","path":"PRIVATEBODY"}),
    ));
    assert!(out.contains("strange"));
    assert!(!out.contains("PRIVATEBODY"));
}
#[test]
fn mcp_tool_shows_name_only() {
    assert!(
        !output(tool(
            "mcp__server__read",
            CanonicalTool::Mcp {
                server: "server".into(),
                tool: "read".into()
            },
            json!({"path":"PRIVATEBODY"})
        ))
        .contains("PRIVATEBODY")
    );
}
#[test]
fn reasoning_and_attachments_are_absent() {
    let mut s = support::read_source(HarnessId::Pi);
    let original = build(&s);
    for e in &mut s.entries {
        match &mut e.kind {
            Kind::User { attachments, .. } => attachments.push(Attachment {
                kind: "file".into(),
                path: Some("PRIVATEBODY".into()),
                raw: json!({"content":"PRIVATEBODY"}),
            }),
            Kind::Step { reasoning, .. } => reasoning.clear(),
            _ => {}
        }
    }
    assert_eq!(original, build(&s));
    let _: Vec<Reasoning> = vec![];
}
#[test]
fn a_secret_straddling_the_cut_is_masked_whole() {
    let out = output(prompt(&format!(
        "{}sk-ant-abcdefghijklmnopqrstuv",
        "x ".repeat(595)
    )));
    assert!(!out.contains("sk-ant-"));
    assert!(!out.contains("abcdef"));
}
#[test]
fn injected_user_entries_show_their_kind_only() {
    let mut e = prompt("PRIVATEBODY");
    e.raw = vec![json!({"type":"custom_message"})];
    let out = output(e);
    assert!(out.contains("custom_message"));
    assert!(!out.contains("PRIVATEBODY"));
}
#[test]
fn masking_a_prompt_with_a_key() {
    let out = output(prompt("password=unusualword"));
    assert!(out.contains("masked"));
    assert!(!out.contains("unusualword"));
}
#[test]
fn ids_are_stable_across_two_builds_of_the_same_session() {
    let s = support::read_source(HarnessId::ClaudeCode);
    assert_eq!(build(&s), build(&s));
    let ids = build(&s)
        .into_iter()
        .map(|v| v.id)
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(ids.len(), build(&s).len());
}
#[test]
fn a_cut_never_splits_a_multibyte_character() {
    let out = build(&session(vec![prompt(&"🙂è".repeat(1000))]));
    assert!(out[0].text.contains("2000 characters"));
}
#[test]
fn all_four_fixtures_build_without_panicking_and_contain_no_tool_result_text() {
    for harness in HarnessId::ALL {
        let s = support::read_source(harness);
        let view = build(&s);
        assert!(!view.is_empty());
        for entry in &s.entries {
            if let Kind::Results { results } = &entry.kind {
                for result in results {
                    let text = result.content.text();
                    if text.len() > 30 {
                        assert!(!view.iter().any(|v| v.text.contains(&text)));
                    }
                }
            }
        }
    }
}
#[test]
fn terminal_controls_are_escaped_in_every_field() {
    let out = build(&session(vec![prompt("hello\u{1b}[2J\rhidden")]));
    assert!(!out[0].text.contains('\u{1b}'));
    assert!(!out[0].text.contains('\r'));
}
