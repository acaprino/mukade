//! mukade is portable by construction, end to end: the real binary, copied alone into a folder,
//! with every place a program might leave files (the temporary folder, the home folder, the
//! application-data folders) pointed at an empty sandbox. Whatever mukade does, the sandbox must
//! stay empty, and everything mukade keeps must be in `mukade-data` beside the executable. There
//! is nothing to switch on: the folder holds the executable and nothing else.

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
    fn new(tag: &str) -> Bench {
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
            .env_remove("MUKADE_DATA_DIR")
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
        support::files_under(&self.sandbox)
    }
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
fn mukade_keeps_every_file_it_makes_beside_its_executable() {
    let bench = Bench::new("footprint");
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
        support::files_under(&tmp).is_empty(),
        "no temporary file is left: {:?}",
        support::files_under(&tmp)
    );

    assert_eq!(
        bench.sandbox_files(),
        Vec::<PathBuf>::new(),
        "nothing reached the temporary folder, the home folder or the application-data folders"
    );
    // Beside the executable mukade made `mukade-data` and the editable `intent-prompts` folder.
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
            "intent-prompts".to_string(),
            "mukade-data".to_string(),
            exe_name
        ]
    );
}

/// An executable somewhere it cannot make its folder (a read-only install; staged here with a
/// plain file where `mukade-data` should be): what needs no file still works and keeps no cache,
/// and what needs one (the OpenCode import document) fails with a reason instead of writing to the
/// system's temporary folder. Nothing reaches the sandbox either way.
#[test]
fn an_executable_that_cannot_make_its_folder_fails_where_it_needs_it_and_leaves_nothing() {
    let bench = Bench::new("blocked");
    let data = bench.bundle.join("mukade-data");
    std::fs::write(&data, "a file, not a folder").unwrap();
    let count = bench.scratch.join("claude-runs.txt");
    let claude = claude_stand_in(&bench.scratch.join("bin"), &count);

    let (code, _, stderr) = hop_to_claude(&bench, &claude);
    assert_eq!(code, Some(0), "{stderr}");
    let (code, _, stderr) = hop_to_claude(&bench, &claude);
    assert_eq!(code, Some(0), "{stderr}");
    assert_eq!(
        runs(&count),
        2,
        "with nowhere to keep a cache, the binary is asked each time"
    );

    let seen = bench.scratch.join("imported.txt");
    let opencode = opencode_stand_in(&bench.scratch.join("bin"), &seen);
    let (code, _, stderr) = hop_to_opencode(&bench, &opencode);
    assert_eq!(code, Some(4), "nothing was written: {stderr}");
    assert!(
        stderr.contains("mukade-data"),
        "the reason names the folder: {stderr}"
    );
    assert!(!seen.exists(), "opencode was never asked to import");
    assert_eq!(bench.sandbox_files(), Vec::<PathBuf>::new());
    assert!(data.is_file(), "what was there is untouched");
}

/// A relative `MUKADE_DATA_DIR` would put mukade's files wherever the caller happens to stand: it
/// is refused before the hop reads or writes anything.
#[test]
fn a_relative_data_directory_is_refused_before_import() {
    let scratch = support::scratch(HarnessId::OpenCode, "footprint-relative-data");
    let (code, _, stderr) = support::cli_in(
        &scratch,
        &scratch,
        &[
            "hop",
            "opencode",
            "fixture",
            "opencode",
            "--from-file",
            &fixture(),
            "--cwd",
            scratch.to_str().unwrap(),
            "--dest-version",
            "2.1.4",
            "--project-id",
            "global",
            "--model-provider",
            "h27",
            "--model",
            "m1",
            "--opencode-exe",
            "no-engine",
        ],
        &[
            (
                "OPENCODE_DB",
                scratch.join("destination.db").to_str().unwrap(),
            ),
            ("MUKADE_DATA_DIR", "relative-data"),
        ],
    );
    assert_ne!(code, Some(0));
    assert!(stderr.contains("MUKADE_DATA_DIR must be absolute"), "{stderr}");
    assert!(!scratch.join("relative-data").exists());
    assert!(!scratch.join("destination.db").exists());
}

/// The check is not tied to a destination that needs a temporary file: any command refuses it,
/// so a later step cannot write beside the current directory.
#[test]
fn a_relative_data_directory_is_refused_even_when_the_destination_needs_no_temporary_file() {
    let scratch = support::scratch(HarnessId::Pi, "footprint-relative-data-pi");
    let (code, _, stderr) = support::cli_in(
        &scratch,
        &scratch,
        &[
            "hop",
            "opencode",
            "fixture",
            "pi",
            "--from-file",
            &fixture(),
            "--cwd",
            scratch.to_str().unwrap(),
            "--dest-version",
            "1.4.0",
        ],
        &[
            ("PI_CODING_AGENT_DIR", scratch.to_str().unwrap()),
            ("MUKADE_DATA_DIR", "relative-data"),
        ],
    );
    assert_ne!(code, Some(0));
    assert!(stderr.contains("MUKADE_DATA_DIR must be absolute"), "{stderr}");
    assert!(!scratch.join("sessions").exists());
}
