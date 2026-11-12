//! Scratch folders for unit tests, removed when the test that made one ends.

use std::path::{Path, PathBuf};

/// A new, empty folder, removed with everything in it when dropped.
pub(crate) struct Scratch(PathBuf);

impl Scratch {
    /// A folder under the system's temporary folder, its name built from `tag`.
    pub(crate) fn new(tag: &str) -> Self {
        Self::under(&std::env::temp_dir(), tag)
    }

    /// A folder below this process's working folder (`target/tmp`), for a test that names it by
    /// a relative path ([`Scratch::relative`]).
    pub(crate) fn in_working_dir(tag: &str) -> Self {
        let working = std::env::current_dir().expect("a working folder");
        Self::under(&working.join("target").join("tmp"), tag)
    }

    fn under(parent: &Path, tag: &str) -> Self {
        // Twelve hex digits keep the path short for cmd.exe while staying unique across parallel tests.
        let id = uuid::Uuid::new_v4().simple().to_string();
        let dir = parent.join(format!("mukade-{tag}-{}", &id[..12]));
        std::fs::create_dir_all(&dir).expect("create the scratch folder");
        Scratch(dir)
    }

    /// This folder relative to the process's working folder.
    pub(crate) fn relative(&self) -> PathBuf {
        let working = std::env::current_dir().expect("a working folder");
        self.0
            .strip_prefix(&working)
            .expect("made in the working folder")
            .to_path_buf()
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
        let path = {
            let scratch = Scratch::new("guard");
            std::fs::write(scratch.join("file.txt"), "x").unwrap();
            assert!(scratch.is_dir());
            scratch.to_path_buf()
        };
        assert!(!path.exists(), "{} is left behind", path.display());

        let scratch = Scratch::in_working_dir("guard-relative");
        assert!(scratch.relative().is_relative());
        assert!(
            scratch.relative().is_dir(),
            "read against the working folder"
        );
    }
}
