//! F9: a resume opens a new terminal window running the harness's interactive command, in the
//! session's folder, with the store variables its installation needs.

use std::ffi::OsString;
use std::path::Path;
use std::process::Command;

/// The variables `apply` sets (or removes, `None`) on a command, collected so they can be put on
/// whatever process actually starts the harness.
pub fn store_env(apply: impl FnOnce(&mut Command)) -> Vec<(OsString, Option<OsString>)> {
    let mut probe = Command::new("probe");
    apply(&mut probe);
    probe
        .get_envs()
        .map(|(key, value)| (key.to_owned(), value.map(ToOwned::to_owned)))
        .collect()
}

fn with_env(command: &mut Command, env: &[(OsString, Option<OsString>)]) {
    for (key, value) in env {
        match value {
            Some(value) => command.env(key, value),
            None => command.env_remove(key),
        };
    }
}

/// Whether `program` is a file in one of the `PATH` folders.
#[cfg_attr(not(windows), allow(dead_code))]
fn on_path(program: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|path| {
        std::env::split_paths(&path).any(|folder| folder.join(program).is_file())
    })
}

/// Start `argv` in a new terminal window. Answers once the terminal has started, not when the
/// harness exits.
pub fn launch(
    argv: &[String],
    cwd: Option<&Path>,
    env: &[(OsString, Option<OsString>)],
) -> Result<(), String> {
    let Some((program, args)) = argv.split_first() else {
        return Err("no command to run".to_string());
    };
    let mut command = terminal_command(program, args, cwd, env)?;
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    with_env(&mut command, env);
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("{}: {error}", command.get_program().to_string_lossy()))
}

/// Windows Terminal when it is installed and nothing has to be set in the environment (a
/// running `wt` opens the tab in its own process, which never sees this one's variables); else a
/// new console window of its own, which inherits them.
#[cfg(windows)]
fn terminal_command(
    program: &str,
    args: &[String],
    cwd: Option<&Path>,
    env: &[(OsString, Option<OsString>)],
) -> Result<Command, String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
    if env.is_empty() && on_path("wt.exe") {
        let mut command = Command::new("wt");
        command.arg("new-tab");
        if let Some(cwd) = cwd {
            command.arg("-d").arg(cwd);
        }
        command.arg("--").arg(program).args(args);
        return Ok(command);
    }
    let mut command = Command::new(program);
    command.args(args).creation_flags(CREATE_NEW_CONSOLE);
    Ok(command)
}

/// Terminal.app runs a `.command` script: the only way to hand it a folder and variables.
#[cfg(target_os = "macos")]
fn terminal_command(
    program: &str,
    args: &[String],
    cwd: Option<&Path>,
    env: &[(OsString, Option<OsString>)],
) -> Result<Command, String> {
    use std::os::unix::fs::PermissionsExt;
    let quote = |text: &str| format!("'{}'", text.replace('\'', r"'\''"));
    let mut script = String::from("#!/bin/sh\n");
    if let Some(cwd) = cwd {
        script.push_str(&format!("cd {} || exit 1\n", quote(&cwd.to_string_lossy())));
    }
    for (key, value) in env {
        let key = key.to_string_lossy();
        match value {
            Some(value) => script.push_str(&format!(
                "export {key}={}\n",
                quote(&value.to_string_lossy())
            )),
            None => script.push_str(&format!("unset {key}\n")),
        }
    }
    script.push_str("exec");
    for part in std::iter::once(program).chain(args.iter().map(String::as_str)) {
        script.push(' ');
        script.push_str(&quote(part));
    }
    script.push('\n');
    let path = std::env::temp_dir().join(format!(
        "mukade-resume-{}-{}.command",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |time| time.as_millis())
    ));
    std::fs::write(&path, script).map_err(|error| format!("{}: {error}", path.display()))?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))
        .map_err(|error| format!("{}: {error}", path.display()))?;
    let mut command = Command::new("open");
    command.arg("-a").arg("Terminal").arg(path);
    Ok(command)
}

/// `$TERMINAL`, else the Debian alternative every desktop installs. Terminals disagree on how a
/// command is handed over: [`linux_terminal_args`] knows the common ones.
#[cfg(all(unix, not(target_os = "macos")))]
fn terminal_command(
    program: &str,
    args: &[String],
    cwd: Option<&Path>,
    _env: &[(OsString, Option<OsString>)],
) -> Result<Command, String> {
    let terminal = std::env::var("TERMINAL")
        .ok()
        .filter(|terminal| !terminal.trim().is_empty())
        .unwrap_or_else(|| "x-terminal-emulator".to_string());
    if !terminal.contains('/') && !on_path(&terminal) {
        return Err(format!(
            "no terminal to open: set TERMINAL, or install {terminal}"
        ));
    }
    let mut command = Command::new(&terminal);
    command.args(linux_terminal_args(&terminal, program, args, cwd));
    Ok(command)
}

/// The arguments that make `terminal` run `program args` (in `cwd` where the terminal is a
/// server that ignores its own working folder). `-e` with the command's words after it, as
/// Debian's `x-terminal-emulator` policy states, for any other.
#[cfg_attr(any(windows, target_os = "macos"), allow(dead_code))]
fn linux_terminal_args(
    terminal: &str,
    program: &str,
    args: &[String],
    cwd: Option<&Path>,
) -> Vec<OsString> {
    let name = Path::new(terminal)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let command = std::iter::once(OsString::from(program)).chain(args.iter().map(OsString::from));
    let mut out: Vec<OsString> = Vec::new();
    match name.as_str() {
        // GNOME's terminals take the command after `--`; their server keeps its own folder.
        "gnome-terminal" | "kgx" | "ptyxis" | "mate-terminal" | "tilix" => {
            if let Some(cwd) = cwd {
                out.push(format!("--working-directory={}", cwd.display()).into());
            }
            out.push("--".into());
        }
        "wezterm" => {
            out.push("start".into());
            if let Some(cwd) = cwd {
                out.push("--cwd".into());
                out.push(cwd.into());
            }
            out.push("--".into());
        }
        // kitty and foot take the command itself.
        "kitty" | "foot" => {}
        _ => out.push("-e".into()),
    }
    out.extend(command);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_store_variables_are_collected_with_their_removals() {
        let env = store_env(|command| {
            command.env("CODEX_HOME", "/stores/codex");
            command.env_remove("CLAUDE_CONFIG_DIR");
        });
        assert!(env.contains(&("CODEX_HOME".into(), Some("/stores/codex".into()))));
        assert!(env.contains(&("CLAUDE_CONFIG_DIR".into(), None)));
    }

    #[test]
    fn each_terminal_gets_the_command_its_own_way() {
        let args = ["--resume".to_string(), "id".to_string()];
        let words = |terminal: &str| {
            linux_terminal_args(terminal, "claude", &args, Some(Path::new("/w")))
                .into_iter()
                .map(|word| word.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            words("/usr/bin/gnome-terminal"),
            ["--working-directory=/w", "--", "claude", "--resume", "id"]
        );
        assert_eq!(
            words("x-terminal-emulator"),
            ["-e", "claude", "--resume", "id"]
        );
        assert_eq!(words("kitty"), ["claude", "--resume", "id"]);
        assert_eq!(
            words("wezterm"),
            ["start", "--cwd", "/w", "--", "claude", "--resume", "id"]
        );
    }

    #[test]
    fn nothing_to_run_is_refused() {
        assert!(launch(&[], None, &[]).is_err());
    }
}
