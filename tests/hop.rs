//! The hop pipeline (§1): read, plan, check, write, and say what happened.
//!
//! The source is the bench's own fixture, staged where its harness's reader looks for it; every
//! store root points inside a scratch directory under Cargo's target tmp.

mod support;

use mukade::hop::{
    self, Compaction, HopError, HopRequest, KeepFrom, ResumePrompt, SessionRef, SourceFacts,
    SummarySource,
};
use mukade::model::{HarnessId, Kind, Session};
use mukade::roots::StoreRoots;
use mukade::summary;
use serde_json::Value;
use std::path::{Path, PathBuf};

use support::{read_destination, snapshot, source_id, stage_fixture, user_texts, written_text};

/// One scratch root for both sides of the hop.
fn roots(scratch: &Path) -> StoreRoots {
    support::roots(scratch)
}

/// A request that reads the harness's fixture from a scratch store and writes it into `dest`.
fn request(source: HarnessId, dest: HarnessId, tag: &str) -> (HopRequest, PathBuf) {
    let scratch = support::scratch(dest, tag);
    stage_fixture(source, &scratch);
    let opts = support::opts(dest, &scratch);
    let request = HopRequest {
        source: SessionRef::id(source, source_id(source)),
        dest,
        cwd: Some(support::cwd_for(dest)),
        compaction: Compaction::None,
        resume: None,
        reasoning: opts.reasoning,
        roots: roots(&scratch),
        write: support::details(&opts),
    };
    (request, scratch)
}

#[test]
fn a_hop_reads_the_source_writes_the_destination_and_reports_a_manifest() {
    let (request, _) = request(HarnessId::ClaudeCode, HarnessId::Pi, "hop-claude-to-pi");
    let receipt = hop::hop(&request).expect("the hop must complete");

    let manifest: &Value = &receipt.manifest;
    assert_eq!(manifest["source"]["harness"], "claude_code");
    assert_eq!(manifest["source"]["session_id"], source_id(HarnessId::ClaudeCode));
    assert_eq!(manifest["source"]["version"], "2.1.311");
    assert_eq!(
        manifest["source"]["content_sha256"].as_str().unwrap().len(),
        64
    );
    assert_eq!(manifest["dest"]["harness"], "pi");
    assert_eq!(manifest["dest"]["session_id"], receipt.dest_id);
    assert_eq!(manifest["library"]["name"], "mukade");
    assert_eq!(manifest["library"]["reader"], "claude_code@2.1.311");
    assert_eq!(manifest["library"]["writer"], "pi@1.2.0");
    assert_eq!(manifest["options"]["compaction"], "none");
    assert_eq!(manifest["options"]["reasoning"], "drop_foreign");
    assert_eq!(manifest["options"]["resume_prompt"], false);
    // The count is what the plan read: the source's conversation without its Meta rows.
    let source = support::read_source(HarnessId::ClaudeCode);
    assert_eq!(
        manifest["counts"]["messages_in"].as_u64().unwrap(),
        source.conversation().count() as u64
    );
    assert_eq!(
        manifest["strict"],
        serde_json::json!({"checked": true, "violations": []})
    );
    assert!(
        manifest["losses"]
            .as_array()
            .unwrap()
            .iter()
            .any(|l| l.as_str().unwrap().contains("compaction")),
        "the source's compaction is reported: {}",
        manifest["losses"]
    );

    // The destination session is new, and it holds the conversation the source had.
    assert_ne!(receipt.dest_id, source_id(HarnessId::ClaudeCode));
    let written = read_destination(HarnessId::Pi, &receipt.location);
    let source = support::read_source(HarnessId::ClaudeCode);
    assert_eq!(
        support::roundtrip_script(&written),
        support::roundtrip_script(&source)
    );
    assert_eq!(receipt.resume_command[0], "pi");
    assert!(receipt.resume_output.is_none(), "no prompt was asked for");
}

