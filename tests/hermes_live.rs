//! A read-only census of a real Hermes Agent home.
//!
//! Ignored by default; it reads `HERMES_HOME` (the Windows default when unset) and never writes:
//! `cargo test -j 4 --test hermes_live -- --ignored --nocapture`.

use mukade::model::{HarnessId, Kind};
use mukade::reader::{Reader, hermes};
use mukade::roots::StoreRoots;
use std::collections::BTreeMap;
use std::path::PathBuf;

fn home() -> Option<PathBuf> {
    std::env::var_os("HERMES_HOME")
        .or_else(|| {
            std::env::var_os("LOCALAPPDATA").map(|dir| PathBuf::from(dir).join("hermes").into())
        })
        .map(PathBuf::from)
}

#[test]
#[ignore = "reads the real Hermes store named by HERMES_HOME"]
fn every_session_of_the_real_store_reads() {
    let Some(home) = home().filter(|home| home.join("state.db").is_file()) else {
        eprintln!("no Hermes state.db: nothing to read");
        return;
    };
    let roots = StoreRoots {
        hermes_home: Some(home),
        ..StoreRoots::default()
    };
    let listed = hermes::Hermes.list(&roots).expect("the store lists");
    let all = hermes::Hermes
        .list_all(&roots)
        .expect("the store lists every session");
    let (mut entries, mut by_kind, mut tools) =
        (0, BTreeMap::<&str, usize>::new(), BTreeMap::new());
    for info in &all {
        let session = hermes::Hermes
            .read(&info.id, &roots)
            .unwrap_or_else(|error| panic!("{} must read: {error:#}", info.id));
        assert_eq!(session.meta.harness, HarnessId::Hermes);
        assert_eq!(session.meta.id, info.id);
        for entry in &session.entries {
            entries += 1;
            *by_kind.entry(mukade_kind(&entry.kind)).or_default() += 1;
            if let Kind::Step { calls, .. } = &entry.kind {
                for call in calls {
                    *tools.entry(call.source_name.clone()).or_insert(0usize) += 1;
                }
            }
        }
    }
    let counts = hermes::store_counts(&roots.hermes_state_db().unwrap()).unwrap();
    eprintln!(
        "listed {} (all {}); counts {counts:?}; {entries} entries {by_kind:?}; tools {tools:?}",
        listed.len(),
        all.len()
    );
}

fn mukade_kind(kind: &Kind) -> &'static str {
    match kind {
        Kind::User { .. } => "user",
        Kind::Step { .. } => "step",
        Kind::Results { .. } => "results",
        Kind::Compaction { .. } => "compaction",
        Kind::Meta { .. } => "meta",
    }
}

/// The real write: a Claude Code fixture hopped into the real Hermes store through `mukade hop`,
/// which runs `hermes sessions import`, then read back with mukade's own reader.
///
/// Ignored by default. It WRITES one session into the store named by `HERMES_HOME` (the Windows
/// default when unset); Hermes keeps it, and nothing else of that store is touched:
/// `cargo test -j 4 --test hermes_live -- --ignored --nocapture the_fixture_hops_into_the_real_store`.
#[test]
#[ignore = "writes one session into the real Hermes store named by HERMES_HOME, through hermes sessions import"]
fn the_fixture_hops_into_the_real_store_and_reads_back() {
    let Some(home) = home().filter(|home| home.join("state.db").is_file()) else {
        eprintln!("no Hermes state.db: nothing to write into");
        return;
    };
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/claude_code/source/c45db7bc-542c-4201-89de-b590c6fdb797.jsonl");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_mukade"))
        .args(["hop", "claude_code", "c45db7bc", "hermes", "--from-file"])
        .arg(&fixture)
        .env("HERMES_HOME", &home)
        .output()
        .expect("mukade runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    eprintln!(
        "exit {:?}\nstdout: {stdout}\nstderr: {stderr}",
        output.status.code()
    );
    assert!(output.status.success(), "the hop failed");
    let receipt: serde_json::Value =
        serde_json::from_str(stdout.lines().last().expect("a receipt line")).expect("JSON receipt");
    let id = receipt["id"].as_str().expect("the allocated id");
    assert_eq!(receipt["manifest"]["strict"]["checked"], true);

    let roots = StoreRoots {
        hermes_home: Some(home),
        ..StoreRoots::default()
    };
    let written = hermes::Hermes
        .read(id, &roots)
        .unwrap_or_else(|error| panic!("{id} must read back: {error:#}"));
    assert_eq!(written.meta.harness, HarnessId::Hermes);
    let source = mukade::harnesses::get(HarnessId::ClaudeCode)
        .read_file(&fixture)
        .expect("the fixture reads");

    let mut users = Vec::new();
    let mut assistants = Vec::new();
    for entry in written.conversation() {
        match &entry.kind {
            Kind::User { text, .. } => users.push(text.clone()),
            Kind::Step { text, .. } => assistants.extend(text.iter().cloned()),
            _ => {}
        }
    }
    let (users, assistants) = (users.join("\n"), assistants.join("\n"));
    eprintln!(
        "{id}: {} user chars, {} assistant chars",
        users.len(),
        assistants.len()
    );
    let mut checked = 0;
    for entry in source.conversation() {
        match &entry.kind {
            Kind::User { text, .. }
                if !text.trim().is_empty() && !text.trim_start().starts_with('<') =>
            {
                assert!(users.contains(text.trim()), "user text lost: {text:?}");
                checked += 1;
            }
            Kind::Step { text, .. } => {
                for part in text.iter().filter(|part| !part.trim().is_empty()) {
                    assert!(
                        assistants.contains(part.trim()),
                        "assistant text lost: {part:?}"
                    );
                    checked += 1;
                }
            }
            _ => {}
        }
    }
    assert!(
        checked >= 2,
        "the fixture gave only {checked} texts to compare"
    );
    eprintln!("{checked} texts of the fixture found in {id}");
}
