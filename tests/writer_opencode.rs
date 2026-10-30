//! OpenCode writer conformance (spec L1, L8, §2.3).

mod support;

use mukade::model::HarnessId;
use mukade::policy::Clock;
use mukade::writer::opencode::{document_for, import_command};
use serde_json::Value;
use std::path::Path;

fn document(tag: &str) -> (Value, mukade::writer::WriteOpts, std::path::PathBuf) {
    let scratch = support::scratch(HarnessId::OpenCode, tag);
    let session = support::read_source(HarnessId::OpenCode);
    let mut opts = support::opts(HarnessId::OpenCode, &scratch);
    opts.project_id = Some("project-under-test".into());
    let mut clock = Clock::new(opts.now.as_deref());
    let cwd = support::cwd_for(HarnessId::OpenCode);
    (
        document_for(&session, "ses_test", &cwd, &opts, &mut clock).expect("document"),
        opts,
        scratch,
    )
}

fn messages(document: &Value) -> Vec<&Value> {
    document["messages"]
        .as_array()
        .map(|m| m.iter().collect())
        .unwrap_or_default()
}

/// L1: without `--standalone`, OpenCode 2.1.4 hands the work to the background service of the
/// user's real home.
#[test]
fn the_import_command_always_carries_standalone() {
    let argv = import_command("opencode", Path::new(r"C:\tmp\doc.json"), r"C:\work");
    assert_eq!(argv[0], "opencode");
    assert_eq!(&argv[1..4], &["session", "import", r"C:\tmp\doc.json"]);
    assert!(argv.contains(&"--standalone".to_string()), "{argv:?}");
    assert_eq!(argv[argv.len() - 3], "--directory");
    assert_eq!(argv[argv.len() - 2], r"C:\work");
}