/// §4.2: the portable form is one user message with the handoff line and the source's own summary,
/// then the tail from the source's boundary.
#[test]
fn portable_carries_the_source_summary_and_the_tail_from_its_boundary() {
    let (mut request, _) = request(HarnessId::ClaudeCode, HarnessId::Pi, "hop-portable");
    request.compaction = Compaction::Portable {
        summary: SummarySource::SourceRecord,
        keep_from: KeepFrom::SourceBoundary,
    };
    let receipt = hop::hop(&request).expect("the hop must complete");
    assert_eq!(receipt.manifest["options"]["compaction"], "portable");
    let counts = receipt.manifest["counts"].clone();

    let written = read_destination(HarnessId::Pi, &receipt.location);
    let source = support::read_source(HarnessId::ClaudeCode);
    assert!(
        counts["messages_in"].as_u64().unwrap() < source.conversation().count() as u64,
        "the cut left fewer messages than the source had: {counts} against {}",
        source.conversation().count()
    );
    let summary = match source.last_compaction().map(|entry| &entry.kind) {
        Some(Kind::Compaction { summary, .. }) => summary.clone(),
        other => panic!("the fixture must hold a compaction, found {other:?}"),
    };
    let held = written_text(&written);
    assert!(
        held.contains("[mukade handoff v1]"),
        "the handoff line travels"
    );
    let texts = user_texts(&written);
    let (handoff, tail) = texts
        .split_first()
        .expect("the handoff is the first user message");
    assert!(handoff.starts_with("[mukade handoff v1]"), "{handoff}");
    assert!(
        handoff.contains(summary.trim()),
        "the source's own summary travels, verbatim:\n{handoff}"
    );
    let probe: String = summary.trim().chars().take(60).collect();
    assert_eq!(
        held.matches(&probe).count(),
        1,
        "the summary travels once, in the handoff: {probe}"
    );
    let tail = tail.join("\n");
    assert!(
        !tail.contains("H27-TOOLS") && !tail.contains("H27-T2"),
        "the turns before the boundary are not carried: {tail}"
    );
    assert!(tail.contains("H27-T3"), "but the tail is: {tail}");
    assert!(
        held.contains("H27-REPLY"),
        "the boundary itself, the reply before the summary, is the tail's first entry"
    );
    assert!(written.conversation().count() < source.conversation().count());
    assert!(
        written.interrupted_last_turn(),
        "the unanswered last turn survives the cut"
    );
}

#[test]
fn portable_can_keep_only_the_last_turns() {
    let (mut request, _) = request(HarnessId::ClaudeCode, HarnessId::Pi, "hop-last-turns");
    request.compaction = Compaction::Portable {
        summary: SummarySource::Supplied("we read two files and said hello".into()),
        keep_from: KeepFrom::LastTurns(1),
    };
    let receipt = hop::hop(&request).expect("the hop must complete");
    let written = read_destination(HarnessId::Pi, &receipt.location);
    let held = written_text(&written);
    assert!(held.contains("we read two files and said hello"));
    assert!(held.contains("[mukade handoff v1]"));
    let texts = user_texts(&written);
    assert_eq!(texts.len(), 2, "the handoff plus one user turn: {texts:?}");
    assert_eq!(texts[1], "H27-HANG", "the last turn is the one kept");
}

/// A tail can start at a named entry, by its id or by an unambiguous prefix: the handoff, then
/// that entry and everything after it.
#[test]
fn a_portable_tail_can_start_at_a_named_entry() {
    let source = support::read_source(HarnessId::ClaudeCode);
    let (index, id) = source
        .entries
        .iter()
        .enumerate()
        .find(|(_, entry)| matches!(&entry.kind, Kind::User { text, .. } if text.starts_with("H27-T3")))
        .map(|(index, entry)| (index, entry.id.clone()))
        .expect("the fixture's T3 turn");
    for named in [id.clone(), id[..8].to_string()] {
        let compaction = Compaction::Portable {
            summary: SummarySource::Supplied("earlier turns".into()),
            keep_from: KeepFrom::Message(named.clone()),
        };
        let (selection, facts) =
            hop::compact(&source, &compaction, &StoreRoots::default()).expect("compact");
        assert!(facts.is_none());
        assert_eq!(selection.entries[0].id, "portable-handoff");
        assert_eq!(selection.entries[1].id, id, "the tail starts at {named}");
        assert_eq!(selection.entries.len(), 1 + source.entries.len() - index);
    }
}

