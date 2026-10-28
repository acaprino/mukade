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
        source.entries.last().unwrap().kind
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
        let scratch = support::scratch(dest, "regression-attachments");
        let opts = opts(dest, &scratch);
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
        let scratch = support::scratch(dest, "regression-attachments-dropped");
        let plan = plan::plan(&source, dest, &opts(dest, &scratch));
        assert_eq!(plan.counts.dropped_attachments, 2, "{dest:?}");
        let Some(plan::PlannedBlock::User { attachments, .. }) = plan.blocks.last() else {
            panic!("{:?}", plan.blocks)
        };
        assert!(attachments.is_empty(), "{dest:?}: {attachments:?}");
    }
    // The source's own harness keeps its own record; only the one with no record is lost.
    let scratch = support::scratch(HarnessId::ClaudeCode, "regression-attachments-dropped");
    let plan = plan::plan(
        &source,
        HarnessId::ClaudeCode,
        &opts(HarnessId::ClaudeCode, &scratch),
    );
    assert_eq!(plan.counts.dropped_attachments, 1);
}

/// Pi and OpenCode carry images in tool results too, each in its own shape.
#[test]
fn an_image_in_a_tool_result_reaches_pi_in_pis_shape() {
    let mut source = session(Kind::Step {
        model: None,
        reasoning: Vec::new(),
        text: Vec::new(),
        calls: vec![ToolCall {
            id: "call-1".into(),
            source_name: "Read".into(),
            canonical: Some(mukade::model::CanonicalTool::Read),
            input: json!({"file_path": "shot.png"}),
        }],
        end: StepEnd::ToolUse,
    });
    source.entries.push(Entry {
        id: "result".into(),
        kind: Kind::Results {
            results: vec![ToolResult {
                call_id: "call-1".into(),
                content: ResultContent::Blocks(vec![
                    json!({"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": "iVBORw0KGgo="}}),
                    json!({"type": "search_result", "title": "x"}),
                ]),
                is_error: false,
                truncated_by_source: false,
            }],
        },
        raw: Vec::new(),
    });
    let scratch = support::scratch(HarnessId::Pi, "regression-result-image");
    let plan = plan::plan(&source, HarnessId::Pi, &opts(HarnessId::Pi, &scratch));
    let Some(plan::PlannedBlock::Results { results }) = plan.blocks.last() else {
        panic!("{:?}", plan.blocks)
    };
    let plan::PlannedResult::Native {
        content: ResultContent::Blocks(blocks),
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
            exe: write_opts.exe,
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

/// A launcher standing in for `opencode`: it records the store it was given, then answers
/// `session import` with `answer` (Windows `cmd` lines, POSIX `sh` lines).
fn mock_opencode(
    dir: &std::path::Path,
    capture: &std::path::Path,
    windows: &str,
    unix: &str,
) -> String {
    support::launcher(
        dir,
        "mock-opencode",
        // Records the store it was given, and the file it was asked to import (`<name>.file`).
        &format!(
            "echo OPENCODE_DB=%OPENCODE_DB%>\"{0}\"\r\necho %3>\"{0}.file\"\r\n{windows}",
            capture.display()
        ),
        &format!(
            "echo \"OPENCODE_DB=$OPENCODE_DB\" > '{0}'\necho \"$3\" > '{0}.file'\n{unix}",
            capture.display()
        ),
    )
    .to_string_lossy()
    .into_owned()
}

/// L1: the import runs against the requested database, and the temporary import document is gone
/// afterwards whether the import failed or succeeded. No real OpenCode is started.
#[test]
fn opencode_import_cleans_its_document_and_uses_the_requested_root() {
    let source = session(Kind::User {
        text: "hello".into(),
        attachments: Vec::new(),
    });
    for (tag, windows, unix, succeeds) in [
        ("fails", "exit /b 3", "exit 3", false),
        (
            "succeeds",
            "echo Imported session: ses_mock",
            "echo 'Imported session: ses_mock'",
            true,
        ),
    ] {
        let scratch = support::scratch(HarnessId::OpenCode, &format!("regression-opencode-{tag}"));
        let root = scratch.join("configured-home");
        let capture = scratch.join("environment.txt");
        let mut opts = opts(HarnessId::OpenCode, &scratch);
        opts.roots.opencode_db = Some(root.join("custom.db"));
        opts.exe = Some(mock_opencode(&scratch.join("bin"), &capture, windows, unix));
        let result = writer::opencode::Opencode.write(&source, &opts);
        match (&result, succeeds) {
            (Ok(written), true) => {
                assert_eq!(written.id, "ses_mock");
                assert_eq!(written.location, root.join("custom.db").to_string_lossy());
            }
            (Err(error), false) => assert!(
                error.to_string().contains("opencode session import failed"),
                "{error:#}"
            ),
            _ => panic!("{tag}: {result:?}"),
        }
        assert_eq!(
            std::fs::read_to_string(&capture).unwrap().trim(),
            format!("OPENCODE_DB={}", root.join("custom.db").to_string_lossy()),
            "{tag}"
        );
        // The import document lives in mukade's own temporary folder (`mukade-data/tmp` beside
        // the executable), not in the OpenCode store, and is gone.
        let imported = std::fs::read_to_string(format!("{}.file", capture.display())).unwrap();
        let imported = std::path::PathBuf::from(imported.trim());
        assert_eq!(
            imported.parent(),
            Some(mukade::portable::temp_dir().unwrap().as_path()),
            "{tag}"
        );
        assert!(
            !imported.exists(),
            "{tag}: the temporary import document was removed"
        );
        assert!(
            !root.exists() || std::fs::read_dir(&root).unwrap().next().is_none(),
            "{tag}: nothing was left in the OpenCode store folder"
        );
    }
}

/// A file a killed mukade left behind is removed by the next run once it is an hour old; a recent
/// one, which may belong to a running mukade, and anything not ours stay.
#[test]
fn a_stale_import_document_of_a_killed_run_is_swept() {
    let source = session(Kind::User {
        text: "hello".into(),
        attachments: Vec::new(),
    });
    let temp = mukade::portable::temp_dir().unwrap();
    let name = |tag: &str| temp.join(format!("mukade-import-{tag}-{}.json", std::process::id()));
    let age = |path: &std::path::Path, seconds: u64| {
        std::fs::write(path, "{}").unwrap();
        std::fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(seconds))
            .unwrap();
    };
    let (stale, fresh) = (name("stale"), name("fresh"));
    age(&stale, 48 * 60 * 60);
    age(&fresh, 5);
    let scratch = support::scratch(HarnessId::OpenCode, "regression-opencode-sweep");
    let mut opts = opts(HarnessId::OpenCode, &scratch);
    let capture = scratch.join("environment.txt");
    opts.exe = Some(mock_opencode(
        &scratch.join("bin"),
        &capture,
        "echo Imported session: ses_mock",
        "echo 'Imported session: ses_mock'",
    ));
    writer::opencode::Opencode.write(&source, &opts).unwrap();
    let (stale_left, fresh_left) = (stale.exists(), fresh.exists());
    let _ = std::fs::remove_file(&stale);
    let _ = std::fs::remove_file(&fresh);
    assert!(!stale_left, "a day-old import document is swept");
    assert!(fresh_left, "a recent one is left alone");
}

