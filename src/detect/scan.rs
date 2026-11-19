//! The installation scan: walk a directory tree and report every harness installation in it,
//! portable and custom ones included, through the same store layout adapters as detection.

use super::engine::engine_beside_store;
use super::folder::Folder;
use super::layout::{StoreLayout, layouts};
use crate::model::HarnessId;
use std::path::{Path, PathBuf};

/// How many folder levels below the scanned directory a scan descends by default. A portable
/// JanuraCode copy keeps its database's folder six levels below the folder that holds the app.
pub const DEFAULT_SCAN_DEPTH: usize = 8;

/// The most folders one scan reads: a whole disk must end in bounded time.
const FOLDER_BUDGET: usize = 100_000;

/// Folders that never hold a harness installation and can be huge. Compared ignoring case.
const SKIPPED_FOLDERS: &[&str] = &[
    "node_modules",
    ".git",
    "target",
    "__pycache__",
    ".venv",
    "$RECYCLE.BIN",
    "System Volume Information",
];

/// A harness installation: its store, as the harness's own variable names it, and the executable
/// kept beside it, when there is one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installation {
    pub harness: HarnessId,
    /// A folder, or OpenCode's database file.
    pub store: PathBuf,
    /// The harness's executable beside the store; `None` when the installation keeps none there
    /// and the standard resolution applies.
    pub exe: Option<PathBuf>,
    pub evidence: Vec<String>,
}

impl Installation {
    fn new(layout: &dyn StoreLayout, store: PathBuf, evidence: Vec<String>) -> Self {
        Installation {
            harness: layout.harness(),
            exe: engine_beside_store(layout.harness(), &store),
            store,
            evidence,
        }
    }
}

/// What a scan found, and whether it saw everything.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Scan {
    /// Ordered by store path.
    pub installations: Vec<Installation>,
    /// Folders read.
    pub visited: usize,
    /// The depth or the folder budget stopped the walk before the whole tree was read.
    pub truncated: bool,
}

