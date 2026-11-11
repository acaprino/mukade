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
