//! Tool translation conformance (docs/tool-conversion-tables.md, spec L11–L14).

mod support;

use mukade::model::{CanonicalTool, HarnessId, ToolCall, ToolResult};
use mukade::tools::{self, Shell, ToolSet, Translation, offered, translate, translate_with};
use serde_json::{Value, json};

fn call(canonical: CanonicalTool, source_name: &str, input: Value) -> ToolCall {
    ToolCall {
        id: "c1".into(),
        source_name: source_name.into(),
        canonical: Some(canonical),
        input,
    }
}

fn name_of(translation: &Translation) -> &'static str {
    match translation {
        Translation::Call { name, .. } => name,
        Translation::Text { .. } => panic!("expected a translated call, got text form"),
    }
}

fn input_of(translation: &Translation) -> &Value {
    match translation {
        Translation::Call { input, .. } => input,
        Translation::Text { .. } => panic!("expected a translated call, got text form"),
    }
}

// ------------------------------------------------------------------------------------- L11

/// L11: no destination ever receives a tool name outside its offered set.
#[test]
fn every_translated_name_is_offered_by_its_destination() {
    let calls = vec![
        call(CanonicalTool::Bash, "Bash", json!({"command": "ls"})),
        call(CanonicalTool::Read, "Read", json!({"file_path": "a.txt"})),
        call(
            CanonicalTool::Write,
            "Write",
            json!({"file_path": "a.txt", "content": "x"}),
        ),
        call(
            CanonicalTool::Edit,
            "Edit",
            json!({"file_path": "a.txt", "old_string": "a", "new_string": "b"}),
        ),
        call(CanonicalTool::Grep, "Grep", json!({"pattern": "x"})),
        call(CanonicalTool::Glob, "Glob", json!({"pattern": "*.rs"})),
        call(CanonicalTool::Ls, "LS", json!({"path": "src"})),
        call(
            CanonicalTool::WebFetch,
            "WebFetch",
            json!({"url": "https://example.org"}),
        ),
        call(
            CanonicalTool::WebSearch,
            "WebSearch",
            json!({"query": "rust"}),
        ),
        call(CanonicalTool::Agent, "Agent", json!({"prompt": "x"})),
        call(CanonicalTool::Skill, "Skill", json!({"skill": "x"})),
        call(
            CanonicalTool::AskUserQuestion,
            "AskUserQuestion",
            json!({"question": "x"}),
        ),
        call(
            CanonicalTool::Mcp {
                server: "s".into(),
                tool: "t".into(),
            },
            "mcp__s__t",
            json!({"a": 1}),
        ),
        call(
            CanonicalTool::Other("StructuredOutput".into()),
            "StructuredOutput",
            json!({}),
        ),
    ];
    for dest in HarnessId::ALL {
        for call in &calls {
            for shell in [Some(Shell::Posix), Some(Shell::PowerShell), None] {
                if let Translation::Call { name, .. } = translate(call, dest, shell) {
                    assert!(
                        offered(dest).contains(name),
                        "{} received {name}, which it does not offer",
                        dest.as_str()
                    );
                }
            }
        }
    }
}

// ------------------------------------------------------------------------------------- L12

/// L12: Pi counts the timeout in seconds, the canonical form in milliseconds.
#[test]
fn shell_timeout_is_converted_for_pi_and_kept_in_milliseconds_elsewhere() {
    let bash = call(
        CanonicalTool::Bash,
        "Bash",
        json!({"command": "make", "timeout": 2500}),
    );
    let pi = translate(&bash, HarnessId::Pi, Some(Shell::Posix));
    assert_eq!(name_of(&pi), "bash");
    assert_eq!(
        input_of(&pi)["timeout"],
        json!(3),
        "2500 ms rounds up to 3 s"
    );

    let opencode = translate(&bash, HarnessId::OpenCode, Some(Shell::Posix));
    assert_eq!(name_of(&opencode), "shell");
    assert_eq!(
        input_of(&opencode)["timeout"],
        json!(2500),
        "OpenCode stays in milliseconds"
    );

    let claude = translate(&bash, HarnessId::ClaudeCode, None);
    assert_eq!(name_of(&claude), "Bash");
    assert_eq!(input_of(&claude)["timeout"], json!(2500));
}

// ------------------------------------------------------------------------------------- L13

#[test]
fn replace_all_blocks_pi_and_codex_and_travels_to_opencode() {
    let edit = call(
        CanonicalTool::Edit,
        "Edit",
        json!({"file_path": "a.txt", "old_string": "a", "new_string": "b", "replace_all": true}),
    );
    assert!(matches!(
        translate(&edit, HarnessId::Pi, None),
        Translation::Text { .. }
    ));
    assert!(matches!(
        translate(&edit, HarnessId::Codex, Some(Shell::Posix)),
        Translation::Text { .. }
    ));
    let opencode = translate(&edit, HarnessId::OpenCode, None);
    assert_eq!(
        input_of(&opencode),
        &json!({"path": "a.txt", "oldString": "a", "newString": "b", "replaceAll": true})
    );
}

