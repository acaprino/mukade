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
    // Pi has a `context` argument: with grep enabled the call reaches it, context renamed.
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
        &json!({"pattern": "x", "context": 3})
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

const PI_WITH_SEARCH: ToolSet = ToolSet {
    names: &["read", "bash", "edit", "write", "grep", "find", "ls"],
};

fn is_text(translation: &Translation) -> bool {
    matches!(translation, Translation::Text { .. })
}

/// Tables §1.2: a call that used an argument with no counterpart is not equivalent; it is never
/// translated with the argument silently left out.
#[test]
fn an_argument_with_no_counterpart_makes_the_call_text_form() {
    let in_workdir = call(
        CanonicalTool::Bash,
        "exec_command",
        json!({"cmd": "ls", "workdir": "C:/w"}),
    );
    assert!(is_text(&translate(
        &in_workdir,
        HarnessId::ClaudeCode,
        None
    )));
    assert!(is_text(&translate(
        &in_workdir,
        HarnessId::Pi,
        Some(Shell::Posix)
    )));
    assert_eq!(
        input_of(&translate(
            &in_workdir,
            HarnessId::OpenCode,
            Some(Shell::Posix)
        )),
        &json!({"command": "ls", "workdir": "C:/w"})
    );
    let with_prompt = call(
        CanonicalTool::WebFetch,
        "WebFetch",
        json!({"url": "https://x", "prompt": "sum it up"}),
    );
    assert!(is_text(&translate(&with_prompt, HarnessId::OpenCode, None)));
    let with_domains = call(
        CanonicalTool::WebSearch,
        "WebSearch",
        json!({"query": "q", "allowed_domains": ["a.org"]}),
    );
    assert!(is_text(&translate(
        &with_domains,
        HarnessId::OpenCode,
        None
    )));
    let hidden = call(
        CanonicalTool::Glob,
        "glob",
        json!({"pattern": "*.rs", "hidden": true}),
    );
    assert!(is_text(&translate_with(
        &hidden,
        HarnessId::Pi,
        None,
        PI_WITH_SEARCH
    )));
    let unknown = call(
        CanonicalTool::Read,
        "Read",
        json!({"file_path": "a", "frobnicate": 1}),
    );
    assert!(is_text(&translate(&unknown, HarnessId::OpenCode, None)));
    // An argument with no effect on the recorded result does not block (tables §4.1).
    let described = call(
        CanonicalTool::Bash,
        "exec_command",
        json!({"cmd": "ls", "yield_time_ms": 1000}),
    );
    assert_eq!(
        name_of(&translate(&described, HarnessId::Pi, Some(Shell::Posix))),
        "bash"
    );
}

/// Each harness's own spelling of a search reaches the others' tools under their names.
#[test]
fn every_grep_spelling_maps_to_the_destination_grep() {
    let claude = call(
        CanonicalTool::Grep,
        "Grep",
        json!({"pattern": "x", "glob": "*.rs", "-i": true, "head_limit": 5}),
    );
    assert_eq!(
        input_of(&translate_with(
            &claude,
            HarnessId::Pi,
            None,
            PI_WITH_SEARCH
        )),
        &json!({"pattern": "x", "glob": "*.rs", "ignoreCase": true, "limit": 5})
    );
    let opencode = call(
        CanonicalTool::Grep,
        "grep",
        json!({"pattern": "x", "include": "*.rs", "literal": true, "caseSensitive": false}),
    );
    assert_eq!(
        input_of(&translate_with(
            &opencode,
            HarnessId::Pi,
            None,
            PI_WITH_SEARCH
        )),
        &json!({"pattern": "x", "glob": "*.rs", "ignoreCase": true, "literal": true})
    );
    assert_eq!(
        input_of(&translate(&opencode, HarnessId::OpenCode, None)),
        &json!({"pattern": "x", "include": "*.rs", "caseSensitive": false, "literal": true})
    );
}

