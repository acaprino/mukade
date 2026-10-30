//! Shared scaffolding for the conformance tests.
//!
//! Two rules hold for every test here: the source session is the fixture under `fixtures/`, and
//! everything written lands in a scratch directory under Cargo's target tmp — never a real store,
//! the user's profile or the system's temporary folder.
#![allow(dead_code)]

use mukade::hop::{ImportOpts, ResumePrompt, SourceFacts, WriteDetails};
use mukade::model::{HarnessId, Kind, Protocol, Session};
use mukade::policy::ReasoningPolicy;
use mukade::reader::{claude_code, codex, opencode, pi};
use mukade::roots::StoreRoots;
use mukade::tools::Shell;
use mukade::writer::WriteOpts;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The binary under test, built by Cargo for the integration tests.
pub const BIN: &str = env!("CARGO_BIN_EXE_mukade");

/// A fixed instant, so a written session is reproducible.
pub const NOW: &str = "2026-10-27T16:00:00.000Z";

const FIXTURES: &str = "fixtures";

/// The native source session each harness produced, recorded as a fixture.
pub fn source_path(harness: HarnessId) -> PathBuf {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(FIXTURES);
    match harness {
        HarnessId::ClaudeCode => {
            base.join("claude_code/source/c45db7bc-542c-4201-89de-b590c6fdb797.jsonl")
        }
        HarnessId::Codex => base.join(
            "codex/source/rollout-2026-10-27T16-43-08-01a17e07-93f7-7a73-85c7-9f0205836600.jsonl",
        ),
        HarnessId::OpenCode => base.join("opencode/source/rows.json"),
        HarnessId::Pi => base
            .join("pi/source/2026-10-27T16-00-39-689Z_01a17e17-9f07-743d-8072-8b3b13d65ab5.jsonl"),
    }
}

/// The id the fixture session was recorded under; the CLI accepts a unique prefix of it.
pub fn source_id(harness: HarnessId) -> &'static str {
    match harness {
        HarnessId::ClaudeCode => "c45db7bc-542c-4201-89de-b590c6fdb797",
        HarnessId::Codex => "01a17e07-93f7-7a73-85c7-9f0205836600",
        HarnessId::OpenCode => "ses_e82023514ffe02y4T8OoUu99VZ",
        HarnessId::Pi => "01a17e17-9f07-743d-8072-8b3b13d65ab5",
    }
}

/// Read a harness's own source session with our reader.
pub fn read_source(harness: HarnessId) -> Session {
    let path = source_path(harness);
    match harness {
        HarnessId::ClaudeCode => claude_code::read_file(&path),
        HarnessId::Codex => codex::read_file(&path),
        HarnessId::OpenCode => opencode::read_file(&path),
        HarnessId::Pi => pi::read_file(&path),
    }
    .expect("the fixture must read")
}

/// A fresh scratch directory for one test, under the harness's own scratch area. The process id
/// and the counter make every call unique: two tests that ask for the same tag (a shared helper,
/// say), or two test runs at once, must not delete each other's store while the other is still
/// running. Scratch folders a day old are left from earlier runs and are removed.
pub fn scratch(harness: HarnessId, tag: &str) -> PathBuf {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    static SWEPT: std::sync::Once = std::sync::Once::new();
    let unique = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    // Inside the build folder (cargo's own scratch area for integration tests): running the tests
    // leaves nothing in the user's profile or the system's temporary folder.
    let override_root = std::env::var_os("MUKADE_TEST_SCRATCH_ROOT");
    let area = override_root
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("mukade-tests"));
    // An explicitly supplied root belongs to the caller: never sweep its pre-existing content.
    if override_root.is_none() {
        SWEPT.call_once(|| sweep(&area));
    }
    let base = area
        .join(harness.as_str())
        .join(format!("{tag}-{}-{unique}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(base.join("home")).expect("create the scratch home");
    base
}

/// Remove the scratch folders of earlier runs, a day old or more.
fn sweep(area: &Path) {
    let day = std::time::Duration::from_secs(24 * 60 * 60);
    for harness in std::fs::read_dir(area).into_iter().flatten().flatten() {
        for run in std::fs::read_dir(harness.path())
            .into_iter()
            .flatten()
            .flatten()
        {
            let old = run
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|modified| modified.elapsed().ok())
                .is_some_and(|age| age > day);
            if old {
                let _ = std::fs::remove_dir_all(run.path());
            }
        }
    }
}

