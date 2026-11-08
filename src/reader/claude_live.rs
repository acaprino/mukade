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

/// How far a recorded creation time may differ from the process's own and still be the same
/// process: two seconds, in FILETIME units (the registration is written a moment after the start).
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
            let recorded = registration
                .proc_start
                .as_deref()
                .and_then(|text| text.trim().parse::<u64>().ok())
                .filter(|recorded| looks_like_filetime(*recorded));
            match (started, recorded) {
                (Some(actual), Some(recorded)) if actual.abs_diff(recorded) > SAME_START => {
                    Liveness::Gone
                }
                _ => Liveness::Running,
            }
        }
    }
}

/// Whether a number can be a creation time in FILETIME units: between 2000 and 2100. A recorded
/// value that is not one is some other encoding, which says nothing about the process.
fn looks_like_filetime(value: u64) -> bool {
    (process::filetime_of_unix_seconds(946_684_800)
        ..process::filetime_of_unix_seconds(4_102_444_800))
        .contains(&value)
}

/// Whether a `pidDomain` (`<platform>:<machine>`) names this machine and platform. A machine name
/// that cannot be determined here is not held against the registration.
fn domain_is_here(domain: &str) -> bool {
    let (platform, machine) = domain.split_once(':').unwrap_or((domain, ""));
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
    match local_machine() {
        Some(local) if !machine.is_empty() => local.eq_ignore_ascii_case(machine),
        _ => true,
    }
}

fn local_machine() -> Option<String> {
    let name = if cfg!(windows) {
        std::env::var("COMPUTERNAME").ok()
    } else {
        std::env::var("HOSTNAME")
            .ok()
            .or_else(|| std::fs::read_to_string("/etc/hostname").ok())
    };
    name.map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty())
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
        if let Some(local) = local_machine() {
            assert!(domain_is_here(&format!(
                "{platform}:{}",
                local.to_uppercase()
            )));
        }
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
