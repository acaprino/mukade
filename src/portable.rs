//! mukade is portable by default: it runs from any folder, a USB stick included, and keeps its
//! own files beside the executable. A host can set an absolute `MUKADE_DATA_DIR` instead.
//!
//! Its cache lives in `cache`, its temporary files in `tmp`, under that selected data folder. The
//! default folder is `mukade-data` beside the executable. The folder is made when something is
//! first written, never at start-up. What mukade writes outside that folder is what it was asked
//! to write: the session in the destination harness's store, and the files named by `--output` and
//! `--manifest`.
//!
//! Where the selected folder cannot be made, mukade does not look for another place: the cache is
//! simply not kept, and the temporary files that must exist, the
//! document OpenCode's `session import` reads and the folder of a summarizing run
//! (`crate::summary`), are an error that says so. The system's
//! temporary folder, the user profile and the application-data folders are never used. A
//! temporary file is removed when its work is done, and one a killed process left behind is
//! removed by the next run once it is an hour old ([`sweep_stale`]).

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const DATA_DIR: &str = "mukade-data";

/// A temporary file this old belongs to a process that is gone.
pub const STALE_AFTER: Duration = Duration::from_secs(60 * 60);

/// The host's explicitly selected data folder, or the portable folder beside the executable.
/// An invalid override is refused rather than silently writing to another location.
pub fn data_dir() -> Option<PathBuf> {
    if let Some(value) = std::env::var_os("MUKADE_DATA_DIR") {
        let path = PathBuf::from(value);
        return path.is_absolute().then_some(path);
    }
    let exe = std::env::current_exe().ok()?;
    data_dir_in(exe.parent())
}

/// [`data_dir`] for a given executable folder.
pub fn data_dir_in(exe_dir: Option<&Path>) -> Option<PathBuf> {
    exe_dir.map(|dir| dir.join(DATA_DIR))
}

/// Where the cache of what harness binaries told us lives: `mukade-data/cache`.
pub fn cache_dir() -> Option<PathBuf> {
    cache_dir_in(data_dir().as_deref())
}

pub fn cache_dir_in(data: Option<&Path>) -> Option<PathBuf> {
    data.map(|dir| dir.join("cache"))
}

/// Where mukade's temporary files go: `mukade-data/tmp`, made now. An error, naming the folder,
/// when it cannot be made.
pub fn temp_dir() -> std::io::Result<PathBuf> {
    temp_dir_in(data_dir().as_deref())
}

pub fn temp_dir_in(data: Option<&Path>) -> std::io::Result<PathBuf> {
    let Some(data) = data else {
        return Err(std::io::Error::other(
            "mukade cannot determine its data directory: MUKADE_DATA_DIR must be absolute",
        ));
    };
    let dir = data.join("tmp");
    std::fs::create_dir_all(&dir).map_err(|error| {
        std::io::Error::new(
            error.kind(),
            format!(
                "mukade could not create its temporary directory {} ({error}): select a writable MUKADE_DATA_DIR or run from a writable folder",
                dir.display()
            ),
        )
    })?;
    Ok(dir)
}

/// Remove files in `dir` that mukade left behind: names that start with `prefix` and end with
/// `suffix`, last modified more than `older_than` ago. Nothing else is touched, and nothing here
/// can fail the caller.
pub fn sweep_stale(dir: &Path, prefix: &str, suffix: &str, older_than: Duration) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with(prefix) || !name.ends_with(suffix) {
            continue;
        }
        let stale = entry
            .metadata()
            .ok()
            .filter(|meta| meta.is_file())
            .and_then(|meta| meta.modified().ok())
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age > older_than);
        if stale {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("mukade-portable-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// There is no mode to switch on: the folder next to the executable is where mukade keeps
    /// what it keeps, whatever else sits there and whatever the environment says.
    #[test]
    fn the_data_folder_is_always_beside_the_executable() {
        let exe = scratch("always");
        assert_eq!(data_dir_in(Some(&exe)), Some(exe.join(DATA_DIR)));
        assert_eq!(data_dir_in(None), None, "an executable with no folder");
        assert!(!exe.join(DATA_DIR).exists(), "looking does not create");
        let _ = std::fs::remove_dir_all(&exe);
    }

    #[test]
    fn the_cache_is_in_the_data_folder() {
        let data = PathBuf::from("data");
        assert_eq!(cache_dir_in(Some(&data)), Some(data.join("cache")));
        assert_eq!(cache_dir_in(None), None);
    }

    /// Temporary files go beside the executable, in a folder made when they are first needed.
    #[test]
    fn temporary_files_go_to_the_data_folder() {
        let exe = scratch("tmp");
        let data = exe.join(DATA_DIR);
        assert_eq!(temp_dir_in(Some(&data)).unwrap(), data.join("tmp"));
        assert!(data.join("tmp").is_dir(), "made when asked for");
        let _ = std::fs::remove_dir_all(&exe);
    }

    /// An executable in a place that cannot be written to (a read-only install): the folder cannot
    /// be made, and mukade says so rather than writing somewhere it was not meant to. A plain file
    /// where the folder should be is how this is staged without needing a read-only disk.
    #[test]
    fn an_unwritable_data_folder_is_an_error_not_a_stray_file() {
        let exe = scratch("blocked");
        let data = exe.join(DATA_DIR);
        std::fs::write(&data, "a file, not a folder").unwrap();
        assert!(temp_dir_in(Some(&data)).is_err());
        assert!(temp_dir_in(None).is_err(), "no executable folder either");
        let _ = std::fs::remove_dir_all(&exe);
    }

    /// Only mukade's own, old temporary files go.
    #[test]
    fn only_old_files_with_mukades_name_are_swept() {
        let dir = scratch("sweep");
        let old = SystemTime::now() - Duration::from_secs(48 * 60 * 60);
        let make = |name: &str, age: Option<SystemTime>| {
            let path = dir.join(name);
            std::fs::write(&path, "x").unwrap();
            if let Some(age) = age {
                std::fs::File::options()
                    .write(true)
                    .open(&path)
                    .unwrap()
                    .set_modified(age)
                    .unwrap();
            }
            path
        };
        let stale = make("mukade-import-old.json", Some(old));
        let fresh = make("mukade-import-new.json", None);
        let other_name = make("notes.json", Some(old));
        let other_suffix = make("mukade-import-old.txt", Some(old));
        sweep_stale(&dir, "mukade-import-", ".json", STALE_AFTER);
        assert!(
            !stale.exists(),
            "a day-old import document of ours is removed"
        );
        assert!(
            fresh.exists(),
            "a recent one may belong to a running mukade"
        );
        assert!(
            other_name.exists() && other_suffix.exists(),
            "files that are not ours stay"
        );
        sweep_stale(&dir.join("missing"), "x", "y", STALE_AFTER);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