/// An id prefix that names more than one entry is refused, never resolved by position.
#[test]
fn an_ambiguous_entry_prefix_is_refused() {
    let mut source = support::read_source(HarnessId::Pi);
    source.entries = ["entry-a", "entry-b"]
        .into_iter()
        .map(|id| mukade::model::Entry {
            id: id.into(),
            kind: Kind::User {
                text: id.into(),
                attachments: Vec::new(),
            },
            raw: Vec::new(),
        })
        .collect();
    let compaction = Compaction::Portable {
        summary: SummarySource::Supplied("earlier turns".into()),
        keep_from: KeepFrom::Message("entry-".into()),
    };
    let error = hop::compact(&source, &compaction, &StoreRoots::default()).expect_err("ambiguous");
    assert!(matches!(error, HopError::Unsupported(_)), "{error:?}");
    assert!(error.to_string().contains("ambiguous"), "{error}");
}

#[test]
fn destination_native_is_refused_with_a_reason_and_nothing_is_written() {
    let (mut request, scratch) =
        request(HarnessId::ClaudeCode, HarnessId::Pi, "hop-native-refused");
    request.compaction = Compaction::DestinationNative;
    let error =
        hop::hop(&request).expect_err("the library does not run the destination's compaction");
    assert!(matches!(error, HopError::Unsupported(_)), "{error:?}");
    assert!(
        error.to_string().contains("triggered by the caller"),
        "{error}"
    );
    assert_eq!(error.exit_code(), 2, "refused with a reason");
    let sessions = scratch.join("home").join("sessions");
    assert!(
        !sessions.exists()
            || std::fs::read_dir(&sessions)
                .map(|mut d| d.next().is_none())
                .unwrap_or(true),
        "nothing was written"
    );
}

#[test]
fn a_source_that_is_not_there_is_a_refusal_not_a_failure() {
    let (mut request, _) = request(HarnessId::ClaudeCode, HarnessId::Pi, "hop-missing");
    request.source = SessionRef::id(HarnessId::ClaudeCode, "00000000-nope");
    let error = hop::hop(&request).expect_err("no such session");
    assert!(matches!(error, HopError::NotFound(_)), "{error:?}");
    assert_eq!(
        error.exit_code(),
        2,
        "nothing was written, the caller may retry"
    );
}

/// §5: the resume command is the destination's own, and the prompt is the last argument.
#[cfg(windows)]
#[test]
fn a_resume_prompt_runs_the_command_and_keeps_its_reply() {
    let argv = vec!["cmd".to_string(), "/c".to_string(), "echo".to_string()];
    let out = hop::run_resume(
        &argv,
        None,
        "hello destination",
        std::time::Duration::from_secs(30),
    )
    .unwrap();
    assert!(out.contains("hello destination"), "{out:?}");

    let failing = vec![
        "cmd".to_string(),
        "/c".to_string(),
        "exit".to_string(),
        "3".to_string(),
    ];
    let error =
        hop::run_resume(&failing, None, "x", std::time::Duration::from_secs(30)).unwrap_err();
    assert!(matches!(error, HopError::ResumeFailed { .. }), "{error:?}");
    assert_eq!(
        error.exit_code(),
        5,
        "the write happened: recovery is the caller's"
    );
}

#[test]
fn a_hop_into_the_same_harness_keeps_the_conversation() {
    let (request, _) = request(HarnessId::Codex, HarnessId::Codex, "hop-codex-to-codex");
    let receipt = hop::hop(&request).expect("the hop must complete");
    let written = read_destination(HarnessId::Codex, &receipt.location);
    let source = support::read_source(HarnessId::Codex);
    assert_eq!(
        support::roundtrip_script(&written),
        support::roundtrip_script(&source)
    );
    assert_eq!(receipt.manifest["dest"]["version"], "0.164.0");
    assert_eq!(receipt.manifest["library"]["writer"], "codex@0.164.0");
    assert_eq!(receipt.manifest["library"]["reader"], "codex@0.164.0");
    // L7: the destination's own session metadata, and the project the caller stated travels with it.
    assert_eq!(
        written.meta.cwd.as_deref(),
        Some(support::cwd_for(HarnessId::Codex).as_str())
    );
}

