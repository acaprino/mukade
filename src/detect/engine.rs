//! The executable a self-contained (portable or custom) installation keeps beside its store.

use crate::model::HarnessId;
use std::path::{Path, PathBuf};

/// How many folders above a store's folder the executable is looked for: a portable JanuraCode
/// keeps it three levels above its database's folder (`<app>/data/share/opencode/opencode.db`,
/// `<app>/resources/opencode-cli/opencode.exe`).
const SEARCH_LEVELS: usize = 3;

/// The harness's executable kept beside a store: the catalog's `beside_store` paths, tried from
/// the store's folder and its nearest ancestors. `None` when no such file exists: the standard
/// resolution then applies.
pub fn engine_beside_store(harness: HarnessId, store: &Path) -> Option<PathBuf> {
    let start = if store.is_file() {
        store.parent()?
    } else {
        store
    };
    let candidates = crate::catalog::engines_beside_store(harness, crate::catalog::platform());
    start.ancestors().take(SEARCH_LEVELS + 1).find_map(|dir| {
        candidates
            .iter()
            .map(|relative| beneath(dir, relative))
            .find(|candidate| candidate.is_file())
    })
}

/// `dir` joined with a catalog path written with `/`, one component at a time, so the result uses
/// the platform's own separator.
fn beneath(dir: &Path, relative: &str) -> PathBuf {
    relative
        .split('/')
        .filter(|part| !part.is_empty())
        .fold(dir.to_path_buf(), |path, part| path.join(part))
}