#[test]
fn an_ordinary_edit_reaches_pi_as_its_own_edits_array() {
    let edit = call(
        CanonicalTool::Edit,
        "Edit",
        json!({"file_path": "a.txt", "old_string": "a", "new_string": "b"}),
    );
    let pi = translate(&edit, HarnessId::Pi, None);
    assert_eq!(name_of(&pi), "edit");
    assert_eq!(
        input_of(&pi),
        &json!({"path": "a.txt", "edits": [{"oldText": "a", "newText": "b"}]})
    );
}

#[test]
fn read_pages_or_allow_large_has_no_counterpart() {
    for input in [
        json!({"file_path": "a.pdf", "pages": "1-2"}),
        json!({"file_path": "a.txt", "allow_large": true}),
    ] {
        let read = call(CanonicalTool::Read, "Read", input);
        assert!(matches!(
            translate(&read, HarnessId::OpenCode, None),
            Translation::Text { .. }
        ));
        assert!(matches!(
            translate(&read, HarnessId::Pi, None),
            Translation::Text { .. }
        ));
    }
    let plain = call(
        CanonicalTool::Read,
        "Read",
        json!({"file_path": "a.txt", "offset": 5, "limit": 10}),
    );
    assert_eq!(
        input_of(&translate(&plain, HarnessId::Pi, None)),
        &json!({"path": "a.txt", "offset": 5, "limit": 10})
    );
    assert_eq!(
        input_of(&translate(&plain, HarnessId::ClaudeCode, None)),
        &json!({"file_path": "a.txt", "offset": 5, "limit": 10})
    );
}

#[test]
fn codex_has_no_read_tool_so_a_read_is_text() {
    let read = call(CanonicalTool::Read, "Read", json!({"file_path": "a.txt"}));
    assert!(matches!(
        translate(&read, HarnessId::Codex, None),
        Translation::Text { .. }
    ));
}

#[test]
fn grep_translates_only_over_the_shared_subset() {
    let plain = call(
        CanonicalTool::Grep,
        "Grep",
        json!({"pattern": "fn main", "path": "src", "-i": true}),
    );
    let opencode = translate(&plain, HarnessId::OpenCode, None);
    assert_eq!(name_of(&opencode), "grep");
    assert_eq!(
        input_of(&opencode),
        &json!({"pattern": "fn main", "path": "src", "caseSensitive": false})
    );

    let with_mode = call(
        CanonicalTool::Grep,
        "Grep",
        json!({"pattern": "x", "output_mode": "content"}),
    );
    assert!(matches!(
        translate(&with_mode, HarnessId::OpenCode, None),
        Translation::Text { .. }
    ));
    assert!(matches!(
        translate(&with_mode, HarnessId::Pi, None),
        Translation::Text { .. }
    ));

    let with_context = call(
        CanonicalTool::Grep,
        "Grep",
        json!({"pattern": "x", "-C": 3}),
    );
    assert!(matches!(
        translate(&with_context, HarnessId::OpenCode, None),
        Translation::Text { .. }
    ));
    // Pi has a `context` argument, so with grep enabled it keeps the call — and drops the context,
    // which would need renaming to reach Pi's own tool.
    let pi_with_grep = ToolSet {
        names: &["read", "bash", "edit", "write", "grep", "find", "ls"],
    };
    assert_eq!(
        input_of(&translate_with(
            &with_context,
            HarnessId::Pi,
            None,
            pi_with_grep
        )),
        &json!({"pattern": "x"})
    );
}

#[test]
fn a_shell_call_of_the_other_dialect_is_text_form() {
    let bash = call(CanonicalTool::Bash, "Bash", json!({"command": "ls -la"}));
    assert!(matches!(
        translate(&bash, HarnessId::Codex, Some(Shell::PowerShell)),
        Translation::Text { .. }
    ));
    assert!(matches!(
        translate(&bash, HarnessId::Codex, None),
        Translation::Text { .. }
    ));
    let command = match translate(&bash, HarnessId::Codex, Some(Shell::Posix)) {
        Translation::Call { input, .. } => input,
        other => panic!("{other:?}"),
    };
    assert_eq!(
        command,
        json!({"cmd": "ls -la"}),
        "the command string is never rewritten"
    );

    let powershell = call(
        CanonicalTool::PowerShell,
        "PowerShell",
        json!({"command": "Get-ChildItem"}),
    );
    assert!(matches!(
        translate(&powershell, HarnessId::Codex, Some(Shell::Posix)),
        Translation::Text { .. }
    ));
    assert_eq!(
        input_of(&translate(
            &powershell,
            HarnessId::Codex,
            Some(Shell::PowerShell)
        )),
        &json!({"cmd": "Get-ChildItem"})
    );
}