/// A harness that exits but leaves a child of its own holding the output open does not hold the
/// resume until that child ends.
#[test]
fn a_resume_returns_once_the_harness_exits_even_if_its_child_holds_the_output() {
    let scratch = support::scratch(HarnessId::Pi, "resume-lingering-child");
    let harness = support::launcher(
        &scratch,
        "harness",
        "echo answered\r\nstart /b ping -n 6 127.0.0.1",
        "echo answered\nsleep 5 &",
    );
    let started = std::time::Instant::now();
    let out = hop::run_resume(
        &[harness.to_string_lossy().into_owned()],
        None,
        "x",
        std::time::Duration::from_secs(30),
    )
    .unwrap();
    assert!(
        started.elapsed() < std::time::Duration::from_secs(3),
        "returned after {:?}",
        started.elapsed()
    );
    assert!(out.contains("answered"), "{out:?}");
}

/// An npm launcher is `cmd.exe` waiting on the real harness: a timeout stops the harness too, not
/// only the launcher, so nothing keeps writing to the session after the caller was told it failed.
#[cfg(windows)]
#[test]
fn a_timed_out_resume_stops_the_harness_behind_its_launcher() {
    let scratch = support::scratch(HarnessId::Pi, "resume-kill-tree");
    let marker = scratch.join("still-running.txt");
    let harness = support::launcher(
        &scratch,
        "harness",
        &format!(
            "ping -n 4 127.0.0.1 >nul\r\necho late>\"{}\"",
            marker.display()
        ),
        "",
    );
    let launcher = support::launcher(
        &scratch,
        "launcher",
        &format!("cmd /c \"{}\"", harness.display()),
        "",
    );
    let _ = hop::run_resume(
        &[launcher.to_string_lossy().into_owned()],
        None,
        "x",
        std::time::Duration::from_millis(500),
    );
    std::thread::sleep(std::time::Duration::from_secs(4));
    assert!(
        !marker.exists(),
        "the harness kept running after the timeout"
    );
}

/// Keeping no turns keeps none: only the handoff travels.
#[test]
fn portable_with_no_turns_keeps_only_the_handoff() {
    let (mut request, _) = request(HarnessId::ClaudeCode, HarnessId::Pi, "hop-no-turns");
    request.compaction = Compaction::Portable {
        summary: SummarySource::Supplied("we read two files and said hello".into()),
        keep_from: KeepFrom::LastTurns(0),
    };
    let receipt = hop::hop(&request).expect("the hop must complete");
    let written = read_destination(HarnessId::Pi, &receipt.location);
    assert_eq!(user_texts(&written).len(), 1, "{:?}", user_texts(&written));
}

/// §1.3: a hop into the source's own harness and project writes a new session beside the source
/// and leaves the source's file exactly as it was.
#[test]
fn a_hop_into_the_same_harness_leaves_the_source_untouched() {
    let (request, scratch) = request(HarnessId::Pi, HarnessId::Pi, "hop-same-harness");
    let store = scratch.join("home").join("sessions");
    let before = snapshot(&store);
    assert_eq!(before.len(), 1, "the staged source");
    let receipt = hop::hop(&request).expect("the hop must complete");
    let after = snapshot(&store);
    assert_eq!(
        after.len(),
        2,
        "one new session, nothing replaced: {after:?}"
    );
    assert!(
        after.contains(&before[0]),
        "the source's bytes and time are unchanged"
    );
    assert_ne!(Path::new(&receipt.location), before[0].0);
}

