//! The adapters: one per harness, each answering for its own reader, writer and store.

mod support;

use mukade::harnesses::{all, get};
use mukade::model::HarnessId;
use mukade::roots::StoreRoots;
use std::path::{Path, PathBuf};

#[test]
fn every_id_has_its_own_adapter() {
    for id in HarnessId::ALL {
        let adapter = get(id);
        assert_eq!(adapter.id(), id);
        assert_eq!(adapter.reader().harness(), id);
        assert_eq!(adapter.writer().expect("a writer").harness(), id);
    }
    let order: Vec<HarnessId> = all().iter().map(|adapter| adapter.id()).collect();
    assert_eq!(order, HarnessId::ALL);
}

#[test]
fn the_store_root_is_the_roots_field() {
    let roots = StoreRoots {
        claude_config_dir: Some(PathBuf::from("stores/claude")),
        codex_home: Some(PathBuf::from("stores/codex")),
        opencode_db: Some(PathBuf::from("stores/opencode.db")),
        pi_agent_dir: Some(PathBuf::from("stores/pi")),
    };
    let field = |id| match id {
        HarnessId::ClaudeCode => roots.claude_config_dir.as_deref(),
        HarnessId::Codex => roots.codex_home.as_deref(),
        HarnessId::OpenCode => roots.opencode_db.as_deref(),
        HarnessId::Pi => roots.pi_agent_dir.as_deref(),
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

    for id in [HarnessId::ClaudeCode, HarnessId::Codex, HarnessId::Pi] {
        let mut roots = StoreRoots::default();
        get(id).set_store_root(&mut roots, &dir);
        assert_eq!(get(id).store_root(&roots), Some(dir.as_path()), "{id}");
    }
}

#[test]
fn the_store_env_names_the_root_or_clears_opencode() {
    let roots = StoreRoots {
        claude_config_dir: Some(PathBuf::from("stores/claude")),
        codex_home: Some(PathBuf::from("stores/codex")),
        opencode_db: Some(PathBuf::from("stores/opencode.db")),
        pi_agent_dir: Some(PathBuf::from("stores/pi")),
    };
    let variable = |id| match id {
        HarnessId::ClaudeCode => "CLAUDE_CONFIG_DIR",
        HarnessId::Codex => "CODEX_HOME",
        HarnessId::OpenCode => "OPENCODE_DB",
        HarnessId::Pi => "PI_CODING_AGENT_DIR",
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
    for id in HarnessId::ALL {
        let session = get(id)
            .read_file(&support::source_path(id))
            .unwrap_or_else(|error| panic!("{id}: {error:#}"));
        assert_eq!(session.meta.harness, id);
    }
}

#[test]
fn names_from_the_catalog_are_the_ones_users_type() {
    use mukade::catalog;
    let store = ["CLAUDE_CONFIG_DIR", "CODEX_HOME", "OPENCODE_DB", "PI_CODING_AGENT_DIR"];
    let exe_env = ["MUKADE_CLAUDE_EXE", "MUKADE_CODEX_EXE", "MUKADE_OPENCODE_EXE", "MUKADE_PI_EXE"];
    let flag = ["--claude-exe", "--codex-exe", "--opencode-exe", "--pi-exe"];
    let lineage = [false, false, true, true];
    for (n, id) in HarnessId::ALL.into_iter().enumerate() {
        assert_eq!(catalog::store_env(id), store[n], "{id}");
        assert_eq!(catalog::binary_env(id), exe_env[n], "{id}");
        assert_eq!(catalog::binary_flag(id), flag[n], "{id}");
        assert_eq!(catalog::lineage_recorded(id), lineage[n], "{id}");
        assert_eq!(mukade::exe::env_var_name(id), catalog::binary_env(id), "{id}");
    }
}

#[test]
fn the_usage_text_names_what_the_catalog_names() {
    for id in HarnessId::ALL {
        assert!(mukade::cli::USAGE.contains(mukade::catalog::binary_flag(id)), "{id}");
        assert!(mukade::cli::USAGE.contains(mukade::catalog::binary_env(id)), "{id}");
    }
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
    for id in HarnessId::ALL {
        for opts in &asks {
            let checked = get(id).validate_write(opts);
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
    for id in HarnessId::ALL {
        let (taken, refused) = if id == HarnessId::OpenCode {
            (opencode, uuid)
        } else {
            (uuid, opencode)
        };
        assert!(get(id).validate_write(&with_id(taken)).is_ok(), "{id} takes {taken}");
        assert!(get(id).validate_write(&with_id(refused)).is_err(), "{id} refuses {refused}");
    }
    assert!(mukade::writer::validate_id_syntax(uuid).is_ok());
    assert!(mukade::writer::validate_id_syntax(opencode).is_ok());
    assert!(mukade::writer::validate_id_syntax("../invalid").is_err());
}

#[test]
fn only_opencode_exports_an_interchange_document() {
    for id in HarnessId::ALL {
        assert_eq!(
            get(id).interchange_document().is_some(),
            id == HarnessId::OpenCode,
            "{id}"
        );
    }
}
