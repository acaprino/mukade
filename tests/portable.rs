//! Portable mode, end to end: the real binary, copied into a folder of its own, with every place a
//! program might leave files (the temporary folder, the home folder, the application-data folders)
//! pointed at an empty sandbox. Whatever mukade does, the sandbox must stay empty, and everything
//! mukade keeps must be in `mukade-data` beside the executable.

mod support;

use mukade::model::HarnessId;
use std::path::{Path, PathBuf};
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_mukade");

struct Bench {
    /// Where the executable is copied to.
    bundle: PathBuf,
    /// The places a program leaves files; each must stay empty.
    sandbox: PathBuf,
    /// The harness stores a hop writes its session to: not mukade's own files.
    store: PathBuf,
    scratch: PathBuf,
}

impl Bench {
    fn new(tag: &str, portable: bool) -> Bench {
        let scratch = support::scratch(HarnessId::ClaudeCode, tag);
        let bundle = scratch.join("bundle");
        let sandbox = scratch.join("sandbox");
        let store = scratch.join("store");
        for dir in [&bundle, &sandbox, &store] {
            std::fs::create_dir_all(dir).unwrap();
        }
        let exe = bundle.join(if cfg!(windows) {
            "mukade.exe"
        } else {
            "mukade"
        });
        std::fs::copy(BIN, &exe).unwrap();
        if portable {
            std::fs::write(bundle.join("mukade.portable"), "").unwrap();
        }
        Bench {
            bundle,
            sandbox,
            store,
            scratch,
        }
    }

    fn exe(&self) -> PathBuf {
        self.bundle.join(if cfg!(windows) {
            "mukade.exe"
        } else {
            "mukade"
        })
    }