/// Claude Code offers Grep, Glob and WebSearch: canonical calls reach it there (L3).
#[test]
fn search_tools_reach_claude_code() {
    let grep = call(
        CanonicalTool::Grep,
        "Grep",
        json!({"pattern": "x", "output_mode": "content", "-n": true}),
    );
    assert_eq!(
        name_of(&translate(&grep, HarnessId::ClaudeCode, None)),
        "Grep"
    );
    assert_eq!(
        input_of(&translate(&grep, HarnessId::ClaudeCode, None)),
        &grep.input
    );
    let from_pi = call(
        CanonicalTool::Grep,
        "grep",
        json!({"pattern": "x", "context": 2, "limit": 9}),
    );
    assert_eq!(
        input_of(&translate(&from_pi, HarnessId::ClaudeCode, None)),
        &json!({"pattern": "x", "-C": 2, "head_limit": 9, "output_mode": "content", "-n": true})
    );
    let glob = call(
        CanonicalTool::Glob,
        "glob",
        json!({"pattern": "*.rs", "path": "src"}),
    );
    assert_eq!(
        name_of(&translate(&glob, HarnessId::ClaudeCode, None)),
        "Glob"
    );
    let search = call(CanonicalTool::WebSearch, "websearch", json!({"query": "q"}));
    assert_eq!(
        name_of(&translate(&search, HarnessId::ClaudeCode, None)),
        "WebSearch"
    );
}

/// L13: OpenCode spells "all occurrences" `replaceAll`; a MultiEdit entry can ask for it too.
#[test]
fn every_spelling_of_replace_all_is_honoured() {
    let opencode = call(
        CanonicalTool::Edit,
        "edit",
        json!({"path": "a", "oldString": "x", "newString": "y", "replaceAll": true}),
    );
    assert!(is_text(&translate(&opencode, HarnessId::Pi, None)));
    assert_eq!(
        input_of(&translate(&opencode, HarnessId::ClaudeCode, None))["replace_all"],
        true
    );
    let multi = call(
        CanonicalTool::MultiEdit,
        "MultiEdit",
        json!({"file_path": "a", "edits": [{"old_string": "x", "new_string": "y", "replace_all": true}]}),
    );
    assert!(is_text(&translate(&multi, HarnessId::Pi, None)));
}

/// OpenCode spells background `background`; Claude's Bash has `run_in_background`.
#[test]
fn a_background_shell_call_keeps_running_in_the_background() {
    let opencode = call(
        CanonicalTool::Bash,
        "shell",
        json!({"command": "serve", "background": true}),
    );
    assert_eq!(
        input_of(&translate(&opencode, HarnessId::ClaudeCode, None)),
        &json!({"command": "serve", "run_in_background": true})
    );
    assert!(is_text(&translate(
        &opencode,
        HarnessId::Pi,
        Some(Shell::Posix)
    )));
    assert!(is_text(&translate(
        &opencode,
        HarnessId::Codex,
        Some(Shell::Posix)
    )));
}

/// OpenCode and Pi write `path`; a single Pi edit is an ordinary edit everywhere.
#[test]
fn path_spellings_and_single_pi_edits_translate() {
    let write = call(
        CanonicalTool::Write,
        "write",
        json!({"path": "a", "content": "c"}),
    );
    assert_eq!(
        input_of(&translate(&write, HarnessId::ClaudeCode, None)),
        &json!({"file_path": "a", "content": "c"})
    );
    let one = call(
        CanonicalTool::Edit,
        "edit",
        json!({"path": "a", "edits": [{"oldText": "x", "newText": "y"}]}),
    );
    assert_eq!(
        input_of(&translate(&one, HarnessId::ClaudeCode, None)),
        &json!({"file_path": "a", "old_string": "x", "new_string": "y"})
    );
    assert_eq!(
        input_of(&translate(&one, HarnessId::OpenCode, None)),
        &json!({"path": "a", "oldString": "x", "newString": "y"})
    );
    let two = call(
        CanonicalTool::Edit,
        "edit",
        json!({"path": "a", "edits": [{"oldText": "x", "newText": "y"}, {"oldText": "p", "newText": "q"}]}),
    );
    assert!(is_text(&translate(&two, HarnessId::ClaudeCode, None)));
    assert_eq!(input_of(&translate(&two, HarnessId::Pi, None)), &two.input);
}

