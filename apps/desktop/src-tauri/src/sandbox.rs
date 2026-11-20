//! Harnesses the window installs. A system install runs the harness's own install command in a
//! terminal the person watches. A sandbox installs the harness with npm into a folder of its own,
//! with a store of its own beside it: trying it touches neither the system's installation nor its
//! sessions. A launcher in the folder sets the store's variable and runs the harness; each sandbox
//! is recorded in `mukade-data/sandboxes.json` and listed as an installation.

use crate::error::{CmdError, CmdResult};
use mukade::harnesses::{self, SandboxInstall};
use mukade::model::HarnessId;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

/// The terminal command that runs `line` and keeps the window open to read what it said.
pub fn terminal_argv(line: &str) -> Vec<String> {
    if cfg!(windows) {
        ["powershell", "-NoExit", "-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", line]
            .map(String::from)
            .to_vec()
    } else {
        vec![
            "sh".into(),
            "-c".into(),
            format!("{line}; echo; echo 'Done. Close this window.'; read -r _"),
        ]
    }
}

/// Run the system install of a harness in a new terminal; answers the command.
pub fn install_system(harness: HarnessId) -> CmdResult<Vec<String>> {
    let line = harnesses::get(harness).system_install_command().ok_or_else(|| {
        CmdError::refused(format!(
            "{} installs with its own script: see docs/harnesses",
            harness.as_str()
        ))
    })?;
    let argv = terminal_argv(line);
    crate::terminal::launch(&argv, None, &[]).map_err(CmdError::failed)?;
    Ok(argv)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sandbox {
    pub harness: String,
    pub name: String,
    /// The sandbox's folder: `app` (the npm install), `home` (the store), the launcher.
    pub root: String,
    pub store: String,
    /// The launcher: it sets the store's variables and runs the harness.
    pub exe: String,
    pub package: String,
    pub created: String,
}

fn registry() -> CmdResult<PathBuf> {
    Ok(mukade::portable::data_dir()
        .ok_or_else(|| CmdError::failed("no mukade-data folder beside the executable"))?
        .join("sandboxes.json"))
}

/// The sandboxes recorded, oldest first; none when there is no record.
pub fn list() -> Vec<Sandbox> {
    registry()
        .ok()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn save(all: &[Sandbox]) -> CmdResult<()> {
    let path = registry()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| CmdError::failed(format!("{}: {error}", parent.display())))?;
    }
    let text = serde_json::to_string_pretty(all).map_err(|error| CmdError::failed(error.to_string()))?;
    std::fs::write(&path, text).map_err(|error| CmdError::failed(format!("{}: {error}", path.display())))
}

/// A sandbox's name: what its folder is called.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 32
        && name.bytes().all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_')
}

/// The launcher's text: set each variable under `home`, then run the harness from `app`.
fn launcher(spec: &SandboxInstall, display: &str, binary: &str) -> String {
    if cfg!(windows) {
        let mut text = format!("@echo off\r\nrem A sandboxed {display}, installed by Mukade: its own store in home\\.\r\n");
        for (name, relative) in spec.env {
            text.push_str(&format!("set \"{name}=%~dp0home\\{}\"\r\n", relative.replace('/', "\\")));
        }
        text.push_str(&format!("\"%~dp0app\\node_modules\\.bin\\{binary}.cmd\" %*\r\n"));
        text
    } else {
        let mut text = format!("#!/bin/sh\n# A sandboxed {display}, installed by Mukade: its own store in home/.\nhere=$(CDPATH= cd -- \"$(dirname -- \"$0\")\" && pwd)\n");
        for (name, relative) in spec.env {
            text.push_str(&format!("export {name}=\"$here/home/{relative}\"\n"));
        }
        text.push_str(&format!("exec \"$here/app/node_modules/.bin/{binary}\" \"$@\"\n"));
        text
    }
}

/// How long an npm install may take.
const NPM_DEADLINE: Duration = Duration::from_secs(15 * 60);

