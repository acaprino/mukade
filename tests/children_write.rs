//! Writing a session's subagent sessions beside it
//! (docs/superpowers/specs/2026-11-12-subagents-design.md, phase 2, task 2).
//!
//! The source is the Claude Code fixture with a `subagents/` folder (a foreground child, a
//! background child with a child of its own, a teammate); each destination's own reader reads
//! back what was written.

mod support;

use mukade::children::import_with_children;
use mukade::hop::{HopError, SessionRef, export};
use mukade::model::{HarnessId, Kind, ParentKind, Session};
use mukade::reader::Reader;
use mukade::reader::claude_code::ClaudeCode;
use mukade::roots::StoreRoots;
use mukade::writer::WriteOpts;
use serde_json::Value;
use support::CLAUDE_PARENT as PARENT;

const FG: &str = "a1111111111111111";
const BG: &str = "a2222222222222222";
const DEEP: &str = "a4444444444444444";
const MATE: &str = "amate-6666666666666666";

fn source(tag: &str) -> (StoreRoots, Session) {
    let scratch = support::scratch(HarnessId::ClaudeCode, tag);
    let roots = support::stage_claude_subagents(&scratch);
    let parent = export(&SessionRef::id(HarnessId::ClaudeCode, PARENT), &roots).unwrap();
    (roots, parent)
}

/// A destination store of its own, and the options a write into it takes.
fn destination(dest: HarnessId, tag: &str) -> (StoreRoots, WriteOpts) {
    let scratch = support::scratch(dest, tag);
    let opts = support::opts(dest, &scratch);
    (opts.roots.clone(), opts)
}

fn hop_children(
    dest: HarnessId,
    source: &(StoreRoots, Session),
    opts: &WriteOpts,
) -> Result<mukade::Receipt, HopError> {
    let import = support::import_opts(opts, &source.1, None);
    import_with_children(dest, &source.1, &source.1, &source.0, &import)
}

fn entry<'a>(manifest: &'a Value, source_id: &str) -> &'a Value {
    manifest["children"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["source_id"] == source_id)
        .unwrap_or_else(|| panic!("no child {source_id} in {}", manifest["children"]))
}

