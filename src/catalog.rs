//! The harness catalog: one versioned JSON file that names, per harness and per platform, the
//! binary, the canonical install locations, the store variable and default, the store's layout,
//! the lineage it records and the resume command mukade returns.
//!
//! `catalog/harnesses.json` ships inside the executable (`include_str!`), so the single-file build
//! still needs nothing beside it. The catalog is data, not a second source of truth about the
//! formats: a reader still decides what a store holds, and `detect` still reports what it saw.
//!
//! Platform selection is a compile-time fact (`windows`, `macos`, `linux`), and a location that
//! needs a variable the platform does not have (`{app_data}` elsewhere) is skipped rather than
//! guessed at. `{home}` is `USERPROFILE` on Windows and `HOME` elsewhere; `{xdg_data_home}` is
//! `XDG_DATA_HOME`, else `{home}/.local/share`.

use crate::model::HarnessId;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// The catalog as it ships.
const CATALOG_JSON: &str = include_str!("../catalog/harnesses.json");

#[derive(Debug, Deserialize)]
pub struct Catalog {
    pub schema: String,
    pub harnesses: Vec<Entry>,
}

/// One harness's entry.
#[derive(Debug, Deserialize)]
pub struct Entry {
    pub id: HarnessId,
    pub display: String,
    pub binary: Binary,
    pub store: Store,
    pub lineage: Lineage,
    /// The argv mukade returns as the native resume command. `{exe}`, `{id}` and `{file}` are
    /// filled in by [`resume`].
    pub resume: Vec<String>,
    /// The argv that opens the session interactively in the harness's own TUI.
    #[serde(default)]
    pub interactive_resume: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct Binary {
    pub name: String,
    pub flag: String,
    pub env: String,
    pub locations: Locations,
    /// Where a self-contained (portable or custom) installation keeps its executable, relative to
    /// the store's folder or one of its nearest ancestors. Read by [`crate::detect`].
    #[serde(default)]
    pub beside_store: Locations,
}

#[derive(Debug, Default, Deserialize)]
pub struct Locations {
    #[serde(default)]
    pub windows: Vec<String>,
    #[serde(default)]
    pub macos: Vec<String>,
    #[serde(default)]
    pub linux: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct Store {
    pub env: String,
    pub default: String,
    /// OpenCode's store is the SQLite database itself, not a directory.
    pub root_is_a_file: bool,
    pub layout: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct Lineage {
    pub recorded: bool,
    pub fields: Vec<String>,
    pub note: String,
}

/// The variables a location is expanded with. Kept as a value so tests can state them instead of
/// reading the machine they run on.
#[derive(Debug, Clone, Default)]
pub struct Vars {
    pub home: Option<PathBuf>,
    pub app_data: Option<PathBuf>,
    pub local_app_data: Option<PathBuf>,
    pub xdg_data_home: Option<PathBuf>,
}

impl Vars {
    /// The variables this process has.
    pub fn from_env() -> Self {
        let var = |name: &str| {
            std::env::var_os(name)
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
        };
        let home = var(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
            .or_else(|| var("HOME"))
            .or_else(|| var("USERPROFILE"));
        Vars {
            home,
            app_data: var("APPDATA"),
            local_app_data: var("LOCALAPPDATA"),
            xdg_data_home: var("XDG_DATA_HOME"),
        }
    }
}

/// The platform key a location list is read from.
pub fn platform() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

/// The catalog, parsed once.
pub fn catalog() -> &'static Catalog {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        serde_json::from_str(CATALOG_JSON).expect("catalog/harnesses.json is valid at build time")
    })
}

/// One harness's entry. Every [`HarnessId`] has one: a test holds the catalog to that.
pub fn entry(harness: HarnessId) -> &'static Entry {
    catalog()
        .harnesses
        .iter()
        .find(|entry| entry.id == harness)
        .expect("the catalog covers the four supported harnesses")
}