/// A launcher standing in for `opencode session import`: it records the store and the document it
/// was given, then runs `windows`/`unix` (a failure or a successful import answer).
fn mock_opencode(
    dir: &Path,
    capture: &Path,
    windows: &str,
    unix: &str,
) -> String {
    support::launcher(
        dir,
        "mock-opencode",
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
    let source = support::read_source(HarnessId::OpenCode);
    for (tag, windows, unix, succeeds) in [
        ("fails", "exit /b 3", "exit 3", false),
        (
            "succeeds",
            "echo Imported session: ses_mock",
            "echo 'Imported session: ses_mock'",
            true,
        ),
    ] {
        let scratch = support::scratch(HarnessId::OpenCode, &format!("opencode-import-{tag}"));
        let root = scratch.join("configured-home");
        let capture = scratch.join("environment.txt");
        let mut opts = support::opts(HarnessId::OpenCode, &scratch);
        opts.project_id = Some("project-under-test".into());
        opts.roots.opencode_db = Some(root.join("custom.db"));
        opts.exe = Some(mock_opencode(
            &scratch.join("bin"),
            &capture,
            windows,
            unix,
        ));
        let result = mukade::writer::for_harness(HarnessId::OpenCode).write(&source, &opts);
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
    let source = support::read_source(HarnessId::OpenCode);
    let temp = mukade::portable::temp_dir().unwrap();
    let name = |tag: &str| temp.join(format!("mukade-import-{tag}-{}.json", std::process::id()));
    let age = |path: &Path, seconds: u64| {
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
    let scratch = support::scratch(HarnessId::OpenCode, "opencode-import-sweep");
    let mut opts = support::opts(HarnessId::OpenCode, &scratch);
    opts.project_id = Some("project-under-test".into());
    let capture = scratch.join("environment.txt");
    opts.exe = Some(mock_opencode(
        &scratch.join("bin"),
        &capture,
        "echo Imported session: ses_mock",
        "echo 'Imported session: ses_mock'",
    ));
    mukade::writer::for_harness(HarnessId::OpenCode)
        .write(&source, &opts)
        .unwrap();
    let (stale_left, fresh_left) = (stale.exists(), fresh.exists());
    let _ = std::fs::remove_file(&stale);
    let _ = std::fs::remove_file(&fresh);
    assert!(!stale_left, "a day-old import document is swept");
    assert!(fresh_left, "a recent one is left alone");
}

/// The `document` command returns the native import document without starting a write, so a host
/// can show it (and its counts) before deciding to import.
#[test]
fn document_opencode_returns_a_native_import_document_without_starting_a_write() {
    let scratch = support::scratch(HarnessId::OpenCode, "opencode-document-command");
    let fixture = support::source_path(HarnessId::OpenCode);
    let (code, stdout, stderr) = support::cli(
        &scratch,
        &[
            "document",
            "opencode",
            "opencode",
            "fixture",
            "--from-file",
            fixture.to_str().unwrap(),
            "--cwd",
            scratch.to_str().unwrap(),
            "--dest-id",
            "ses_preview",
            "--dest-agent",
            "analyst",
            "--project-id",
            "global",
            "--model-provider",
            "h27",
            "--model",
            "m1",
            "--without-tools",
        ],
    );
    assert!(code, "{stderr}");
    let result: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(result["document"]["info"]["id"], "ses_preview");
    assert_eq!(result["document"]["info"]["agent"], "analyst");
    assert!(result["counts"]["tools_omitted"].as_u64().unwrap() > 0);
    assert!(!scratch.join("home/opencode.db").exists());
}

/// L8: one assistant message per step, holding reasoning, text and **both** tool parts, with each
/// call's result inside its own part.
#[test]
fn one_assistant_message_per_step_holds_both_calls_and_their_results() {
    let (doc, _, _) = document("writer-opencode-grouping");
    let assistant = messages(&doc)
        .into_iter()
        .find(|m| m["type"] == "assistant")
        .expect("an assistant message")
        .clone();
    let content = assistant["content"].as_array().unwrap();
    let kinds: Vec<&str> = content
        .iter()
        .map(|b| b["type"].as_str().unwrap_or(""))
        .collect();
    assert_eq!(
        kinds,
        vec!["reasoning", "text", "tool", "tool"],
        "one row per step, all its blocks"
    );

    let tools: Vec<&Value> = content.iter().filter(|b| b["type"] == "tool").collect();
    assert_eq!(tools[0]["id"], "toolu_h27call1x0");
    assert_eq!(tools[0]["name"], "read");
    assert_eq!(tools[0]["state"]["status"], "completed");
    assert!(
        tools[0]["state"]["input"]["path"]
            .as_str()
            .unwrap()
            .ends_with("alpha.txt")
    );
    assert_eq!(
        tools[0]["state"]["content"][0]["text"],
        "Read file C:\\Users\\alfio\\AppData\\Local\\shinatama-verify\\H27\\opencode\\work\\alpha.txt, lines 1-1\n1: ALPHA-CONTENT-42",
        "the result stays in the part that made the call"
    );
    assert_eq!(tools[1]["id"], "toolu_h27call1x1");
    assert!(
        assistant["content"].as_array().unwrap()[0]["native"]["signature"].as_str()
            == Some("h27-mock-signature")
    );
    assert_eq!(assistant["finish"], "tool-calls");

    // One assistant message per step: the source has five steps, but the fifth is the row the
    // killed process left behind — empty text, no call — and an empty assistant message is invalid
    // in every wire dialect, so the plan records it as dropped instead of writing it.
    assert_eq!(
        messages(&doc)
            .iter()
            .filter(|m| m["type"] == "assistant")
            .count(),
        4
    );
}

#[test]
fn the_document_states_where_the_session_belongs() {
    let (doc, opts, _) = document("writer-opencode-info");
    assert_eq!(doc["info"]["id"], "ses_test");
    assert_eq!(
        doc["info"]["location"]["directory"],
        support::cwd_for(HarnessId::OpenCode)
    );
    assert_eq!(doc["info"]["title"], opts.title.clone().unwrap());
    assert_eq!(doc["info"]["model"]["providerID"], "h27");
    assert_eq!(
        doc["info"]["projectID"], "project-under-test",
        "the destination's own project id"
    );
    assert_eq!(doc["info"]["cost"], 0);
    assert_eq!(
        doc["info"]["tokens"]["input"], 0,
        "OpenCode refuses an info without tokens"
    );
    let users: Vec<&str> = messages(&doc)
        .iter()
        .filter(|m| m["type"] == "user")
        .filter_map(|m| m["text"].as_str())
        .collect();
    assert!(
        users
            .first()
            .is_some_and(|u| u.starts_with("H27-TOOLS read")),
        "{users:?}"
    );
    assert!(
        users.iter().any(|u| u.contains("H27-SUMMARY")),
        "the source's summary travelled"
    );
}

/// OpenCode 2.1.4 refuses to import a tool part in state `error` without its `error`, a
/// `Session.StructuredError` ([V] 2026-10-28: `session import` answers `Bad Request`). OpenCode
/// itself records a failed call as `tool.execution` with the result's text as the message.
#[test]
fn a_failed_call_carries_its_error_text() {
    let scratch = support::scratch(HarnessId::OpenCode, "writer-opencode-error");
    let mut session = support::read_source(HarnessId::OpenCode);
    let failed = session
        .entries
        .iter_mut()
        .find_map(|entry| match &mut entry.kind {
            mukade::model::Kind::Results { results } => results.first_mut(),
            _ => None,
        })
        .expect("the fixture has a tool result");
    failed.is_error = true;
    failed.content = mukade::model::ResultContent::Text("ENOENT: no such file".into());
    let mut opts = support::opts(HarnessId::OpenCode, &scratch);
    opts.project_id = Some("project-under-test".into());
    let mut clock = Clock::new(opts.now.as_deref());
    let cwd = support::cwd_for(HarnessId::OpenCode);
    let document = document_for(&session, "ses_test", &cwd, &opts, &mut clock).expect("document");
    let state = messages(&document)
        .iter()
        .filter_map(|message| message["content"].as_array())
        .flatten()
        .map(|part| &part["state"])
        .find(|state| state["status"] == "error")
        .expect("the failed call is a tool part in state error");
    assert_eq!(state["error"]["type"], "tool.execution", "{state}");
    assert_eq!(state["error"]["message"], "ENOENT: no such file", "{state}");
}

/// Timestamps follow the conversation: each message after the one before it, and nothing
/// completed before it was created.
#[test]
fn document_times_never_go_backwards() {
    let (document, _, _) = document("writer-opencode-times");
    let mut last = 0;
    for message in messages(&document) {
        let created = message["time"]["created"].as_i64().unwrap();
        assert!(created > last, "{message}");
        if let Some(completed) = message["time"]["completed"].as_i64() {
            assert!(completed >= created, "{message}");
            last = completed;
        } else {
            last = created;
        }
        for part in message["content"].as_array().into_iter().flatten() {
            if let (Some(c), Some(d)) = (
                part["time"]["created"].as_i64(),
                part["time"]["completed"].as_i64(),
            ) {
                assert!(d >= c, "{part}");
            }
        }
    }
}

/// A working directory that starts with `-` would reach `opencode` as an option.
#[test]
fn a_working_directory_that_looks_like_an_option_is_refused() {
    let scratch = support::scratch(HarnessId::OpenCode, "writer-opencode-dash");
    let session = support::read_source(HarnessId::OpenCode);
    let mut opts = support::opts(HarnessId::OpenCode, &scratch);
    opts.project_id = Some("project-under-test".into());
    opts.cwd = Some("--print-logs".into());
    let error = mukade::writer::for_harness(HarnessId::OpenCode)
        .write(&session, &opts)
        .unwrap_err();
    assert!(error.to_string().contains("starts with"), "{error:#}");
}

/// The sorted keys of a JSON object.
fn keys(value: &Value) -> Vec<String> {
    let mut keys: Vec<String> = value
        .as_object()
        .map(|map| map.keys().cloned().collect())
        .unwrap_or_default();
    keys.sort();
    keys
}

/// The first value in `values` whose `type` is `kind`.
fn of_type<'a>(values: impl IntoIterator<Item = &'a Value>, kind: &str) -> &'a Value {
    values
        .into_iter()
        .find(|value| value["type"] == kind)
        .unwrap_or_else(|| panic!("no {kind} in the recorded export"))
}

/// The document is checked against OpenCode's own recorded export, not only against our own
/// reader: the keys of `info`, of each message type and of a tool part are the recorded ones, so
/// a field renamed in the writer and the reader together no longer passes.
#[test]
fn the_import_document_has_the_shape_of_opencodes_own_export() {
    let recorded: Value = serde_json::from_str(
        &std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("fixtures/opencode/source/session-export.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let (document, _, _) = document("writer-opencode-shape");
    let theirs = messages(&recorded);
    let ours = messages(&document);

    // `info`: every key the export has, except `outcome`, which OpenCode records for its own
    // sessions: real imports without it succeed (checked 2026-10-28 against OpenCode 2.1.4).
    for key in keys(&recorded["info"]) {
        if key == "outcome" {
            continue;
        }
        assert!(
            document["info"].get(&key).is_some(),
            "info.{key} is in OpenCode's export and missing from the document"
        );
    }
    for kind in ["user", "assistant"] {
        let expected = keys(of_type(theirs.iter().copied(), kind));
        let written = keys(of_type(ours.iter().copied(), kind));
        for key in &expected {
            // `tokens` is recorded by OpenCode itself and optional on import.
            if key == "tokens" {
                continue;
            }
            assert!(
                written.contains(key),
                "{kind} message: {key} missing from {written:?}"
            );
        }
    }
    let part = |messages: &[&Value]| {
        of_type(
            messages
                .iter()
                .filter_map(|m| m["content"].as_array())
                .flatten(),
            "tool",
        )
        .clone()
    };
    let (their_tool, our_tool) = (part(&theirs), part(&ours));
    for key in keys(&their_tool) {
        // `ran` is recorded by OpenCode when the tool started; it is optional on import.
        if key == "time" || key == "ran" {
            continue;
        }
        assert!(our_tool.get(&key).is_some(), "tool part: {key} missing");
    }
    assert_eq!(keys(&their_tool["state"]), keys(&our_tool["state"]));
}

/// OpenCode keeps a provider's reasoning in that provider's own shape and replays it from there.
/// Through OpenRouter (OpenCode 2.1.4's OpenAI-compatible route, seen live on 2026-10-28) that is
/// `native.reasoningDetails`, OpenRouter's `reasoning_details`; a `native.signature` is not
/// replayed. So toward the `openrouter` provider each block is written as a reasoning detail:
/// signed text as `reasoning.text` with its signature, an encrypted payload as
/// `reasoning.encrypted` with its data, the format naming the protocol that made it.
#[test]
fn reasoning_toward_openrouter_is_written_as_its_reasoning_details() {
    use mukade::model::{Entry, Kind, Opaque, Protocol, Reasoning, StepEnd};
    let scratch = support::scratch(HarnessId::OpenCode, "writer-opencode-details");
    let mut opts = support::opts(HarnessId::OpenCode, &scratch);
    opts.project_id = Some("global".into());
    opts.dest_protocol = Some(Protocol::AnthropicMessages);
    let reasoning = |text: &str, signature: Option<&str>, encrypted: Option<&str>| Reasoning {
        text: text.into(),
        opaque: Some(Opaque {
            protocol: Protocol::AnthropicMessages,
            model: Some("anthropic/claude-haiku-5.5".into()),
            signature: signature.map(str::to_owned),
            encrypted: encrypted.map(str::to_owned),
        }),
    };
    let mut session = support::read_source(HarnessId::OpenCode);
    session.entries.push(Entry {
        id: "thinking-step".into(),
        kind: Kind::Step {
            model: None,
            reasoning: vec![
                reasoning("", None, Some("REDACTED-DATA")),
                reasoning("17 times 23", Some("SIG"), None),
            ],
            text: vec!["391".into()],
            calls: vec![],
            end: StepEnd::Completed,
        },
        raw: vec![],
    });
    // The reasoning parts of the step added last (the fixture has its own before it).
    let parts_of = |doc: &Value| -> Vec<Value> {
        messages(doc)
            .into_iter()
            .rev()
            .find(|m| m["type"] == "assistant")
            .and_then(|m| m["content"].as_array().cloned())
            .unwrap_or_default()
            .into_iter()
            .filter(|p| p["type"] == "reasoning")
            .collect()
    };

    opts.model_provider = Some("openrouter".into());
    opts.model = Some("anthropic/claude-haiku-5.5".into());
    let mut clock = Clock::new(opts.now.as_deref());
    let doc = document_for(&session, "ses_t", "D:/w", &opts, &mut clock).unwrap();
    let parts = parts_of(&doc);
    assert_eq!(parts.len(), 2, "{parts:?}");
    assert_eq!(
        parts[0]["native"],
        serde_json::json!({"reasoningDetails": [{"type": "reasoning.encrypted",
            "data": "REDACTED-DATA", "format": "anthropic-claude-v1", "index": 0}]})
    );
    assert_eq!(
        parts[1]["native"],
        serde_json::json!({"reasoningDetails": [{"type": "reasoning.text",
            "text": "17 times 23", "signature": "SIG", "format": "anthropic-claude-v1", "index": 1}]})
    );

    // Toward another provider the shape stays OpenCode's own for Anthropic.
    opts.model_provider = Some("anthropic".into());
    opts.model = Some("claude-haiku-5-5".into());
    let mut clock = Clock::new(opts.now.as_deref());
    let doc = document_for(&session, "ses_t", "D:/w", &opts, &mut clock).unwrap();
    let parts = parts_of(&doc);
    assert_eq!(parts[1]["native"], serde_json::json!({"signature": "SIG"}));
}
