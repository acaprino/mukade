//! Where each harness keeps its sessions.
//!
//! The library never assumes a default store: every entry point is handed a [`StoreRoots`], which
//! the caller fills either from the harness's own environment variables or from a scratch
//! directory (docs/session-interchange-spec.md §1.3 rule 2).

use crate::model::HarnessId;
use std::path::{Path, PathBuf};

/// The store locations of the four harnesses. `None` is refused for Claude Code, Codex and Pi,
/// which have no store to read or write without one; OpenCode then uses the store its own
/// configuration names.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StoreRoots {
    /// Claude Code: `CLAUDE_CONFIG_DIR`; the sessions live under `projects/`.
    pub claude_config_dir: Option<PathBuf>,
    /// Codex CLI: `CODEX_HOME`; the rollouts live under `sessions/`.
    pub codex_home: Option<PathBuf>,
    /// OpenCode: the database file named by `OPENCODE_DB`.
    pub opencode_db: Option<PathBuf>,
    /// Pi: `PI_CODING_AGENT_DIR`; the sessions live under `sessions/`.
    pub pi_agent_dir: Option<PathBuf>,
}

impl StoreRoots {
    /// The roots the calling process declares through the harnesses' own variables.
    pub fn from_env() -> Self {
        // An empty variable is no store: as a path it would put the store beside the cwd.
        let var = |name: &str| {
            std::env::var_os(name)
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
        };
        StoreRoots {
            claude_config_dir: var("CLAUDE_CONFIG_DIR"),
            codex_home: var("CODEX_HOME"),
            opencode_db: var("OPENCODE_DB"),
            pi_agent_dir: var("PI_CODING_AGENT_DIR"),
        }
    }

    /// Claude Code's `projects/` directory, where one folder per project holds the session files.
    pub fn claude_projects(&self) -> Option<PathBuf> {
        self.claude_config_dir
            .as_deref()
            .map(|d| d.join("projects"))
    }

    /// Codex's `sessions/` directory.
    pub fn codex_sessions(&self) -> Option<PathBuf> {
        self.codex_home.as_deref().map(|d| d.join("sessions"))
    }

    /// Pi's `sessions/` directory.
    pub fn pi_sessions(&self) -> Option<PathBuf> {
        self.pi_agent_dir.as_deref().map(|d| d.join("sessions"))
    }

    /// One harness's store, as its variable names it.
    pub fn root(&self, harness: HarnessId) -> Option<&Path> {
        match harness {
            HarnessId::ClaudeCode => self.claude_config_dir.as_deref(),
            HarnessId::Codex => self.codex_home.as_deref(),
            HarnessId::OpenCode => self.opencode_db.as_deref(),
            HarnessId::Pi => self.pi_agent_dir.as_deref(),
        }
    }

    /// Point one harness at an explicit store, e.g. a portable installation's own. OpenCode's
    /// store is the database file: an existing folder means its `opencode.db`.
    pub fn set_root(&mut self, harness: HarnessId, root: &Path) {
        let root = root.to_path_buf();
        match harness {
            HarnessId::ClaudeCode => self.claude_config_dir = Some(root),
            HarnessId::Codex => self.codex_home = Some(root),
            HarnessId::OpenCode => {
                self.opencode_db = Some(if root.is_dir() {
                    root.join("opencode.db")
                } else {
                    root
                })
            }
            HarnessId::Pi => self.pi_agent_dir = Some(root),
        }
    }
}

impl StoreRoots {
    /// Point a harness process at this store: the variable the harness reads, set to its root when
    /// one is configured. OpenCode's database is passed, or the variable removed so the process
    /// does not inherit another store (L1).
    pub fn apply_to(&self, harness: HarnessId, command: &mut std::process::Command) {
        let (variable, root) = match harness {
            HarnessId::ClaudeCode => ("CLAUDE_CONFIG_DIR", &self.claude_config_dir),
            HarnessId::Codex => ("CODEX_HOME", &self.codex_home),
            HarnessId::Pi => ("PI_CODING_AGENT_DIR", &self.pi_agent_dir),
            HarnessId::OpenCode => {
                crate::reader::opencode::opencode_environment(command, self);
                return;
            }
        };
        if let Some(root) = root {
            command.env(variable, root);
        }
    }
}

/// The project folder name Claude Code derives from a working directory (`wL` in Claude Code
/// 2.1.311): every UTF-16 unit outside `[a-zA-Z0-9]` becomes `-` (`C:\a\b` → `C--a-b`), and a
/// name longer than 200 is cut there and suffixed with `-` and the base-36 hash of the path.
pub fn claude_project_dir_name(cwd: &Path) -> String {
    const MAX: usize = 200;
    let path = cwd.to_string_lossy();
    let units: Vec<u16> = path.encode_utf16().collect();
    let name: String = units
        .iter()
        .map(|&unit| match char::from_u32(u32::from(unit)) {
            Some(c) if c.is_ascii_alphanumeric() => c,
            _ => '-',
        })
        .collect();
    if name.len() <= MAX {
        return name;
    }
    // Claude's `jme`: a 32-bit `h = h * 31 + unit` over the original path's UTF-16 units.
    let hash = units.iter().fold(0i32, |h, &unit| {
        h.wrapping_shl(5)
            .wrapping_sub(h)
            .wrapping_add(i32::from(unit))
    });
    format!(
        "{}-{}",
        &name[..MAX],
        base36(u64::from(hash.unsigned_abs()))
    )
}

fn base36(mut n: u64) -> String {
    const DIGITS: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut out = Vec::new();
    loop {
        out.push(DIGITS[(n % 36) as usize]);
        n /= 36;
        if n == 0 {
            break;
        }
    }
    out.reverse();
    String::from_utf8(out).expect("ASCII digits")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_harness_root_is_set_and_read_back() {
        let mut roots = StoreRoots::default();
        roots.set_root(HarnessId::Pi, Path::new("portable/pi/agent"));
        assert_eq!(roots.root(HarnessId::Pi), Some(Path::new("portable/pi/agent")));
        assert_eq!(roots.root(HarnessId::ClaudeCode), None);
    }

    #[test]
    fn an_opencode_folder_means_its_database() {
        let dir = std::env::temp_dir().join(format!("mukade-roots-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut roots = StoreRoots::default();
        roots.set_root(HarnessId::OpenCode, &dir);
        assert_eq!(roots.opencode_db, Some(dir.join("opencode.db")));
        roots.set_root(HarnessId::OpenCode, &dir.join("other.db"));
        assert_eq!(roots.opencode_db, Some(dir.join("other.db")));
    }
}
