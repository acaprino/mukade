//! The harnesses page: each harness's binary as the window would run it (and where it came from),
//! its version and its standard store; and the folders a system scan reads. Read only: finding an
//! installation is `mukade::detect`'s job (ADR-0001), the scan itself is `locations::scan_folders`.

use mukade::catalog;
use mukade::exe::{self, Source};
use mukade::model::HarnessId;
use mukade::roots::StoreRoots;
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HarnessView {
    pub id: &'static str,
    pub display: String,
    /// `configured` (a variable or a flag names it), `installed` (on PATH or at the catalog's
    /// location) or `missing`.
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binary: Option<String>,
    /// Where the binary came from: `PATH`, the catalog's location, the variable or flag naming it.
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// The standard store, as this process's variables or the catalog name it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store: Option<String>,
    pub store_exists: bool,
    /// The variable that names a custom binary.
    pub binary_env: String,
    /// The variable that names the store.
    pub store_env: String,
    /// Whether mukade writes it: a hop or an import can land there.
    pub destination: bool,
    /// The command that installs it on the system, run in a terminal.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install_command: Option<String>,
    /// The npm package a sandbox installs; none when the harness is not one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sandbox_package: Option<&'static str>,
}

/// Every harness, in the catalog's order. A version costs a `--version` the first time a binary
/// is seen; the library keeps it after that.
pub fn inventory(standard: &StoreRoots) -> Vec<HarnessView> {
    let cache = mukade::cache::dir();
    HarnessId::ALL
        .into_iter()
        .map(|harness| {
            let entry = catalog::entry(harness);
            let (binary, source) = exe::resolve_source(harness, None);
            let found = source != Source::Name;
            let store = standard.root(harness).cloned();
            HarnessView {
                id: harness.as_str(),
                display: entry.display.clone(),
                status: match source {
                    Source::Override | Source::Env => "configured",
                    Source::Path | Source::WellKnown => "installed",
                    Source::Name => "missing",
                },
                version: if found {
                    exe::version_of_in(&binary, cache.as_deref())
                } else {
                    None
                },
                source: match source {
                    Source::Override => entry.binary.flag.clone(),
                    Source::Env => entry.binary.env.clone(),
                    Source::Path => "PATH".to_string(),
                    Source::WellKnown => "the catalog's location".to_string(),
                    Source::Name => String::new(),
                },
                binary: found.then_some(binary),
                store_exists: store.as_ref().is_some_and(|path| path.exists()),
                store: store.map(|path| path.display().to_string()),
                binary_env: entry.binary.env.clone(),
                store_env: catalog::store_env(harness).to_string(),
                destination: mukade::harnesses::get(harness).destination().is_some(),
                install_command: crate::sandbox::system_command(harness),
                sandbox_package: crate::sandbox::package(harness).map(|(package, _)| package),
            }
        })
        .collect()
}

/// Folders a system scan does not read: the system's own, and the recycle bin.
const SYSTEM_FOLDERS: [&str; 8] = [
    "windows",
    "$recycle.bin",
    "system volume information",
    "recovery",
    "perflogs",
    "$windows.~bt",
    "$windows.~ws",
    "msocache",
];

/// The folders a system scan reads: the user's home, then every top-level folder of every drive
/// (the system's own left out). Each is walked by `mukade::detect` with its depth and folder
/// budget, so a scan is bounded however large the disk.
pub fn system_roots() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(home) = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }) {
        roots.push(PathBuf::from(home));
    }
    for drive in drives() {
        let Ok(entries) = std::fs::read_dir(&drive) else {
            continue;
        };
        let mut folders: Vec<PathBuf> = entries
            .flatten()
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
            .map(|entry| entry.path())
            .filter(|path| !skipped(path, &roots))
            .collect();
        folders.sort();
        roots.extend(folders);
    }
    roots
}

/// Whether a store found by a system scan sits inside a git checkout, below `root`: test homes and
/// fixtures live there, never a harness's own store.
pub fn in_checkout(store: &Path, root: &Path) -> bool {
    store
        .ancestors()
        .skip(1)
        .take_while(|dir| dir.starts_with(root))
        .any(|dir| dir.join(".git").exists())
}

/// A drive's system folder, a folder already read through another root, or the home's parent
/// (`C:\Users` holds other people's homes).
fn skipped(path: &Path, roots: &[PathBuf]) -> bool {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    SYSTEM_FOLDERS.contains(&name.as_str())
        || roots.iter().any(|root| root.starts_with(path) || path.starts_with(root))
}

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetDriveTypeW(root_path_name: *const u16) -> u32;
}

/// The fixed drives only. The type is asked without touching the drive: an empty card reader or
/// optical drive would otherwise answer with the system's "no disk" dialog.
#[cfg(windows)]
fn drives() -> Vec<PathBuf> {
    const DRIVE_FIXED: u32 = 3;
    (b'A'..=b'Z')
        .map(|letter| format!("{}:\\", letter as char))
        .filter(|root| {
            let wide: Vec<u16> = root.encode_utf16().chain(Some(0)).collect();
            // SAFETY: `wide` is a NUL-terminated UTF-16 string that outlives the call.
            unsafe { GetDriveTypeW(wide.as_ptr()) == DRIVE_FIXED }
        })
        .map(PathBuf::from)
        .collect()
}

#[cfg(not(windows))]
fn drives() -> Vec<PathBuf> {
    // The root, and what is mounted under the usual places.
    let mut drives = vec![PathBuf::from("/")];
    for mounts in ["/mnt", "/media", "/Volumes"] {
        if let Ok(entries) = std::fs::read_dir(mounts) {
            drives.extend(entries.flatten().map(|entry| entry.path()));
        }
    }
    drives
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_harness_is_listed_once_with_its_variables() {
        let standard = StoreRoots::default();
        let list = inventory(&standard);
        assert_eq!(list.len(), HarnessId::ALL.len());
        for (view, harness) in list.iter().zip(HarnessId::ALL) {
            assert_eq!(view.id, harness.as_str());
            assert!(!view.binary_env.is_empty());
            assert!(!view.store_env.is_empty());
            assert_eq!(view.binary.is_some(), view.status != "missing");
        }
    }

    #[test]
    fn a_store_inside_a_git_checkout_is_not_an_installation() {
        let scratch = crate::scratch::Scratch::new("checkout");
        let repo = scratch.join("work").join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        let store = repo.join("fixtures").join(".claude");
        std::fs::create_dir_all(&store).unwrap();
        assert!(in_checkout(&store, &scratch.join("work")));
        let portable = scratch.join("work").join("apps").join(".claude");
        std::fs::create_dir_all(&portable).unwrap();
        assert!(!in_checkout(&portable, &scratch.join("work")));
    }

    #[test]
    fn a_system_folder_and_a_folder_read_through_another_root_are_skipped() {
        let home = PathBuf::from("C:/Users/me");
        assert!(skipped(Path::new("C:/Windows"), std::slice::from_ref(&home)));
        assert!(skipped(Path::new("C:/Users"), std::slice::from_ref(&home)));
        assert!(!skipped(Path::new("D:/Software"), std::slice::from_ref(&home)));
    }
}