/// SourceFacts fingerprint the source's own records: the harness and session id it read, and a
/// SHA-256 over each record's compact JSON, one per line (the expected digest computed with
/// Python's hashlib). The fingerprint is stable across reads, and differs between sessions.
#[test]
fn the_source_facts_fingerprint_the_records_stably() {
    let source = support::read_source(HarnessId::Codex);
    let facts = SourceFacts::of(&source);
    assert_eq!(facts.harness, "codex");
    assert_eq!(facts.session_id, source_id(HarnessId::Codex));
    assert_eq!(facts.content_sha256.len(), 64);
    assert_eq!(
        facts,
        SourceFacts::of(&support::read_source(HarnessId::Codex)),
        "stable"
    );
    assert_ne!(
        facts.content_sha256,
        SourceFacts::of(&support::read_source(HarnessId::Pi)).content_sha256,
        "two sessions do not share a fingerprint"
    );

    let mut session = support::read_source(HarnessId::Pi);
    // A session with no file digest (built by hand, or from a document) is fingerprinted by its
    // records.
    session.meta.source_sha256 = None;
    session.entries = vec![mukade::model::Entry {
        id: "e".into(),
        kind: Kind::Meta {
            kind: mukade::model::MetaKind::Internal,
        },
        raw: vec![
            serde_json::json!({"a": 1}),
            serde_json::json!({"b": "café"}),
        ],
    }];
    assert_eq!(
        SourceFacts::of(&session).content_sha256,
        "18f569be308bed04a7a26c666d30444b2be698f0adaa689b43fe9ff89d9c9c79"
    );
}

/// The manifest's fingerprint of a source is the SHA-256 of its own bytes, so it can be checked
/// with `sha256sum` (the digests below were computed with Python's hashlib).
#[test]
fn a_sources_fingerprint_is_the_sha256_of_its_file() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures");
    let digest = |session: Result<Session, anyhow::Error>| {
        SourceFacts::of(&session.expect("the fixture reads")).content_sha256
    };
    assert_eq!(
        digest(mukade::reader::claude_code::read_file(&fixtures.join(
            "claude_code/source/c45db7bc-542c-4201-89de-b590c6fdb797.jsonl"
        ))),
        "df8f8f84d7fd3d21e66088171255f12021f06090c0349dce8ee7783e726206de"
    );
    assert_eq!(
        digest(mukade::reader::codex::read_file(&fixtures.join(
            "codex/source/rollout-2026-10-27T16-43-08-01a17e07-93f7-7a73-85c7-9f0205836600.jsonl"
        ))),
        "6fc8faaa724eaa70ce6ccef6132a19d3baa290a50a2e9c5c484d7c4d7fe932d7"
    );
    assert_eq!(
        digest(mukade::reader::pi::read_file(&fixtures.join(
            "pi/source/2026-10-27T16-00-39-689Z_01a17e17-9f07-743d-8072-8b3b13d65ab5.jsonl"
        ))),
        "978673389f8cbe70357fcb56624453069b6d86a38e04ddc277b9190694b51c73"
    );
    assert_eq!(
        digest(mukade::reader::opencode::read_file(
            &fixtures.join("opencode/source/session-export.json")
        )),
        "91f1a54dd0b5adc20bf1f7de65593d1c5d04f01100c1f0e191992c5a5823a8ab"
    );
}

/// A summary as the source's model would have written it (the run itself is `summary`'s).
fn generated(harness: HarnessId, text: &str) -> SummarySource {
    SummarySource::Generated(summary::Summary {
        text: text.into(),
        harness,
        harness_version: Some("1.4.0".into()),
        model: Some("anthropic/claude-x".into()),
        usage: None,
        leftover: None,
        prompt: None,
    })
}

