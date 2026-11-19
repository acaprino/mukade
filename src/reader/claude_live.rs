//! Which Claude Code sessions a process is running now.
//!
//! While a session runs, Claude Code keeps `<CLAUDE_CONFIG_DIR>/sessions/<pid>.json` for it: the
//! process id, the session id, the working directory, the name, and (to tell the process from a
//! later one given the same id) `procStart`, the process's creation time, and `pidDomain`, the
//! machine and platform the id belongs to. The file goes with the process, but a process that is
//! killed leaves it behind, so a registration is only a claim until the process is looked at.
//!
//! The `<pid>.<hash>.key` files beside them are the peer-messaging keys; they are not read.

use crate::process::{self, Process};
use serde_json::Value;
use std::path::{Path, PathBuf};

/// One `sessions/<pid>.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Registration {
    pub pid: u32,
    pub session_id: String,
    pub cwd: Option<String>,
    pub name: Option<String>,
    pub started_ms: Option<i64>,
    pub updated_ms: Option<i64>,
    /// The process's creation time as the registration records it (a Windows FILETIME).
    pub proc_start: Option<String>,
    /// `<platform>:<machine>` the process id belongs to.
    pub pid_domain: Option<String>,
    pub file: PathBuf,
}

/// What looking at a registration's process found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Liveness {
    /// The process is running, and it is the one that registered.
    Running,
    /// No such process (or another one has taken its id): the registration is a leftover.
    Gone,
    /// The registration belongs to another machine or platform; its process cannot be looked at.
    Unknown,
}

/// The registrations under `config_dir/sessions`. `None` when there is no such folder: that Claude
/// Code keeps no registry, so nothing can be said about which sessions run. A file that is not a
/// registration (a key, a damaged file, one without a session id) is skipped.
pub fn registrations(config_dir: &Path) -> Option<Vec<Registration>> {
    let folder = config_dir.join("sessions");
    let entries = std::fs::read_dir(&folder).ok()?;
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let file = entry.path();
        let Some(stem) = file
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.strip_suffix(".json"))
        else {
            continue;
        };
        if stem.is_empty() || !stem.bytes().all(|byte| byte.is_ascii_digit()) {
            continue;
        }
        let Some(text) = std::fs::read(&file).ok() else {
            continue;
        };
        let Ok(body) = serde_json::from_slice::<Value>(&text) else {
            continue;
        };
        let text_of = |key: &str| {
            body.get(key)
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
        };
        let Some(session_id) = text_of("sessionId") else {
            continue;
        };
        let Some(pid) = body
            .get("pid")
            .and_then(Value::as_u64)
            .or_else(|| stem.parse().ok())
            .and_then(|pid| u32::try_from(pid).ok())
        else {
            continue;
        };
        out.push(Registration {
            pid,
            session_id,
            cwd: text_of("cwd"),
            name: text_of("name"),
            started_ms: body.get("startedAt").and_then(Value::as_i64),
            updated_ms: body.get("updatedAt").and_then(Value::as_i64),
            proc_start: body.get("procStart").and_then(|value| match value {
                Value::String(text) => Some(text.clone()),
                Value::Number(number) => Some(number.to_string()),
                _ => None,
            }),
            pid_domain: text_of("pidDomain"),
            file,
        });
    }
    out.sort_by(|a, b| {
        a.pid
            .cmp(&b.pid)
            .then_with(|| a.session_id.cmp(&b.session_id))
    });
    Some(out)
}

/// How far a recorded Windows creation time may differ from the process's own and still be the
/// same process: two seconds, in FILETIME units (the registration is written a moment after the
/// start).
const SAME_START: u64 = 2 * 10_000_000;

/// Look at the process of `registration`.
pub fn liveness(registration: &Registration) -> Liveness {
    if registration
        .pid_domain
        .as_deref()
        .is_some_and(|domain| !domain_is_here(domain))
    {
        return Liveness::Unknown;
    }
    match process::inspect(registration.pid) {
        Process::Gone => Liveness::Gone,
        Process::Running { started } => {
            let recorded = registration.proc_start.as_deref().and_then(recorded_start);
            match (started, recorded) {
                (Some(actual), Some(recorded)) if !same_start(actual, recorded) => Liveness::Gone,
                _ => Liveness::Running,
            }
        }
    }
}

/// A recorded `procStart` in the unit [`process::inspect`] gives on this platform: a FILETIME on
/// Windows (a value that is not one is some other encoding, which says nothing about the
/// process), clock ticks since boot on Linux. Elsewhere its unit is not known and it is not read.
fn recorded_start(text: &str) -> Option<u64> {
    let value = text.trim().parse::<u64>().ok()?;
    if cfg!(windows) {
        Some(value).filter(|value| looks_like_filetime(*value))
    } else if cfg!(target_os = "linux") {
        Some(value)
    } else {
        None
    }
}

