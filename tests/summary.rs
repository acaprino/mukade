//! A summary written by the source harness's own model: the command each harness is asked with,
//! where its reply is read, and what a failure leaves (design 2026-10-28, live record
//! `docs/verification/source-model-summary-live.md`).
//!
//! No real harness is reached: the runs go to fake launchers in a scratch folder.

mod support;

use mukade::hop::HopError;
use mukade::model::{HarnessId, Kind, ModelRef, Session};
use mukade::roots::StoreRoots;
use mukade::summary::{self, SummarizeOpts};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

/// The runs below share mukade's own temporary folder: one at a time, so each can check it left
/// nothing there.
static RUNS: Mutex<()> = Mutex::new(());

fn scratch(tag: &str) -> PathBuf {
    support::scratch(HarnessId::Pi, tag)
}

fn args_of(session: &Session, opts: &SummarizeOpts, scratch: &Path) -> Vec<String> {
    summary::summary_command(session, opts, scratch)
        .expect("a command")
        .args
}

/// The session as if every step had been answered by one model.
fn with_model(mut session: Session, provider: Option<&str>, id: &str) -> Session {
    let model = ModelRef {
        provider: provider.map(str::to_owned),
        id: id.into(),
    };
    session.meta.models = vec![model.clone()];
    set_step_models(&mut session, Some(model));
    session
}

fn set_step_models(session: &mut Session, to: Option<ModelRef>) {
    for entry in &mut session.entries {
        if let Kind::Step { model, .. } = &mut entry.kind {
            *model = to.clone();
        }
    }
}

/// A launcher that plays OpenCode: `session delete` does `on_delete`, any other call prints
/// `events` and exits with `code`.
fn fake_opencode(dir: &Path, events: &str, code: i32, on_delete: (&str, &str)) -> PathBuf {
    let file = dir.join("events.jsonl");
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(&file, events).unwrap();
    support::launcher(
        &dir.join("bin"),
        "opencode",
        &format!(
            "if \"%1\"==\"session\" (\r\n{}\r\n)\r\ntype \"{}\"\r\nexit /b {code}",
            on_delete.0,
            file.display()
        ),
        &format!(
            "if [ \"$1\" = session ]; then\n{}\nfi\ncat \"{}\"\nexit {code}",
            on_delete.1,
            file.display()
        ),
    )
}

/// The first event OpenCode prints for a fork: it names the fork before any text.
const FORK_STARTED: &str =
    r#"{"type":"step_start","timestamp":1,"sessionID":"ses_fork1","part":{"type":"step-start"}}"#;

fn leftovers() -> Vec<String> {
    let Ok(dir) = mukade::portable::temp_dir() else {
        return Vec::new();
    };
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .filter(|name| name.starts_with("summary-"))
                .collect()
        })
        .unwrap_or_default()
}

/// A `.cmd` launcher (npm installs of Pi and OpenCode) cannot take a line break, and `"%^&|<>!`
/// are not safe there: the prompt is one plain line.
#[test]
fn the_prompt_is_one_line_a_cmd_launcher_can_take() {
    let prompt = summary::SUMMARY_PROMPT;
    assert!(!prompt.starts_with('-') && !prompt.contains('\n') && !prompt.contains('\r'));
    assert!(
        prompt
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || " .,;:()".contains(c)),
        "{prompt}"
    );
}

