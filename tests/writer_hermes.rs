//! Hermes Agent writer: a session reaches Hermes through its own `sessions import`.
//!
//! No real Hermes is started here: a launcher stands in for `hermes`, copies the Claude Code file
//! it is handed, and answers like a successful import. The file is what the import reads, so these
//! tests are about it: what Hermes keeps of a Claude session, and what mukade must therefore have
//! put there. The real import is `tests/hermes_live.rs` (ignored).

mod support;

use mukade::harnesses;
use mukade::model::{HarnessId, Kind, Session};
use mukade::writer::{WriteOpts, WriteOutcomeUnknown, for_harness};
use serde_json::Value;
use std::path::{Path, PathBuf};

const HERMES: HarnessId = HarnessId::Hermes;

/// What the stand-in `hermes` recorded.
struct Run {
    /// The Claude Code file, copied before the import returned.
    rows: Vec<Value>,
    /// The path the import was handed.
    handed: PathBuf,
    /// `HERMES_HOME` as the process saw it.
    home: String,
    /// The arguments.
    args: Vec<String>,
}

/// A stand-in for `hermes sessions import` that records its arguments, its environment and the
/// file, and then answers `reply` with `code`.
fn stand_in(dir: &Path, code: i32, reply: &str) -> PathBuf {
    let capture = dir.join("capture");
    let win = format!(
        "echo %HERMES_HOME%>\"{c}.home\"\r\necho %1 %2 %3 %4>\"{c}.args\"\r\necho %5>\"{c}.handed\"\r\n\
         copy /Y %5 \"{c}.jsonl\" >nul\r\n{reply_win}exit /b {code}",
        c = capture.display(),
        reply_win = if reply.is_empty() { String::new() } else { format!("echo {reply}\r\n") },
    );
    let unix = format!(
        "printf '%s' \"$HERMES_HOME\" > '{c}.home'\necho \"$1 $2 $3 $4\" > '{c}.args'\necho \"$5\" > '{c}.handed'\n\
         cp \"$5\" '{c}.jsonl'\n{reply_unix}exit {code}",
        c = capture.display(),
        reply_unix = if reply.is_empty() { String::new() } else { format!("echo '{reply}'\n") },
    );
    support::launcher(dir, "hermes", &win, &unix)
}

fn observed(dir: &Path) -> Run {
    let capture = dir.join("capture");
    let text = |suffix: &str| {
        std::fs::read_to_string(format!("{}.{suffix}", capture.display()))
            .unwrap_or_else(|error| panic!("{suffix}: {error}"))
            .trim()
            .to_string()
    };
    Run {
        rows: text("jsonl")
            .lines()
            .map(|line| serde_json::from_str(line).expect("each row is JSON"))
            .collect(),
        handed: PathBuf::from(text("handed")),
        home: text("home"),
        args: text("args").split_whitespace().map(str::to_string).collect(),
    }
}

/// The texts of a row's message, whether its content is a string or blocks.
fn texts_of(row: &Value) -> Vec<String> {
    match &row["message"]["content"] {
        Value::String(text) => vec![text.clone()],
        Value::Array(blocks) => blocks
            .iter()
            .filter_map(|b| b["text"].as_str().map(str::to_string))
            .collect(),
        _ => Vec::new(),
    }
}

fn opts(scratch: &Path, exe: &Path) -> WriteOpts {
    WriteOpts {
        exe: Some(exe.display().to_string()),
        dest_protocol: None,
        ..support::opts(HERMES, scratch)
    }
}

fn run(session: &Session, tag: &str, code: i32, reply: &str) -> (anyhow::Result<mukade::writer::Written>, Run, WriteOpts) {
    let scratch = support::scratch(HERMES, tag);
    let exe = stand_in(&scratch.join("bin"), code, reply);
    let opts = opts(&scratch, &exe);
    let written = for_harness(HERMES).write(session, &opts);
    let seen = observed(&scratch.join("bin"));
    (written, seen, opts)
}

const CONFIRMED: &str = "Imported Claude Code session as 20261112_000000_abc123";

// ---------------------------------------------------------------------------------------------