/// Whether two start times name the same process: within [`SAME_START`] on Windows, equal on Linux
/// (both are the kernel's own count of ticks, read the same way).
fn same_start(actual: u64, recorded: u64) -> bool {
    if cfg!(windows) {
        actual.abs_diff(recorded) <= SAME_START
    } else {
        actual == recorded
    }
}

/// Whether a number can be a creation time in FILETIME units: between 2000 and 2100.
fn looks_like_filetime(value: u64) -> bool {
    (process::filetime_of_unix_seconds(946_684_800)
        ..process::filetime_of_unix_seconds(4_102_444_800))
        .contains(&value)
}

/// Whether a `pidDomain` names this machine, platform and process-id namespace.
///
/// Claude Code writes `win32:<computer name>` on Windows and
/// `linux:<machine id>:pid:[<namespace inode>]` on Linux (`/etc/machine-id`, and the target of
/// `/proc/self/ns/pid`: a container has its own ids). The machine is the part after the platform,
/// up to the next `:`, and is matched against the machine id or the host name; what cannot be
/// determined here is not held against the registration.
fn domain_is_here(domain: &str) -> bool {
    let (platform, rest) = domain.split_once(':').unwrap_or((domain, ""));
    let here = if cfg!(windows) {
        "win32"
    } else if cfg!(target_os = "macos") {
        "darwin"
    } else {
        std::env::consts::OS
    };
    if !platform.eq_ignore_ascii_case(here) {
        return false;
    }
    let (machine, namespace) = rest.split_once(':').unwrap_or((rest, ""));
    let names = local_machine_names();
    if !machine.is_empty()
        && !names.is_empty()
        && !names.iter().any(|name| name.eq_ignore_ascii_case(machine))
    {
        return false;
    }
    match (namespace.is_empty(), local_pid_namespace()) {
        (false, Some(local)) => local == namespace,
        _ => true,
    }
}

/// The names this machine goes by in a `pidDomain`: its machine id (Linux) and its host name.
fn local_machine_names() -> Vec<String> {
    let mut names = Vec::new();
    if cfg!(windows) {
        names.extend(std::env::var("COMPUTERNAME").ok());
    } else {
        for file in ["/etc/machine-id", "/var/lib/dbus/machine-id"] {
            names.extend(std::fs::read_to_string(file).ok());
        }
        names.extend(std::env::var("HOSTNAME").ok());
        names.extend(std::fs::read_to_string("/etc/hostname").ok());
    }
    let mut names: Vec<String> = names
        .into_iter()
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty())
        .collect();
    names.dedup();
    names
}

/// This process's process-id namespace as Linux names it (`pid:[4026531836]`).
fn local_pid_namespace() -> Option<String> {
    let link = std::fs::read_link("/proc/self/ns/pid").ok()?;
    Some(link.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_domain_names_a_platform_and_a_machine() {
        let platform = if cfg!(windows) {
            "win32"
        } else if cfg!(target_os = "macos") {
            "darwin"
        } else {
            std::env::consts::OS
        };
        assert!(domain_is_here(platform), "no machine named: this one");
        assert!(!domain_is_here("plan9:anywhere"));
        assert!(!domain_is_here(&format!(
            "{platform}:definitely-another-machine-7f3a"
        )));
        for local in local_machine_names() {
            assert!(domain_is_here(&format!(
                "{platform}:{}",
                local.to_uppercase()
            )));
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_linux_domain_names_the_machine_id_and_the_pid_namespace() {
        let Some(namespace) = local_pid_namespace() else {
            return;
        };
        let names = local_machine_names();
        let Some(machine) = names.first() else {
            return;
        };
        assert!(domain_is_here(&format!("linux:{machine}:{namespace}")));
        assert!(
            !domain_is_here(&format!("linux:{machine}:pid:[1]")),
            "another namespace's ids are not ours"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_linux_registration_of_this_process_is_running_and_a_later_start_is_not() {
        let stat = std::fs::read_to_string("/proc/self/stat").unwrap();
        let ticks: u64 = stat[stat.rfind(')').unwrap() + 1..]
            .split_whitespace()
            .nth(19)
            .unwrap()
            .parse()
            .unwrap();
        let mut registration = Registration {
            pid: std::process::id(),
            session_id: "s".into(),
            cwd: None,
            name: None,
            started_ms: None,
            updated_ms: None,
            proc_start: Some(ticks.to_string()),
            pid_domain: None,
            file: PathBuf::new(),
        };
        assert_eq!(liveness(&registration), Liveness::Running);
        registration.proc_start = Some((ticks + 1).to_string());
        assert_eq!(
            liveness(&registration),
            Liveness::Gone,
            "the id was reused by a later process"
        );
    }

    #[test]
    fn only_a_plausible_creation_time_is_compared() {
        assert!(looks_like_filetime(134_386_175_913_086_879));
        assert!(
            !looks_like_filetime(1_794_143_992_646),
            "milliseconds since 1970 are not one"
        );
        assert!(!looks_like_filetime(0));
    }
}
