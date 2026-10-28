//! Which binary to spawn for a harness.
//!
//! Resolution order: an explicit override (a CLI flag or a library option) → the harness's
//! `MUKADE_*_EXE` variable → the binary on `PATH` → the well-known install locations → the bare
//! command name, which is what a spawn would try anyway. A default install is found on its own;
//! a portable or otherwise custom one is always one flag away.

use crate::model::HarnessId;
use std::path::{Path, PathBuf};

/// The command name a default install puts on `PATH`.
pub fn binary_name(harness: HarnessId) -> &'static str {
    match harness {
        HarnessId::ClaudeCode => "claude",
        HarnessId::Codex => "codex",
        HarnessId::OpenCode => "opencode",
        HarnessId::Pi => "pi",
    }
}

/// The variable a caller uses to name a custom executable for this harness.
pub fn env_var_name(harness: HarnessId) -> &'static str {
    match harness {
        HarnessId::ClaudeCode => "MUKADE_CLAUDE_EXE",
        HarnessId::Codex => "MUKADE_CODEX_EXE",
        HarnessId::OpenCode => "MUKADE_OPENCODE_EXE",
        HarnessId::Pi => "MUKADE_PI_EXE",
    }
}

/// The executable to spawn for `harness`. `explicit` wins over every other source.
pub fn resolve(harness: HarnessId, explicit: Option<&str>) -> String {
    let env = std::env::var(env_var_name(harness))
        .ok()
        .filter(|value| !value.is_empty());
    let name = binary_name(harness);
    let well_known = default_candidates(harness)
        .into_iter()
        .find(|path| path.is_file());
    pick(explicit, env, on_path(name), well_known, name)
}

/// The pure ordering of [`resolve`], kept separate so the precedence is testable without a
/// process environment.
fn pick(
    explicit: Option<&str>,
    env: Option<String>,
    on_path: Option<PathBuf>,
    well_known: Option<PathBuf>,
    name: &str,
) -> String {
    if let Some(path) = explicit.filter(|value| !value.is_empty()) {
        return path.to_string();
    }
    if let Some(value) = env {
        return value;
    }
    if let Some(path) = on_path {
        return path.to_string_lossy().into_owned();
    }
    if let Some(path) = well_known {
        return path.to_string_lossy().into_owned();
    }
    name.to_string()
}

/// The binary on `PATH`, as a full path. Windows tries the `PATHEXT` extensions, so a `pi.cmd`
/// launcher is found as well as a `pi.exe`.
fn on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let extensions: Vec<String> = if cfg!(windows) && Path::new(name).extension().is_none() {
        std::env::var("PATHEXT")
            .map(|value| {
                value
                    .split(';')
                    .filter(|ext| !ext.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_else(|_| vec![".EXE".into(), ".CMD".into(), ".BAT".into()])
    } else {
        vec![String::new()]
    };
    for dir in std::env::split_paths(&path) {
        for extension in &extensions {
            let candidate = dir.join(format!("{name}{extension}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Where a default install of this harness lives when it is not on `PATH`. Only locations the
/// harness's own installers use are listed; a custom location is what the overrides are for.
fn default_candidates(harness: HarnessId) -> Vec<PathBuf> {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from);
    let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    let roaming = std::env::var_os("APPDATA").map(PathBuf::from);
    let mut out = Vec::new();
    match harness {
        HarnessId::ClaudeCode => {
            // The native installer: ~/.local/bin/claude (claude.exe on Windows).
            if let Some(home) = home {
                out.push(home.join(".local").join("bin").join("claude.exe"));
                out.push(home.join(".local").join("bin").join("claude"));
            }
        }
        HarnessId::Codex => {
            // The Codex CLI installer: %LOCALAPPDATA%\Programs\OpenAI\Codex\bin\codex.exe.
            if let Some(local) = local {
                out.push(
                    local
                        .join("Programs")
                        .join("OpenAI")
                        .join("Codex")
                        .join("bin")
                        .join("codex.exe"),
                );
            }
        }
        HarnessId::OpenCode => {
            // The real binary of the npm package, not the `opencode.cmd` shim (which would append
            // a second `--standalone`); the install script's ~/.opencode/bin on Unix.
            if let Some(roaming) = roaming {
                out.push(
                    roaming
                        .join("npm")
                        .join("node_modules")
                        .join("@opencode")
                        .join("cli")
                        .join("bin")
                        .join("opencode.exe"),
                );
            }
            if let Some(home) = home {
                out.push(home.join(".opencode").join("bin").join("opencode"));
            }
        }
        HarnessId::Pi => {
            // The npm package `@earendil-works/pi-coding-agent` puts a `pi` launcher in the
            // global npm bin directory.
            if let Some(roaming) = roaming {
                out.push(roaming.join("npm").join("pi.cmd"));
                out.push(roaming.join("npm").join("pi"));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_override_order_is_explicit_env_path_well_known_name() {
        let name = "pi";
        let env = Some("env-pi".to_string());
        let on_path = Some(PathBuf::from("path-pi"));
        let well_known = Some(PathBuf::from("default-pi"));
        assert_eq!(
            pick(
                Some("flag-pi"),
                env.clone(),
                on_path.clone(),
                well_known.clone(),
                name
            ),
            "flag-pi"
        );
        assert_eq!(
            pick(None, env, on_path.clone(), well_known.clone(), name),
            "env-pi"
        );
        assert_eq!(
            pick(None, None, on_path, well_known.clone(), name),
            "path-pi"
        );
        assert_eq!(pick(None, None, None, well_known, name), "default-pi");
        assert_eq!(pick(None, None, None, None, name), "pi");
    }

    #[test]
    fn an_empty_override_does_not_hide_the_other_sources() {
        assert_eq!(
            pick(Some(""), Some("env-pi".into()), None, None, "pi"),
            "env-pi"
        );
    }

    #[test]
    fn every_harness_has_a_name_and_a_variable() {
        for harness in [
            HarnessId::ClaudeCode,
            HarnessId::Codex,
            HarnessId::OpenCode,
            HarnessId::Pi,
        ] {
            assert!(!binary_name(harness).is_empty());
            assert!(env_var_name(harness).starts_with("MUKADE_"));
            assert!(env_var_name(harness).ends_with("_EXE"));
        }
    }
}
