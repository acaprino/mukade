//! The prompt an intent hands to the source harness's own model.
//!
//! The defaults are embedded in the binary and copied, once, to `intent-prompts` beside the
//! executable. Every `.md` file in that folder is an intent whose name is the file stem: edit a
//! default, or drop in a new file, and the next summary can select it with `--intent <name>`.
//! `generic.md` is used when no intent is named.
//!
//! The text is passed as one command-line argument. Whitespace in the file is therefore folded to
//! single spaces: Markdown paragraphs stay readable in the file and reach a Windows `.cmd` launcher
//! as one line. A batch launcher still gets only the character subset it can pass safely, and a
//! prompt outside it is refused before the harness starts (see `crate::summary`).

use std::io::Write;
use std::path::{Path, PathBuf};

/// The folder, beside the executable, that holds the editable prompts.
pub const FOLDER: &str = "intent-prompts";

/// The intent used when `--intent` is not given.
pub const DEFAULT_INTENT: &str = "generic";

/// A prompt file larger than this is almost certainly a command-line limit waiting to fail.
pub const MAX_PROMPT_BYTES: u64 = 64 * 1024;

/// The prompts that ship with mukade. The first one is the default.
pub const DEFAULT_PROMPTS: &[(&str, &str)] = &[
    (
        "generic",
        include_str!("../assets/intent-prompts/generic.md"),
    ),
    (
        "continue",
        include_str!("../assets/intent-prompts/continue.md"),
    ),
    (
        "execute",
        include_str!("../assets/intent-prompts/execute.md"),
    ),
    ("review", include_str!("../assets/intent-prompts/review.md")),
    (
        "analyze",
        include_str!("../assets/intent-prompts/analyze.md"),
    ),
    (
        "brainstorm",
        include_str!("../assets/intent-prompts/brainstorm.md"),
    ),
    ("plan", include_str!("../assets/intent-prompts/plan.md")),
];

/// One prompt that can be selected with `--intent`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    /// Whether this name is one of the prompts that ship with mukade.
    pub builtin: bool,
    /// Whether a file in the prompt folder supplies it (an edited default is also custom).
    pub custom: bool,
}

/// A prompt resolved for one summarizing run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPrompt {
    pub name: String,
    pub text: String,
}

/// Where the prompts live: `MUKADE_INTENT_DIR` when a host sets it, else `intent-prompts` beside
/// the executable.
pub fn prompt_dir() -> Result<PathBuf, String> {
    if let Some(dir) = std::env::var_os("MUKADE_INTENT_DIR") {
        if dir.is_empty() {
            return Err("MUKADE_INTENT_DIR is empty".into());
        }
        let dir = PathBuf::from(dir);
        if !dir.is_absolute() {
            return Err("MUKADE_INTENT_DIR must be an absolute folder".into());
        }
        return Ok(dir);
    }
    let exe = std::env::current_exe()
        .map_err(|error| format!("cannot locate the mukade executable: {error}"))?;
    let parent = exe
        .parent()
        .ok_or_else(|| format!("{} has no parent folder", exe.display()))?;
    Ok(parent.join(FOLDER))
}

/// Create the folder and write every default prompt that is not there yet. Existing files are
/// never replaced, so an edited prompt survives every later start. Called on start-up; a failure
/// is reported by the command that needs a prompt, not by unrelated commands.
pub fn install_defaults() -> Result<PathBuf, String> {
    let dir = prompt_dir()?;
    install_defaults_in(&dir)?;
    Ok(dir)
}

/// [`install_defaults`] for an explicit folder, for tests and hosts that choose one.
pub fn install_defaults_in(dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|error| {
        format!(
            "cannot create the intent prompt folder {}: {error}",
            dir.display()
        )
    })?;
    for (name, text) in DEFAULT_PROMPTS {
        let path = dir.join(format!("{name}.md"));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => file
                .write_all(text.as_bytes())
                .map_err(|error| format!("cannot write {}: {error}", path.display()))?,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => {
                return Err(format!("cannot write {}: {error}", path.display()));
            }
        }
    }
    Ok(())
}

