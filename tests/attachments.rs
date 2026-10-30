//! Images and attachments between harnesses: each destination's own shape, and no silent loss.
//!
//! Every test writes into a scratch store; no real harness runs.

mod support;

use mukade::model::{Attachment, Entry, HarnessId, Kind, Session};
use mukade::writer::{self, plan};
use serde_json::json;

/// A session holding only the Claude fixture's metadata and one entry, so a plan has a single
/// attachment to carry.
fn session(kind: Kind) -> Session {
    let mut session = support::read_source(HarnessId::ClaudeCode);
    session
        .entries
        .retain(|entry| matches!(entry.kind, Kind::Meta { .. }));
    session.entries.push(Entry {
        id: "attachment".into(),
        kind,
        raw: Vec::new(),
    });
    session
}

#[test]
fn attachments_reach_every_writer_in_its_own_image_shape() {
    const PNG: &str = "iVBORw0KGgo=";
    let attachment = Attachment {
        kind: "image".into(),
        path: None,
        raw: json!({"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": PNG}}),
    };
    let source = session(Kind::User {
        text: "look".into(),
        attachments: vec![attachment.clone()],
    });
    let restored = mukade::wire::from_document(&mukade::wire::to_document(&source)).unwrap();
    assert_eq!(
        restored.entries.last().unwrap().kind,
        source.entries.last().unwrap().kind,
        "the document keeps the attachment as the source wrote it"
    );

    for (dest, expected) in [
        (HarnessId::ClaudeCode, attachment.raw.clone()),
        (
            HarnessId::Codex,
            json!({"type": "input_image", "image_url": format!("data:image/png;base64,{PNG}")}),
        ),
        (
            HarnessId::Pi,
            json!({"type": "image", "data": PNG, "mimeType": "image/png"}),
        ),
        (
            HarnessId::OpenCode,
            json!({"data": PNG, "mime": "image/png", "source": {"type": "inline"}}),
        ),
    ] {
        let scratch = support::scratch(dest, "attachments-carried");
        let mut opts = support::opts(dest, &scratch);
        opts.project_id = Some("project-under-test".into());
        assert_eq!(
            plan::plan(&source, dest, &opts).counts.dropped_attachments,
            0,
            "{dest:?}"
        );
        if dest == HarnessId::OpenCode {
            let mut clock = mukade::policy::Clock::new(Some("2026-10-28T00:00:00Z"));
            let doc = writer::opencode::document_for(
                &source,
                "ses_test",
                opts.cwd.as_deref().unwrap(),
                &opts,
                &mut clock,
            )
            .unwrap();
            assert_eq!(doc["messages"][0]["files"][0], expected);
            continue;
        }
        let written = writer::for_harness(dest).write(&source, &opts).unwrap();
        let back = support::read_destination(dest, &written.location);
        // The last conversation entry: a written Claude session ends with its `last-prompt` row.
        let Some(Kind::User { attachments, .. }) =
            back.conversation().last().map(|entry| &entry.kind)
        else {
            panic!("the user turn did not survive for {dest:?}");
        };
        assert_eq!(attachments.len(), 1, "one attachment carried for {dest:?}");
        assert_eq!(
            attachments[0].raw, expected,
            "the image in {dest:?}'s own shape"
        );
    }
}

/// No loss is silent: an attachment no destination shape fits (a Claude PDF `document`, a record
/// a document carried without its raw block) is dropped and counted, never passed on as is.
#[test]
fn an_attachment_the_destination_cannot_hold_is_dropped_and_counted() {
    let document = Attachment {
        kind: "document".into(),
        path: None,
        raw: json!({"type": "document", "source": {"type": "base64", "media_type": "application/pdf", "data": "JVBERi0="}}),
    };
    let bare = Attachment {
        kind: "image".into(),
        path: Some("image.png".into()),
        raw: serde_json::Value::Null,
    };
    let source = session(Kind::User {
        text: "look".into(),
        attachments: vec![document, bare],
    });
    for dest in [HarnessId::Codex, HarnessId::OpenCode, HarnessId::Pi] {
        let scratch = support::scratch(dest, "attachments-dropped");
        let plan = plan::plan(&source, dest, &support::opts(dest, &scratch));
        assert_eq!(plan.counts.dropped_attachments, 2, "{dest:?}");
        let Some(plan::PlannedBlock::User { attachments, .. }) = plan.blocks.last() else {
            panic!("{:?}", plan.blocks)
        };
        assert!(attachments.is_empty(), "{dest:?}: {attachments:?}");
    }
    // The source's own harness keeps its own record; only the one with no record is lost.
    let scratch = support::scratch(HarnessId::ClaudeCode, "attachments-dropped");
    let plan = plan::plan(
        &source,
        HarnessId::ClaudeCode,
        &support::opts(HarnessId::ClaudeCode, &scratch),
    );
    assert_eq!(plan.counts.dropped_attachments, 1);
}

/// Pi and OpenCode carry images in tool results too, each in its own shape; a block they have no
/// shape for is counted, not silently dropped.
#[test]
fn an_image_in_a_tool_result_reaches_pi_in_pis_shape() {
    let mut source = session(Kind::Step {
        model: None,
        reasoning: Vec::new(),
        text: Vec::new(),
        calls: vec![mukade::model::ToolCall {
            id: "call-1".into(),
            source_name: "Read".into(),
            canonical: Some(mukade::model::CanonicalTool::Read),
            input: json!({"file_path": "shot.png"}),
        }],
        end: mukade::model::StepEnd::ToolUse,
    });
    source.entries.push(Entry {
        id: "result".into(),
        kind: Kind::Results {
            results: vec![mukade::model::ToolResult {
                call_id: "call-1".into(),
                content: mukade::model::ResultContent::Blocks(vec![
                    json!({"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": "iVBORw0KGgo="}}),
                    json!({"type": "search_result", "title": "x"}),
                ]),
                is_error: false,
                truncated_by_source: false,
            }],
        },
        raw: Vec::new(),
    });
    let scratch = support::scratch(HarnessId::Pi, "attachments-result-image");
    let plan = plan::plan(&source, HarnessId::Pi, &support::opts(HarnessId::Pi, &scratch));
    let Some(plan::PlannedBlock::Results { results }) = plan.blocks.last() else {
        panic!("{:?}", plan.blocks)
    };
    let plan::PlannedResult::Native {
        content: mukade::model::ResultContent::Blocks(blocks),
        ..
    } = &results[0]
    else {
        panic!("{results:?}")
    };
    assert_eq!(
        blocks,
        &vec![json!({"type": "image", "data": "iVBORw0KGgo=", "mimeType": "image/png"})]
    );
    assert_eq!(
        plan.counts.dropped_result_blocks, 1,
        "the block Pi has no shape for is counted"
    );
}