    /// Run mukade with every location a program might write to redirected into the sandbox.
    fn run(&self, args: &[&str]) -> (Option<i32>, String, String) {
        let place = |name: &str| {
            let dir = self.sandbox.join(name);
            std::fs::create_dir_all(&dir).unwrap();
            dir
        };
        let mut command = Command::new(self.exe());
        command
            .args(args)
            .current_dir(&self.scratch)
            .env("TEMP", place("temp"))
            .env("TMP", place("temp"))
            .env("TMPDIR", place("temp"))
            .env("HOME", place("home"))
            .env("USERPROFILE", place("home"))
            .env("APPDATA", place("appdata"))
            .env("LOCALAPPDATA", place("localappdata"))
            .env("XDG_CACHE_HOME", place("xdg-cache"))
            .env("XDG_DATA_HOME", place("xdg-data"))
            .env("XDG_CONFIG_HOME", place("xdg-config"))
            .env("CLAUDE_CONFIG_DIR", &self.store)
            .env_remove("MUKADE_CACHE_DIR")
            .env_remove("MUKADE_PORTABLE")
            .env_remove("OPENCODE_DB");
        let output = command.output().expect("mukade runs");
        (
            output.status.code(),
            String::from_utf8_lossy(&output.stdout).into_owned(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
        )
    }

    /// Every file under the sandbox: there must be none.
    fn sandbox_files(&self) -> Vec<PathBuf> {
        files_under(&self.sandbox)
    }
}

fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

fn fixture() -> String {
    support::source_path(HarnessId::ClaudeCode)
        .to_string_lossy()
        .into_owned()
}

/// A launcher that counts its runs in `count` and answers `--version` with `claude <version>`.
fn claude_stand_in(dir: &Path, count: &Path) -> PathBuf {
    support::launcher(
        dir,
        "claude",
        &format!("echo x>>\"{}\"\r\necho claude 2.9.9", count.display()),
        &format!("echo x >> '{}'\necho claude 2.9.9", count.display()),
    )
}

/// A launcher for `opencode session import <file> …`: it answers with the id it "imported".
fn opencode_stand_in(dir: &Path, seen: &Path) -> PathBuf {
    support::launcher(
        dir,
        "opencode",
        &format!(
            "echo %3>\"{}\"\r\necho Imported session: ses_stand_in",
            seen.display()
        ),
        &format!(
            "echo \"$3\" > '{}'\necho 'Imported session: ses_stand_in'",
            seen.display()
        ),
    )
}

/// Where the stand-in was asked to import from.
fn imported_from(seen: &Path) -> PathBuf {
    PathBuf::from(std::fs::read_to_string(seen).unwrap().trim())
}

fn runs(count: &Path) -> usize {
    std::fs::read_to_string(count).map_or(0, |text| text.lines().count())
}

fn hop_to_claude(bench: &Bench, claude: &Path) -> (Option<i32>, String, String) {
    let cwd = bench.scratch.join("work");
    std::fs::create_dir_all(&cwd).unwrap();
    bench.run(&[
        "hop",
        "claude_code",
        "x",
        "claude_code",
        "--from-file",
        &fixture(),
        "--cwd",
        cwd.to_str().unwrap(),
        "--claude-exe",
        claude.to_str().unwrap(),
    ])
}

fn hop_to_opencode(bench: &Bench, opencode: &Path) -> (Option<i32>, String, String) {
    let cwd = bench.scratch.join("work");
    std::fs::create_dir_all(&cwd).unwrap();
    bench.run(&[
        "hop",
        "claude_code",
        "x",
        "opencode",
        "--from-file",
        &fixture(),
        "--cwd",
        cwd.to_str().unwrap(),
        "--opencode-exe",
        opencode.to_str().unwrap(),
        "--dest-version",
        "2.1.4",
        "--project-id",
        "global",
        "--model-provider",
        "h27",
        "--model",
        "m1",
    ])
}

#[test]
fn a_portable_bundle_keeps_every_file_it_makes_beside_its_executable() {
    let bench = Bench::new("portable-footprint", true);
    let count = bench.scratch.join("claude-runs.txt");
    let claude = claude_stand_in(&bench.scratch.join("bin"), &count);

    // A hop to Claude Code asks the binary for its version once, and remembers the answer.
    let (code, _, stderr) = hop_to_claude(&bench, &claude);
    assert_eq!(code, Some(0), "{stderr}");
    assert_eq!(runs(&count), 1);
    let cache = bench
        .bundle
        .join("mukade-data")
        .join("cache")
        .join("probes.json");
    assert!(
        cache.is_file(),
        "the cache is in mukade-data beside the executable"
    );
    // The next run is another process: it finds the answer in the cache, and does not ask again.
    let (code, _, stderr) = hop_to_claude(&bench, &claude);
    assert_eq!(code, Some(0), "{stderr}");
    assert_eq!(runs(&count), 1, "the second hop used the cache");

    // A hop to OpenCode needs a file to import: it is made in mukade-data/tmp and removed.
    let seen = bench.scratch.join("imported.txt");
    let opencode = opencode_stand_in(&bench.scratch.join("bin"), &seen);
    let (code, _, stderr) = hop_to_opencode(&bench, &opencode);
    assert_eq!(code, Some(0), "{stderr}");
    let tmp = bench.bundle.join("mukade-data").join("tmp");
    let imported = imported_from(&seen);
    assert_eq!(
        imported.parent(),
        Some(tmp.as_path()),
        "the document was made in mukade-data/tmp"
    );
    assert!(!imported.exists(), "and removed");
    assert!(
        files_under(&tmp).is_empty(),
        "no temporary file is left: {:?}",
        files_under(&tmp)
    );

    assert_eq!(
        bench.sandbox_files(),
        Vec::<PathBuf>::new(),
        "nothing reached the temporary folder, the home folder or the application-data folders"
    );
    // The only things beside the executable that mukade made are in mukade-data.
    let mut made: Vec<String> = std::fs::read_dir(&bench.bundle)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    made.sort();
    let exe_name = bench
        .exe()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    assert_eq!(
        made,
        [
            "mukade-data".to_string(),
            exe_name,
            "mukade.portable".to_string()
        ]
    );
}

#[test]
fn without_portable_mode_nothing_is_kept_and_the_temporary_file_is_removed() {
    let bench = Bench::new("plain-footprint", false);
    let count = bench.scratch.join("claude-runs.txt");
    let claude = claude_stand_in(&bench.scratch.join("bin"), &count);

    let (code, _, stderr) = hop_to_claude(&bench, &claude);
    assert_eq!(code, Some(0), "{stderr}");
    let (code, _, stderr) = hop_to_claude(&bench, &claude);
    assert_eq!(code, Some(0), "{stderr}");
    assert_eq!(
        runs(&count),
        2,
        "with no cache folder, the binary is asked each time"
    );

    let seen = bench.scratch.join("imported.txt");
    let opencode = opencode_stand_in(&bench.scratch.join("bin"), &seen);
    let (code, _, stderr) = hop_to_opencode(&bench, &opencode);
    assert_eq!(code, Some(0), "{stderr}");

    assert!(
        !bench.bundle.join("mukade-data").exists(),
        "nothing is created beside the executable"
    );
    let imported = imported_from(&seen);
    assert_eq!(
        imported.parent(),
        Some(bench.sandbox.join("temp").as_path()),
        "the document was made in the system's temporary folder"
    );
    assert!(!imported.exists(), "and removed");
    // The import document was made in the (sandboxed) system temporary folder, and is gone.
    assert_eq!(bench.sandbox_files(), Vec::<PathBuf>::new());
}

/// Asking for a cache folder keeps one, wherever it is, and only there.
#[test]
fn a_named_cache_folder_is_the_only_place_a_cache_goes() {
    let bench = Bench::new("named-cache", false);
    let count = bench.scratch.join("claude-runs.txt");
    let claude = claude_stand_in(&bench.scratch.join("bin"), &count);
    let cache = bench.scratch.join("my-cache");
    let cwd = bench.scratch.join("work");
    std::fs::create_dir_all(&cwd).unwrap();
    let hop = || {
        let mut command = Command::new(bench.exe());
        command
            .args([
                "hop",
                "claude_code",
                "x",
                "claude_code",
                "--from-file",
                &fixture(),
                "--cwd",
                cwd.to_str().unwrap(),
                "--claude-exe",
                claude.to_str().unwrap(),
            ])
            .env("CLAUDE_CONFIG_DIR", &bench.store)
            .env("MUKADE_CACHE_DIR", &cache)
            .env_remove("MUKADE_PORTABLE");
        command.output().unwrap()
    };
    assert!(hop().status.success());
    assert!(hop().status.success());
    assert_eq!(runs(&count), 1);
    assert!(cache.join("probes.json").is_file());
    assert!(!bench.bundle.join("mukade-data").exists());
}
