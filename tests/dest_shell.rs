//! The destination's own shell and offered set, read from its configuration: `dest::shell_in`,
//! `tools::offered_in` and the shell-name helper they share.

mod support;

use mukade::dest;
use mukade::model::HarnessId;
use mukade::roots::StoreRoots;
use mukade::tools::{self, Shell};
use std::path::Path;

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

#[test]
fn a_shell_name_maps_to_its_dialect() {
    for name in ["bash", "sh", "zsh", "dash", "ksh"] {
        assert_eq!(tools::shell_named(name), Some(Shell::Posix), "{name}");
    }
    for name in ["pwsh", "powershell"] {
        assert_eq!(tools::shell_named(name), Some(Shell::PowerShell), "{name}");
    }
    // A path, `.exe` and the case are read the same way on every platform.
    let paths = [
        (
            r"C:\Program Files\PowerShell\7\pwsh.exe",
            Some(Shell::PowerShell),
        ),
        (
            r"C:\Windows\System32\WindowsPowerShell\v1.0\PowerShell.EXE",
            Some(Shell::PowerShell),
        ),
        (r"C:\Program Files\Git\bin\bash.exe", Some(Shell::Posix)),
        ("/usr/bin/zsh", Some(Shell::Posix)),
        ("/bin/bash.exe", Some(Shell::Posix)),
        ("BASH", Some(Shell::Posix)),
    ];
    for (name, dialect) in paths {
        assert_eq!(tools::shell_named(name), dialect, "{name}");
    }
    // No dialect mukade translates toward.
    for name in [
        "cmd",
        r"C:\Windows\System32\cmd.exe",
        "fish",
        "/usr/bin/nu",
        "",
        "  ",
    ] {
        assert_eq!(tools::shell_named(name), None, "{name:?}");
    }
}

#[test]
fn codex_runs_powershell_on_windows_and_a_posix_shell_elsewhere() {
    let expected = if cfg!(windows) {
        Shell::PowerShell
    } else {
        Shell::Posix
    };
    let roots = StoreRoots::default();
    assert_eq!(
        dest::shell_in(HarnessId::Codex, &roots, None),
        Some(expected)
    );
}

#[test]
fn claude_code_names_no_single_shell() {
    let scratch = support::scratch(HarnessId::ClaudeCode, "dest-shell-claude");
    let roots = StoreRoots {
        claude_config_dir: Some(scratch.join("home")),
        ..Default::default()
    };
    assert_eq!(
        dest::shell_in(HarnessId::ClaudeCode, &roots, Some(&scratch)),
        None
    );
}

/// Pi runs its `bash` tool with `shellPath` when its settings name one: the project's
/// `.pi/settings.json` over the agent folder's. Unset, it is a POSIX shell on every platform.
#[test]
fn pi_runs_the_shell_its_settings_name() {
    let scratch = support::scratch(HarnessId::Pi, "dest-shell-pi");
    let agent = scratch.join("home");
    let project = scratch.join("work");
    std::fs::create_dir_all(&project).unwrap();
    let roots = StoreRoots {
        pi_agent_dir: Some(agent.clone()),
        ..Default::default()
    };
    let shell = || dest::shell_in(HarnessId::Pi, &roots, Some(&project));

    assert_eq!(shell(), Some(Shell::Posix), "no settings");
    assert_eq!(
        dest::shell_in(HarnessId::Pi, &StoreRoots::default(), None),
        Some(Shell::Posix),
        "no agent folder and no project"
    );

    write(
        &agent.join("settings.json"),
        r#"{"shellPath":"C:\\Program Files\\PowerShell\\7\\pwsh.exe"}"#,
    );
    assert_eq!(shell(), Some(Shell::PowerShell), "the agent folder's");

    // A project file without the key leaves the agent folder's in place.
    write(
        &project.join(".pi/settings.json"),
        r#"{"defaultModel":"x"}"#,
    );
    assert_eq!(shell(), Some(Shell::PowerShell), "the project's names none");

    write(
        &project.join(".pi/settings.json"),
        r#"{"shellPath":"/usr/bin/bash"}"#,
    );
    assert_eq!(shell(), Some(Shell::Posix), "the project's wins");

    // A shell of no dialect mukade knows: Pi runs it, so no dialect is assumed.
    write(
        &project.join(".pi/settings.json"),
        r#"{"shellPath":"/usr/bin/fish"}"#,
    );
    assert_eq!(shell(), None, "an unknown shell");
}

/// Codex and Pi offer the pinned list in every configuration.
#[test]
fn offered_in_is_the_pinned_list_where_no_configuration_adds_one() {
    let scratch = support::scratch(HarnessId::Pi, "dest-shell-offered");
    let roots = StoreRoots {
        codex_home: Some(scratch.join("home")),
        pi_agent_dir: Some(scratch.join("home")),
        ..Default::default()
    };
    for id in [HarnessId::Codex, HarnessId::Pi] {
        assert_eq!(
            tools::offered_in(id, &roots, Some(&scratch)),
            tools::offered(id),
            "{id}"
        );
    }
}