#[test]
fn resume_failure_returns_the_written_receipt_and_manifest() {
    let source = session(Kind::User {
        text: "hello".into(),
        attachments: Vec::new(),
    });
    let scratch = support::scratch(HarnessId::Pi, "regression-resume-failure");
    // A launcher that always fails stands in for Pi: no real harness is started.
    let mut write_opts = opts(HarnessId::Pi, &scratch);
    let work = scratch.join("work");
    std::fs::create_dir_all(&work).unwrap();
    write_opts.cwd = Some(work.to_string_lossy().into_owned());
    let pi = support::launcher(&scratch.join("bin"), "pi", "exit /b 3", "exit 3");
    write_opts.exe = Some(pi.to_string_lossy().into_owned());
    let opts = import_opts(
        write_opts,
        &source,
        ResumePrompt::new("hello").with_timeout(std::time::Duration::from_secs(10)),
    );
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

/// §5: the destination resumes from the session's own working directory, not from wherever the
/// caller happens to run.
#[test]
fn a_resume_runs_in_the_sessions_working_directory() {
    let source = session(Kind::User {
        text: "hello".into(),
        attachments: Vec::new(),
    });
    let scratch = support::scratch(HarnessId::Pi, "regression-resume-cwd");
    // A name the test process's own working directory does not contain.
    let work = scratch.join("resume-runs-here");
    std::fs::create_dir_all(&work).unwrap();
    let mut write_opts = opts(HarnessId::Pi, &scratch);
    write_opts.cwd = Some(work.to_string_lossy().into_owned());
    let pi = support::launcher(&scratch.join("bin"), "pi", "echo %CD%", "pwd");
    write_opts.exe = Some(pi.to_string_lossy().into_owned());
    let opts = import_opts(write_opts, &source, ResumePrompt::new("hello"));

    let receipt = hop::import(HarnessId::Pi, &source, &opts).unwrap();
    let output = receipt.resume_output.unwrap();
    assert!(
        output.contains("resume-runs-here"),
        "the resume ran in {output:?}"
    );
}

fn import_opts(
    write_opts: mukade::writer::WriteOpts,
    source: &Session,
    resume: ResumePrompt,
) -> ImportOpts {
    ImportOpts {
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
            exe: write_opts.exe,
        },
        resume: Some(resume),
        source_facts: SourceFacts::of(source),
        compaction: "none".into(),
    }
}

