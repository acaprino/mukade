//! Scratch folders for unit tests, removed when the test that made one ends.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

/// A new, empty folder under the system's temporary folder, removed with everything in it when
/// dropped. Its name holds the process id and a counter, so tests running at once never share one.
pub(crate) struct Scratch(PathBuf);

impl Scratch {
    pub(crate) fn new(tag: &str) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let next = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("mukade-tui-{tag}-{}-{next}", std::process::id()));
        // A folder of an earlier process that had the same id.
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create the scratch folder");
        Scratch(dir)
    }
}

impl std::ops::Deref for Scratch {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.0
    }
}

impl AsRef<Path> for Scratch {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::Scratch;

    #[test]
    fn a_scratch_folder_goes_when_its_test_ends() {
        let (first, second) = (Scratch::new("guard"), Scratch::new("guard"));
        assert_ne!(first.to_path_buf(), second.to_path_buf(), "one folder each");
        std::fs::write(first.join("file.txt"), "x").unwrap();
        let path = first.to_path_buf();
        drop(first);
        assert!(!path.exists(), "{} is left behind", path.display());
    }
}