/// The design's default: the summary alone stands for the whole history.
#[test]
fn a_model_summary_replaces_the_whole_history() {
    let source = support::read_source(HarnessId::Pi);
    let compaction = Compaction::Portable {
        summary: generated(HarnessId::Pi, "S"),
        keep_from: KeepFrom::LastTurns(0),
    };
    let (selection, facts) =
        hop::compact(&source, &compaction, &StoreRoots::default()).expect("compact");
    assert_eq!(selection.entries.len(), 1);
    assert!(
        matches!(&selection.entries[0].kind,
            Kind::Compaction { summary: text, .. } if *text == format!("{}\nS", summary::MODEL_SUMMARY_HANDOFF)),
        "{:?}",
        selection.entries[0].kind
    );
    assert_eq!(
        facts.expect("the summary's facts").left_out,
        source.entries.len()
    );
    assert_eq!(compaction.name(), "summary");
}

/// `--keep-turns 2`: the handoff, then the source from its second-to-last user turn on, unchanged.
#[test]
fn a_model_summary_can_keep_the_last_turns() {
    let source = support::read_source(HarnessId::ClaudeCode);
    let compaction = Compaction::Portable {
        summary: generated(HarnessId::ClaudeCode, "S"),
        keep_from: KeepFrom::LastTurns(2),
    };
    let (selection, facts) =
        hop::compact(&source, &compaction, &StoreRoots::default()).expect("compact");
    let users: Vec<usize> = source
        .entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| matches!(entry.kind, Kind::User { .. }))
        .map(|(i, _)| i)
        .collect();
    let cut = users[users.len() - 2];
    assert!(matches!(selection.entries[0].kind, Kind::Compaction { .. }));
    assert_eq!(selection.entries[1..], source.entries[cut..]);
    assert_eq!(
        selection.entries[1..]
            .iter()
            .filter(|entry| matches!(entry.kind, Kind::User { .. }))
            .count(),
        2
    );
    assert_eq!(facts.expect("the summary's facts").left_out, cut);
}

/// The manifest says where the history went: a summary by the source's model, not the source's
/// own compaction.
#[test]
fn a_model_summary_hop_says_where_the_summary_came_from() {
    let (mut request, _) = request(HarnessId::ClaudeCode, HarnessId::Pi, "hop-model-summary");
    request.compaction = Compaction::Portable {
        summary: generated(HarnessId::ClaudeCode, "we read two files and said hello"),
        keep_from: KeepFrom::LastTurns(0),
    };
    let receipt = hop::hop(&request).expect("the hop must complete");
    let options = &receipt.manifest["options"];
    assert_eq!(options["compaction"], "summary");
    assert_eq!(options["summary"]["source"], "source_model");
    assert_eq!(options["summary"]["harness"], "claude_code");
    assert_eq!(options["summary"]["prompt"], "v1");
    let losses: Vec<&str> = receipt.manifest["losses"]
        .as_array()
        .unwrap()
        .iter()
        .map(|loss| loss.as_str().unwrap())
        .collect();
    assert!(
        losses
            .iter()
            .any(|loss| loss.contains("travelled as a summary written by claude_code's model")),
        "{losses:?}"
    );
    assert!(
        !losses.iter().any(
            |loss| loss.contains("the source's own compaction is in the session")
                || loss.contains("the source's compaction summary travels")
        ),
        "{losses:?}"
    );
    let written = read_destination(HarnessId::Pi, &receipt.location);
    assert!(
        written_text(&written)
            .contains("written by the source agent's model when the session was moved")
    );
}

#[test]
fn a_failed_summary_is_a_failure_where_nothing_changed() {
    assert_eq!(HopError::SummaryFailed("x".into()).exit_code(), 4);
}

/// §4.4: a file source compacted by `--compaction portable` goes through the same selection as a
/// store source, and the manifest counts the messages the cut left out.
#[test]
fn file_compaction_uses_the_same_selection_and_accurate_manifest() {
    let source = support::read_source(HarnessId::ClaudeCode);
    let scratch = support::scratch(HarnessId::Pi, "hop-file-compaction");
    let write_opts = support::opts(HarnessId::Pi, &scratch);
    let compaction = Compaction::Portable {
        summary: SummarySource::SourceRecord,
        keep_from: KeepFrom::SourceBoundary,
    };
    let mut opts = support::import_opts(&write_opts, &source, None);
    opts.compaction = compaction.name().into();
    let receipt = hop::hop_session(HarnessId::Pi, &source, &compaction, &opts).unwrap();
    assert!(
        receipt.manifest["counts"]["messages_in"].as_u64().unwrap()
            < source.conversation().count() as u64
    );
    assert_eq!(receipt.manifest["options"]["compaction"], "portable");
}

