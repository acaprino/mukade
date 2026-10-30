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

/// The command name a default install puts on `PATH`, from the harness catalog.
pub fn binary_name(harness: HarnessId) -> &'static str {
    &crate::catalog::entry(harness).binary.name
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

/// Where the binary [`resolve`] returned came from, in the order the resolution tries them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// The caller's `--<harness>-exe`.
    Override,
    /// The harness's own `MUKADE_*_EXE` variable.
    Env,
    /// The binary on `PATH`.
    Path,
    /// A well-known install location.
    WellKnown,
    /// Nothing was found: the plain command name is all there is.
    Name,
}

/// The executable to spawn for `harness`. `explicit` wins over every other source.
pub fn resolve(harness: HarnessId, explicit: Option<&str>) -> String {
    resolve_source(harness, explicit).0
}

/// [`resolve`], keeping which source answered. `mukade harnesses` prints it, so a caller can tell
/// a binary it named from one mukade found on its own.
pub fn resolve_source(harness: HarnessId, explicit: Option<&str>) -> (String, Source) {
    let env = std::env::var(env_var_name(harness))
        .ok()
        .filter(|value| !value.is_empty());
    let name = binary_name(harness);
    let well_known = default_candidates(harness)
        .into_iter()
        .find(|path| path.is_file());
    pick_source(explicit, env, on_path(name), well_known, name)
}

/// The version a harness binary reports, found as cheaply as it can be:
///
/// 1. an npm launcher (`pi.cmd`, or a symlink into `node_modules`) names the package it starts:
///    the version is in that package's `package.json`, and no process is started (Pi, a Node
///    program, takes about half a second to say it);
/// 2. a version already learned about this very file (same path, size and modification time);
/// 3. otherwise the binary is asked for `--version`, with a deadline, and the answer is kept.
pub fn version_of(exe: &str) -> Option<String> {
    version_of_in(exe, crate::cache::dir().as_deref())
}

/// [`version_of`] with the cache folder stated.
pub fn version_of_in(exe: &str, cache: Option<&Path>) -> Option<String> {
    let program = PathBuf::from(spawnable(exe));
    if let Some(version) = package_version(&program) {
        return Some(version);
    }
    let key = crate::cache::identity(&program).map(|identity| format!("version|{identity}"));
    if let (Some(dir), Some(key)) = (cache, &key)
        && let Some(known) = crate::cache::get(dir, key)
    {
        return Some(known);
    }
    let version = probe_version(&program)?;
    if let (Some(dir), Some(key)) = (cache, &key) {
        crate::cache::put(dir, key, &version);
    }
    Some(version)
}

/// How long `--version` may take before it is given up on.
const VERSION_TIMEOUT: Duration = Duration::from_secs(30);

fn probe_version(program: &Path) -> Option<String> {
    let mut command = Command::new(program);
    command.arg("--version");
    if let Ok(cwd) = std::env::current_dir() {
        command.current_dir(cwd);
    }
    let output = run_within(&mut command, VERSION_TIMEOUT).ok()?;
    output.status.filter(|status| status.success())?;
    first_version(&String::from_utf8_lossy(&output.stdout))
}

/// The first `x.y.z` token of a line like `codex-cli 0.164.0` or `2.1.4`.
pub fn first_version(text: &str) -> Option<String> {
    text.split_whitespace().find_map(|token| {
        let token = token.trim_start_matches('v');
        let parts: Vec<&str> = token.split('.').collect();
        let numeric = parts.len() >= 2
            && parts
                .iter()
                .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()));
        numeric.then(|| token.to_string())
    })
}

/// The version of the npm package a launcher starts, read from the package itself.
fn package_version(program: &Path) -> Option<String> {
    let dir = package_dir_of(program)?;
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("package.json")).ok()?).ok()?;
    // A command-line package: one that declares what it installs.
    manifest.get("bin")?;
    manifest.get("version")?.as_str().map(str::to_owned)
}