/// The store roots a test may write to: every harness rooted inside the scratch home.
pub fn roots(scratch: &Path) -> StoreRoots {
    let home = scratch.join("home");
    StoreRoots {
        claude_config_dir: Some(home.clone()),
        codex_home: Some(home.clone()),
        opencode_db: Some(home.join("opencode.db")),
        pi_agent_dir: Some(home),
    }
}

/// Copy a harness's fixture into the store layout its reader expects, and answer the id the CLI
/// resolves it by (the full id, or the unique prefix the tests prefer).
pub fn stage_fixture(harness: HarnessId, scratch: &Path) -> String {
    let home = scratch.join("home");
    let fixture = source_path(harness);
    let file = match harness {
        HarnessId::ClaudeCode => home
            .join("projects")
            .join("claude-project")
            .join("c45db7bc-542c-4201-89de-b590c6fdb797.jsonl"),
        HarnessId::Codex => home
            .join("sessions")
            .join("2026")
            .join("10")
            .join("27")
            .join("rollout-2026-10-27T16-43-08-01a17e07-93f7-7a73-85c7-9f0205836600.jsonl"),
        HarnessId::Pi => home
            .join("sessions")
            .join("--pi-project--")
            .join("2026-10-27T16-00-39-689Z_01a17e17-9f07-743d-8072-8b3b13d65ab5.jsonl"),
        // OpenCode is read from its export document, which the tests pass with `--from-file`.
        HarnessId::OpenCode => return source_id(harness).to_string(),
    };
    std::fs::create_dir_all(file.parent().expect("a store directory")).expect("create the store");
    std::fs::copy(&fixture, &file).expect("stage the fixture");
    match harness {
        HarnessId::OpenCode => unreachable!(),
        HarnessId::ClaudeCode | HarnessId::Codex | HarnessId::Pi => {
            source_id(harness)[..8].to_string()
        }
    }
}

/// The destination's own facts as a write needs them, copied from the options the test stated.
pub fn details(opts: &WriteOpts) -> WriteDetails {
    WriteDetails {
        dest_version: opts.dest_version.clone(),
        dest_protocol: opts.dest_protocol.clone(),
        dest_shell: opts.dest_shell,
        offered: opts.offered,
        model_provider: opts.model_provider.clone(),
        model: opts.model.clone(),
        project_id: opts.project_id.clone(),
        dest_id: opts.dest_id.clone(),
        dest_path: opts.dest_path.clone(),
        dest_agent: opts.dest_agent.clone(),
        metadata: opts.metadata.clone(),
        tool_map: opts.tool_map.clone(),
        without_tools: opts.without_tools,
        now: opts.now.clone(),
        exe: opts.exe.clone(),
    }
}

/// The import options for writing `source` with `write_opts`, with an optional resume prompt.
pub fn import_opts(
    write_opts: &WriteOpts,
    source: &Session,
    resume: Option<ResumePrompt>,
) -> ImportOpts {
    ImportOpts {
        cwd: write_opts.cwd.clone(),
        roots: write_opts.roots.clone(),
        reasoning: write_opts.reasoning,
        write: details(write_opts),
        resume,
        source_facts: SourceFacts::of(source),
        compaction: "none".into(),
        summary: None,
    }
}

/// Read a session a writer just wrote, with the reader its destination provides.
pub fn read_destination(harness: HarnessId, location: &str) -> Session {
    let path = Path::new(location);
    match harness {
        HarnessId::ClaudeCode => claude_code::read_file(path),
        HarnessId::Codex => codex::read_file(path),
        HarnessId::OpenCode => opencode::read_file(path),
        HarnessId::Pi => pi::read_file(path),
    }
    .expect("the destination session must read back")
}

/// The name a kind is asserted by, the same in every reader's conformance test.
pub fn kind_name(kind: &Kind) -> &'static str {
    match kind {
        Kind::User { .. } => "user",
        Kind::Step { .. } => "step",
        Kind::Results { .. } => "results",
        Kind::Compaction { .. } => "compaction",
        Kind::Meta { .. } => "meta",
    }
}

