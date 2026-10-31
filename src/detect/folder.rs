//! One folder as the store layouts see it: read once, its entries compared by their exact names.

use std::path::{Path, PathBuf};

/// A folder's entries, read once. Names are compared exactly: `Path::join(..).is_dir()` ignores
/// case on Windows, and `D:\Projects` is not Claude Code's `projects/`.
pub(crate) struct Folder {
    path: PathBuf,
    entries: Vec<Entry>,
}

struct Entry {
    name: String,
    /// A folder, not a link to one: `file_type` does not follow symbolic links or junctions.
    is_folder: bool,
    is_file: bool,
}

impl Folder {
    /// The folder's entries; an unreadable folder has none.
    pub(crate) fn read(path: &Path) -> Self {
        let entries = std::fs::read_dir(path)
            .map(|entries| {
                entries
                    .flatten()
                    .map(|entry| {
                        let kind = entry.file_type().ok();
                        Entry {
                            name: entry.file_name().to_string_lossy().into_owned(),
                            is_folder: kind.is_some_and(|kind| kind.is_dir()),
                            is_file: kind.is_some_and(|kind| kind.is_file()),
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        Folder {
            path: path.to_path_buf(),
            entries,
        }
    }

    /// The child folder with exactly this name, when there is one.
    pub(crate) fn folder(&self, name: &str) -> Option<PathBuf> {
        self.entries
            .iter()
            .any(|entry| entry.is_folder && entry.name == name)
            .then(|| self.path.join(name))
    }

    /// The child file with exactly this name, when there is one.
    pub(crate) fn file(&self, name: &str) -> Option<PathBuf> {
        self.entries
            .iter()
            .any(|entry| entry.is_file && entry.name == name)
            .then(|| self.path.join(name))
    }

    /// The child folders, by name, in name order.
    pub(crate) fn subfolders(&self) -> Vec<(&str, PathBuf)> {
        let mut out: Vec<(&str, PathBuf)> = self
            .entries
            .iter()
            .filter(|entry| entry.is_folder)
            .map(|entry| (entry.name.as_str(), self.path.join(&entry.name)))
            .collect();
        out.sort_by(|left, right| left.0.cmp(right.0));
        out
    }
}

/// Files under `dir` whose name matches, walking at most `depth` levels and a bounded number of
/// entries: recognition must stay cheap even on a large store.
pub(crate) fn count_files(dir: &Path, depth: usize, matches: &dyn Fn(&str) -> bool) -> usize {
    fn walk(dir: &Path, depth: usize, matches: &dyn Fn(&str) -> bool, budget: &mut usize) -> usize {
        if depth == 0 || *budget == 0 {
            return 0;
        }
        let mut found = 0;
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                *budget = budget.saturating_sub(1);
                if *budget == 0 {
                    break;
                }
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().into_owned();
                if path.is_dir() {
                    found += walk(&path, depth - 1, matches, budget);
                } else if matches(&name) {
                    found += 1;
                }
            }
        }
        found
    }
    walk(dir, depth, matches, &mut 10_000)
}

/// The 16-byte header every SQLite database starts with.
pub(crate) fn is_sqlite(path: &Path) -> bool {
    use std::io::Read;
    let mut header = [0u8; 16];
    std::fs::File::open(path)
        .and_then(|mut file| file.read_exact(&mut header))
        .is_ok_and(|_| &header == b"SQLite format 3\0")
}
