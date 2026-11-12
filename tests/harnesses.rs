//! The adapters: one per harness, each answering for its own reader, writer and store.

mod support;

use mukade::harnesses::{Destination, all, get};
use mukade::model::HarnessId;
use mukade::roots::StoreRoots;
use std::path::{Path, PathBuf};

/// The adapter of `id` as a destination.
fn destination(id: HarnessId) -> &'static dyn Destination {
    mukade::harnesses::destination(id).expect("a destination")
}

/// The harnesses mukade writes, in [`HarnessId::ALL`] order.
fn destinations() -> Vec<HarnessId> {
    HarnessId::ALL
        .into_iter()
        .filter(|id| get(*id).destination().is_some())
        .collect()
}

/// The harnesses whose fixture session `support::source_path` names.
const WITH_FIXTURE: [HarnessId; 4] = [
    HarnessId::ClaudeCode,
    HarnessId::Codex,
    HarnessId::OpenCode,
    HarnessId::Pi,
];

#[test]
fn every_id_has_its_own_adapter() {
    for id in HarnessId::ALL {
        let adapter = get(id);
        assert_eq!(adapter.id(), id);
        assert_eq!(adapter.reader().harness(), id);
        if let Some(destination) = adapter.destination() {
            assert_eq!(destination.writer().harness(), id);
        }
        assert_eq!(id.as_str().parse::<HarnessId>().unwrap(), id);
    }
    let order: Vec<HarnessId> = all().iter().map(|adapter| adapter.id()).collect();
    assert_eq!(order, HarnessId::ALL);
}

#[test]
fn hermes_and_deepseek_harness_are_named_as_users_type_them() {
    assert_eq!(HarnessId::ALL.len(), 6);
    assert_eq!("hermes".parse::<HarnessId>().unwrap(), HarnessId::Hermes);
    assert_eq!(
        "deepseek_harness".parse::<HarnessId>().unwrap(),
        HarnessId::DeepSeekHarness
    );
    assert_eq!(
        serde_json::to_value(HarnessId::DeepSeekHarness).unwrap(),
        "deepseek_harness"
    );
    let unknown = "nonesuch".parse::<HarnessId>().unwrap_err().to_string();
    assert!(unknown.contains("hermes, deepseek_harness"), "{unknown}");
}

/// Every registered harness is a destination now (Hermes through its own import). A harness with
/// no writer is refused before planning: `src/harnesses/mod.rs` tests that with a source-only one.
#[test]
fn every_registered_harness_is_a_destination() {
    for id in HarnessId::ALL {
        assert!(mukade::harnesses::destination(id).is_ok(), "{id}");
    }
}

/// A root for every harness, each its own.
fn every_root() -> StoreRoots {
    StoreRoots {
        claude_config_dir: Some(PathBuf::from("stores/claude")),
        codex_home: Some(PathBuf::from("stores/codex")),
        opencode_db: Some(PathBuf::from("stores/opencode.db")),
        pi_agent_dir: Some(PathBuf::from("stores/pi")),
        hermes_home: Some(PathBuf::from("stores/hermes")),
        dsh_home: Some(PathBuf::from("stores/dsh")),
    }
}

#[test]
fn the_store_root_is_the_roots_field() {
    let roots = every_root();
    let field = |id| match id {
        HarnessId::ClaudeCode => roots.claude_config_dir.as_deref(),
        HarnessId::Codex => roots.codex_home.as_deref(),
        HarnessId::OpenCode => roots.opencode_db.as_deref(),
        HarnessId::Pi => roots.pi_agent_dir.as_deref(),
        HarnessId::Hermes => roots.hermes_home.as_deref(),
        HarnessId::DeepSeekHarness => roots.dsh_home.as_deref(),
    };
    for id in HarnessId::ALL {
        assert_eq!(get(id).store_root(&roots), field(id), "{id}");
        assert_eq!(get(id).store_root(&StoreRoots::default()), None, "{id}");
    }
}

