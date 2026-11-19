//! A read-only census of a real DeepSeek Harness home.
//!
//! Ignored by default; it reads `DSH_HOME` (`~/.dsh` when unset) and never writes:
//! `cargo test -j 4 --test deepseek_live -- --ignored --nocapture`.

use mukade::model::{HarnessId, Kind};
use mukade::reader::{Reader, deepseek_harness as dsh};
use mukade::roots::StoreRoots;
use std::collections::BTreeMap;
use std::path::PathBuf;

fn home() -> Option<PathBuf> {
    std::env::var_os("DSH_HOME").map(PathBuf::from).or_else(|| {
        std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(|home| PathBuf::from(home).join(".dsh"))
    })
}

#[test]
#[ignore = "reads the real DeepSeek Harness store named by DSH_HOME"]
fn every_session_of_the_real_store_reads() {
    let Some(home) = home().filter(|home| home.join("sessions").is_dir()) else {
        eprintln!("no DeepSeek Harness sessions folder: nothing to read");
        return;
    };
    let roots = StoreRoots {
        dsh_home: Some(home),
        ..StoreRoots::default()
    };
    let listed = dsh::DeepSeekHarness.list(&roots).expect("the store lists");
    let all = dsh::DeepSeekHarness
        .list_all(&roots)
        .expect("the store lists every session");
    let (mut entries, mut by_kind, mut tools) =
        (0, BTreeMap::<&str, usize>::new(), BTreeMap::new());
    for info in &all {
        let session = dsh::DeepSeekHarness
            .read(&info.id, &roots)
            .unwrap_or_else(|error| panic!("{} must read: {error:#}", info.id));
        assert_eq!(session.meta.harness, HarnessId::DeepSeekHarness);
        assert_eq!(session.meta.id, info.id);
        assert_eq!(session.meta.parent, info.parent);
        for entry in &session.entries {
            entries += 1;
            *by_kind.entry(kind_name(&entry.kind)).or_default() += 1;
            if let Kind::Step { calls, .. } = &entry.kind {
                for call in calls {
                    *tools.entry(call.source_name.clone()).or_insert(0usize) += 1;
                }
            }
        }
    }
    let counts = dsh::store_counts(&roots.dsh_sessions().unwrap()).unwrap();
    eprintln!(
        "listed {} (all {}); counts {counts:?}; {entries} entries {by_kind:?}; tools {tools:?}",
        listed.len(),
        all.len()
    );
}

fn kind_name(kind: &Kind) -> &'static str {
    match kind {
        Kind::User { .. } => "user",
        Kind::Step { .. } => "step",
        Kind::Results { .. } => "results",
        Kind::Compaction { .. } => "compaction",
        Kind::Meta { .. } => "meta",
    }
}
