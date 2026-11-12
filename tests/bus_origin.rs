mod support;
use mukade::reader::origin::{Origin, origin_of};
use mukade::{Entry, HarnessId, Kind};
use serde_json::{Value, json};

fn user(raw: Vec<Value>) -> Entry {
    Entry {
        id: "u".into(),
        kind: Kind::User {
            text: "hello".into(),
            attachments: vec![],
        },
        raw,
    }
}

#[test]
fn pi_bash_execution_is_a_shell_command_without_its_output() {
    let origin = origin_of(
        HarnessId::Pi,
        &user(vec![
            json!({"type":"message","message":{"role":"bashExecution","command":"ls","output":"SECRET=abc"}}),
        ]),
    );
    assert_eq!(
        origin,
        Origin::ShellCommand {
            command: "ls".into()
        }
    );
    assert!(!format!("{origin:?}").contains("SECRET"));
}

#[test]
fn pi_custom_and_branch_summary_are_injected() {
    for kind in ["custom_message", "branch_summary"] {
        assert!(matches!(
            origin_of(HarnessId::Pi, &user(vec![json!({"type":kind})])),
            Origin::Injected(_)
        ));
    }
}

#[test]
fn claude_ismeta_row_is_injected() {
    assert_eq!(
        origin_of(
            HarnessId::ClaudeCode,
            &user(vec![json!({"type":"user","isMeta":true})])
        ),
        Origin::Injected("expanded_command")
    );
}

#[test]
fn claude_empty_raw_user_text_is_typed() {
    assert_eq!(
        origin_of(HarnessId::ClaudeCode, &user(vec![])),
        Origin::Typed
    );
}

#[test]
fn fixture_user_prompts_are_typed_for_all_four_harnesses() {
    for harness in HarnessId::ALL {
        let session = support::read_source(harness);
        let prompts: Vec<_> = session
            .entries
            .iter()
            .filter(|e| matches!(e.kind, Kind::User { .. }))
            .collect();
        assert!(!prompts.is_empty());
        for entry in prompts {
            assert_eq!(
                origin_of(harness, entry),
                Origin::Typed,
                "{harness}: {}",
                entry.id
            );
        }
    }
}

#[test]
fn unexpected_raw_shape_is_unknown() {
    for harness in HarnessId::ALL {
        assert_eq!(
            origin_of(harness, &user(vec![json!({"surprise":true})])),
            Origin::Unknown
        );
    }
}

#[test]
fn codex_harness_provenance_is_not_a_typed_prompt() {
    let entry = user(vec![
        json!({"type":"response_item","payload":{"type":"message","role":"user","internal_chat_message_metadata_passthrough":{"content_item_metadata":[{"provenance":{"type":"harness"}}]}}}),
    ]);
    assert!(matches!(
        origin_of(HarnessId::Codex, &entry),
        Origin::Injected(_)
    ));
}