#[test]
fn an_opencode_root_folder_means_its_database() {
    let dir = support::scratch(HarnessId::OpenCode, "adapter-root");
    let file = dir.join("store.db");
    std::fs::write(&file, b"").unwrap();

    let mut roots = StoreRoots::default();
    let opencode = get(HarnessId::OpenCode);
    opencode.set_store_root(&mut roots, &dir);
    assert_eq!(
        opencode.store_root(&roots),
        Some(dir.join("opencode.db").as_path())
    );
    opencode.set_store_root(&mut roots, &file);
    assert_eq!(opencode.store_root(&roots), Some(file.as_path()));

    for id in HarnessId::ALL
        .into_iter()
        .filter(|id| *id != HarnessId::OpenCode)
    {
        let mut roots = StoreRoots::default();
        get(id).set_store_root(&mut roots, &dir);
        assert_eq!(get(id).store_root(&roots), Some(dir.as_path()), "{id}");
    }
}

/// The field is set as it is: a folder stays a folder, even OpenCode's, as `StoreRoots::from_env`
/// and the inventory read a variable.
#[test]
fn the_store_field_is_set_as_it_is() {
    let dir = support::scratch(HarnessId::OpenCode, "adapter-field");
    for adapter in all() {
        let mut roots = StoreRoots::default();
        *adapter.store_root_mut(&mut roots) = Some(dir.clone());
        assert_eq!(
            adapter.store_root(&roots),
            Some(dir.as_path()),
            "{}",
            adapter.id()
        );
    }
}

/// OpenCode reads a relative database path beneath its data directory and keeps `:memory:`; the
/// others read a path as it is.
#[test]
fn a_store_path_is_resolved_the_way_its_harness_reads_it() {
    let vars = mukade::catalog::Vars {
        home: Some(PathBuf::from("/home/u")),
        xdg_data_home: Some(PathBuf::from("/data")),
        ..Default::default()
    };
    let opencode = get(HarnessId::OpenCode);
    let data = mukade::catalog::store_default_with(HarnessId::OpenCode, &vars)
        .and_then(|default| default.parent().map(Path::to_path_buf))
        .expect("an OpenCode data directory");
    assert_eq!(
        opencode.resolve_store_path(PathBuf::from("custom.db"), &vars),
        data.join("custom.db")
    );
    assert_eq!(
        opencode.resolve_store_path(PathBuf::from(":memory:"), &vars),
        PathBuf::from(":memory:")
    );
    assert!(opencode.store_is_in_memory(Path::new(":memory:")));
    for id in HarnessId::ALL
        .into_iter()
        .filter(|id| *id != HarnessId::OpenCode)
    {
        let adapter = get(id);
        assert_eq!(
            adapter.resolve_store_path(PathBuf::from("custom"), &vars),
            PathBuf::from("custom"),
            "{id}"
        );
        assert!(!adapter.store_is_in_memory(Path::new(":memory:")), "{id}");
    }
}

#[test]
fn the_store_env_names_the_root_or_clears_opencode() {
    let roots = every_root();
    let variable = |id| match id {
        HarnessId::ClaudeCode => "CLAUDE_CONFIG_DIR",
        HarnessId::Codex => "CODEX_HOME",
        HarnessId::OpenCode => "OPENCODE_DB",
        HarnessId::Pi => "PI_CODING_AGENT_DIR",
        HarnessId::Hermes => "HERMES_HOME",
        HarnessId::DeepSeekHarness => "DSH_HOME",
    };
    let env_of = |command: &std::process::Command, name: &str| {
        command
            .get_envs()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| value.map(Path::new).map(Path::to_path_buf))
    };
    for id in HarnessId::ALL {
        let mut command = std::process::Command::new("harness");
        get(id).apply_store_env(&roots, &mut command);
        assert_eq!(
            env_of(&command, variable(id)),
            Some(get(id).store_root(&roots).map(Path::to_path_buf)),
            "{id}"
        );
    }

    let mut command = std::process::Command::new("opencode");
    get(HarnessId::OpenCode).apply_store_env(&StoreRoots::default(), &mut command);
    assert_eq!(
        env_of(&command, "OPENCODE_DB"),
        Some(None),
        "removed, not inherited"
    );
    let mut command = std::process::Command::new("pi");
    get(HarnessId::Pi).apply_store_env(&StoreRoots::default(), &mut command);
    assert_eq!(
        env_of(&command, "PI_CODING_AGENT_DIR"),
        None,
        "left as inherited"
    );
}

