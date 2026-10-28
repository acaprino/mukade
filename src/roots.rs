//! Where each harness keeps its sessions.
//!
//! The library never assumes a default store: every entry point is handed a [`StoreRoots`], which
//! the caller fills either from the harness's own environment variables or from a scratch
//! directory (docs/session-interchange-spec.md §1.3 rule 2).

use std::path::{Path, PathBuf};

/// The store locations of the four harnesses. `None` means the harness's own default, which the
/// callers only use outside tests.
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
        let var = |name: &str| std::env::var_os(name).map(PathBuf::from);
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
}

/// The project folder name Claude Code derives from a working directory: every character that is
/// not alphanumeric becomes `-` (`C:\a\b` → `C--a-b`).
pub fn claude_project_dir_name(cwd: &Path) -> String {
    cwd.to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}
