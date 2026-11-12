//! Where the app finds sessions: each harness's standard store, and the installations a scan found
//! (portable or custom ones, with the engine kept beside each store).
//!
//! A location only carries what the library reported: recognizing an installation is
//! `mukade::detect`'s job, never this crate's (ADR-0001).

use mukade::catalog;
use mukade::detect::Installation;
use mukade::exe;
use mukade::model::HarnessId;
use mukade::roots::StoreRoots;
use serde::Serialize;
use std::path::{Path, PathBuf};

/// One place sessions live, with everything needed to read, resume and write there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Location {
    pub harness: HarnessId,
    /// What the installation column shows.
    pub label: String,
    /// The stores this location reads and writes: its own for its harness, the standard ones for
    /// the others.
    pub roots: StoreRoots,
    /// The installation's own executable; `None` means the harness's standard resolution.
    pub exe: Option<String>,
    /// The store a scan found; `None` for the harness's standard store.
    pub store: Option<PathBuf>,
}

impl Location {
    /// The executable that opens and writes this location's sessions.
    pub fn engine(&self) -> String {
        self.exe
            .clone()
            .unwrap_or_else(|| exe::resolve(self.harness, None))
    }

    /// What names this location to the interface: the harness for its standard store, the harness
    /// and the store for a scanned one.
    pub fn key(&self) -> String {
        match &self.store {
            None => self.harness.as_str().to_string(),
            Some(store) => format!("{}@{}", self.harness.as_str(), store.display()),
        }
    }

    pub fn is_destination(&self) -> bool {
        mukade::harnesses::get(self.harness).destination().is_some()
    }

    /// Whether a session here can be read, and hopped, with every branch it keeps.
    pub fn reads_all_branches(&self) -> bool {
        mukade::hop::reads_all_branches(self.harness)
    }
}

/// How an installation reaches the interface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InstallationView {
    pub key: String,
    pub harness: &'static str,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store: Option<String>,
    /// Shown for a scanned installation, which names its own engine.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub engine: Option<String>,
    pub scanned: bool,
    /// Whether mukade writes this harness: a hop or an import can land here.
    pub destination: bool,
    /// Whether a session here can be hopped with every branch it keeps (the hop's all-branches
    /// option, `HopRequest::all_branches`), as the library answers.
    pub all_branches: bool,
}

impl From<&Location> for InstallationView {
    fn from(location: &Location) -> Self {
        Self {
            key: location.key(),
            harness: location.harness.as_str(),
            label: location.label.clone(),
            store: location
                .store
                .as_ref()
                .map(|store| store.display().to_string()),
            engine: location
                .store
                .as_ref()
                .map(|_| location.exe.clone().unwrap_or_else(|| "(standard resolution)".into())),
            scanned: location.store.is_some(),
            destination: location.is_destination(),
            all_branches: location.reads_all_branches(),
        }
    }
}

/// The stores this process's variables name, each missing one at the catalog's default location.
pub fn standard_roots() -> StoreRoots {
    let mut roots = StoreRoots::from_env();
    for harness in mukade::harnesses::all() {
        let root = harness.store_root_mut(&mut roots);
        if root.is_none() {
            *root = catalog::store_default(harness.id());
        }
    }
    roots
}

/// Every harness's standard store, in the catalog's order.
pub fn standard_locations(roots: &StoreRoots) -> Vec<Location> {
    HarnessId::ALL
        .into_iter()
        .map(|harness| Location {
            harness,
            label: catalog::entry(harness).display.clone(),
            roots: roots.clone(),
            exe: None,
            store: None,
        })
        .collect()
}

/// Append a location for each installation whose store is not already listed. Answers how many
/// were added.
pub fn add_installations(
    locations: &mut Vec<Location>,
    found: &[Installation],
    standard: &StoreRoots,
) -> usize {
    let mut added = 0;
    for installation in found {
        let listed = locations.iter().any(|location| {
            location.harness == installation.harness
                && location.roots.root(installation.harness) == Some(&installation.store)
        });
        if listed {
            continue;
        }
        let mut roots = standard.clone();
        roots.set_root(installation.harness, &installation.store);
        locations.push(Location {
            harness: installation.harness,
            label: format!(
                "{} · {}",
                catalog::entry(installation.harness).display,
                installation_name(installation)
            ),
            roots,
            exe: installation
                .exe
                .as_ref()
                .map(|exe| exe.to_string_lossy().into_owned()),
            store: Some(installation.store.clone()),
        });
        added += 1;
    }
    added
}

/// What scanning one folder came to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ScanNote {
    pub dir: String,
    pub added: usize,
    /// The scan hit its depth or folder budget before it had looked everywhere.
    pub stopped_early: bool,
}

/// Scan each folder (through `mukade::detect`) and add the installations found.
pub fn scan_folders(
    locations: &mut Vec<Location>,
    folders: &[PathBuf],
    standard: &StoreRoots,
) -> Vec<ScanNote> {
    folders
        .iter()
        .map(|folder| {
            let scan =
                mukade::detect::scan_installations(folder, mukade::detect::DEFAULT_SCAN_DEPTH);
            ScanNote {
                dir: folder.display().to_string(),
                added: add_installations(locations, &scan.installations, standard),
                stopped_early: scan.truncated,
            }
        })
        .collect()
}

