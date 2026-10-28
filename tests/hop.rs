//! The hop pipeline (§1): read, plan, check, write, and say what happened.
//!
//! The source is the bench's own fixture, staged where its harness's reader looks for it; every
//! store root points inside a scratch directory under the system temporary folder.

mod support;

use mukade::hop::{
    self, Compaction, HopError, HopRequest, KeepFrom, SessionRef, SourceFacts, SummarySource,
    WriteDetails,
};
use mukade::model::{HarnessId, Kind, Session};
use mukade::roots::StoreRoots;
use serde_json::Value;
use std::path::{Path, PathBuf};

const CLAUDE_ID: &str = "c45db7bc-542c-4201-89de-b590c6fdb797";
const CODEX_ID: &str = "01a17e07-93f7-7a73-85c7-9f0205836600";

fn source_id(harness: HarnessId) -> &'static str {
    match harness {
        HarnessId::ClaudeCode => CLAUDE_ID,
        HarnessId::Codex => CODEX_ID,
        HarnessId::OpenCode => "ses_e82023514ffe02y4T8OoUu99VZ",
        HarnessId::Pi => "01a17e17-9f07-743d-8072-8b3b13d65ab5",
    }
}

/// Copy a harness's fixture into the store layout its reader expects.
fn stage(harness: HarnessId, scratch: &Path) {
    let home = scratch.join("home");
    let text = std::fs::read_to_string(support::source_path(harness)).expect("read the fixture");
    let file = match harness {
        HarnessId::ClaudeCode => home
            .join("projects")
            .join("claude-project")
            .join(format!("{CLAUDE_ID}.jsonl")),
        HarnessId::Codex => home
            .join("sessions")
            .join("2026")
            .join("10")
            .join("27")
            .join(format!("rollout-2026-10-27T16-43-08-{CODEX_ID}.jsonl")),
        HarnessId::Pi => home
            .join("sessions")
            .join("--pi-project--")
            .join("2026-10-27T16-00-39-689Z_01a17e17-9f07-743d-8072-8b3b13d65ab5.jsonl"),
        HarnessId::OpenCode => home.join("opencode-sessions.json"),
    };
    std::fs::create_dir_all(file.parent().unwrap()).expect("create the store directory");
    std::fs::write(file, text).expect("stage the fixture");
}

/// One scratch root for both sides of the hop.
fn roots(scratch: &Path) -> StoreRoots {
    let home = scratch.join("home");
    StoreRoots {
        claude_config_dir: Some(home.clone()),
        codex_home: Some(home.clone()),
        opencode_db: Some(home.join("opencode.db")),
        pi_agent_dir: Some(home),
    }
}

/// A request that reads the harness's fixture from a scratch store and writes it into `dest`.
fn request(source: HarnessId, dest: HarnessId, tag: &str) -> (HopRequest, PathBuf) {
    let scratch = support::scratch(dest, tag);
    stage(source, &scratch);
    let opts = support::opts(dest, &scratch);
    let request = HopRequest {
        source: SessionRef::id(source, source_id(source)),
        dest,
        cwd: Some(support::cwd_for(dest)),
        compaction: Compaction::None,
        resume: None,
        reasoning: opts.reasoning,
        roots: roots(&scratch),
        write: WriteDetails {
            dest_version: opts.dest_version,
            dest_protocol: opts.dest_protocol,
            dest_shell: opts.dest_shell,
            offered: opts.offered,
            model_provider: opts.model_provider,
            model: opts.model,
            project_id: opts.project_id,
            now: opts.now,
            exe: opts.exe,
        },
    };
    (request, scratch)
}

fn read_destination(harness: HarnessId, location: &str) -> Session {
    let path = Path::new(location);
    match harness {
        HarnessId::ClaudeCode => mukade::reader::claude_code::read_file(path),
        HarnessId::Codex => mukade::reader::codex::read_file(path),
        HarnessId::OpenCode => mukade::reader::opencode::read_file(path),
        HarnessId::Pi => mukade::reader::pi::read_file(path),
    }
    .expect("the destination session must read back")
}

fn user_texts(session: &Session) -> Vec<String> {
    session
        .conversation()
        .filter_map(|entry| match &entry.kind {
            Kind::User { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

fn written_text(session: &Session) -> String {
    session
        .entries
        .iter()
        .flat_map(|entry| entry.raw.clone())
        .map(|record| record.to_string())
        .collect::<Vec<String>>()
        .join("\n")
}

#[test]
fn a_hop_reads_the_source_writes_the_destination_and_reports_a_manifest() {
    let (request, _) = request(HarnessId::ClaudeCode, HarnessId::Pi, "hop-claude-to-pi");
    let receipt = hop::hop(&request).expect("the hop must complete");

    let manifest: &Value = &receipt.manifest;
    assert_eq!(manifest["source"]["harness"], "claude_code");
    assert_eq!(manifest["source"]["session_id"], CLAUDE_ID);
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
    assert_ne!(receipt.dest_id, CLAUDE_ID);
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

#[test]
fn the_source_facts_are_a_fingerprint_of_the_records() {
    let source = support::read_source(HarnessId::Codex);
    let facts = SourceFacts::of(&source);
    assert_eq!(facts.harness, "codex");
    assert_eq!(facts.session_id, CODEX_ID);
    assert_eq!(facts.content_sha256.len(), 64);
    assert_eq!(
        facts,
        SourceFacts::of(&support::read_source(HarnessId::Codex)),
        "stable"
    );
    let other = SourceFacts::of(&support::read_source(HarnessId::Pi));
    assert_ne!(facts.content_sha256, other.content_sha256);
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

/// §4.2: the source's summary travels once, in the handoff; the source's own compaction record in
/// the tail is not written a second time.
#[test]
fn portable_writes_the_source_summary_once() {
    let (mut request, _) = request(HarnessId::ClaudeCode, HarnessId::Pi, "hop-portable-once");
    request.compaction = Compaction::Portable {
        summary: SummarySource::SourceRecord,
        keep_from: KeepFrom::SourceBoundary,
    };
    let receipt = hop::hop(&request).expect("the hop must complete");
    let written = read_destination(HarnessId::Pi, &receipt.location);
    let source = support::read_source(HarnessId::ClaudeCode);
    let summary = match source.last_compaction().map(|entry| &entry.kind) {
        Some(Kind::Compaction { summary, .. }) => summary.clone(),
        other => panic!("the fixture must hold a compaction, found {other:?}"),
    };
    let probe: String = summary.trim().chars().take(60).collect();
    assert_eq!(written_text(&written).matches(&probe).count(), 1, "{probe}");
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

/// Every file under a directory, with its bytes and modification time.
fn snapshot(dir: &Path) -> Vec<(PathBuf, Vec<u8>, std::time::SystemTime)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let modified = entry.metadata().unwrap().modified().unwrap();
                out.push((path.clone(), std::fs::read(&path).unwrap(), modified));
            }
        }
    }
    out.sort();
    out
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

/// SourceFacts fingerprint the source's own records: SHA-256 over each record's compact JSON, one
/// per line (the expected digest computed with Python's hashlib).
#[test]
fn the_source_fingerprint_is_the_digest_of_its_records() {
    let mut session = support::read_source(HarnessId::Pi);
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