/// Every harness installation under `root`. The walk reads at most `max_depth` folder levels
/// below `root`, breadth first, never follows links or junctions, skips dependency and build
/// folders, and does not descend into a store's own session folders. It only reads.
pub fn scan_installations(root: &Path, max_depth: usize) -> Scan {
    let mut scan = Scan::default();
    if root.is_file() {
        scan.installations = layouts()
            .into_iter()
            .filter_map(|layout| {
                let evidence = layout.file_evidence(root);
                (!evidence.is_empty())
                    .then(|| Installation::new(layout, root.to_path_buf(), evidence))
            })
            .collect();
        return scan;
    }
    let mut queue = std::collections::VecDeque::from([(root.to_path_buf(), 0usize)]);
    while let Some((path, depth)) = queue.pop_front() {
        if scan.visited >= FOLDER_BUDGET {
            scan.truncated = true;
            break;
        }
        scan.visited += 1;
        let folder = Folder::read(&path);
        let mut session_folders: Vec<&str> = Vec::new();
        for layout in layouts() {
            let evidence = layout.evidence(&folder);
            if evidence.is_empty() {
                continue;
            }
            session_folders.extend(layout.session_folders());
            scan.installations
                .push(Installation::new(layout, layout.store(&path), evidence));
        }
        let children: Vec<PathBuf> = folder
            .subfolders()
            .into_iter()
            .filter(|(name, _)| {
                !session_folders.contains(name)
                    && !SKIPPED_FOLDERS
                        .iter()
                        .any(|skipped| skipped.eq_ignore_ascii_case(name))
            })
            .map(|(_, child)| child)
            .collect();
        if depth >= max_depth {
            scan.truncated |= !children.is_empty();
            continue;
        }
        queue.extend(children.into_iter().map(|child| (child, depth + 1)));
    }
    scan.installations
        .sort_by(|left, right| left.store.cmp(&right.store));
    scan
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detect::tests::{scratch, touch};

    /// A portable JanuraCode copy: OpenCode's database under the app's XDG data folder, the
    /// engine in the Electron resources, and a marker beside the data.
    fn portable_januracode(app: &Path) -> PathBuf {
        touch(&app.join("data/.portable"), b"");
        touch(
            &app.join("data/share/opencode/opencode.db"),
            b"SQLite format 3\0rest of the file",
        );
        let engine = app.join(if cfg!(windows) {
            "resources/opencode-cli/opencode.exe"
        } else {
            "resources/opencode-cli/opencode"
        });
        touch(&engine, b"engine");
        engine
    }

    #[test]
    fn a_scan_finds_installations_nested_below_the_directory() {
        let root = scratch("scan-nested");
        touch(
            &root.join("home/.claude/projects/C--work/0c1d2e3f-0000-4000-8000-000000000001.jsonl"),
            b"{}\n",
        );
        touch(
            &root.join("home/.pi/agent/sessions/--C--work--/stamp_id.jsonl"),
            b"{}\n",
        );
        portable_januracode(&root.join("apps/JanuraCode"));

        let scan = scan_installations(&root, DEFAULT_SCAN_DEPTH);
        let found: Vec<(HarnessId, PathBuf)> = scan
            .installations
            .iter()
            .map(|found| (found.harness, found.store.clone()))
            .collect();
        assert_eq!(
            found,
            vec![
                (
                    HarnessId::OpenCode,
                    root.join("apps/JanuraCode/data/share/opencode/opencode.db")
                ),
                (HarnessId::ClaudeCode, root.join("home/.claude")),
                (HarnessId::Pi, root.join("home/.pi/agent")),
            ]
        );
        assert!(!scan.truncated);
    }

    #[test]
    fn a_portable_installation_carries_the_engine_beside_its_store() {
        let root = scratch("scan-engine");
        let engine = portable_januracode(&root.join("JanuraCode"));
        let scan = scan_installations(&root, DEFAULT_SCAN_DEPTH);
        assert_eq!(scan.installations.len(), 1);
        assert_eq!(scan.installations[0].exe.as_deref(), Some(engine.as_path()));
    }

    #[test]
    fn a_store_without_an_engine_nearby_has_none() {
        let root = scratch("scan-no-engine");
        touch(
            &root.join(".claude/projects/C--work/0c1d2e3f-0000-4000-8000-000000000002.jsonl"),
            b"{}\n",
        );
        let scan = scan_installations(&root, DEFAULT_SCAN_DEPTH);
        assert_eq!(scan.installations.len(), 1);
        assert_eq!(scan.installations[0].exe, None);
    }

    #[test]
    fn a_scan_stops_at_its_depth_and_says_so() {
        let root = scratch("scan-depth");
        portable_januracode(&root.join("a/b/JanuraCode"));
        // The database's folder is six levels below the root: a depth of 3 cannot reach it.
        let shallow = scan_installations(&root, 3);
        assert!(shallow.installations.is_empty());
        assert!(shallow.truncated);
        let deep = scan_installations(&root, DEFAULT_SCAN_DEPTH);
        assert_eq!(deep.installations.len(), 1);
    }

    #[test]
    fn a_scan_skips_dependency_and_build_folders() {
        let root = scratch("scan-skip");
        portable_januracode(&root.join("node_modules/pkg"));
        portable_januracode(&root.join("target/release"));
        portable_januracode(&root.join(".git/x"));
        assert!(
            scan_installations(&root, DEFAULT_SCAN_DEPTH)
                .installations
                .is_empty()
        );
    }

    #[test]
    fn a_database_file_is_scanned_as_its_own_installation() {
        let root = scratch("scan-file");
        let engine = portable_januracode(&root.join("JanuraCode"));
        let db = root.join("JanuraCode/data/share/opencode/opencode.db");
        let scan = scan_installations(&db, DEFAULT_SCAN_DEPTH);
        assert_eq!(scan.installations.len(), 1);
        assert_eq!(scan.installations[0].store, db);
        assert_eq!(scan.installations[0].exe.as_deref(), Some(engine.as_path()));
    }
}