/// The commands of the live record's "Final commands" table, the prompt last.
#[test]
fn each_harness_gets_its_non_saving_command() {
    let dir = scratch("summary-commands");
    let prompt = summary::SUMMARY_PROMPT.to_string();
    let opts = SummarizeOpts::default();

    let claude = with_model(support::read_source(HarnessId::ClaudeCode), None, "m1");
    assert_eq!(
        args_of(&claude, &opts, &dir),
        [
            "-p",
            "--tools",
            "",
            "--resume",
            claude.meta.id.as_str(),
            "--no-session-persistence",
            "--output-format",
            "json",
            "--model",
            "m1",
            prompt.as_str(),
        ]
    );

    let codex = with_model(support::read_source(HarnessId::Codex), Some("openai"), "m1");
    let command = summary::summary_command(&codex, &opts, &dir).unwrap();
    let last = dir.join("last-message.txt");
    assert_eq!(command.output_file.as_deref(), Some(last.as_path()));
    assert_eq!(
        command.args,
        [
            "exec",
            "fork",
            codex.meta.id.as_str(),
            "--ephemeral",
            "--skip-git-repo-check",
            "-c",
            "sandbox_mode=\"read-only\"",
            "-o",
            last.to_str().unwrap(),
            "-m",
            "m1",
            prompt.as_str(),
        ]
    );

    let pi = with_model(support::read_source(HarnessId::Pi), Some("anthropic"), "m1");
    let file = pi
        .meta
        .source_path
        .clone()
        .expect("the Pi reader names its file");
    let sessions = dir.join("sessions");
    assert_eq!(
        args_of(&pi, &opts, &dir),
        [
            "-p",
            "--fork",
            file.as_str(),
            "--session-dir",
            sessions.to_str().unwrap(),
            "--no-tools",
            "--provider",
            "anthropic",
            "--model",
            "m1",
            prompt.as_str(),
        ]
    );

    let opencode = with_model(
        support::read_source(HarnessId::OpenCode),
        Some("anthropic"),
        "m1",
    );
    assert_eq!(
        args_of(&opencode, &opts, &dir),
        [
            "run",
            "--standalone",
            "--session",
            opencode.meta.id.as_str(),
            "--fork",
            "--format",
            "json",
            "-m",
            "anthropic/m1",
            prompt.as_str(),
        ]
    );
}

#[test]
fn the_sessions_own_model_is_used_unless_one_is_named() {
    let dir = scratch("summary-models");
    let named = SummarizeOpts {
        model: Some("cheap".into()),
        ..SummarizeOpts::default()
    };
    let claude = with_model(support::read_source(HarnessId::ClaudeCode), None, "m1");
    assert!(
        args_of(&claude, &named, &dir)
            .windows(2)
            .any(|w| w == ["--model", "cheap"])
    );
    let pi = with_model(support::read_source(HarnessId::Pi), Some("anthropic"), "m1");
    let args = args_of(&pi, &named, &dir);
    assert!(
        args.windows(2).any(|w| w == ["--model", "cheap"]),
        "{args:?}"
    );
    assert!(!args.contains(&"--provider".to_string()), "{args:?}");

    for harness in [
        HarnessId::ClaudeCode,
        HarnessId::Codex,
        HarnessId::Pi,
        HarnessId::OpenCode,
    ] {
        let mut session = support::read_source(harness);
        session.meta.models.clear();
        set_step_models(&mut session, None);
        let args = args_of(&session, &SummarizeOpts::default(), &dir);
        assert!(
            !args
                .iter()
                .any(|a| a == "--model" || a == "-m" || a == "--provider"),
            "{harness:?}: {args:?}"
        );
    }

    // OpenCode names a model by its provider: without one, the configured default is used.
    let opencode = with_model(support::read_source(HarnessId::OpenCode), None, "m1");
    let args = args_of(&opencode, &SummarizeOpts::default(), &dir);
    assert!(!args.contains(&"-m".to_string()), "{args:?}");
}

/// A fixture, or a project deleted since: the run starts from mukade's own folder.
#[test]
fn a_session_whose_directory_is_gone_runs_from_mukades_own() {
    let dir = scratch("summary-cwd");
    let mut session = support::read_source(HarnessId::ClaudeCode);
    session.meta.cwd = Some(dir.join("gone").display().to_string());
    let command = summary::summary_command(&session, &SummarizeOpts::default(), &dir).unwrap();
    assert_eq!(command.cwd, None);
    session.meta.cwd = Some(dir.display().to_string());
    let command = summary::summary_command(&session, &SummarizeOpts::default(), &dir).unwrap();
    assert_eq!(command.cwd.as_deref(), Some(dir.as_path()));
}