/// The well-known binary locations of a platform, expanded. An entry that needs an unset variable
/// is skipped: a location that cannot be named is not a candidate.
pub fn binary_candidates_with(harness: HarnessId, platform: &str, vars: &Vars) -> Vec<PathBuf> {
    let entry = entry(harness);
    let list = match platform {
        "windows" => &entry.binary.locations.windows,
        "macos" => &entry.binary.locations.macos,
        _ => &entry.binary.locations.linux,
    };
    list.iter()
        .filter_map(|location| expand(location, vars))
        .collect()
}

/// The executable paths a self-contained installation may keep beside its store, relative to the
/// store's folder or an ancestor of it, for a platform.
pub fn engines_beside_store(harness: HarnessId, platform: &str) -> &'static [String] {
    let entry = entry(harness);
    match platform {
        "windows" => &entry.binary.beside_store.windows,
        "macos" => &entry.binary.beside_store.macos,
        _ => &entry.binary.beside_store.linux,
    }
}

/// [`binary_candidates_with`] for this machine's platform and variables.
pub fn binary_candidates(harness: HarnessId) -> Vec<PathBuf> {
    binary_candidates_with(harness, platform(), &Vars::from_env())
}

/// The harness's default store on this platform, when its own convention names one.
pub fn store_default(harness: HarnessId) -> Option<PathBuf> {
    store_default_with(harness, &Vars::from_env())
}

/// [`store_default`] with the variables stated.
pub fn store_default_with(harness: HarnessId, vars: &Vars) -> Option<PathBuf> {
    expand(&entry(harness).store.default, vars)
}

/// The variable that points a harness at its store.
pub fn store_env(harness: HarnessId) -> &'static str {
    &entry(harness).store.env
}

/// Whether the store root is the SQLite file itself (OpenCode) rather than a directory.
pub fn store_is_file(harness: HarnessId) -> bool {
    entry(harness).store.root_is_a_file
}

/// The patterns a store's layout makes, for a person reading the inventory.
pub fn layout(harness: HarnessId) -> &'static [String] {
    &entry(harness).store.layout
}

/// The native resume command, with the executable and the session filled in.
pub fn resume(harness: HarnessId, exe: &str, id: &str, file: Option<&Path>) -> Vec<String> {
    entry(harness)
        .resume
        .iter()
        .map(|part| match part.as_str() {
            "{exe}" => exe.to_string(),
            "{id}" => id.to_string(),
            "{file}" => file
                .map(|file| file.display().to_string())
                .unwrap_or_else(|| id.to_string()),
            other => other.to_string(),
        })
        .collect()
}

/// The command that opens a session interactively in the harness's own terminal UI.
pub fn interactive_resume(harness: HarnessId, exe: &str, id: &str, file: Option<&Path>) -> Vec<String> {
    let template = if entry(harness).interactive_resume.is_empty() {
        &entry(harness).resume
    } else {
        &entry(harness).interactive_resume
    };
    template
        .iter()
        .map(|part| match part.as_str() {
            "{exe}" => exe.to_string(),
            "{id}" => id.to_string(),
            "{file}" => file
                .map(|file| file.display().to_string())
                .unwrap_or_else(|| id.to_string()),
            other => other.to_string(),
        })
        .collect()
}