/// §5: the destination resumes against the store the session was written to, not whatever the
/// caller's own environment names.
#[test]
fn a_resume_uses_the_store_it_was_written_to() {
    let source = session(Kind::User {
        text: "hello".into(),
        attachments: Vec::new(),
    });
    let scratch = support::scratch(HarnessId::Pi, "regression-resume-store");
    let work = scratch.join("work");
    std::fs::create_dir_all(&work).unwrap();
    let mut write_opts = opts(HarnessId::Pi, &scratch);
    write_opts.cwd = Some(work.to_string_lossy().into_owned());
    let pi = support::launcher(
        &scratch.join("bin"),
        "pi",
        "echo store=%PI_CODING_AGENT_DIR%",
        "echo store=$PI_CODING_AGENT_DIR",
    );
    write_opts.exe = Some(pi.to_string_lossy().into_owned());
    let opts = import_opts(write_opts, &source, ResumePrompt::new("hello"));
    let receipt = hop::import(HarnessId::Pi, &source, &opts).unwrap();
    let output = receipt.resume_output.unwrap();
    let home = scratch.join("home").to_string_lossy().into_owned();
    assert!(output.contains(&home), "{output:?} should name {home}");
}

/// A resume prompt the destination would misread is refused before anything is written: one that
/// starts with `-` is an option to every harness CLI.
#[test]
fn a_resume_prompt_that_looks_like_an_option_is_refused_before_the_write() {
    let source = session(Kind::User {
        text: "hello".into(),
        attachments: Vec::new(),
    });
    let scratch = support::scratch(HarnessId::Pi, "regression-resume-dash");
    let opts = import_opts(
        opts(HarnessId::Pi, &scratch),
        &source,
        ResumePrompt::new("--help"),
    );
    let error = hop::import(HarnessId::Pi, &source, &opts).unwrap_err();
    assert!(matches!(error, HopError::Unsupported(_)), "{error:?}");
    assert!(
        !scratch.join("home").join("sessions").exists(),
        "nothing was written"
    );
}

/// Rust refuses to pass a line break to a `.cmd` launcher; that is known before the write.
#[cfg(windows)]
#[test]
fn a_multi_line_resume_prompt_for_a_cmd_launcher_is_refused_before_the_write() {
    let source = session(Kind::User {
        text: "hello".into(),
        attachments: Vec::new(),
    });
    let scratch = support::scratch(HarnessId::Pi, "regression-resume-lines");
    let mut write_opts = opts(HarnessId::Pi, &scratch);
    let pi = support::launcher(&scratch.join("bin"), "pi", "echo hi", "echo hi");
    write_opts.exe = Some(pi.to_string_lossy().into_owned());
    let opts = import_opts(
        write_opts,
        &source,
        ResumePrompt::new("first line\nsecond line"),
    );
    let error = hop::import(HarnessId::Pi, &source, &opts).unwrap_err();
    assert!(matches!(error, HopError::Unsupported(_)), "{error:?}");
    assert!(
        !scratch.join("home").join("sessions").exists(),
        "nothing was written"
    );
}