#[test]
fn a_pi_session_without_its_file_is_refused() {
    let mut session = support::read_source(HarnessId::Pi);
    session.meta.source_path = None;
    let error =
        summary::summary_command(&session, &SummarizeOpts::default(), &scratch("pi-nofile"))
            .unwrap_err();
    assert!(matches!(error, HopError::Unsupported(_)), "{error:?}");
}

#[test]
fn replies_are_read_where_each_harness_puts_them() {
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures");
    let claude = std::fs::read_to_string(fixtures.join("claude_code/summary-print.json")).unwrap();
    let reply = summary::parse_reply(HarnessId::ClaudeCode, &claude, None).unwrap();
    assert!(reply.text.starts_with("Handoff Summary"), "{}", reply.text);
    let usage: serde_json::Value = serde_json::from_str(reply.usage.as_deref().unwrap()).unwrap();
    assert_eq!(usage["total_cost_usd"], 0.51535);

    let refused = r#"{"type":"result","subtype":"error_max_turns","is_error":true,"result":""}"#;
    assert!(summary::parse_reply(HarnessId::ClaudeCode, refused, None).is_err());
    let errored =
        r#"{"type":"result","subtype":"success","is_error":true,"result":"API Error: 400"}"#;
    assert!(summary::parse_reply(HarnessId::ClaudeCode, errored, None).is_err());

    let codex = summary::parse_reply(
        HarnessId::Codex,
        "codex\nRiassunto\n",
        Some("Riassunto: è pronto.\r\n"),
    )
    .unwrap();
    assert_eq!(codex.text, "Riassunto: è pronto.");

    let opencode = std::fs::read_to_string(fixtures.join("opencode/summary-run.jsonl")).unwrap();
    let reply = summary::parse_reply(HarnessId::OpenCode, &opencode, None).unwrap();
    assert!(reply.text.starts_with("## Handoff"), "{}", reply.text);
    assert_eq!(
        reply.fork.as_deref(),
        Some("ses_e7be56c69ffer6BEbBHfEuQ7mG")
    );

    let pi = summary::parse_reply(HarnessId::Pi, "  The summary.\n", None).unwrap();
    assert_eq!(pi.text, "The summary.");

    for harness in [HarnessId::Pi, HarnessId::Codex] {
        let error = summary::parse_reply(harness, " \n", Some(" \r\n")).unwrap_err();
        assert!(error.contains("no summary"), "{error}");
    }
}

#[test]
fn a_fake_pi_summarizes_and_leaves_nothing_behind() {
    let _one = RUNS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let dir = scratch("summary-fake-pi");
    let pi = support::launcher(
        &dir.join("bin"),
        "pi",
        "echo The summary.",
        "echo The summary.",
    );
    let session = support::read_source(HarnessId::Pi);
    let before = std::fs::read(support::source_path(HarnessId::Pi)).unwrap();
    let opts = SummarizeOpts {
        exe: Some(pi.display().to_string()),
        ..SummarizeOpts::default()
    };
    let summary = summary::summarize(&session, &StoreRoots::default(), &opts).unwrap();
    assert_eq!(summary.text, "The summary.");
    assert_eq!(summary.harness, HarnessId::Pi);
    assert_eq!(summary.leftover, None);
    assert!(leftovers().is_empty(), "{:?}", leftovers());
    assert_eq!(
        std::fs::read(support::source_path(HarnessId::Pi)).unwrap(),
        before
    );
}