fn text_of(session: &Session) -> String {
    session
        .conversation()
        .filter_map(|e| match &e.kind {
            Kind::User { text, .. } => Some(text.clone()),
            Kind::Step { text, .. } => Some(text.join(" ")),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Into Claude Code: each child is a `subagents/agent-<id>.jsonl` of the written parent with its
/// meta.json, and the destination's own reader lists them under it.
#[test]
fn claude_children_are_written_into_the_parents_subagents_folder() {
    let source = source("cw-claude-src");
    let (dest_roots, opts) = destination(HarnessId::ClaudeCode, "cw-claude-dst");

    let receipt = hop_children(HarnessId::ClaudeCode, &source, &opts).unwrap();

    let manifest = &receipt.manifest;
    let kids = manifest["children"].as_array().unwrap();
    assert_eq!(kids.len(), 4, "{manifest:#}");
    assert!(kids.iter().all(|c| c["link"] == "native"), "{kids:?}");
    let parent_file = std::path::Path::new(&receipt.location);
    let folder = parent_file.with_extension("").join("subagents");
    for kid in kids {
        let file = std::path::PathBuf::from(kid["dest_path"].as_str().unwrap());
        assert!(file.starts_with(&folder), "{}", file.display());
        assert!(file.is_file());
        assert!(file.with_extension("").with_extension("meta.json").is_file(), "{file:?}");
    }

    // Read back: the destination's reader lists each child under the parent it was written with.
    let all = ClaudeCode.list_all(&dest_roots).unwrap();
    let listed = |dest_id: &str| all.iter().find(|s| s.id == dest_id).unwrap().clone();
    let fg = listed(entry(manifest, &format!("{PARENT}.{FG}"))["dest_id"].as_str().unwrap());
    assert_eq!(fg.parent.as_deref(), Some(receipt.dest_id.as_str()));
    assert_eq!(fg.title.as_deref(), Some("Read module A"));
    let Some(ParentKind::Spawn { agent, call_id, .. }) = &fg.parent_kind else {
        panic!("{:?}", fg.parent_kind)
    };
    assert_eq!(agent.as_deref(), Some("Explore"));
    assert_eq!(
        call_id, &None,
        "the parent's Agent call is text form: there is no call of the destination to name"
    );
    let parent = ClaudeCode.read(&receipt.dest_id, &dest_roots).unwrap();
    assert!(
        text_of(&parent).contains(&format!("→ {}", fg.id)),
        "the text form names the child: {}",
        text_of(&parent)
    );
    let bg_dest = entry(manifest, &format!("{PARENT}.{BG}"))["dest_id"].as_str().unwrap();
    let deep = listed(entry(manifest, &format!("{PARENT}.{DEEP}"))["dest_id"].as_str().unwrap());
    assert_eq!(deep.parent.as_deref(), Some(bg_dest), "a child of a child hangs from it");
    assert_eq!(entry(manifest, &format!("{PARENT}.{DEEP}"))["depth"], 2);

    // And the child is the conversation it was.
    let session = ClaudeCode.read(fg.id.as_str(), &dest_roots).unwrap();
    assert!(text_of(&session).contains("Read module A and report what it exports."));
    assert!(text_of(&session).contains("Module A exports parse()."));
}

/// Into Codex and Pi, which have no link: each child is a session of its own, the manifest says
/// so, and the parent's text-form spawn call names the destination child.
#[test]
fn children_without_a_native_link_are_sessions_of_their_own_named_by_the_parent() {
    for dest in [HarnessId::Codex, HarnessId::Pi] {
        let source = source(&format!("cw-{}-src", dest.as_str()));
        let (_, opts) = destination(dest, &format!("cw-{}-dst", dest.as_str()));

        let receipt = hop_children(dest, &source, &opts).unwrap();

        let manifest = &receipt.manifest;
        let kids = manifest["children"].as_array().unwrap();
        assert_eq!(kids.len(), 4, "{dest:?}: {manifest:#}");
        assert!(kids.iter().all(|c| c["link"] == "none"), "{dest:?}: {kids:?}");
        let fg = entry(manifest, &format!("{PARENT}.{FG}"));
        let written = support::read_destination(dest, fg["dest_path"].as_str().unwrap());
        assert!(
            text_of(&written).contains("Module A exports parse()."),
            "{dest:?}: the child is its conversation"
        );
        assert_eq!(written.meta.parent, None, "{dest:?}: no native link is written");
        // The parent says which session its first call became.
        let parent = support::read_destination(dest, &receipt.location);
        let first_call = parent
            .conversation()
            .filter_map(|e| match &e.kind {
                Kind::Step { text, .. } => Some(text.join("\n")),
                _ => None,
            })
            .find(|t| t.contains("Read module A"))
            .unwrap_or_else(|| panic!("{dest:?}: the call travelled as text"));
        assert!(
            first_call.contains(&format!("→ {}", fg["dest_id"].as_str().unwrap())),
            "{dest:?}: {first_call}"
        );
        // A child written whole counts its own losses in the parent's manifest, none lost here.
        assert_eq!(
            manifest["children"][0]["compaction"], "none",
            "{dest:?}: {manifest:#}"
        );
    }
}

/// A child's own spawn call names the child it started in the child's text, too.
#[test]
fn a_child_of_a_child_is_named_by_its_own_parent() {
    let source = source("cw-deep-src");
    let (_, opts) = destination(HarnessId::Pi, "cw-deep-dst");

    let receipt = hop_children(HarnessId::Pi, &source, &opts).unwrap();

    let manifest = &receipt.manifest;
    let bg = entry(manifest, &format!("{PARENT}.{BG}"));
    let deep = entry(manifest, &format!("{PARENT}.{DEEP}"));
    let written = support::read_destination(HarnessId::Pi, bg["dest_path"].as_str().unwrap());
    let says = text_of(&written);
    assert!(
        says.contains(&format!("→ {}", deep["dest_id"].as_str().unwrap())),
        "{says}"
    );
}

/// What the parent has no child for, and what stays unread, is the same without `--children`:
/// `import_with_children` of a session with no subagents is a plain hop with an empty list.
#[test]
fn a_session_without_children_gets_an_empty_children_list() {
    let scratch = support::scratch(HarnessId::ClaudeCode, "cw-none-src");
    let id = support::stage_fixture(HarnessId::ClaudeCode, &scratch);
    let roots = support::roots(&scratch);
    let parent = export(&SessionRef::id(HarnessId::ClaudeCode, &id), &roots).unwrap();
    let (_, opts) = destination(HarnessId::Pi, "cw-none-dst");

    let receipt = hop_children(HarnessId::Pi, &(roots, parent), &opts).unwrap();

    assert_eq!(receipt.manifest["children"], serde_json::json!([]));
}

/// A child that cannot be written stops the children there. The parent stays written, and the
/// error carries its receipt with what was written.
#[test]
fn a_child_that_cannot_be_written_leaves_the_parent_and_fails_the_hop() {
    let source = source("cw-fail-src");
    let (dest_roots, mut opts) = destination(HarnessId::ClaudeCode, "cw-fail-dst");
    // The parent's id is reserved, so its folder is known: a file stands where `subagents` goes.
    let reserved = "5ab1c0de-0000-4000-8000-0000000000ff";
    opts.dest_id = Some(reserved.into());
    let project = mukade::roots::claude_project_dir_name(std::path::Path::new(
        opts.cwd.as_deref().unwrap(),
    ));
    let folder = dest_roots
        .claude_config_dir
        .clone()
        .unwrap()
        .join("projects")
        .join(project)
        .join(reserved);
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("subagents"), "in the way").unwrap();

    let error = hop_children(HarnessId::ClaudeCode, &source, &opts).unwrap_err();

    assert_eq!(error.exit_code(), 5, "{error}");
    let HopError::ChildWriteFailed { reason, receipt } = error else {
        panic!("{error:?}")
    };
    assert!(reason.starts_with(&format!("child {PARENT}.{FG}: ")), "{reason}");
    assert_eq!(receipt.dest_id, reserved, "the parent was written");
    assert!(std::path::Path::new(&receipt.location).is_file());
    assert_eq!(receipt.manifest["children"], serde_json::json!([]));
    let losses = receipt.manifest["losses"].to_string();
    assert!(losses.contains("not written"), "{losses}");
    assert!(
        losses.contains(&format!("child {PARENT}.{MATE}: not written, an earlier child failed")),
        "the children after it were not tried: {losses}"
    );
}

/// A spawn call the host maps to a tool of the destination (`tool_map`) is written as that tool's
/// call; the child's meta.json then names the destination's call, the way Claude Code does.
#[test]
fn a_spawn_call_written_as_a_tool_call_is_the_childs_tool_use_id() {
    let source = source("cw-toolmap-src");
    let (dest_roots, mut opts) = destination(HarnessId::ClaudeCode, "cw-toolmap-dst");
    let call = source
        .1
        .entries
        .iter()
        .find_map(|e| match &e.kind {
            Kind::Step { calls, .. } => calls.iter().find(|c| c.id == "toolu_fg"),
            _ => None,
        })
        .unwrap();
    opts.tool_map.push((
        call.source_name.clone(),
        "Agent".into(),
        call.input.clone(),
        call.input.clone(),
    ));

    let receipt = hop_children(HarnessId::ClaudeCode, &source, &opts).unwrap();

    let all = ClaudeCode.list_all(&dest_roots).unwrap();
    let fg_dest = entry(&receipt.manifest, &format!("{PARENT}.{FG}"))["dest_id"].as_str().unwrap();
    let fg = all.iter().find(|s| s.id == fg_dest).unwrap();
    let Some(ParentKind::Spawn { call_id, .. }) = &fg.parent_kind else {
        panic!("{:?}", fg.parent_kind)
    };
    assert_eq!(call_id.as_deref(), Some("toolu_fg"));
    let parent = ClaudeCode.read(&receipt.dest_id, &dest_roots).unwrap();
    assert!(
        !text_of(&parent).contains(&format!("→ {fg_dest}")),
        "a call of the destination is not named in text: {}",
        text_of(&parent)
    );
}

/// Into OpenCode: a subagent's import document names the session that spawned it, `info.parentID`;
/// a session's own document names none. (The engine honours it: the import needs the parent in
/// the store already, which is why the parent is imported first.)
#[test]
fn an_opencode_subagent_document_names_its_parent() {
    use mukade::writer::{ChildOf, Subagents};
    let scratch = support::scratch(HarnessId::OpenCode, "cw-opencode-doc");
    let session = support::read_source(HarnessId::OpenCode);
    let mut opts = support::opts(HarnessId::OpenCode, &scratch);
    opts.project_id = Some("project-under-test".into());
    let cwd = support::cwd_for(HarnessId::OpenCode);
    let document = |opts: &WriteOpts| {
        let mut clock = mukade::policy::Clock::new(opts.now.as_deref());
        mukade::writer::opencode::document_for(&session, "ses_child", &cwd, opts, &mut clock)
            .expect("document")
    };

    assert!(document(&opts)["info"].get("parentID").is_none());
    opts.subagents = Subagents {
        of: Some(ChildOf {
            root_id: "ses_parent".into(),
            parent_id: "ses_parent".into(),
            depth: 1,
            ..ChildOf::default()
        }),
        ..Subagents::default()
    };
    let child = document(&opts);

    assert_eq!(child["info"]["parentID"], "ses_parent");
    assert_eq!(child["info"]["id"], "ses_child");
}

/// `writer.child_id` is in the destination's own shape: a `ses_` id for OpenCode, the parent's id
/// and an agent id for Claude Code, a uuid for the others.
#[test]
fn a_childs_id_has_its_destinations_shape() {
    let root = "5ab1c0de-0000-4000-8000-0000000000aa";
    let id = |h| mukade::writer::for_harness(h).child_id(root);
    assert!(id(HarnessId::OpenCode).starts_with("ses_"));
    let claude = id(HarnessId::ClaudeCode);
    let (parent, agent) = claude.split_once('.').unwrap();
    assert_eq!(parent, root);
    assert!(agent.starts_with('a') && agent.len() == 17, "{claude}");
    assert!(uuid::Uuid::parse_str(&id(HarnessId::Codex)).is_ok());
    assert!(uuid::Uuid::parse_str(&id(HarnessId::Pi)).is_ok());
}

/// A subagent written straight through the Claude Code writer: its parent's folder is `<root id>`
/// beside the parent's file, whatever that file is called.
fn write_claude_child(
    tag: &str,
    root_file_name: &str,
    of: mukade::writer::ChildOf,
    title: Option<&str>,
) -> (std::path::PathBuf, Value) {
    let scratch = support::scratch(HarnessId::ClaudeCode, tag);
    let mut session = support::read_source(HarnessId::ClaudeCode);
    session.meta.title = title.map(str::to_string);
    let mut opts = support::opts(HarnessId::ClaudeCode, &scratch);
    let project = scratch.join("home/projects/some-project");
    let root_id = "5ab1c0de-0000-4000-8000-0000000000aa";
    opts.dest_id = Some(format!("{root_id}.a0123456789abcdef"));
    opts.subagents = mukade::writer::Subagents {
        of: Some(mukade::writer::ChildOf {
            root_id: root_id.into(),
            root_location: project.join(root_file_name).to_string_lossy().into_owned(),
            parent_id: root_id.into(),
            depth: 1,
            ..of
        }),
        assigned_id: true,
        ..Default::default()
    };
    let written = mukade::writer::for_harness(HarnessId::ClaudeCode)
        .write(&session, &opts)
        .unwrap();
    let file = std::path::PathBuf::from(&written.location);
    let meta = std::fs::read_to_string(file.with_extension("").with_extension("meta.json")).unwrap();
    (file, serde_json::from_str(&meta).unwrap())
}

/// The file is what Claude Code writes for a subagent: user, assistant and attachment rows of the
/// sidechain, with no title or last-prompt bookkeeping (those belong to the session's own file and
/// carry its id) and no invented fields.
#[test]
fn a_claude_child_file_holds_only_the_rows_a_real_one_does() {
    let (file, _) = write_claude_child(
        "cw-child-rows",
        "5ab1c0de-0000-4000-8000-0000000000aa.jsonl",
        Default::default(),
        Some("A title the source had"),
    );

    let text = std::fs::read_to_string(file).unwrap();
    let rows: Vec<Value> = text.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert!(!rows.is_empty());
    for row in &rows {
        let kind = row["type"].as_str().unwrap();
        assert!(
            !matches!(kind, "custom-title" | "last-prompt"),
            "a bookkeeping row of the session's own file: {row}"
        );
        assert!(row.get("promptSource").is_none(), "{row}");
        assert_eq!(row["isSidechain"], true);
        assert_eq!(row["agentId"], "a0123456789abcdef");
    }
}

/// meta.json has the keys Claude Code writes, and a description that is a string.
#[test]
fn a_claude_childs_meta_json_has_the_keys_a_real_one_has() {
    let (_, meta) = write_claude_child(
        "cw-child-meta",
        "5ab1c0de-0000-4000-8000-0000000000aa.jsonl",
        mukade::writer::ChildOf {
            agent: Some("Explore".into()),
            ..Default::default()
        },
        None,
    );

    assert_eq!(meta["agentType"], "Explore");
    assert_eq!(meta["description"], "Explore", "no title: the agent type, never null");
    assert_eq!(meta["requestShape"], "foreground");
    assert_eq!(meta["requestNonInteractive"], false);
    assert_eq!(meta["isBuiltIn"], false);
    assert_eq!(meta["spawnDepth"], 1);
    assert!(meta.get("toolUseId").is_none());
}

/// A parent written to a `--dest-path` that is not `<id>.jsonl` still has its children in
/// `<its folder>/<id>/subagents/`, the folder Claude Code looks in for that session id.
#[test]
fn a_claude_childs_folder_is_the_parents_id_not_its_file_name() {
    let (file, _) = write_claude_child(
        "cw-child-folder",
        "renamed-parent.jsonl",
        Default::default(),
        Some("t"),
    );

    assert!(
        file.ends_with("some-project/5ab1c0de-0000-4000-8000-0000000000aa/subagents/agent-a0123456789abcdef.jsonl"),
        "{}",
        file.display()
    );
}

/// A child's own losses are the parent's too, prefixed with the child's id: a hop's manifest is the
/// one place that says everything the hop left behind.
#[test]
fn a_childs_own_losses_are_prefixed_into_the_parents() {
    let source = source("cw-losses-src");
    let (_, opts) = destination(HarnessId::Pi, "cw-losses-dst");

    let receipt = hop_children(HarnessId::Pi, &source, &opts).unwrap();

    let losses: Vec<String> = receipt.manifest["losses"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l.as_str().unwrap().to_string())
        .collect();
    // The background child spawns the deep one: its Agent call is text form in Pi.
    assert!(
        losses.iter().any(|l| l.starts_with(&format!("child {PARENT}.{BG}: tool calls written as text"))),
        "{losses:?}"
    );
    assert!(
        losses.iter().filter(|l| l.starts_with("child ")).all(|l| l.contains(": ")),
        "{losses:?}"
    );
}

/// A child the reader refuses is a loss line, not a failure: the other children are written.
#[test]
fn a_refused_child_is_a_loss_line_and_the_others_are_written() {
    let scratch = support::scratch(HarnessId::ClaudeCode, "cw-refused-src");
    let roots = support::stage_claude_subagents(&scratch);
    let file = scratch
        .join("home/projects/D--work-modules")
        .join(PARENT)
        .join("subagents")
        .join(format!("agent-{FG}.jsonl"));
    std::fs::write(
        &file,
        format!(
            "{{\"parentUuid\":\"no-such-row\",\"isSidechain\":true,\"agentId\":\"{FG}\",\"type\":\"user\",\"message\":{{\"role\":\"user\",\"content\":\"x\"}},\"uuid\":\"u1\",\"timestamp\":\"2026-11-12T09:00:10.000Z\",\"sessionId\":\"{PARENT}\"}}\n"
        ),
    )
    .unwrap();
    let parent = export(&SessionRef::id(HarnessId::ClaudeCode, PARENT), &roots).unwrap();
    let (_, opts) = destination(HarnessId::Pi, "cw-refused-dst");

    let receipt = hop_children(HarnessId::Pi, &(roots, parent), &opts).unwrap();

    let kids = receipt.manifest["children"].as_array().unwrap();
    assert_eq!(kids.len(), 3, "{:#}", receipt.manifest);
    assert!(kids.iter().all(|c| c["source_id"] != format!("{PARENT}.{FG}").as_str()));
    let losses = receipt.manifest["losses"].to_string();
    assert!(losses.contains(&format!("child {PARENT}.{FG}: ")), "{losses}");
}

/// What a child is written with follows the session's compaction: `native` is deterministic and
/// applies to each child; `portable` and `summary` are the session's own history alone.
#[test]
fn a_childs_compaction_follows_the_sessions_only_when_it_is_native() {
    use mukade::native_compaction::NativeBudget;
    let source = source("cw-compaction-src");
    for (mode, budget, expected) in [
        ("native", Some(40_000), "native"),
        ("summary", None, "none"),
        ("portable", None, "none"),
    ] {
        let (_, opts) = destination(HarnessId::Pi, &format!("cw-compaction-{mode}"));
        let mut import = support::import_opts(&opts, &source.1, None);
        import.compaction = mode.into();
        import.native_budget = budget.map(|context| NativeBudget {
            context,
            max_output_tokens: Some(32_000),
            text_token_factor: None,
        });

        let receipt =
            import_with_children(HarnessId::Pi, &source.1, &source.1, &source.0, &import).unwrap();

        let kids = receipt.manifest["children"].as_array().unwrap();
        assert_eq!(kids.len(), 4, "{mode}: {:#}", receipt.manifest);
        assert!(
            kids.iter().all(|c| c["compaction"] == expected),
            "{mode}: {kids:?}"
        );
    }
}

/// A request built with `HopRequest::new` carries no children, and a library caller turns them on
/// with the one field: `hop_into` writes them.
#[test]
fn a_request_from_new_carries_no_children_until_asked() {
    use mukade::hop::{HopRequest, hop_into};
    let source = source("cw-new-src");
    let (dest_roots, opts) = destination(HarnessId::Pi, "cw-new-dst");
    let base = HopRequest::new(
        SessionRef::id(HarnessId::ClaudeCode, PARENT),
        HarnessId::Pi,
        source.0.clone(),
    );
    assert!(!base.children);
    let request = HopRequest {
        children: true,
        cwd: opts.cwd.clone(),
        write: support::details(&opts),
        ..base
    };

    let receipt = hop_into(&request, &dest_roots).unwrap();

    assert_eq!(receipt.manifest["children"].as_array().unwrap().len(), 4);
}