/// A boundary that names a record not in the conversation cannot say where the tail starts: the
/// hop refuses instead of guessing.
#[test]
fn an_ambiguous_compaction_boundary_is_refused() {
    let mut source = support::read_source(HarnessId::ClaudeCode);
    source.entries.retain(|entry| matches!(entry.kind, Kind::Meta { .. }));
    source.entries.push(mukade::model::Entry {
        id: "compact".into(),
        kind: Kind::Compaction {
            summary: "summary".into(),
            kept_from: Some("entry".into()),
            trigger: Default::default(),
            tokens_before: None,
        },
        raw: Vec::new(),
    });
    for text in ["a", "b"] {
        source.entries.push(mukade::model::Entry {
            id: format!("entry-{text}"),
            kind: Kind::User {
                text: text.into(),
                attachments: Vec::new(),
            },
            raw: Vec::new(),
        });
    }
    let scratch = support::scratch(HarnessId::Pi, "hop-ambiguous-boundary");
    let write_opts = support::opts(HarnessId::Pi, &scratch);
    let compaction = Compaction::Portable {
        summary: SummarySource::SourceRecord,
        keep_from: KeepFrom::SourceBoundary,
    };
    let opts = support::import_opts(&write_opts, &source, None);
    let error =
        hop::hop_session(HarnessId::Pi, &source, &compaction, &opts).expect_err("ambiguous");
    assert!(matches!(error, HopError::Unsupported(_)), "{error}");
}

/// §5: a resume that fails after the write still hands back the receipt and the manifest, so the
/// caller can recover without writing the session again.
#[test]
fn resume_failure_returns_the_written_receipt_and_manifest() {
    let source = support::read_source(HarnessId::Pi);
    let scratch = support::scratch(HarnessId::Pi, "hop-resume-failure");
    let work = scratch.join("work");
    std::fs::create_dir_all(&work).unwrap();
    let mut write_opts = support::opts(HarnessId::Pi, &scratch);
    write_opts.cwd = Some(work.to_string_lossy().into_owned());
    write_opts.exe = Some(
        support::launcher(&scratch.join("bin"), "pi", "exit /b 3", "exit 3")
            .to_string_lossy()
            .into_owned(),
    );
    let resume = ResumePrompt::new("hello").with_timeout(std::time::Duration::from_secs(10));
    let opts = support::import_opts(&write_opts, &source, Some(resume));
    let error = hop::import(HarnessId::Pi, &source, &opts).expect_err("the launcher fails");
    let HopError::ResumeFailed { receipt, .. } = error else {
        panic!("wrong failure: {error:?}")
    };
    assert!(!receipt.dest_id.is_empty());
    assert!(Path::new(&receipt.location).exists());
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
    let source = support::read_source(HarnessId::Pi);
    let scratch = support::scratch(HarnessId::Pi, "hop-resume-cwd");
    // A name the test process's own working directory does not contain.
    let work = scratch.join("resume-runs-here");
    std::fs::create_dir_all(&work).unwrap();
    let mut write_opts = support::opts(HarnessId::Pi, &scratch);
    write_opts.cwd = Some(work.to_string_lossy().into_owned());
    write_opts.exe = Some(
        support::launcher(&scratch.join("bin"), "pi", "echo %CD%", "pwd")
            .to_string_lossy()
            .into_owned(),
    );
    let opts = support::import_opts(&write_opts, &source, Some(ResumePrompt::new("hello")));
    let receipt = hop::import(HarnessId::Pi, &source, &opts).unwrap();
    let output = receipt.resume_output.unwrap();
    assert!(
        output.contains("resume-runs-here"),
        "the resume ran in {output:?}"
    );
}