#[test]
fn a_summary_that_fails_writes_nothing_and_says_why() {
    let _one = RUNS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let dir = scratch("summary-fails");
    let session = support::read_source(HarnessId::Pi);
    let run = |exe: &Path, timeout: Duration| {
        summary::summarize(
            &session,
            &StoreRoots::default(),
            &SummarizeOpts {
                exe: Some(exe.display().to_string()),
                timeout,
                ..SummarizeOpts::default()
            },
        )
        .unwrap_err()
    };

    let exits = support::launcher(&dir.join("exits"), "pi", "exit /b 3", "exit 3");
    let error = run(&exits, Duration::from_secs(30));
    assert_eq!(error.exit_code(), 4);
    let HopError::SummaryFailed(reason) = &error else {
        panic!("{error:?}")
    };
    assert!(
        reason.contains("<summary prompt v1>") && reason.contains('3'),
        "{reason}"
    );
    assert!(!reason.contains(summary::SUMMARY_PROMPT), "{reason}");

    let silent = support::launcher(&dir.join("silent"), "pi", "rem", ":");
    let error = run(&silent, Duration::from_secs(30));
    assert!(error.to_string().contains("no summary"), "{error}");

    let slow = support::launcher(
        &dir.join("slow"),
        "pi",
        "ping -n 4 127.0.0.1 >nul",
        "sleep 3",
    );
    let error = run(&slow, Duration::from_millis(500));
    assert!(
        error.to_string().contains("did not finish within"),
        "{error}"
    );
    assert!(leftovers().is_empty(), "{:?}", leftovers());
}

/// The fork OpenCode makes for the summary is deleted; when it cannot be, the hop goes on and says
/// how to remove it.
#[test]
fn an_opencode_fork_that_cannot_be_deleted_is_named() {
    let _one = RUNS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let dir = scratch("summary-opencode-fork");
    let events = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join("opencode")
        .join("summary-run.jsonl");
    let opencode = support::launcher(
        &dir.join("bin"),
        "opencode",
        &format!(
            "if \"%1\"==\"session\" exit /b 1\r\ntype \"{}\"",
            events.display()
        ),
        &format!(
            "[ \"$1\" = session ] && exit 1\ncat \"{}\"",
            events.display()
        ),
    );
    let session = support::read_source(HarnessId::OpenCode);
    let summary = summary::summarize(
        &session,
        &StoreRoots::default(),
        &SummarizeOpts {
            exe: Some(opencode.display().to_string()),
            ..SummarizeOpts::default()
        },
    )
    .unwrap();
    assert!(summary.text.starts_with("## Handoff"), "{}", summary.text);
    let leftover = summary.leftover.expect("the fork that stayed is named");
    assert!(
        leftover.ends_with("session delete ses_e7be56c69ffer6BEbBHfEuQ7mG --standalone"),
        "{leftover}"
    );
}

/// A Claude session that was killed or hit an API error records `<synthetic>` replies: that is no
/// model to ask, and the session's own model is the last real one that answered.
#[test]
fn a_synthetic_reply_is_not_a_model_to_ask() {
    let dir = scratch("summary-synthetic");
    let mut session = with_model(
        support::read_source(HarnessId::ClaudeCode),
        None,
        "claude-sonnet-5",
    );
    let last = session
        .entries
        .iter()
        .rposition(|entry| matches!(entry.kind, Kind::Step { .. }))
        .expect("a step");
    if let Kind::Step { model, .. } = &mut session.entries[last].kind {
        *model = Some(ModelRef {
            provider: None,
            id: "<synthetic>".into(),
        });
    }
    session.meta.models.push(ModelRef {
        provider: None,
        id: "<synthetic>".into(),
    });
    let args = args_of(&session, &SummarizeOpts::default(), &dir);
    assert!(
        args.windows(2).any(|w| w == ["--model", "claude-sonnet-5"]),
        "{args:?}"
    );
}

