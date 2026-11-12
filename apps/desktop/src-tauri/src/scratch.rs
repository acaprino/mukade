//! Scratch folders for unit tests, removed when the test that made one ends. Under
//! `MUKADE_TEST_SCRATCH_ROOT` when the variable names a folder, else the system's temporary one.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

pub(crate) struct Scratch(PathBuf);

impl Scratch {
    pub(crate) fn new(tag: &str) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let next = NEXT.fetch_add(1, Ordering::Relaxed);
        let root = std::env::var_os("MUKADE_TEST_SCRATCH_ROOT")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let dir = root.join(format!(
            "mukade-desktop-{tag}-{}-{next}",
            std::process::id()
        ));
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

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