/// Write the harness's own fixture into a fresh scratch store, and answer the write and the rows
/// the file holds: what every writer's conformance test starts from.
pub fn write_fixture(
    harness: HarnessId,
    tag: &str,
) -> (mukade::writer::Written, Vec<serde_json::Value>) {
    let scratch = scratch(harness, tag);
    let session = read_source(harness);
    let opts = opts(harness, &scratch);
    let written = mukade::writer::for_harness(harness)
        .write(&session, &opts)
        .expect("write");
    let rows = read_jsonl(Path::new(&written.location));
    (written, rows)
}

/// The user turns of a session, in order.
pub fn user_texts(session: &Session) -> Vec<String> {
    session
        .conversation()
        .filter_map(|entry| match &entry.kind {
            Kind::User { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

/// Every raw record of a written session, as text: for assertions about what the file itself says.
pub fn written_text(session: &Session) -> String {
    session
        .entries
        .iter()
        .flat_map(|entry| entry.raw.clone())
        .map(|record| record.to_string())
        .collect::<Vec<String>>()
        .join("\n")
}

/// Every file under a directory, with its bytes and modification time, in path order.
pub fn snapshot(dir: &Path) -> Vec<(PathBuf, Vec<u8>, std::time::SystemTime)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let modified = entry.metadata().expect("metadata").modified().expect("mtime");
                out.push((path.clone(), std::fs::read(&path).expect("read"), modified));
            }
        }
    }
    out.sort();
    out
}

/// Every file under a directory, in path order.
pub fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// Run the real binary with every harness root pointed at a scratch home.
pub fn cli(scratch: &Path, args: &[&str]) -> (bool, String, String) {
    let (code, stdout, stderr) = cli_code(scratch, args);
    (code == Some(0), stdout, stderr)
}

/// [`cli`], keeping the exit code and allowing environment overrides applied after the isolation.
pub fn cli_code(scratch: &Path, args: &[&str]) -> (Option<i32>, String, String) {
    cli_in(scratch, scratch, args, &[])
}

/// [`cli_code`] run from `cwd`, with `overrides` applied last so a test can point one variable at
/// its own store.
pub fn cli_in(
    cwd: &Path,
    scratch: &Path,
    args: &[&str],
    overrides: &[(&str, &str)],
) -> (Option<i32>, String, String) {
    let home = scratch.join("home");
    // No real harness can be reached: PATH holds an empty directory, and the homes the
    // well-known install locations hang from point inside the scratch.
    let isolated = scratch.join("isolated");
    std::fs::create_dir_all(isolated.join("bin")).expect("create the isolated bin");
    let mut command = Command::new(BIN);
    command
        .args(args)
        .current_dir(cwd)
        .env("PATH", isolated.join("bin"))
        .env("APPDATA", &isolated)
        .env("LOCALAPPDATA", &isolated)
        .env("USERPROFILE", &isolated)
        .env("HOME", &isolated)
        .env_remove("OPENCODE_CONFIG")
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("XDG_DATA_HOME")
        .env_remove("XDG_STATE_HOME")
        .env_remove("XDG_CACHE_HOME")
        .env_remove("MUKADE_DATA_DIR")
        .env("CLAUDE_CONFIG_DIR", &home)
        .env("CODEX_HOME", &home)
        .env("PI_CODING_AGENT_DIR", &home)
        .env("OPENCODE_DB", home.join("opencode.db"))
        // The developer's own overrides must not leak into the assertions.
        .env_remove("MUKADE_CLAUDE_EXE")
        .env_remove("MUKADE_CODEX_EXE")
        .env_remove("MUKADE_OPENCODE_EXE")
        .env_remove("MUKADE_PI_EXE");
    for (name, value) in overrides {
        command.env(name, value);
    }
    let output = command.output().expect("the binary must run");
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// A launcher standing in for a harness binary: `<name>.cmd` on Windows, an executable shell
/// script `<name>` elsewhere. Like the real ones, it receives the resume prompt as its last
/// argument.
pub fn launcher(dir: &Path, name: &str, windows: &str, unix: &str) -> PathBuf {
    std::fs::create_dir_all(dir).expect("create the launcher directory");
    if cfg!(windows) {
        let path = dir.join(format!("{name}.cmd"));
        std::fs::write(&path, format!("@echo off\r\n{windows}\r\n")).expect("write the launcher");
        path
    } else {
        let path = dir.join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{unix}\n")).expect("write the launcher");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
                .expect("make the launcher executable");
        }
        path
    }
}