#[test]
fn the_import_is_asked_for_a_claude_session_by_its_path_and_the_id_comes_from_its_answer() {
    let source = support::read_source(HarnessId::ClaudeCode);
    let (written, seen, opts) = run(&source, "hermes-write", 0, CONFIRMED);
    let written = written.expect("the write");
    assert_eq!(written.id, "20261112_000000_abc123");
    assert_eq!(
        PathBuf::from(&written.location),
        opts.roots.hermes_home.clone().unwrap().join("state.db")
    );
    assert_eq!(seen.args, ["sessions", "import", "--from", "claude"]);
    assert_eq!(PathBuf::from(&seen.home), opts.roots.hermes_home.clone().unwrap());
    // The file is mukade's own and is gone when the write is.
    assert!(!seen.handed.exists(), "{}", seen.handed.display());
    assert_eq!(
        written.resume,
        [
            opts.exe.clone().unwrap().as_str(),
            "chat",
            "-Q",
            "--resume",
            "20261112_000000_abc123",
            "-q"
        ]
    );
}

#[test]
fn every_tool_call_reaches_the_import_in_text_form_with_its_result() {
    let source = support::read_source(HarnessId::ClaudeCode);
    let (written, seen, _) = run(&source, "hermes-text", 0, CONFIRMED);
    written.expect("the write");
    let mut calls = 0;
    for row in &seen.rows {
        assert!(matches!(row["type"].as_str(), Some("user" | "assistant")), "{row}");
        for block in row["message"]["content"].as_array().into_iter().flatten() {
            // Hermes keeps a `tool_use` as `[ran tool: x]` and a `tool_result` as nothing.
            assert_eq!(block["type"], "text", "{block}");
        }
        calls += texts_of(row)
            .iter()
            .filter(|t| t.starts_with("[tool call · "))
            .count();
    }
    let source_calls: usize = source
        .conversation()
        .map(|e| match &e.kind {
            Kind::Step { calls, .. } => calls.len(),
            _ => 0,
        })
        .sum();
    assert!(source_calls >= 2);
    assert_eq!(calls, source_calls, "one text-form call for each call");
    // The results the import would drop travel as user text.
    let texts: Vec<String> = seen
        .rows
        .iter()
        .filter(|r| r["type"] == "user")
        .flat_map(texts_of)
        .collect();
    assert!(texts.iter().any(|t| t.starts_with("[tool result · id ")), "{texts:#?}");
    // And no text of a user turn opens with a context tag Hermes would drop.
    assert!(texts.iter().all(|t| !t.trim_start().starts_with("<system-reminder")));
}

#[test]
fn a_folder_that_is_here_is_named_and_one_that_is_not_names_none() {
    let source = support::read_source(HarnessId::ClaudeCode);
    let scratch = support::scratch(HERMES, "hermes-cwd-exists");
    let existing = scratch.join("project");
    std::fs::create_dir_all(&existing).unwrap();
    let exe = stand_in(&scratch.join("bin"), 0, CONFIRMED);
    let mut with = opts(&scratch, &exe);
    with.cwd = Some(existing.display().to_string());
    for_harness(HERMES).write(&source, &with).expect("write");
    let seen = observed(&scratch.join("bin"));
    assert!(seen.rows.iter().all(|r| r["cwd"] == existing.display().to_string()));

    let scratch = support::scratch(HERMES, "hermes-cwd-missing");
    let exe = stand_in(&scratch.join("bin"), 0, CONFIRMED);
    let mut without = opts(&scratch, &exe);
    without.cwd = Some(scratch.join("not-here").display().to_string());
    for_harness(HERMES).write(&source, &without).expect("write");
    let seen = observed(&scratch.join("bin"));
    let named = seen.rows[0]["cwd"].as_str().unwrap();
    assert!(!Path::new(named).exists(), "Hermes keeps a folder only when it exists: {named}");
    assert!(
        seen.rows.iter().all(|r| r["cwd"] == named),
        "every row names the same folder"
    );
}

#[test]
fn an_import_that_failed_or_did_not_confirm_may_have_written_and_says_so() {
    let source = support::read_source(HarnessId::ClaudeCode);
    let (failed, seen, opts) = run(&source, "hermes-failed", 3, "");
    let error = failed.unwrap_err();
    let unknown = error.downcast_ref::<WriteOutcomeUnknown>().expect("an unknown outcome");
    assert!(unknown.reason.contains("failed"), "{}", unknown.reason);
    assert_eq!(
        unknown.location.as_deref(),
        Some(opts.roots.hermes_home.clone().unwrap().join("state.db").to_str().unwrap())
    );
    assert!(!seen.handed.exists(), "the temporary file goes whatever came of the import");

    let (silent, seen, _) = run(&source, "hermes-silent", 0, "all done");
    let error = silent.unwrap_err();
    let unknown = error.downcast_ref::<WriteOutcomeUnknown>().expect("an unknown outcome");
    assert!(unknown.reason.contains("did not confirm"), "{}", unknown.reason);
    assert!(!seen.handed.exists());
}