/// The `node_modules/<package>` folder a launcher belongs to: the one a Windows npm `.cmd` shim
/// names, or the one a symlinked launcher (npm's `bin` on Unix) resolves into.
fn package_dir_of(program: &Path) -> Option<PathBuf> {
    let in_node_modules = |dir: &Path| {
        let parent = dir.parent()?;
        let scoped = parent
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with('@'));
        let holder = if scoped { parent.parent()? } else { parent };
        (holder.file_name()? == "node_modules").then(|| dir.to_path_buf())
    };
    if is_batch(&program.to_string_lossy()) {
        // `"%dp0%\node_modules\@scope\name\dist\cli.js" %*`, beside the shim.
        let text = std::fs::read_to_string(program).ok()?;
        let at = text.find("node_modules")? + "node_modules".len();
        let mut names = text[at..]
            .split(['\\', '/', '"', ' ', '\r', '\n'])
            .filter(|part| !part.is_empty());
        let first = names.next()?;
        let mut dir = program.parent()?.join("node_modules").join(first);
        if first.starts_with('@') {
            dir = dir.join(names.next()?);
        }
        return in_node_modules(&dir).filter(|dir| dir.is_dir());
    }
    let real = std::fs::canonicalize(program).ok()?;
    real.ancestors()
        .skip(1)
        .take(8)
        .find_map(|dir| in_node_modules(dir).filter(|dir| dir.join("package.json").is_file()))
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
#[cfg(test)]
fn pick(
    explicit: Option<&str>,
    env: Option<String>,
    on_path: Option<PathBuf>,
    well_known: Option<PathBuf>,
    name: &str,
) -> String {
    pick_source(explicit, env, on_path, well_known, name).0
}