/// The working directory a hop files the session under: the destination's own project folder.
pub fn cwd_for(harness: HarnessId) -> String {
    std::env::temp_dir()
        .join(format!("mukade-{}-work", harness.as_str()))
        .display()
        .to_string()
}

/// Write options aimed at a scratch root, with the destination's own facts stated.
pub fn opts(harness: HarnessId, scratch: &Path) -> WriteOpts {
    let home = scratch.join("home");
    let roots = StoreRoots {
        claude_config_dir: Some(home.clone()),
        codex_home: Some(home.clone()),
        opencode_db: Some(home.join("opencode.db")),
        pi_agent_dir: Some(home),
    };
    WriteOpts {
        roots,
        cwd: Some(cwd_for(harness)),
        dest_version: Some(version(harness).to_string()),
        dest_protocol: Some(protocol(harness)),
        dest_shell: Some(shell(harness)),
        offered: None,
        model_provider: Some("h27".to_string()),
        model: None,
        project_id: None,
        dest_id: None,
        dest_path: None,
        dest_agent: None,
        metadata: None,
        tool_map: Vec::new(),
        without_tools: false,
        reasoning: ReasoningPolicy::DropForeign,
        title: Some("interchange test".to_string()),
        now: Some(NOW.to_string()),
        exe: Some(mukade::exe::binary_name(harness).to_string()),
    }
}

fn version(harness: HarnessId) -> &'static str {
    match harness {
        HarnessId::ClaudeCode => "2.1.311",
        HarnessId::Codex => "0.164.0",
        HarnessId::OpenCode => "2.1.4",
        HarnessId::Pi => "1.2.0",
    }
}

/// The protocol the destination speaks; the bench's OpenCode and Pi both talked to an Anthropic
/// provider, which is what their own stores record.
pub fn protocol(harness: HarnessId) -> Protocol {
    match harness {
        HarnessId::ClaudeCode | HarnessId::OpenCode | HarnessId::Pi => Protocol::AnthropicMessages,
        HarnessId::Codex => Protocol::OpenaiResponses,
    }
}

fn shell(harness: HarnessId) -> Shell {
    match harness {
        HarnessId::Codex => Shell::PowerShell,
        _ => Shell::Posix,
    }
}

/// The conversation as a script, so two sessions can be compared independently of the metadata
/// each harness writes around it. Inputs are left out on purpose: a hop may rename arguments and
/// convert units, and that is not a loss.
pub fn script(session: &Session) -> Vec<String> {
    session
        .conversation()
        .map(|e| match &e.kind {
            Kind::User { text, .. } => format!("user|{text}"),
            Kind::Step {
                text, calls, end, ..
            } => {
                let calls: Vec<String> = calls
                    .iter()
                    .map(|c| format!("{}={:?}", c.id, c.canonical))
                    .collect();
                format!("step|{}|{}|{end:?}", text.join("\u{1}"), calls.join(","))
            }
            Kind::Results { results } => {
                let r: Vec<String> = results
                    .iter()
                    .map(|r| format!("{}={}={}", r.call_id, r.content.text(), r.is_error))
                    .collect();
                format!("results|{}", r.join(","))
            }
            // A writer renders a compaction as one user message, so that is what a round trip
            // must show.
            Kind::Compaction { summary, .. } => format!("user|{summary}"),
            Kind::Meta { .. } => "meta".to_string(),
        })
        .collect()
}

/// The conversation of a session with `Meta` entries removed, for comparisons.
pub fn conversation_script(session: &Session) -> Vec<String> {
    script(session)
        .into_iter()
        .filter(|s| s != "meta")
        .collect()
}

/// The conversation as a round trip must show it: a step that lost **everything** (an assistant
/// turn with no text and no call, like OpenCode's killed row) is not written as an empty message —
/// every wire dialect refuses one — so it is not expected back. The plan records it in
/// `dropped_empty_steps` instead of dropping it silently.
pub fn roundtrip_script(session: &Session) -> Vec<String> {
    conversation_script(session)
        .into_iter()
        .filter(|s| !s.starts_with("step|||"))
        .collect()
}

/// Read a whole JSON Lines file into values.
pub fn read_jsonl(path: &Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("each line is JSON"))
        .collect()
}
