//! Where the explorer finds sessions: each harness's standard store, and the installations a scan
//! found (portable or custom ones, with the engine kept beside each store).
//!
//! A location only carries what the library reported: recognizing an installation is
//! `mukade::detect`'s job, never this crate's (ADR-0001).

use mukade::catalog;
use mukade::detect::Installation;
use mukade::exe;
use mukade::model::HarnessId;
use mukade::roots::StoreRoots;
use std::path::{Path, PathBuf};

/// One place sessions live, with everything needed to read, resume and write there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Location {
    pub harness: HarnessId,
    /// What the harness column shows.
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
}

/// The four harnesses' standard stores, in the catalog's order.
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
            location.roots.root(installation.harness) == Some(&installation.store)
                && location.harness == installation.harness
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

/// Where a session at `source` can hop: every other location, the same harness elsewhere
/// included (a Claude Code session into a portable JanuraCode, an OpenCode one into another).
pub fn destinations(locations: &[Location], source: usize) -> Vec<Location> {
    locations
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != source)
        .map(|(_, location)| location.clone())
        .collect()
}

/// Scan each folder (through `mukade::detect`) and add the installations found. Answers a status
/// line for the explorer.
pub fn scan_folders(
    locations: &mut Vec<Location>,
    folders: &[PathBuf],
    standard: &StoreRoots,
) -> String {
    let mut added = 0;
    let mut truncated = false;
    for folder in folders {
        let scan = mukade::detect::scan_installations(folder, mukade::detect::DEFAULT_SCAN_DEPTH);
        truncated |= scan.truncated;
        added += add_installations(locations, &scan.installations, standard);
    }
    let names: Vec<String> = folders.iter().map(|f| f.display().to_string()).collect();
    format!(
        "{added} installation(s) added from {}{}",
        names.join(", "),
        if truncated {
            " · scan stopped early (depth or folder budget)"
        } else {
            ""
        }
    )
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

#[cfg(test)]
mod tests {
    use super::*;

    fn standard_roots() -> StoreRoots {
        StoreRoots {
            claude_config_dir: Some(PathBuf::from("home/.claude")),
            codex_home: Some(PathBuf::from("home/.codex")),
            opencode_db: Some(PathBuf::from("home/share/opencode/opencode.db")),
            pi_agent_dir: Some(PathBuf::from("home/.pi/agent")),
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
    fn every_harness_has_a_standard_location() {
        let locations = standard_locations(&standard_roots());
        let harnesses: Vec<HarnessId> = locations.iter().map(|l| l.harness).collect();
        assert_eq!(harnesses, HarnessId::ALL.to_vec());
        assert!(
            locations
                .iter()
                .all(|l| l.store.is_none() && l.exe.is_none())
        );
    }

    #[test]
    fn a_scanned_installation_is_a_location_with_its_own_store_and_engine() {
        let roots = standard_roots();
        let mut locations = standard_locations(&roots);
        let added = add_installations(&mut locations, &[januracode()], &roots);
        assert_eq!(added, 1);
        let found = locations.last().unwrap();
        assert_eq!(found.harness, HarnessId::OpenCode);
        assert_eq!(
            found.roots.root(HarnessId::OpenCode),
            Some(januracode().store.as_path())
        );
        // The other harnesses keep their standard stores.
        assert_eq!(
            found.roots.root(HarnessId::ClaudeCode),
            roots.root(HarnessId::ClaudeCode)
        );
        assert_eq!(
            found.exe.as_deref(),
            Some("apps/JanuraCode/resources/opencode-cli/opencode.exe")
        );
        assert!(found.label.contains("JanuraCode"), "{}", found.label);
    }

    #[test]
    fn a_store_already_listed_is_not_added_twice() {
        let roots = standard_roots();
        let mut locations = standard_locations(&roots);
        let standard_claude = Installation {
            harness: HarnessId::ClaudeCode,
            store: PathBuf::from("home/.claude"),
            exe: None,
            evidence: Vec::new(),
        };
        assert_eq!(
            add_installations(
                &mut locations,
                &[standard_claude, januracode(), januracode()],
                &roots
            ),
            1
        );
        assert_eq!(locations.len(), 5);
    }

    /// ADR-0001 INV-002: for the same folder the explorer lists exactly the installations the
    /// library scan (the one `mukade scan` prints) reports.
    #[test]
    fn a_folder_scan_lists_exactly_what_the_library_scan_reports() {
        let root = std::env::temp_dir().join(format!("mukade-tui-scan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let write = |relative: &str, bytes: &[u8]| {
            let path = root.join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bytes).unwrap();
        };
        write(
            "apps/JanuraCode/data/share/opencode/opencode.db",
            b"SQLite format 3\0rest of the file",
        );
        write(
            "apps/JanuraCode/resources/opencode-cli/opencode.exe",
            b"engine",
        );
        write(
            "home/.claude/projects/C--work/0c1d2e3f-0000-4000-8000-000000000009.jsonl",
            b"{}\n",
        );

        let library = mukade::detect::scan_installations(&root, mukade::detect::DEFAULT_SCAN_DEPTH);
        let mut locations = Vec::new();
        scan_folders(
            &mut locations,
            std::slice::from_ref(&root),
            &StoreRoots::default(),
        );
        let listed: Vec<(HarnessId, Option<PathBuf>)> = locations
            .iter()
            .map(|location| (location.harness, location.store.clone()))
            .collect();
        let reported: Vec<(HarnessId, Option<PathBuf>)> = library
            .installations
            .iter()
            .map(|found| (found.harness, Some(found.store.clone())))
            .collect();
        assert_eq!(listed, reported);
        assert_eq!(listed.len(), 2);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_hop_offers_every_other_location_including_the_same_harness_elsewhere() {
        let roots = standard_roots();
        let mut locations = standard_locations(&roots);
        add_installations(&mut locations, &[januracode()], &roots);
        let opencode = locations
            .iter()
            .position(|l| l.harness == HarnessId::OpenCode && l.store.is_none())
            .unwrap();
        let offered = destinations(&locations, opencode);
        assert_eq!(offered.len(), 4);
        assert!(
            !offered
                .iter()
                .any(|l| l.harness == HarnessId::OpenCode && l.store.is_none())
        );
        assert!(
            offered
                .iter()
                .any(|l| l.harness == HarnessId::OpenCode && l.store.is_some())
        );
    }
}