/// A short name for an installation: the folder that holds both its store and its engine (a
/// portable app's own folder), else the store's folder.
fn installation_name(installation: &Installation) -> String {
    let store_folder = if catalog::store_is_file(installation.harness) {
        installation.store.parent()
    } else {
        Some(installation.store.as_path())
    };
    let shared = installation.exe.as_deref().and_then(|exe| {
        exe.ancestors()
            .skip(1)
            .find(|ancestor| installation.store.starts_with(ancestor))
    });
    shared
        .or(store_folder)
        .and_then(Path::file_name)
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| installation.store.display().to_string())
}

/// Where a session at `source` can hop: every other location whose harness mukade writes, the same
/// harness elsewhere included (a Claude Code session into a portable JanuraCode, an OpenCode one
/// into another).
pub fn destinations<'a>(locations: &'a [Location], source: &Location) -> Vec<&'a Location> {
    locations
        .iter()
        .filter(|location| location.key() != source.key() && location.is_destination())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn standard() -> StoreRoots {
        StoreRoots {
            claude_config_dir: Some(PathBuf::from("home/.claude")),
            codex_home: Some(PathBuf::from("home/.codex")),
            opencode_db: Some(PathBuf::from("home/share/opencode/opencode.db")),
            pi_agent_dir: Some(PathBuf::from("home/.pi/agent")),
            hermes_home: Some(PathBuf::from("home/.hermes")),
            dsh_home: Some(PathBuf::from("home/.dsh")),
        }
    }

    fn januracode() -> Installation {
        Installation {
            harness: HarnessId::OpenCode,
            store: PathBuf::from("apps/JanuraCode/data/share/opencode/opencode.db"),
            exe: Some(PathBuf::from(
                "apps/JanuraCode/resources/opencode-cli/opencode.exe",
            )),
            evidence: vec!["opencode.db is a SQLite database".into()],
        }
    }

    #[test]
    fn every_harness_has_a_standard_location_keyed_by_its_name() {
        let locations = standard_locations(&standard());
        let harnesses: Vec<HarnessId> = locations.iter().map(|l| l.harness).collect();
        assert_eq!(harnesses, HarnessId::ALL.to_vec());
        assert!(locations.iter().all(|l| l.store.is_none() && l.exe.is_none()));
        assert_eq!(locations[0].key(), HarnessId::ALL[0].as_str());
        let keys: std::collections::HashSet<String> = locations.iter().map(Location::key).collect();
        assert_eq!(keys.len(), locations.len(), "keys are unique");
    }

    #[test]
    fn an_installation_says_whether_its_sessions_hop_with_every_branch() {
        for location in standard_locations(&standard()) {
            let view = InstallationView::from(&location);
            assert_eq!(view.all_branches, mukade::hop::reads_all_branches(location.harness), "{}", view.key);
        }
        let view = |harness: HarnessId| {
            let locations = standard_locations(&standard());
            let found = locations.iter().find(|l| l.harness == harness).unwrap();
            InstallationView::from(found).all_branches
        };
        assert!(view(HarnessId::ClaudeCode));
        assert!(!view(HarnessId::OpenCode));
    }

    #[test]
    fn a_scanned_installation_keeps_its_own_store_and_engine_and_the_standard_other_stores() {
        let roots = standard();
        let mut locations = standard_locations(&roots);
        assert_eq!(add_installations(&mut locations, &[januracode()], &roots), 1);
        let found = locations.last().unwrap();
        assert_eq!(found.harness, HarnessId::OpenCode);
        assert_eq!(found.roots.root(HarnessId::OpenCode), Some(januracode().store.as_path()));
        assert_eq!(found.roots.root(HarnessId::Pi), roots.root(HarnessId::Pi));
        assert!(found.label.contains("JanuraCode"), "{}", found.label);
        assert!(found.key().contains('@'), "{}", found.key());
        let view = InstallationView::from(found);
        assert!(view.scanned);
        assert!(view.engine.as_deref().unwrap().contains("opencode.exe"));
        assert!(InstallationView::from(&locations[0]).engine.is_none());
    }

    #[test]
    fn an_installation_already_listed_is_not_added_twice() {
        let roots = standard();
        let mut locations = standard_locations(&roots);
        let same_as_standard = Installation {
            harness: HarnessId::Pi,
            store: roots.root(HarnessId::Pi).unwrap().to_path_buf(),
            exe: None,
            evidence: Vec::new(),
        };
        assert_eq!(
            add_installations(&mut locations, &[same_as_standard, januracode(), januracode()], &roots),
            1
        );
        assert_eq!(locations.len(), HarnessId::ALL.len() + 1);
    }

    #[test]
    fn destinations_are_every_other_location_of_a_harness_mukade_writes() {
        let roots = standard();
        let mut locations = standard_locations(&roots);
        add_installations(&mut locations, &[januracode()], &roots);
        let claude = &locations[0];
        let found = destinations(&locations, claude);
        assert!(found.iter().all(|l| l.key() != claude.key()));
        assert!(found.iter().all(|l| l.is_destination()));
        // The same harness elsewhere is a destination: a hop from the standard OpenCode into the
        // portable one.
        let standard_opencode = locations.iter().find(|l| l.harness == HarnessId::OpenCode && l.store.is_none()).unwrap();
        assert!(destinations(&locations, standard_opencode).iter().any(|l| l.store.is_some()));
        // A source-only harness is never offered.
        assert!(found.iter().all(|l| mukade::harnesses::get(l.harness).destination().is_some()));
    }
}