/// L5: Pi's `edits` entries need both texts; an empty list is not an edit.
#[test]
fn a_multi_edit_pi_cannot_hold_is_text_form() {
    let missing = call(
        CanonicalTool::MultiEdit,
        "MultiEdit",
        json!({"file_path": "a", "edits": [{"new_string": "y"}]}),
    );
    assert!(is_text(&translate(&missing, HarnessId::Pi, None)));
    let empty = call(
        CanonicalTool::MultiEdit,
        "MultiEdit",
        json!({"file_path": "a", "edits": []}),
    );
    assert!(is_text(&translate(&empty, HarnessId::Pi, None)));
}

/// The offered sets are the ones tables §2 records for the pinned versions, written out here so a
/// change to either is a change to both.
#[test]
fn the_offered_sets_are_the_tables_lists() {
    fn sorted<'a>(names: &[&'a str]) -> Vec<&'a str> {
        let mut names = names.to_vec();
        names.sort_unstable();
        names
    }
    assert_eq!(
        sorted(offered(HarnessId::ClaudeCode).names),
        sorted(&[
            "Agent",
            "Bash",
            "CronCreate",
            "CronDelete",
            "CronList",
            "Edit",
            "EnterWorktree",
            "ExitWorktree",
            "Glob",
            "Grep",
            "ListAgents",
            "Monitor",
            "NotebookEdit",
            "Read",
            "ReportFindings",
            "ScheduleWakeup",
            "SendMessage",
            "Skill",
            "TaskStop",
            "WebFetch",
            "WebSearch",
            "Workflow",
            "Write",
        ])
    );
    assert_eq!(
        sorted(offered(HarnessId::OpenCode).names),
        sorted(&[
            "edit",
            "glob",
            "grep",
            "question",
            "read",
            "shell",
            "skill",
            "subagent",
            "webfetch",
            "websearch",
            "write",
            "execute",
        ])
    );
    assert_eq!(
        sorted(offered(HarnessId::Pi).names),
        sorted(&["read", "bash", "edit", "write"])
    );
    assert_eq!(
        sorted(offered(HarnessId::Codex).names),
        sorted(&[
            "exec_command",
            "write_stdin",
            "request_user_input",
            "view_image",
            "get_goal",
            "create_goal",
            "update_goal",
            "web_search",
        ])
    );
}

/// A value the source's provider encrypted (an OpenAI Fernet token, `gAAAAA…`) is readable only by
/// that provider: in the text form of a call it is replaced by a placeholder with its length,
/// wherever it sits in the arguments. Codex Desktop's `send_message`, `followup_task` and
/// `spawn_agent` carry their whole `message` that way (decided with the user on 2026-10-28:
/// encrypted content is excluded).
#[test]
fn encrypted_argument_values_are_not_written_into_text_form() {
    let token = format!("gAAAAABq4cOT{}", "Ab-_9z".repeat(60));
    let len = token.chars().count();
    let call = call(
        CanonicalTool::Other("followup_task".into()),
        "followup_task",
        json!({
            "target": "/root/cessation_reconciliation",
            "message": token,
            "items": [{"note": format!("before {token} after")}],
            "short": "gAAAAAhello",
        }),
    );
    let text = tools::text_form_call(&call, HarnessId::ClaudeCode);
    let placeholder = format!("<encrypted by the source's provider, {len} chars>");
    assert!(!text.contains(&token), "{text}");
    assert!(text.contains(&format!("message: {placeholder}")), "{text}");
    assert!(
        text.contains(&format!("before {placeholder} after")),
        "{text}"
    );
    assert!(
        text.contains("target: /root/cessation_reconciliation"),
        "{text}"
    );
    assert!(
        text.contains("short: gAAAAAhello"),
        "a short look-alike stays: {text}"
    );

    let script = call_with_input(json!(format!(
        "text(await tools.send_message({{message:\"{token}\"}}));"
    )));
    let text = tools::text_form_unanswered(&script);
    assert!(
        text.contains(&format!(
            "tools.send_message({{message:\"{placeholder}\"}})"
        )),
        "{text}"
    );
}

fn call_with_input(input: Value) -> ToolCall {
    call(CanonicalTool::Other("exec".into()), "exec", input)
}
