//! Resolve a path a command was handed (`--root`, `--dest-root`) to one installation.

use super::scan::{DEFAULT_SCAN_DEPTH, Installation, scan_installations};
use crate::model::HarnessId;
use std::path::Path;

/// What a path handed to a command names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Located {
    /// A store, or a folder that holds exactly one installation.
    One(Installation),
    /// Nothing recognized: the path itself, nor anything below it.
    Nothing,
    /// More than one candidate: the caller must name the harness or one of the stores.
    Several(Vec<Installation>),
}

/// The installation `path` names, of `harness` when one is given. The path itself wins: a store
/// is that store, even when other installations sit below it. Otherwise the path is scanned and
/// must hold exactly one installation.
pub fn locate_installation(path: &Path, harness: Option<HarnessId>) -> Located {
    let of_harness = |depth: usize| -> Vec<Installation> {
        scan_installations(path, depth)
            .installations
            .into_iter()
            .filter(|found| harness.is_none_or(|harness| found.harness == harness))
            .collect()
    };
    let mut found = of_harness(0);
    if found.is_empty() {
        found = of_harness(DEFAULT_SCAN_DEPTH);
    }
    match found.len() {
        0 => Located::Nothing,
        1 => Located::One(found.remove(0)),
        _ => Located::Several(found),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detect::tests::{scratch, touch};

    fn claude_store(dir: &Path) {
        touch(
            &dir.join("projects/C--work/0c1d2e3f-0000-4000-8000-000000000003.jsonl"),
            b"{}\n",
        );
    }

    #[test]
    fn a_store_names_itself_even_with_installations_below_it() {
        let root = scratch("locate-self");
        claude_store(&root);
        claude_store(&root.join("nested/.claude"));
        match locate_installation(&root, None) {
            Located::One(found) => assert_eq!(found.store, root),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_folder_names_the_only_installation_below_it() {
        let root = scratch("locate-below");
        claude_store(&root.join("home/.claude"));
        match locate_installation(&root, Some(HarnessId::ClaudeCode)) {
            Located::One(found) => assert_eq!(found.store, root.join("home/.claude")),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn several_installations_are_reported_and_a_harness_narrows_them() {
        let root = scratch("locate-several");
        claude_store(&root.join("a/.claude"));
        claude_store(&root.join("b/.claude"));
        touch(
            &root.join("c/.pi/agent/models.json"),
            br#"{"providers":{}}"#,
        );
        assert!(matches!(
            locate_installation(&root, Some(HarnessId::ClaudeCode)),
            Located::Several(found) if found.len() == 2
        ));
        assert!(matches!(
            locate_installation(&root, Some(HarnessId::Pi)),
            Located::One(_)
        ));
        assert_eq!(
            locate_installation(&root, Some(HarnessId::Codex)),
            Located::Nothing
        );
    }
}