#[test]
fn read_file_reads_each_fixture() {
    for id in WITH_FIXTURE.into_iter().chain([HarnessId::DeepSeekHarness]) {
        let session = get(id)
            .read_file(&support::source_path(id))
            .unwrap_or_else(|error| panic!("{id}: {error:#}"));
        assert_eq!(session.meta.harness, id);
    }
}

#[test]
fn names_from_the_catalog_are_the_ones_users_type() {
    use mukade::catalog;
    let store = [
        "CLAUDE_CONFIG_DIR",
        "CODEX_HOME",
        "OPENCODE_DB",
        "PI_CODING_AGENT_DIR",
        "HERMES_HOME",
        "DSH_HOME",
    ];
    let exe_env = [
        "MUKADE_CLAUDE_EXE",
        "MUKADE_CODEX_EXE",
        "MUKADE_OPENCODE_EXE",
        "MUKADE_PI_EXE",
        "MUKADE_HERMES_EXE",
        "MUKADE_DSH_EXE",
    ];
    let flag = [
        "--claude-exe",
        "--codex-exe",
        "--opencode-exe",
        "--pi-exe",
        "--hermes-exe",
        "--dsh-exe",
    ];
    let binary = ["claude", "codex", "opencode", "pi", "hermes", "dsh"];
    let lineage = [false, false, true, true, true, true];
    for (n, id) in HarnessId::ALL.into_iter().enumerate() {
        assert_eq!(catalog::store_env(id), store[n], "{id}");
        assert_eq!(catalog::binary_env(id), exe_env[n], "{id}");
        assert_eq!(catalog::binary_flag(id), flag[n], "{id}");
        assert_eq!(catalog::lineage_recorded(id), lineage[n], "{id}");
        assert_eq!(mukade::exe::binary_name(id), binary[n], "{id}");
        assert_eq!(
            mukade::exe::env_var_name(id),
            catalog::binary_env(id),
            "{id}"
        );
    }
}

#[test]
fn the_usage_text_names_what_the_catalog_names() {
    for id in HarnessId::ALL {
        assert!(
            mukade::cli::USAGE.contains(mukade::catalog::binary_flag(id)),
            "{id}"
        );
        assert!(
            mukade::cli::USAGE.contains(mukade::catalog::binary_env(id)),
            "{id}"
        );
        assert!(
            mukade::cli::USAGE.contains(mukade::catalog::store_env(id)),
            "{id}"
        );
    }
    let names: Vec<&str> = HarnessId::ALL.iter().map(|id| id.as_str()).collect();
    assert!(
        mukade::cli::USAGE.contains(&format!("harnesses: {}", names.join(" | "))),
        "the harnesses line names every harness"
    );
}

#[test]
fn only_opencode_takes_an_agent_or_metadata() {
    use mukade::writer::WriteOpts;
    let asks = [
        WriteOpts {
            dest_agent: Some("analyst".into()),
            ..WriteOpts::default()
        },
        WriteOpts {
            metadata: Some(serde_json::json!({"origin": "test"})),
            ..WriteOpts::default()
        },
    ];
    for id in destinations() {
        for opts in &asks {
            let checked = destination(id).validate_write(opts);
            if id == HarnessId::OpenCode {
                assert!(checked.is_ok(), "{id}: {checked:?}");
            } else {
                let error = checked.expect_err(id.as_str()).to_string();
                assert!(error.contains("OpenCode"), "{id}: {error}");
            }
        }
    }
}

#[test]
fn each_destination_takes_its_own_id_format() {
    use mukade::writer::WriteOpts;
    let with_id = |id: &str| WriteOpts {
        dest_id: Some(id.into()),
        ..WriteOpts::default()
    };
    let uuid = "00000000-0000-4000-8000-000000000001";
    let opencode = "ses_0123abc";
    for id in destinations() {
        if id == HarnessId::Hermes {
            // Hermes allocates its ids: every one is refused.
            assert!(
                destination(id).validate_write(&with_id(uuid)).is_err(),
                "{id}"
            );
            assert!(
                destination(id).validate_write(&with_id(opencode)).is_err(),
                "{id}"
            );
            continue;
        }
        let (taken, refused) = if id == HarnessId::OpenCode {
            (opencode, uuid)
        } else {
            (uuid, opencode)
        };
        assert!(
            destination(id).validate_write(&with_id(taken)).is_ok(),
            "{id} takes {taken}"
        );
        assert!(
            destination(id).validate_write(&with_id(refused)).is_err(),
            "{id} refuses {refused}"
        );
    }
    assert!(mukade::writer::validate_id_syntax(uuid).is_ok());
    assert!(mukade::writer::validate_id_syntax(opencode).is_ok());
    assert!(mukade::writer::validate_id_syntax("../invalid").is_err());
    let malformed = mukade::writer::validate_id_syntax("ses_bad!")
        .expect_err("a malformed OpenCode id")
        .to_string();
    assert_eq!(malformed, "invalid OpenCode destination session ID");
}