/// Every intent this installation offers, defaults and custom files together, by name.
pub fn list() -> Result<Vec<Entry>, String> {
    let dir = prompt_dir()?;
    Ok(list_in(&dir))
}

/// [`list`] for an explicit folder.
pub fn list_in(dir: &Path) -> Vec<Entry> {
    let mut entries: Vec<Entry> = DEFAULT_PROMPTS
        .iter()
        .map(|(name, _)| Entry {
            name: (*name).to_string(),
            path: dir.join(format!("{name}.md")),
            builtin: true,
            custom: false,
        })
        .collect();
    if let Ok(files) = std::fs::read_dir(dir) {
        for file in files.flatten() {
            let path = file.path();
            if !path.is_file()
                || !path
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
            {
                continue;
            }
            let Some(name) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            if name.is_empty() || !valid_name(name) {
                continue;
            }
            match entries.iter_mut().find(|entry| entry.name == name) {
                Some(entry) => {
                    entry.path = path;
                    entry.custom = true;
                }
                None => entries.push(Entry {
                    name: name.to_string(),
                    path,
                    builtin: false,
                    custom: true,
                }),
            }
        }
    }
    entries.sort_by(|left, right| left.name.cmp(&right.name));
    entries
}

/// Resolve the prompt to ask for: the named intent, or the generic one. A file in the prompt folder
/// wins over the embedded default of the same name.
pub fn resolve(name: Option<&str>) -> Result<ResolvedPrompt, String> {
    let dir = prompt_dir()?;
    resolve_in(&dir, name)
}

/// [`resolve`] for an explicit folder.
pub fn resolve_in(dir: &Path, name: Option<&str>) -> Result<ResolvedPrompt, String> {
    let name = name.unwrap_or(DEFAULT_INTENT);
    if !valid_name(name) {
        return Err(format!(
            "invalid intent name {name}: use a file name without its .md extension"
        ));
    }
    let path = prompt_path(dir, name).unwrap_or_else(|| dir.join(format!("{name}.md")));
    let (text, origin) = match std::fs::read(&path) {
        Ok(bytes) => {
            if bytes.len() as u64 > MAX_PROMPT_BYTES {
                return Err(format!(
                    "intent prompt {} is larger than {} bytes",
                    path.display(),
                    MAX_PROMPT_BYTES
                ));
            }
            let text = String::from_utf8(bytes)
                .map_err(|_| format!("intent prompt {} is not UTF-8", path.display()))?;
            (text, path.display().to_string())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            match DEFAULT_PROMPTS.iter().find(|(builtin, _)| *builtin == name) {
                Some((_, text)) => ((*text).to_string(), "the built-in prompt".to_string()),
                None => {
                    let available = list_in(dir);
                    let names: Vec<&str> =
                        available.iter().map(|entry| entry.name.as_str()).collect();
                    return Err(format!(
                        "unknown intent {name}: no {} and no built-in prompt; available: {}",
                        path.display(),
                        names.join(", ")
                    ));
                }
            }
        }
        Err(error) => return Err(format!("cannot read {}: {error}", path.display())),
    };
    let text = one_line(&text);
    if text.is_empty() {
        return Err(format!("intent prompt {origin} is empty"));
    }
    Ok(ResolvedPrompt {
        name: name.to_string(),
        text,
    })
}

/// The file that supplies `name`, accepting a `.md` extension in any case. The direct path is
/// tried first; the folder is scanned only when it is not there, so a case-sensitive filesystem
/// reads `Review.MD` for the intent `Review` too.
fn prompt_path(dir: &Path, name: &str) -> Option<PathBuf> {
    let direct = dir.join(format!("{name}.md"));
    if direct.is_file() {
        return Some(direct);
    }
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .find(|path| {
            path.is_file()
                && path
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
                && path.file_stem().and_then(|stem| stem.to_str()) == Some(name)
        })
}

/// A name is one path component that can be joined to the prompt folder without escaping it.
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains(['/', '\\', '\0'])
        && !name.to_ascii_lowercase().ends_with(".md")
}

/// Markdown is written for a person; the harness gets the same words on one line.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}