/// §5: the destination resumes against the store the session was written to, not whatever the
/// caller's own environment names.
#[test]
fn a_resume_uses_the_store_it_was_written_to() {
    let source = support::read_source(HarnessId::Pi);
    let scratch = support::scratch(HarnessId::Pi, "hop-resume-store");
    let work = scratch.join("work");
    std::fs::create_dir_all(&work).unwrap();
    let mut write_opts = support::opts(HarnessId::Pi, &scratch);
    write_opts.cwd = Some(work.to_string_lossy().into_owned());
    write_opts.exe = Some(
        support::launcher(
            &scratch.join("bin"),
            "pi",
            "echo store=%PI_CODING_AGENT_DIR%",
            "echo store=$PI_CODING_AGENT_DIR",
        )
        .to_string_lossy()
        .into_owned(),
    );
    let opts = support::import_opts(&write_opts, &source, Some(ResumePrompt::new("hello")));
    let output = hop::import(HarnessId::Pi, &source, &opts)
        .unwrap()
        .resume_output
        .unwrap();
    let home = scratch.join("home").to_string_lossy().into_owned();
    assert!(output.contains(&home), "{output:?} should name {home}");
}

/// A resume prompt the destination would misread is refused before anything is written: one that
/// starts with `-` is an option to every harness CLI.
#[test]
fn a_resume_prompt_that_looks_like_an_option_is_refused_before_the_write() {
    let source = support::read_source(HarnessId::Pi);
    let scratch = support::scratch(HarnessId::Pi, "hop-resume-dash");
    let opts = support::import_opts(
        &support::opts(HarnessId::Pi, &scratch),
        &source,
        Some(ResumePrompt::new("--help")),
    );
    let error = hop::import(HarnessId::Pi, &source, &opts).expect_err("a CLI option");
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
    let source = support::read_source(HarnessId::Pi);
    let scratch = support::scratch(HarnessId::Pi, "hop-resume-lines");
    let mut write_opts = support::opts(HarnessId::Pi, &scratch);
    write_opts.exe = Some(
        support::launcher(&scratch.join("bin"), "pi", "echo hi", "echo hi")
            .to_string_lossy()
            .into_owned(),
    );
    let opts = support::import_opts(
        &write_opts,
        &source,
        Some(ResumePrompt::new("first line\nsecond line")),
    );
    let error = hop::import(HarnessId::Pi, &source, &opts).expect_err("a line break");
    assert!(matches!(error, HopError::Unsupported(_)), "{error:?}");
    assert!(
        !scratch.join("home").join("sessions").exists(),
        "nothing was written"
    );
}

/// A source whose bytes changed after the caller fingerprinted it is refused before the
/// destination is written.
#[test]
fn a_changed_source_is_refused_before_the_destination_is_written() {
    let scratch = support::scratch(HarnessId::Pi, "hop-changed-source");
    let fixture = support::source_path(HarnessId::ClaudeCode);
    let original = std::fs::read(&fixture).unwrap();
    let run = |expected: &str| {
        support::cli_in(
            &scratch,
            &scratch,
            &[
                "hop",
                "claude_code",
                "fixture",
                "pi",
                "--from-file",
                fixture.to_str().unwrap(),
                "--cwd",
                scratch.to_str().unwrap(),
                "--dest-version",
                "1.4.0",
                "--expect-sha256",
                expected,
            ],
            &[
                ("PI_CODING_AGENT_DIR", scratch.to_str().unwrap()),
                (
                    "MUKADE_DATA_DIR",
                    scratch.join("data").to_str().unwrap(),
                ),
            ],
        )
    };
    let (code, _, stderr) = run(&"0".repeat(64));
    assert_eq!(code, Some(2), "{stderr}");
    assert!(stderr.contains("source changed"), "{stderr}");
    assert!(!scratch.join("sessions").exists());
    let (code, _, stderr) = run(&mukade::sha256::sha256_hex(&original));
    assert_eq!(code, Some(0), "{stderr}");
    assert!(scratch.join("sessions").exists());
    assert_eq!(std::fs::read(&fixture).unwrap(), original);
}
