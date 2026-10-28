//! Portable mode: mukade runs from any folder, a USB stick included, and writes nothing anywhere
//! else.
//!
//! Portable mode is on when a file named `mukade.portable` or a folder named `mukade-data` sits
//! next to the executable, or when `MUKADE_PORTABLE` is `1`, `true` or `yes` (`0`, `false` or `no`
//! turns it off whatever sits next to the executable). Then everything mukade keeps for itself
//! lives in `mukade-data/` beside the executable: its cache in `mukade-data/cache`, its temporary
//! files in `mukade-data/tmp`. The folder is created when something is first written to it, never
//! at start-up. Moving or deleting the folder holding the executable moves or deletes all of it.
//!
//! Outside portable mode mukade keeps no cache (unless `MUKADE_CACHE_DIR` names a folder) and its
//! temporary files go to the system's temporary folder. In both modes a temporary file is removed
//! when its work is done, and one a killed process left behind is removed by the next run once it
//! is an hour old ([`sweep_stale`]).

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const MARKER_FILE: &str = "mukade.portable";
const DATA_DIR: &str = "mukade-data";

/// A temporary file this old belongs to a process that is gone.
pub const STALE_AFTER: Duration = Duration::from_secs(60 * 60);

type Env<'a> = &'a dyn Fn(&str) -> Option<OsString>;

/// The data folder of portable mode, when portable mode is on.
pub fn data_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    data_dir_in(exe.parent(), &|name| std::env::var_os(name))
}

/// [`data_dir`] for a given executable folder and environment.
pub fn data_dir_in(exe_dir: Option<&Path>, env: Env) -> Option<PathBuf> {
    let flag = env("MUKADE_PORTABLE")
        .map(|value| value.to_string_lossy().trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty());
    let beside_exe = exe_dir.map(|dir| dir.join(DATA_DIR));
    match flag.as_deref() {
        Some("0" | "false" | "no") => None,
        Some("1" | "true" | "yes") => beside_exe,
        _ => {
            let dir = exe_dir?;
            (dir.join(MARKER_FILE).is_file() || dir.join(DATA_DIR).is_dir())
                .then(|| dir.join(DATA_DIR))
        }
    }
}

/// Where the cache of what harness binaries told us lives: `MUKADE_CACHE_DIR` when it names a
/// folder, else `mukade-data/cache` in portable mode, else nowhere (no cache).
pub fn cache_dir() -> Option<PathBuf> {
    cache_dir_in(data_dir().as_deref(), &|name| std::env::var_os(name))
}

pub fn cache_dir_in(data: Option<&Path>, env: Env) -> Option<PathBuf> {
    env("MUKADE_CACHE_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| data.map(|dir| dir.join("cache")))
}

/// Where mukade's temporary files go: `mukade-data/tmp` in portable mode, else the system's.
pub fn temp_dir() -> PathBuf {
    data_dir().map_or_else(std::env::temp_dir, |dir| dir.join("tmp"))
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

    fn env_of<'a>(vars: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<OsString> + 'a {
        move |name| {
            vars.iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| OsString::from(value))
        }
    }

    /// Nothing next to the executable and nothing asked for: not portable, and so nothing is
    /// created or kept.
    #[test]
    fn a_plain_install_is_not_portable() {
        let exe = scratch("plain");
        assert_eq!(data_dir_in(Some(&exe), &env_of(&[])), None);
        assert_eq!(
            cache_dir_in(None, &env_of(&[])),
            None,
            "no cache unless asked for"
        );
        assert!(!exe.join(DATA_DIR).exists(), "looking does not create");
        let _ = std::fs::remove_dir_all(&exe);
    }

    #[test]
    fn a_marker_file_or_an_existing_data_folder_makes_it_portable() {
        let exe = scratch("marker");
        std::fs::write(exe.join(MARKER_FILE), "").unwrap();
        assert_eq!(
            data_dir_in(Some(&exe), &env_of(&[])),
            Some(exe.join(DATA_DIR))
        );
        std::fs::remove_file(exe.join(MARKER_FILE)).unwrap();
        std::fs::create_dir_all(exe.join(DATA_DIR)).unwrap();
        assert_eq!(
            data_dir_in(Some(&exe), &env_of(&[])),
            Some(exe.join(DATA_DIR))
        );
        let _ = std::fs::remove_dir_all(&exe);
    }

    #[test]
    fn the_environment_turns_it_on_or_off_whatever_sits_beside_the_executable() {
        let exe = scratch("env");
        assert_eq!(
            data_dir_in(Some(&exe), &env_of(&[("MUKADE_PORTABLE", "1")])),
            Some(exe.join(DATA_DIR))
        );
        std::fs::write(exe.join(MARKER_FILE), "").unwrap();
        assert_eq!(
            data_dir_in(Some(&exe), &env_of(&[("MUKADE_PORTABLE", "0")])),
            None
        );
        assert_eq!(
            data_dir_in(Some(&exe), &env_of(&[("MUKADE_PORTABLE", "")])),
            Some(exe.join(DATA_DIR))
        );
        assert_eq!(
            data_dir_in(None, &env_of(&[("MUKADE_PORTABLE", "1")])),
            None,
            "no executable folder"
        );
        let _ = std::fs::remove_dir_all(&exe);
    }

    /// An explicit cache folder wins; else the portable one; else none.
    #[test]
    fn the_cache_lives_where_it_was_told_to_or_in_the_data_folder() {
        let data = PathBuf::from("data");
        assert_eq!(
            cache_dir_in(Some(&data), &env_of(&[])),
            Some(data.join("cache"))
        );
        assert_eq!(
            cache_dir_in(Some(&data), &env_of(&[("MUKADE_CACHE_DIR", "elsewhere")])),
            Some(PathBuf::from("elsewhere"))
        );
        assert_eq!(
            cache_dir_in(None, &env_of(&[("MUKADE_CACHE_DIR", "elsewhere")])),
            Some(PathBuf::from("elsewhere"))
        );
        assert_eq!(
            cache_dir_in(Some(&data), &env_of(&[("MUKADE_CACHE_DIR", "")])),
            Some(data.join("cache"))
        );
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
