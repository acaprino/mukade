//! Regressions for the conversion paths that previously hid losses or skipped a stage.

mod support;

use mukade::hop::{self, HopError, ImportOpts, ResumePrompt, SourceFacts, WriteDetails};
use mukade::model::{
    Attachment, Entry, HarnessId, Kind, ResultContent, Session, StepEnd, ToolCall, ToolResult,
};
use mukade::roots::StoreRoots;
use mukade::writer::{self, Writer, plan};
use serde_json::json;

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

fn opts(dest: HarnessId, scratch: &std::path::Path) -> mukade::writer::WriteOpts {
    let mut opts = support::opts(dest, scratch);
    opts.roots = StoreRoots {
        claude_config_dir: Some(scratch.join("home")),
        codex_home: Some(scratch.join("home")),
        opencode_db: Some(scratch.join("home").join("opencode.db")),
        pi_agent_dir: Some(scratch.join("home")),
    };
    opts.cwd = Some(scratch.join("work").to_string_lossy().into_owned());
    opts.project_id = Some("project-under-test".into());
    opts
}

#[test]
fn attachments_survive_documents_and_all_four_native_writers() {
    let attachment = Attachment {
        kind: "image".into(),
        path: Some("image.png".into()),
        raw: json!({"type": "image", "source": {"data": "synthetic-image"}}),
    };
    let source = session(Kind::User {
        text: "look".into(),
        attachments: vec![attachment.clone()],
    });
    let restored = mukade::wire::from_document(&mukade::wire::to_document(&source)).unwrap();
    assert_eq!(
        restored.entries.last().unwrap().kind,
        source.entries.last().unwrap().kind
    );

    for dest in [
        HarnessId::ClaudeCode,
        HarnessId::Codex,
        HarnessId::OpenCode,
        HarnessId::Pi,
    ] {
        let scratch = support::scratch(dest, "regression-attachments");
        let opts = opts(dest, &scratch);
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
            assert_eq!(
                doc["messages"][0]["files"][0]["source"]["data"],
                "synthetic-image"
            );
            continue;
        }
        let written = writer::for_harness(dest).write(&source, &opts).unwrap();
        let back = match dest {
            HarnessId::ClaudeCode => {
                mukade::reader::claude_code::read_file(std::path::Path::new(&written.location))
            }
            HarnessId::Codex => {
                mukade::reader::codex::read_file(std::path::Path::new(&written.location))
            }
            HarnessId::Pi => mukade::reader::pi::read_file(std::path::Path::new(&written.location)),
            HarnessId::OpenCode => unreachable!(),
        }
        .unwrap();
        let Some(Kind::User { attachments, .. }) = back.entries.last().map(|entry| &entry.kind)
        else {
            panic!("the user turn did not survive for {dest:?}");
        };
        assert_eq!(
            attachments.len(),
            1,
            "one attachment preserved for {dest:?}"
        );
        assert_eq!(
            attachments[0].raw, attachment.raw,
            "attachment record preserved for {dest:?}"
        );
    }
}

#[test]
fn flattened_structured_results_are_counted_and_reported() {
    let mut source = session(Kind::Step {
        model: None,
        reasoning: Vec::new(),
        text: Vec::new(),
        calls: vec![ToolCall {
            id: "call-1".into(),
            source_name: "Read".into(),
            canonical: None,
            input: json!({}),
        }],
        end: StepEnd::ToolUse,
    });
    source.entries.push(Entry {
        id: "result".into(),
        kind: Kind::Results {
            results: vec![ToolResult {
                call_id: "call-1".into(),
                content: ResultContent::Blocks(vec![
                    json!({"type": "text", "text": "kept"}),
                    json!({"type": "image", "data": "not-text"}),
                ]),
                is_error: false,
                truncated_by_source: false,
            }],
        },
        raw: Vec::new(),
    });
    let scratch = support::scratch(HarnessId::ClaudeCode, "regression-loss-count");
    let opts = opts(HarnessId::ClaudeCode, &scratch);
    let plan = plan::plan(&source, HarnessId::ClaudeCode, &opts);
    assert_eq!(plan.counts.dropped_result_blocks, 1);
    let losses = mukade::manifest::Manifest::losses_for(&plan.counts, false, false);
    assert!(
        losses
            .iter()
            .any(|loss| loss.contains("structured tool-result blocks"))
    );
}

#[test]
fn file_compaction_uses_the_same_selection_and_accurate_manifest() {
    let scratch = support::scratch(HarnessId::Pi, "regression-file-compaction");
    let source =
        mukade::reader::claude_code::read_file(&support::source_path(HarnessId::ClaudeCode))
            .unwrap();
    let write_opts = opts(HarnessId::Pi, &scratch);
    let request = hop::HopRequest {
        source: hop::SessionRef::id(HarnessId::ClaudeCode, "fixture"),
        dest: HarnessId::Pi,
        cwd: write_opts.cwd.clone(),
        compaction: mukade::hop::Compaction::None,
        resume: None,
        reasoning: write_opts.reasoning,
        roots: write_opts.roots.clone(),
        write: WriteDetails {
            dest_version: write_opts.dest_version,
            dest_protocol: write_opts.dest_protocol,
            dest_shell: write_opts.dest_shell,
            offered: write_opts.offered,
            model_provider: write_opts.model_provider,
            model: write_opts.model,
            project_id: write_opts.project_id,
            now: write_opts.now,
            opencode_exe: write_opts.opencode_exe,
        },
    };
    let compaction = mukade::hop::Compaction::Portable {
        summary: mukade::hop::SummarySource::SourceRecord,
        keep_from: mukade::hop::KeepFrom::SourceBoundary,
    };
    let opts = ImportOpts {
        cwd: request.cwd,
        roots: request.roots,
        reasoning: request.reasoning,
        write: request.write,
        resume: None,
        source_facts: SourceFacts::of(&source),
        compaction: compaction.name().into(),
    };
    let receipt = hop::hop_session(HarnessId::Pi, &source, &compaction, &opts).unwrap();
    assert!(
        receipt.manifest["counts"]["messages_in"].as_u64().unwrap()
            < source.conversation().count() as u64
    );
    assert_eq!(receipt.manifest["options"]["compaction"], "portable");
    let _ = scratch;
}

