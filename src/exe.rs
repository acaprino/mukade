//! Which binary to spawn for a harness.
//!
//! Resolution order: an explicit override (a CLI flag or a library option) → the harness's
//! `MUKADE_*_EXE` variable → the binary on `PATH` → the well-known install locations → the bare
//! command name, which is what a spawn would try anyway. A default install is found on its own;
//! a portable or otherwise custom one is always one flag away.

use crate::model::HarnessId;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

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
    // OpenCode's npm package puts an `opencode.cmd` shim on `PATH` that adds its own
    // `--standalone`; its real binary, a well-known location, is preferred over it.
    if harness == HarnessId::OpenCode {
        return pick(explicit, env, well_known, on_path(name), name);
    }
    pick(explicit, env, on_path(name), well_known, name)
}

/// The program to spawn for a command. A bare name (no directory, no extension) is looked up on
/// `PATH` the way a shell would, `PATHEXT` included: `Command::new("pi")` on Windows finds only a
/// `pi.exe`, and an npm install is a `pi.cmd` launcher.
pub fn spawnable(program: &str) -> String {
    let path = Path::new(program);
    if path.components().count() == 1
        && path.extension().is_none()
        && let Some(found) = on_path(program)
    {
        return found.to_string_lossy().into_owned();
    }
    program.to_string()
}

/// Whether a program is a Windows batch launcher, which cannot take every argument: Rust refuses
/// to pass a line break to one.
pub fn is_batch(program: &str) -> bool {
    Path::new(program)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("cmd") || e.eq_ignore_ascii_case("bat"))
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
    on_path_in(name, &std::env::var_os("PATH")?)
}

/// [`on_path`] over a given `PATH` value. An empty or relative entry is skipped: it would name the
/// current directory, and a binary found there is not the one installed.
fn on_path_in(name: &str, path: &std::ffi::OsStr) -> Option<PathBuf> {
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
    for dir in std::env::split_paths(path).filter(|dir| dir.is_absolute()) {
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

// ---------------------------------------------------------------------------------------------
// running a harness

/// How long the output of a process that has exited may still take to arrive. Past it, whatever
/// holds the pipes open is a process the harness left behind, not the harness's answer.
const DRAIN_AFTER_EXIT: Duration = Duration::from_secs(1);

/// What a harness command did within its deadline.
#[derive(Debug)]
pub struct Finished {
    /// `None` when the deadline passed and the command was stopped.
    pub status: Option<std::process::ExitStatus>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// Run a command with stdin closed, its output collected, and a deadline. Past the deadline the
/// command is stopped with everything it started: an npm install puts a `.cmd` launcher in front
/// of the harness, so the child is `cmd.exe` and killing it alone would leave the harness running.
/// Once the command has exited, a process it left behind holding the pipes open does not keep the
/// caller waiting.
pub fn run_within(command: &mut Command, timeout: Duration) -> std::io::Result<Finished> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdout = child.stdout.take().map(drain);
    let stderr = child.stderr.take().map(drain);
    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break Some(status);
        }
        if Instant::now() >= deadline {
            kill_tree(&mut child);
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let drained_by = Instant::now() + DRAIN_AFTER_EXIT;
    Ok(Finished {
        status,
        stdout: stdout.map(|pipe| pipe.take(drained_by)).unwrap_or_default(),
        stderr: stderr.map(|pipe| pipe.take(drained_by)).unwrap_or_default(),
    })
}

/// One output pipe, read on its own thread into a buffer that can be taken whether or not the
/// pipe has closed.
struct Drained {
    bytes: Arc<Mutex<Vec<u8>>>,
    reader: std::thread::JoinHandle<()>,
}

fn drain(mut pipe: impl Read + Send + 'static) -> Drained {
    let bytes = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&bytes);
    let reader = std::thread::spawn(move || {
        let mut chunk = [0u8; 8192];
        while let Ok(n) = pipe.read(&mut chunk) {
            if n == 0 {
                break;
            }
            sink.lock()
                .unwrap_or_else(|e| e.into_inner())
                .extend_from_slice(&chunk[..n]);
        }
    });
    Drained { bytes, reader }
}

impl Drained {
    /// What arrived by the time the pipe closed, or by `by` if something still holds it open.
    fn take(self, by: Instant) -> Vec<u8> {
        while !self.reader.is_finished() && Instant::now() < by {
            std::thread::sleep(Duration::from_millis(10));
        }
        self.bytes.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

/// Stop a process and what it started.
#[cfg(windows)]
fn kill_tree(child: &mut std::process::Child) {
    let taskkill = std::env::var_os("SystemRoot")
        .map(|root| Path::new(&root).join("System32").join("taskkill.exe"))
        .unwrap_or_else(|| "taskkill.exe".into());
    let _ = Command::new(taskkill)
        .args(["/PID", &child.id().to_string(), "/T", "/F"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let _ = child.kill();
}

/// Elsewhere a launcher `exec`s the harness, so the child is the harness itself.
#[cfg(not(windows))]
fn kill_tree(child: &mut std::process::Child) {
    let _ = child.kill();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_or_relative_path_entry_is_not_searched() {
        let dir = std::env::temp_dir().join(format!("mukade-on-path-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let launcher = dir.join(if cfg!(windows) {
            "fakeharness.cmd"
        } else {
            "fakeharness"
        });
        std::fs::write(&launcher, "").unwrap();
        let path =
            std::env::join_paths([PathBuf::new(), PathBuf::from("relative"), dir.clone()]).unwrap();
        let found = on_path_in("fakeharness", &path);
        let _ = std::fs::remove_dir_all(&dir);
        let found = found.expect("the absolute entry is searched");
        assert!(found.is_absolute(), "{found:?}");
        assert!(found.starts_with(&dir), "{found:?}");
        assert!(on_path_in("fakeharness", std::ffi::OsStr::new("")).is_none());
    }

    #[test]
    fn a_batch_launcher_is_recognized_by_its_extension() {
        assert!(is_batch(r"C:\npm\pi.cmd"));
        assert!(is_batch("run.BAT"));
        assert!(!is_batch("pi"));
        assert!(!is_batch(r"C:\bin\pi.exe"));
    }

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