#[test]
fn only_opencode_exports_an_interchange_document() {
    for id in destinations() {
        assert_eq!(
            destination(id).interchange_document().is_some(),
            id == HarnessId::OpenCode,
            "{id}"
        );
    }
}

#[test]
fn only_codex_reads_every_branch() {
    for id in WITH_FIXTURE {
        let adapter = get(id);
        let by_path = adapter.read_all_branches(&support::source_path(id));
        let by_id = adapter.read_all_branches_by_id("any-id", &StoreRoots::default());
        if id == HarnessId::Codex {
            let session = by_path
                .expect("Codex reads every branch")
                .expect("the fixture reads");
            assert_eq!(session.meta.harness, id);
            let error = by_id
                .expect("Codex reads every branch by id")
                .expect_err("no store");
            assert!(error.to_string().contains("CODEX_HOME"), "{error:#}");
        } else {
            assert!(by_path.is_none(), "{id}");
            assert!(by_id.is_none(), "{id}");
        }
    }
}

#[test]
fn only_pi_summarizes_from_a_file() {
    for id in HarnessId::ALL {
        assert_eq!(get(id).summarizes_from_file(), id == HarnessId::Pi, "{id}");
    }
}

#[test]
fn only_pi_keeps_the_end_of_a_failed_step() {
    for id in destinations() {
        assert_eq!(
            destination(id).failed_steps_are_replies(),
            id != HarnessId::Pi,
            "{id}"
        );
    }
}

#[test]
fn claude_code_codex_deepseek_harness_and_hermes_flatten_result_blocks() {
    for id in destinations() {
        assert_eq!(
            destination(id).flattens_result_blocks(),
            matches!(
                id,
                HarnessId::ClaudeCode
                    | HarnessId::Codex
                    | HarnessId::DeepSeekHarness
                    | HarnessId::Hermes
            ),
            "{id}"
        );
    }
}

#[test]
fn store_stats_count_the_fixture_store() {
    for id in [HarnessId::ClaudeCode, HarnessId::Codex, HarnessId::Pi] {
        let scratch = support::scratch(id, "adapter-stats");
        support::stage_fixture(id, &scratch);
        let stats = get(id).store_stats(&support::roots(&scratch));
        let sessions = stats
            .sessions
            .unwrap_or_else(|error| panic!("{id}: {error}"));
        assert!(sessions >= 1, "{id}: {sessions}");
        match id {
            HarnessId::Pi => assert!(
                matches!(stats.forks, Some(Ok(_))),
                "{id}: {:?}",
                stats.forks
            ),
            _ => assert!(stats.forks.is_none(), "{id} does not record lineage"),
        }
    }
    let scratch = support::scratch(HarnessId::OpenCode, "adapter-stats");
    let stats = get(HarnessId::OpenCode).store_stats(&support::roots(&scratch));
    assert!(stats.sessions.is_err(), "no database to count");
}

/// Lineage is one fact: `harnesses` says "not recorded" from the catalog for a store it cannot
/// read and from the adapter's fork count for one it reads, so the two agree for every harness.
#[test]
fn the_catalog_records_lineage_where_the_adapter_counts_forks() {
    for id in HarnessId::ALL {
        let scratch = support::scratch(id, "adapter-lineage");
        for roots in [StoreRoots::default(), support::roots(&scratch)] {
            assert_eq!(
                get(id).store_stats(&roots).forks.is_some(),
                mukade::catalog::lineage_recorded(id),
                "{id}: the catalog's lineage and the adapter's fork count disagree"
            );
        }
    }
}