#[test]
fn an_ambiguous_compaction_boundary_is_refused() {
    let mut source = session(Kind::User {
        text: "tail".into(),
        attachments: Vec::new(),
    });
    source.entries.insert(
        0,
        Entry {
            id: "compact".into(),
            kind: Kind::Compaction {
                summary: "summary".into(),
                kept_from: Some("entry".into()),
                trigger: Default::default(),
                tokens_before: None,
            },
            raw: Vec::new(),
        },
    );
    source.entries.insert(
        1,
        Entry {
            id: "entry-a".into(),
            kind: Kind::User {
                text: "a".into(),
                attachments: Vec::new(),
            },
            raw: Vec::new(),
        },
    );
    source.entries.insert(
        2,
        Entry {
            id: "entry-b".into(),
            kind: Kind::User {
                text: "b".into(),
                attachments: Vec::new(),
            },
            raw: Vec::new(),
        },
    );
    let scratch = support::scratch(HarnessId::Pi, "regression-ambiguous");
    let write_opts = opts(HarnessId::Pi, &scratch);
    let opts = ImportOpts {
        cwd: write_opts.cwd,
        roots: write_opts.roots,
        reasoning: write_opts.reasoning,
        write: WriteDetails::default(),
        resume: None,
        source_facts: SourceFacts::of(&source),
        compaction: "portable".into(),
    };
    let error = hop::hop_session(
        HarnessId::Pi,
        &source,
        &mukade::hop::Compaction::Portable {
            summary: mukade::hop::SummarySource::SourceRecord,
            keep_from: mukade::hop::KeepFrom::SourceBoundary,
        },
        &opts,
    )
    .unwrap_err();
    assert!(matches!(error, HopError::Unsupported(_)), "{error}");
}

#[test]
fn opencode_import_cleans_its_document_and_uses_the_requested_root() {
    let source = session(Kind::User {
        text: "hello".into(),
        attachments: Vec::new(),
    });
    let scratch = support::scratch(HarnessId::OpenCode, "regression-opencode-process");
    let root = scratch.join("configured-home");
    let script = scratch.join("mock-opencode.cmd");
    std::fs::write(&script, "@echo off\r\nset OPENCODE_DB > \"%MUKADE_CAPTURE%\"\r\nif not \"%1\"==\"session\" exit /b 2\r\nexit /b 3\r\n").unwrap();
    let capture = scratch.join("environment.txt");
    // The test's environment is private to this subprocess; no real OpenCode store is invoked.
    let mut opts = opts(HarnessId::OpenCode, &scratch);
    opts.roots.opencode_db = Some(root.join("custom.db"));
    opts.opencode_exe = Some(script.to_string_lossy().into_owned());
    // This executable deliberately fails import, making cleanup observable in the failure path.
    let old = std::env::var_os("MUKADE_CAPTURE");
    unsafe { std::env::set_var("MUKADE_CAPTURE", &capture) };
    let error = writer::opencode::Opencode
        .write(&source, &opts)
        .unwrap_err();
    match old {
        Some(value) => unsafe { std::env::set_var("MUKADE_CAPTURE", value) },
        None => unsafe { std::env::remove_var("MUKADE_CAPTURE") },
    }
    assert!(
        error.to_string().contains("opencode session import failed"),
        "{error:#}"
    );
    assert_eq!(
        std::fs::read_to_string(capture).unwrap().trim(),
        format!("OPENCODE_DB={}", root.join("custom.db").to_string_lossy())
    );
    assert!(!root.join("mukade-import-test.json").exists());
    assert_eq!(
        std::fs::read_dir(&root).unwrap().count(),
        0,
        "the temporary import document was removed"
    );
}

#[test]
fn resume_failure_returns_the_written_receipt_and_manifest() {
    let source = session(Kind::User {
        text: "hello".into(),
        attachments: Vec::new(),
    });
    let scratch = support::scratch(HarnessId::Pi, "regression-resume-failure");
    let write_opts = opts(HarnessId::Pi, &scratch);
    let opts = ImportOpts {
        cwd: write_opts.cwd,
        roots: write_opts.roots,
        reasoning: write_opts.reasoning,
        write: WriteDetails {
            dest_version: write_opts.dest_version,
            dest_protocol: write_opts.dest_protocol,
            dest_shell: write_opts.dest_shell,
            offered: write_opts.offered,
            model_provider: write_opts.model_provider,
            model: write_opts.model,
            project_id: write_opts.project_id,
            now: write_opts.now,
            opencode_exe: write_opts.opencode_exe,
        },
        resume: Some(ResumePrompt::new("hello").with_timeout(std::time::Duration::from_secs(2))),
        source_facts: SourceFacts::of(&source),
        compaction: "none".into(),
    };
    let error = hop::import(HarnessId::Pi, &source, &opts).unwrap_err();
    let HopError::ResumeFailed { receipt, .. } = error else {
        panic!("wrong failure")
    };
    assert!(!receipt.dest_id.is_empty());
    assert!(std::path::Path::new(&receipt.location).exists());
    assert!(
        receipt.manifest["losses"]
            .as_array()
            .unwrap()
            .iter()
            .any(|loss| loss.as_str().unwrap().contains("resume prompt"))
    );
}
