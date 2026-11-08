//! Whether a process with a given id is running on this machine, and when it started.
//!
//! Claude Code registers each running session as `sessions/<pid>.json`; telling a live session
//! from a registration its process left behind needs this answer. It has to be quick (a listing
//! asks once per registration), and it must not start anything: a process is looked at, never
//! signalled. On Windows that is two Win32 calls, declared here because the dependency list is
//! closed on purpose (`Cargo.toml`); elsewhere `/proc`, or `ps` where there is no `/proc`.

/// What looking at a process id found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Process {
    /// Nothing runs under that id.
    Gone,
    /// A process runs under that id. `started` is when it started, in the platform's own unit,
    /// where that is known: Windows' creation time (a FILETIME, in 100 ns since 1601). It tells a
    /// process from a later one that was given the same id.
    Running { started: Option<u64> },
}

/// Look at process `pid`.
pub fn inspect(pid: u32) -> Process {
    imp::inspect(pid)
}

/// The Windows FILETIME of a Unix time in seconds (100 ns units since 1601-01-01).
pub fn filetime_of_unix_seconds(seconds: u64) -> u64 {
    (seconds + 11_644_473_600) * 10_000_000
}

#[cfg(windows)]
mod imp {
    use super::Process;
    use std::ffi::c_void;

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct FileTime {
        low: u32,
        high: u32,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn OpenProcess(desired_access: u32, inherit_handle: i32, process_id: u32) -> *mut c_void;
        fn GetProcessTimes(
            process: *mut c_void,
            creation: *mut FileTime,
            exit: *mut FileTime,
            kernel: *mut FileTime,
            user: *mut FileTime,
        ) -> i32;
        fn CloseHandle(object: *mut c_void) -> i32;
    }

    /// Enough to read a process's times, and granted for processes the caller may not otherwise
    /// open (other users', elevated ones).
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    /// What `OpenProcess` fails with for an id nothing runs under.
    const ERROR_INVALID_PARAMETER: i32 = 87;

    pub fn inspect(pid: u32) -> Process {
        // SAFETY: `OpenProcess` takes plain integers and returns a handle or null. The handle is
        // used only for `GetProcessTimes`, whose four out-pointers are live locals of the
        // declared layout (two u32s, a FILETIME), and is closed before returning.
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if handle.is_null() {
                return match std::io::Error::last_os_error().raw_os_error() {
                    Some(ERROR_INVALID_PARAMETER) => Process::Gone,
                    // The process is there but is not ours to look into.
                    _ => Process::Running { started: None },
                };
            }
            let (mut creation, mut exit, mut kernel, mut user) = (
                FileTime::default(),
                FileTime::default(),
                FileTime::default(),
                FileTime::default(),
            );
            let read = GetProcessTimes(handle, &mut creation, &mut exit, &mut kernel, &mut user);
            CloseHandle(handle);
            if read == 0 {
                return Process::Running { started: None };
            }
            // A process that has exited can still be opened while something holds a handle to
            // it; it has an exit time then.
            if exit.low != 0 || exit.high != 0 {
                return Process::Gone;
            }
            Process::Running {
                started: Some((u64::from(creation.high) << 32) | u64::from(creation.low)),
            }
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use super::Process;
    use std::path::Path;

    pub fn inspect(pid: u32) -> Process {
        let proc = Path::new("/proc");
        if proc.join("self").exists() {
            return if proc.join(pid.to_string()).exists() {
                Process::Running { started: None }
            } else {
                Process::Gone
            };
        }
        // No /proc (macOS, the BSDs): `ps -p` prints the id when the process exists.
        let found = std::process::Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "pid="])
            .stderr(std::process::Stdio::null())
            .output()
            .ok()
            .is_some_and(|out| !String::from_utf8_lossy(&out.stdout).trim().is_empty());
        if found {
            Process::Running { started: None }
        } else {
            Process::Gone
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_process_is_running() {
        assert!(matches!(
            inspect(std::process::id()),
            Process::Running { .. }
        ));
    }

    #[test]
    fn an_id_nothing_runs_under_is_gone() {
        assert_eq!(inspect(u32::MAX - 1), Process::Gone);
        let mut child = std::process::Command::new(if cfg!(windows) { "cmd" } else { "true" })
            .args(if cfg!(windows) {
                &["/c", "exit"][..]
            } else {
                &[][..]
            })
            .spawn()
            .expect("a command that exits at once");
        let pid = child.id();
        child.wait().unwrap();
        assert_eq!(inspect(pid), Process::Gone, "a process that has exited");
    }

    #[cfg(windows)]
    #[test]
    fn windows_says_when_the_process_started() {
        let Process::Running {
            started: Some(started),
        } = inspect(std::process::id())
        else {
            panic!("a running process has a creation time");
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let now = filetime_of_unix_seconds(now);
        let hour = 3_600 * 10_000_000;
        assert!(
            started <= now + hour,
            "not in the future: {started} vs {now}"
        );
        assert!(
            started + 24 * hour > now,
            "started within the day: {started} vs {now}"
        );
    }
}