/// [`pick`], keeping the source that answered: an explicit override, the harness's own variable,
/// `PATH`, a well-known install location, then the bare command name.
fn pick_source(
    explicit: Option<&str>,
    env: Option<String>,
    on_path: Option<PathBuf>,
    well_known: Option<PathBuf>,
    name: &str,
) -> (String, Source) {
    if let Some(path) = explicit.filter(|value| !value.is_empty()) {
        return (path.to_string(), Source::Override);
    }
    if let Some(value) = env {
        return (value, Source::Env);
    }
    if let Some(path) = on_path {
        return (path.to_string_lossy().into_owned(), Source::Path);
    }
    if let Some(path) = well_known {
        return (path.to_string_lossy().into_owned(), Source::WellKnown);
    }
    (name.to_string(), Source::Name)
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

/// Where a default install of this harness lives when it is not on `PATH`. The locations are the
/// catalog's (`catalog/harnesses.json`), per platform: only locations a harness's own installer
/// uses are listed, and a custom location is what the overrides are for.
fn default_candidates(harness: HarnessId) -> Vec<PathBuf> {
    crate::catalog::binary_candidates(harness)
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

    /// The order `resolve` tries is the order the source names: a caller's override first, then
    /// the harness's variable, then `PATH`, then a well-known location, then the bare name.
    #[test]
    fn the_source_names_which_step_answered() {
        let path = PathBuf::from("/on/path/pi");
        let well_known = PathBuf::from("/well/known/pi");
        let cases = [
            (
                pick_source(
                    Some("/mine/pi"),
                    Some("/env/pi".into()),
                    Some(path.clone()),
                    Some(well_known.clone()),
                    "pi",
                ),
                ("/mine/pi", Source::Override),
            ),
            (
                pick_source(
                    Some(""),
                    Some("/env/pi".into()),
                    Some(path.clone()),
                    Some(well_known.clone()),
                    "pi",
                ),
                ("/env/pi", Source::Env),
            ),
            (
                pick_source(
                    None,
                    None,
                    Some(path.clone()),
                    Some(well_known.clone()),
                    "pi",
                ),
                ("/on/path/pi", Source::Path),
            ),
            (
                pick_source(None, None, None, Some(well_known), "pi"),
                ("/well/known/pi", Source::WellKnown),
            ),
            (
                pick_source(None, None, None, None, "pi"),
                ("pi", Source::Name),
            ),
        ];
        for ((found, source), (path, expected)) in cases {
            assert_eq!((found.as_str(), source), (path, expected));
        }
        // An empty override is not an override: it is the same as none at all.
        assert_eq!(pick(Some(""), None, None, None, "pi"), "pi");
    }

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mukade-exe-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A launcher that counts how often it is run, and prints `faketool <version>`.
    fn counting_launcher(dir: &Path, count: &Path, version: &str) -> PathBuf {
        if cfg!(windows) {
            let path = dir.join("faketool.cmd");
            std::fs::write(
                &path,
                format!(
                    "@echo off\r\necho x>>\"{}\"\r\necho faketool {version}\r\n",
                    count.display()
                ),
            )
            .unwrap();
            path
        } else {
            let path = dir.join("faketool");
            std::fs::write(
                &path,
                format!(
                    "#!/bin/sh\necho x >> '{}'\necho faketool {version}\n",
                    count.display()
                ),
            )
            .unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            }
            path
        }
    }

    fn runs(count: &Path) -> usize {
        std::fs::read_to_string(count).map_or(0, |text| text.lines().count())
    }

    /// A version already learned about this very file is not asked for again; a changed file is.
    #[test]
    fn a_probe_is_asked_once_per_binary() {
        let dir = scratch("probe-cache");
        let (cache, count) = (dir.join("cache"), dir.join("count.txt"));
        let tool = counting_launcher(&dir, &count, "3.4.5");
        let program = tool.to_string_lossy().into_owned();
        assert_eq!(
            version_of_in(&program, Some(&cache)).as_deref(),
            Some("3.4.5")
        );
        assert_eq!(
            version_of_in(&program, Some(&cache)).as_deref(),
            Some("3.4.5")
        );
        assert_eq!(runs(&count), 1, "the second answer came from the cache");
        // An updated binary (another size) is a different binary.
        counting_launcher(&dir, &count, "3.4.50");
        assert_eq!(
            version_of_in(&program, Some(&cache)).as_deref(),
            Some("3.4.50")
        );
        assert_eq!(runs(&count), 2);
        // Without a cache folder it just asks.
        assert_eq!(version_of_in(&program, None).as_deref(), Some("3.4.50"));
        assert_eq!(runs(&count), 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// An npm launcher's version is the package's own: no process is started.
    #[cfg(windows)]
    #[test]
    fn an_npm_launchers_version_is_read_from_its_package() {
        let dir = scratch("npm-shim");
        let package = dir.join("node_modules").join("@scope").join("tool");
        std::fs::create_dir_all(&package).unwrap();
        std::fs::write(
            package.join("package.json"),
            r#"{"name":"@scope/tool","version":"9.8.7","bin":{"tool":"cli.js"}}"#,
        )
        .unwrap();
        let shim = dir.join("tool.cmd");
        std::fs::write(
            &shim,
            "@ECHO off\r\nSETLOCAL\r\nCALL :find_dp0\r\nnode \"%dp0%\\node_modules\\@scope\\tool\\cli.js\" %*\r\n",
        )
        .unwrap();
        // The cache folder is None and the shim would not run: only the package can answer.
        assert_eq!(
            version_of_in(&shim.to_string_lossy(), None).as_deref(),
            Some("9.8.7")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A `package.json` somewhere above a native binary is not that binary's package.
    #[test]
    fn a_stray_package_json_above_a_binary_is_not_its_package() {
        let dir = scratch("stray");
        std::fs::write(
            dir.join("package.json"),
            r#"{"version":"0.0.1","bin":{"x":"y"}}"#,
        )
        .unwrap();
        let bin = dir.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let count = dir.join("count.txt");
        let tool = counting_launcher(&bin, &count, "2.0.0");
        assert_eq!(
            version_of_in(&tool.to_string_lossy(), None).as_deref(),
            Some("2.0.0")
        );
        let _ = std::fs::remove_dir_all(&dir);
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