#[test]
fn a_destination_names_the_model_its_own_configuration_states() {
    let scratch = support::scratch(HarnessId::Codex, "adapter-model");
    let home = scratch.join("home");
    std::fs::write(home.join("config.toml"), "model_provider = \"azure\"\n").unwrap();
    let data = scratch.join("data");
    std::fs::create_dir_all(data.join("config/opencode")).unwrap();
    std::fs::write(
        data.join("config/opencode/opencode.json"),
        r#"{"model":"gateway/auto"}"#,
    )
    .unwrap();
    let roots = StoreRoots {
        codex_home: Some(home),
        opencode_db: Some(data.join("share/opencode/opencode.db")),
        ..StoreRoots::default()
    };
    for id in destinations() {
        let expected = match id {
            HarnessId::Codex => Some(("azure".to_string(), None)),
            HarnessId::OpenCode => Some(("gateway".to_string(), Some("auto".to_string()))),
            _ => None,
        };
        assert_eq!(destination(id).configured_model(&roots), expected, "{id}");
        assert_eq!(
            mukade::dest::configured_model_in(id, &roots),
            expected,
            "{id}: dest delegates"
        );
    }
}

#[test]
fn only_opencode_files_sessions_under_a_project() {
    for id in [HarnessId::ClaudeCode, HarnessId::Codex, HarnessId::Pi] {
        let found =
            destination(id).project_id("no-such-exe", "somewhere", &StoreRoots::default(), None);
        assert!(matches!(found, Ok(None)), "{id}");
    }
}

#[test]
fn every_adapter_builds_a_summary_command() {
    use mukade::hop::HopError;
    use mukade::summary::SummarizeOpts;
    let scratch = support::scratch(HarnessId::Pi, "adapter-summary");
    for id in WITH_FIXTURE {
        let mut session = support::read_source(id);
        let name = mukade::exe::binary_name(id);
        let opts = SummarizeOpts {
            exe: Some(name.to_string()),
            ..SummarizeOpts::default()
        };
        if id == HarnessId::Pi {
            session.meta.source_path = None;
            let refusal = get(id)
                .summary_command(&session, &opts, &scratch)
                .expect_err("a Pi session with no file is not summarized");
            assert!(matches!(refusal, HopError::Unsupported(_)), "{refusal}");
            session.meta.source_path = Some("session.jsonl".into());
        }
        let command = get(id)
            .summary_command(&session, &opts, &scratch)
            .unwrap_or_else(|error| panic!("{id}: {error}"));
        assert_eq!(command.program, name, "{id}");
        assert_eq!(
            command.args.last().map(String::as_str),
            Some(mukade::summary::SUMMARY_PROMPT),
            "{id}"
        );
    }
}

/// Claude Code resumes a session by its own id; a subagent listed as `<parent-id>.<agentId>`
/// is refused with the parent to resume instead. DeepSeek Harness ships no terminal UI: every
/// session of it is refused, with the headless command that continues it. Every other listed
/// session resumes.
#[test]
fn only_a_claude_subagent_and_any_deepseek_session_are_refused_a_resume() {
    let child = mukade::reader::SessionInfo {
        id: "p-1.a1111111111111111".into(),
        parent: Some("p-1".into()),
        parent_kind: Some(mukade::model::ParentKind::spawn()),
        ..Default::default()
    };
    let root = mukade::reader::SessionInfo {
        id: "p-1".into(),
        ..Default::default()
    };
    for id in HarnessId::ALL {
        let adapter = get(id);
        let refusal = adapter.resume_refusal(&child);
        if id == HarnessId::DeepSeekHarness {
            for refused in [adapter.resume_refusal(&root), refusal] {
                let refused = refused.expect("dsh has no interface to open a session in");
                assert!(
                    refused.contains("dsh --profile headless --session-id p-1"),
                    "{refused}"
                );
            }
            continue;
        }
        assert_eq!(adapter.resume_refusal(&root), None, "{id}");
        if id.as_str() == "claude_code" {
            assert!(
                refusal
                    .as_deref()
                    .is_some_and(|r| r.ends_with("resume p-1")),
                "{refusal:?}"
            );
        } else {
            assert_eq!(refusal, None, "{id}");
        }
    }
}