/// A subagent's model is recorded in the session's models, but the steps of the branch the
/// harness resumes say which model the conversation was having.
#[test]
fn the_model_of_the_conversation_wins_over_a_subagents() {
    let dir = scratch("summary-subagent-model");
    let mut session = with_model(
        support::read_source(HarnessId::ClaudeCode),
        None,
        "main-model",
    );
    session.meta.models.push(ModelRef {
        provider: None,
        id: "subagent-model".into(),
    });
    let args = args_of(&session, &SummarizeOpts::default(), &dir);
    assert!(
        args.windows(2).any(|w| w == ["--model", "main-model"]),
        "{args:?}"
    );
}

/// OpenCode's `run` has no switch for tools: every permission is denied through its inline
/// configuration, so the summarizing run cannot edit a file or run a command.
#[test]
fn opencode_summarizes_with_every_tool_denied() {
    let session = support::read_source(HarnessId::OpenCode);
    let command =
        summary::summary_command(&session, &SummarizeOpts::default(), &scratch("oc-deny")).unwrap();
    assert!(
        command
            .env
            .iter()
            .any(|(key, value)| key == "OPENCODE_CONFIG_CONTENT"
                && value == r#"{"permission":{"*":"deny"}}"#),
        "{:?}",
        command.env
    );
}

/// A run that worked instead of summarizing, or that broke off with an error, is no summary.
#[test]
fn an_opencode_run_that_used_a_tool_or_failed_is_no_summary() {
    let tool = [
        FORK_STARTED,
        r#"{"type":"tool_use","sessionID":"ses_fork1","part":{"type":"tool","tool":"read"}}"#,
        r#"{"type":"text","sessionID":"ses_fork1","part":{"type":"text","text":"Now I will fix it."}}"#,
    ]
    .join("\n");
    let error = summary::parse_reply(HarnessId::OpenCode, &tool, None).unwrap_err();
    assert!(error.contains("tool"), "{error}");
    let broken = [
        FORK_STARTED,
        r#"{"type":"text","sessionID":"ses_fork1","part":{"type":"text","text":"Handoff, half"}}"#,
        r#"{"type":"error","sessionID":"ses_fork1","error":{"name":"APIError","data":{"message":"context too long"}}}"#,
    ]
    .join("\n");
    let error = summary::parse_reply(HarnessId::OpenCode, &broken, None).unwrap_err();
    assert!(error.contains("context too long"), "{error}");
}

/// The fork exists as soon as OpenCode names it: it is deleted when the run then fails too.
#[test]
fn an_opencode_fork_is_deleted_when_the_run_fails() {
    let _one = RUNS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let dir = scratch("summary-oc-fails-deleted");
    let marker = dir.join("deleted.txt");
    let opencode = fake_opencode(
        &dir,
        FORK_STARTED,
        0,
        (
            &format!("echo deleted>\"{}\"\r\nexit /b 0", marker.display()),
            &format!("echo deleted > \"{}\"\nexit 0", marker.display()),
        ),
    );
    let error = summary::summarize(
        &support::read_source(HarnessId::OpenCode),
        &StoreRoots::default(),
        &SummarizeOpts {
            exe: Some(opencode.display().to_string()),
            ..SummarizeOpts::default()
        },
    )
    .unwrap_err();
    assert!(matches!(error, HopError::SummaryFailed(_)), "{error:?}");
    assert!(marker.exists(), "the fork was not deleted");
}

/// A failed run whose fork could not be deleted says how to remove it.
#[test]
fn a_failed_run_names_the_fork_it_left_behind() {
    let _one = RUNS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let dir = scratch("summary-oc-fails-left");
    let opencode = fake_opencode(&dir, FORK_STARTED, 1, ("exit /b 1", "exit 1"));
    let error = summary::summarize(
        &support::read_source(HarnessId::OpenCode),
        &StoreRoots::default(),
        &SummarizeOpts {
            exe: Some(opencode.display().to_string()),
            ..SummarizeOpts::default()
        },
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("session delete ses_fork1 --standalone"),
        "{error}"
    );
}