/// Install a harness with npm into `<mukade-data>/sandboxes/<harness>-<name>`, write its launcher,
/// make its store, and record it. Blocks while npm runs.
pub fn install(harness: HarnessId, name: &str) -> CmdResult<Sandbox> {
    let spec = harnesses::get(harness).sandbox_install().ok_or_else(|| {
        CmdError::refused(format!("{} is not an npm package: it has no sandbox", harness.as_str()))
    })?;
    let package = spec.package;
    if !valid_name(name) {
        return Err(CmdError::refused("a sandbox's name: lowercase letters, digits, - and _ (up to 32)"));
    }
    let data = mukade::portable::data_dir().ok_or_else(|| CmdError::failed("no mukade-data folder beside the executable"))?;
    let root = data.join("sandboxes").join(format!("{}-{name}", harness.as_str()));
    if root.exists() {
        return Err(CmdError::refused(format!("{} exists already", root.display())));
    }
    let npm = mukade::exe::spawnable("npm");
    let app = root.join("app");
    std::fs::create_dir_all(&app).map_err(|error| CmdError::failed(format!("{}: {error}", app.display())))?;
    let mut command = Command::new(&npm);
    command.args(["install", "--prefix"]).arg(&app).args(["--no-fund", "--no-audit", "--loglevel=error"]);
    if spec.ignore_scripts {
        command.arg("--ignore-scripts");
    }
    command.arg(package);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let finished = mukade::exe::run_within(&mut command, NPM_DEADLINE).map_err(|error| {
        let _ = std::fs::remove_dir_all(&root);
        CmdError::failed(format!("npm could not start ({error}): install Node.js 22 or later"))
    })?;
    if !finished.status.is_some_and(|status| status.success()) {
        let _ = std::fs::remove_dir_all(&root);
        let said = String::from_utf8_lossy(&finished.stderr);
        let tail: Vec<&str> = said.lines().rev().take(6).collect::<Vec<_>>().into_iter().rev().collect();
        return Err(CmdError::failed(match finished.status {
            None => "npm did not finish within 15 minutes; nothing was kept".to_string(),
            Some(_) => format!("npm install {package} failed; nothing was kept: {}", tail.join(" · ")),
        }));
    }
    let binary = mukade::exe::binary_name(harness);
    let store = spec.store.split('/').fold(root.join("home"), |path, part| path.join(part));
    let folder = if mukade::catalog::store_is_file(harness) { store.parent().map(Path::to_path_buf) } else { Some(store.clone()) };
    if let Some(folder) = folder {
        for sub in spec.store_folders {
            std::fs::create_dir_all(folder.join(sub)).map_err(|error| CmdError::failed(format!("{}: {error}", folder.display())))?;
        }
        std::fs::create_dir_all(&folder).map_err(|error| CmdError::failed(format!("{}: {error}", folder.display())))?;
    }
    let exe = root.join(if cfg!(windows) { format!("{binary}.cmd") } else { binary.to_string() });
    let script = launcher(&spec, &mukade::catalog::entry(harness).display, binary);
    std::fs::write(&exe, script).map_err(|error| CmdError::failed(format!("{}: {error}", exe.display())))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755));
    }
    let sandbox = Sandbox {
        harness: harness.as_str().to_string(),
        name: name.to_string(),
        root: root.display().to_string(),
        store: store.display().to_string(),
        exe: exe.display().to_string(),
        package: package.to_string(),
        created: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .and_then(|since| chrono::DateTime::from_timestamp_millis(since.as_millis() as i64))
            .map(|time| time.to_rfc3339())
            .unwrap_or_default(),
    };
    let mut all = list();
    all.push(sandbox.clone());
    save(&all)?;
    Ok(sandbox)
}

/// Forget a sandbox: it is no longer listed. Its folder, and the sessions in it, stay.
pub fn forget(root: &str) -> CmdResult<()> {
    let mut all = list();
    let before = all.len();
    all.retain(|sandbox| sandbox.root != root);
    if all.len() == before {
        return Err(CmdError::not_found(format!("no sandbox at {root}")));
    }
    save(&all)
}

/// Open a sandbox's harness in a new terminal, in its folder.
pub fn run(root: &str) -> CmdResult<Vec<String>> {
    let sandbox = list().into_iter().find(|sandbox| sandbox.root == root).ok_or_else(|| CmdError::not_found(format!("no sandbox at {root}")))?;
    let argv = vec![sandbox.exe.clone()];
    crate::terminal::launch(&argv, Some(Path::new(&sandbox.root)), &[]).map_err(CmdError::failed)?;
    Ok(argv)
}

/// The sandboxes whose store exists, as installations the window lists (an OpenCode database
/// appears at the first run).
pub fn installations() -> Vec<mukade::detect::Installation> {
    list()
        .into_iter()
        .filter_map(|sandbox| {
            let harness: HarnessId = sandbox.harness.parse().ok()?;
            let store = PathBuf::from(&sandbox.store);
            store.exists().then(|| mukade::detect::Installation {
                harness,
                store,
                exe: Some(PathBuf::from(&sandbox.exe)),
                evidence: vec![format!("a sandbox installed by Mukade ({})", sandbox.package)],
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_launcher_sets_the_store_and_runs_the_npm_binary() {
        let claude = harnesses::get(HarnessId::ClaudeCode).sandbox_install().unwrap();
        let text = launcher(&claude, "Claude Code", "claude");
        assert!(text.contains("CLAUDE_CONFIG_DIR"), "{text}");
        assert!(text.contains("claude"), "{text}");
        let spec = harnesses::get(HarnessId::OpenCode).sandbox_install().unwrap();
        let opencode = launcher(&spec, "OpenCode", "opencode");
        for var in ["XDG_DATA_HOME", "XDG_CONFIG_HOME", "XDG_STATE_HOME", "XDG_CACHE_HOME"] {
            assert!(opencode.contains(var), "{var}");
        }
    }

    #[test]
    fn every_harness_but_hermes_has_a_package_and_a_system_command() {
        for harness in HarnessId::ALL {
            let adapter = harnesses::get(harness);
            assert_eq!(adapter.sandbox_install().is_some(), harness != HarnessId::Hermes, "{harness:?}");
            assert_eq!(adapter.system_install_command().is_some(), harness != HarnessId::Hermes, "{harness:?}");
        }
        assert!(valid_name("try-1"));
        assert!(!valid_name("Try"));
        assert!(!valid_name(""));
    }
}