/// Expand `{home}` and the data-directory tokens of a catalog path. A token the platform does not
/// provide makes the whole location unknown (`None`), never a path with a literal `{token}` in it.
pub fn expand(location: &str, vars: &Vars) -> Option<PathBuf> {
    let mut out = location.to_string();
    let mut replace = |token: &str, value: Option<&PathBuf>| -> Option<()> {
        if !out.contains(token) {
            return Some(());
        }
        let value = value?;
        out = out.replace(token, &value.to_string_lossy());
        Some(())
    };
    replace("{home}", vars.home.as_ref())?;
    replace("{app_data}", vars.app_data.as_ref())?;
    replace("{local_app_data}", vars.local_app_data.as_ref())?;
    if out.contains("{xdg_data_home}") {
        let data = vars.xdg_data_home.clone().or_else(|| {
            vars.home
                .as_ref()
                .map(|home| home.join(".local").join("share"))
        })?;
        out = out.replace("{xdg_data_home}", &data.to_string_lossy());
    }
    if out.contains('{') {
        return None;
    }
    // `/` is valid on every platform; a Windows display reads better with its own separator.
    Some(PathBuf::from(
        out.replace('/', std::path::MAIN_SEPARATOR_STR),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars() -> Vars {
        Vars {
            home: Some(PathBuf::from("/home/user")),
            app_data: Some(PathBuf::from("/app-data")),
            local_app_data: Some(PathBuf::from("/local-app-data")),
            xdg_data_home: None,
        }
    }

    #[test]
    fn the_catalog_covers_every_harness_once() {
        assert_eq!(catalog().schema, "mukade.harness-catalog/1");
        assert_eq!(catalog().harnesses.len(), HarnessId::ALL.len());
        for harness in HarnessId::ALL {
            let entry = entry(harness);
            assert_eq!(entry.id, harness);
            assert!(!entry.display.is_empty());
            assert!(!entry.store.layout.is_empty());
            assert_eq!(entry.binary.env, crate::exe::env_var_name(harness));
        }
    }

    #[test]
    fn every_platform_states_its_own_binary_locations() {
        for harness in HarnessId::ALL {
            let entry = entry(harness);
            assert!(
                entry.binary.locations.windows.iter().all(|l| !l.is_empty()),
                "{harness:?}"
            );
            // macOS and Linux may be PATH-only, but the list must exist, not be missing.
            assert!(entry.binary.locations.macos.len() < 4);
            assert!(entry.binary.locations.linux.len() < 4);
        }
        let windows = binary_candidates_with(HarnessId::ClaudeCode, "windows", &vars());
        assert!(windows.iter().any(|path| path.ends_with("claude.exe")));
        let linux = binary_candidates_with(HarnessId::ClaudeCode, "linux", &vars());
        assert!(linux.iter().any(|path| path.ends_with("claude")));
    }

    #[test]
    fn a_token_the_platform_lacks_makes_the_location_unknown() {
        let unix = Vars {
            home: Some(PathBuf::from("/home/user")),
            ..Vars::default()
        };
        let windows_only = binary_candidates_with(HarnessId::Codex, "windows", &unix);
        assert!(windows_only.is_empty(), "{windows_only:?}");
        assert_eq!(
            binary_candidates_with(HarnessId::Codex, "windows", &vars()),
            vec![PathBuf::from(
                "/local-app-data/Programs/OpenAI/Codex/bin/codex.exe"
            )]
        );
        // An unknown token is not left in a path as text.
        assert!(expand("{nonesuch}/x", &unix).is_none());
    }

    #[test]
    fn a_store_default_falls_back_to_the_local_data_directory() {
        assert_eq!(
            store_default_with(HarnessId::OpenCode, &vars()),
            Some(PathBuf::from(
                "/home/user/.local/share/opencode/opencode.db"
            ))
        );
        assert_eq!(
            store_default_with(HarnessId::Pi, &vars()),
            Some(PathBuf::from("/home/user/.pi/agent"))
        );
        assert!(store_is_file(HarnessId::OpenCode));
        assert!(!store_is_file(HarnessId::ClaudeCode));
    }

    #[test]
    fn the_resume_command_is_the_one_the_catalog_names() {
        let claude = resume(HarnessId::ClaudeCode, "claude", "abc", None);
        assert_eq!(claude, ["claude", "-p", "--resume", "abc"]);
        let opencode = resume(HarnessId::OpenCode, "opencode", "ses_1", None);
        assert_eq!(
            opencode,
            ["opencode", "run", "--standalone", "--session", "ses_1"]
        );
        let pi = resume(
            HarnessId::Pi,
            "pi",
            "id",
            Some(Path::new("/sessions/id.jsonl")),
        );
        assert_eq!(pi, ["pi", "-p", "--session", "/sessions/id.jsonl"]);
    }
}