#[test]
fn background_execution_blocks_translation_where_it_does_not_exist() {
    let background = call(
        CanonicalTool::Bash,
        "Bash",
        json!({"command": "serve", "run_in_background": true}),
    );
    assert!(matches!(
        translate(&background, HarnessId::Codex, Some(Shell::Posix)),
        Translation::Text { .. }
    ));
    assert!(matches!(
        translate(&background, HarnessId::Pi, Some(Shell::Posix)),
        Translation::Text { .. }
    ));
    assert_eq!(
        input_of(&translate(
            &background,
            HarnessId::OpenCode,
            Some(Shell::Posix)
        )),
        &json!({"command": "serve", "background": true})
    );
}

#[test]
fn glob_and_ls_reach_only_the_destinations_that_offer_them() {
    let glob = call(
        CanonicalTool::Glob,
        "Glob",
        json!({"pattern": "**/*.rs", "path": "src"}),
    );
    assert_eq!(
        name_of(&translate(&glob, HarnessId::OpenCode, None)),
        "glob"
    );
    assert!(matches!(
        translate(&glob, HarnessId::Codex, None),
        Translation::Text { .. }
    ));
    // Pi offers `find` and `ls` only when they are enabled; with the pinned default set the call
    // is text form, and a caller that knows the destination's settings can widen the set.
    assert!(matches!(
        translate(&glob, HarnessId::Pi, None),
        Translation::Text { .. }
    ));
    let pi_with_find = ToolSet {
        names: &["read", "bash", "edit", "write", "grep", "find", "ls"],
    };
    assert_eq!(
        name_of(&translate_with(&glob, HarnessId::Pi, None, pi_with_find)),
        "find"
    );
    assert_eq!(
        input_of(&translate_with(&glob, HarnessId::Pi, None, pi_with_find)),
        &json!({"pattern": "**/*.rs", "path": "src"})
    );

    let ls = call(CanonicalTool::Ls, "LS", json!({"path": "src"}));
    assert_eq!(
        name_of(&translate_with(&ls, HarnessId::Pi, None, pi_with_find)),
        "ls"
    );
    assert!(matches!(
        translate(&ls, HarnessId::Pi, None),
        Translation::Text { .. }
    ));
    assert!(matches!(
        translate(&ls, HarnessId::OpenCode, None),
        Translation::Text { .. }
    ));
}

// ------------------------------------------------------------------------------------- L14

/// L14: subagents, skills, questions, MCP and harness-internal tools are text form everywhere.
#[test]
fn capability_tools_are_text_form_in_every_direction() {
    let calls = [
        call(CanonicalTool::Agent, "Agent", json!({"prompt": "x"})),
        call(CanonicalTool::Skill, "Skill", json!({"skill": "x"})),
        call(
            CanonicalTool::AskUserQuestion,
            "AskUserQuestion",
            json!({"question": "x"}),
        ),
        call(
            CanonicalTool::Mcp {
                server: "s".into(),
                tool: "t".into(),
            },
            "mcp__s__t",
            json!({"a": 1}),
        ),
        call(
            CanonicalTool::Other("StructuredOutput".into()),
            "StructuredOutput",
            json!({"x": 1}),
        ),
        call(CanonicalTool::Other("Monitor".into()), "Monitor", json!({})),
    ];
    for dest in HarnessId::ALL {
        for call in &calls {
            assert!(
                matches!(
                    translate(call, dest, Some(Shell::Posix)),
                    Translation::Text { .. }
                ),
                "{} translated {}",
                dest.as_str(),
                call.source_name
            );
        }
    }
}

#[test]
fn text_form_keeps_the_source_name_id_and_arguments() {
    let call = call(
        CanonicalTool::Agent,
        "Agent",
        json!({"description": "look around", "prompt": "read the docs"}),
    );
    let text = tools::text_form_call(&call, HarnessId::Pi);
    assert!(
        text.starts_with("[tool call · Agent · id c1 · not available in pi]"),
        "{text}"
    );
    assert!(text.contains("description: look around"), "{text}");
    assert!(text.contains("prompt: read the docs"), "{text}");

    let result = ToolResult {
        call_id: "c1".into(),
        content: mukade::model::ResultContent::Text("out\nmore".into()),
        is_error: true,
        truncated_by_source: false,
    };
    let text = tools::text_form_result(&result);
    assert_eq!(text, "[tool result · id c1] (error)\nout\nmore");
}

#[test]
fn a_call_without_a_canonical_form_is_text() {
    let raw = ToolCall {
        id: "c1".into(),
        source_name: "mystery".into(),
        canonical: None,
        input: json!({}),
    };
    assert!(matches!(
        translate(&raw, HarnessId::ClaudeCode, None),
        Translation::Text { .. }
    ));
}