#[test]
fn what_hermes_cannot_take_is_refused_before_anything_runs() {
    let source = support::read_source(HarnessId::ClaudeCode);
    let scratch = support::scratch(HERMES, "hermes-refusals");
    let exe = stand_in(&scratch.join("bin"), 0, CONFIRMED);
    let base = opts(&scratch, &exe);
    let refused = |change: &dyn Fn(&mut WriteOpts), expect: &str| {
        let mut opts = base.clone();
        change(&mut opts);
        let error = format!("{:#}", for_harness(HERMES).write(&source, &opts).unwrap_err());
        assert!(error.contains(expect), "{error}");
    };
    refused(&|o| o.dest_id = Some("20261112_000000_abc123".into()), "--dest-id");
    refused(&|o| o.dest_path = Some(base.roots.hermes_home.clone().unwrap().join("x")), "--dest-path");
    refused(&|o| o.dest_agent = Some("a".into()), "--dest-agent");
    refused(&|o| o.metadata = Some(serde_json::json!({})), "--metadata-file");
    refused(
        &|o| {
            o.native_budget = Some(mukade::native_compaction::NativeBudget {
                context: 100_000,
                max_output_tokens: None,
                text_token_factor: None,
            })
        },
        "--compaction native",
    );
    refused(&|o| o.roots.hermes_home = None, "HERMES_HOME");
    assert!(
        !scratch.join("bin").join("capture.args").exists(),
        "the stand-in was never started"
    );
}

#[test]
fn the_hop_counts_every_call_as_text_form_and_passes_the_strict_gate() {
    for source_id in [HarnessId::ClaudeCode, HarnessId::Codex, HarnessId::Pi, HarnessId::DeepSeekHarness] {
        let source = support::read_source(source_id);
        let scratch = support::scratch(HERMES, &format!("hermes-hop-{}", source_id.as_str()));
        let exe = stand_in(&scratch.join("bin"), 0, CONFIRMED);
        let write_opts = opts(&scratch, &exe);
        let receipt = mukade::hop::import(HERMES, &source, &support::import_opts(&write_opts, &source, None))
            .unwrap_or_else(|error| panic!("{source_id}: {error:?}"));
        assert_eq!(receipt.dest_id, "20261112_000000_abc123", "{source_id}");
        let counts = &receipt.manifest["counts"];
        assert_eq!(receipt.manifest["strict"]["checked"], true, "{source_id}");
        // `tool_calls` counts the calls written as calls: none. Every call is text form.
        assert_eq!(counts["tool_calls"], 0, "{source_id}: {counts}");
        assert!(counts["tools_text_form"].as_u64().unwrap() >= 1, "{source_id}: {counts}");
        for repair in ["dropped_orphan_results", "dropped_duplicate_results", "unanswered_calls"] {
            assert_eq!(counts[repair], 0, "{source_id}: {repair}");
        }
        assert_eq!(receipt.manifest["dest"]["harness"], "hermes", "{source_id}");
        // The prompts of the source are the user turns the import is handed.
        let seen = observed(&scratch.join("bin"));
        let handed: String = seen
            .rows
            .iter()
            .filter(|r| r["type"] == "user")
            .flat_map(texts_of)
            .collect::<Vec<_>>()
            .join("\n");
        let first = support::user_texts(&source).into_iter().next().unwrap_or_default();
        assert!(handed.contains(first.trim()), "{source_id}: {first:?} not in {handed:?}");
    }
}

#[test]
fn hermes_is_a_destination_that_offers_no_tool() {
    let destination = harnesses::get(HERMES).destination().expect("a destination now");
    assert!(destination.offered().names.is_empty());
    assert_eq!(destination.protocol(), None);
    let call = mukade::model::ToolCall {
        id: "c".into(),
        source_name: "Read".into(),
        canonical: Some(mukade::model::CanonicalTool::Read),
        input: serde_json::json!({"file_path": "a"}),
    };
    assert!(matches!(
        mukade::tools::translate(&call, HERMES, None),
        mukade::tools::Translation::Text { .. }
    ));
}
