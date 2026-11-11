//! The architecture rule: a decision that depends on which harness a session comes from or goes to
//! is made in that harness's adapter, never by naming the harness elsewhere.

use std::path::Path;

/// The names a branch on one harness is written with.
const HARNESS_ARMS: [&str; 4] = [
    "HarnessId::ClaudeCode",
    "HarnessId::Codex",
    "HarnessId::OpenCode",
    "HarnessId::Pi",
];

/// The folders searched, from the repository root.
const SEARCHED: [&str; 2] = ["src", "mukade-tui/src"];

/// Where naming a harness is the point: the adapters, the readers, writers and detectors of one
/// harness, and the enum itself. A path ending in `/` is a folder; any other is a file name
/// prefix (`src/reader/codex` covers `codex.rs` and `codex_state.rs`).
const ADAPTER_FILES: &[&str] = &[
    "src/harnesses/",
    "src/reader/claude_code",
    "src/reader/claude_history",
    "src/reader/claude_live",
    "src/reader/codex",
    "src/reader/codex_state",
    "src/reader/opencode",
    "src/reader/pi",
    "src/reader/codex/",
    "src/reader/claude_code/",
    "src/writer/claude_code.rs",
    "src/writer/codex.rs",
    "src/writer/opencode.rs",
    "src/writer/pi.rs",
    "src/detect/claude_code.rs",
    "src/detect/codex.rs",
    "src/detect/opencode.rs",
    "src/detect/pi.rs",
    "src/model/harness.rs",
];

/// Files allowed to name a harness outside its adapter, each with its reason. Empty: every such
/// decision has moved into an adapter.
const ALLOWED: &[(&str, &str)] = &[];

#[test]
fn no_harness_branch_outside_its_adapter() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    for folder in SEARCHED {
        rust_files(&root.join(folder), &mut files);
    }
    let mut walked = 0;
    let mut checked = 0;
    let mut adapter_hits = 0;
    let mut sites = Vec::new();
    let mut allowed_hits = vec![0; ALLOWED.len()];
    for file in &files {
        walked += 1;
        let relative = file
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let text = std::fs::read_to_string(file).unwrap();
        let hits = arms_outside_tests(&text);
        if ADAPTER_FILES.iter().any(|skip| relative.starts_with(skip)) {
            adapter_hits += hits.len();
            continue;
        }
        checked += 1;
        if let Some(index) = ALLOWED.iter().position(|(path, _)| *path == relative) {
            allowed_hits[index] += hits.len();
            continue;
        }
        sites.extend(
            hits.into_iter()
                .map(|(line, text)| format!("{relative}:{line}: {}", text.trim())),
        );
    }

    assert!(walked > 50, "only {walked} files walked: the search lost its folders");
    assert!(checked > 30, "only {checked} files checked: the adapter list skips too much");
    assert!(adapter_hits > 0, "the adapters name no harness: the search finds nothing");
    let stale: Vec<&str> = ALLOWED
        .iter()
        .zip(&allowed_hits)
        .filter(|(_, hits)| **hits == 0)
        .map(|((path, _), _)| *path)
        .collect();
    assert!(stale.is_empty(), "allowed files that no longer name a harness: {stale:?}");
    assert!(
        sites.is_empty(),
        "{} harness branches outside an adapter; move each into `harnesses::get(id)`:\n{}",
        sites.len(),
        sites.join("\n")
    );
}

/// Every `.rs` file under `folder`.
fn rust_files(folder: &Path, files: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(folder).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}

/// The lines of `text`, numbered from 1, that name one harness, test code left out. Test code is
/// the item after a `#[cfg(test)]` line: through the `}` at the attribute's own indentation, or
/// its first line alone when that line ends the item (`;` or `}`).
fn arms_outside_tests(text: &str) -> Vec<(usize, &str)> {
    let mut hits = Vec::new();
    let mut lines = text.lines().enumerate();
    while let Some((index, line)) = lines.next() {
        if line.trim() == "#[cfg(test)]" {
            let indent = &line[..line.len() - line.trim_start().len()];
            let close = format!("{indent}}}");
            let mut first = true;
            for (_, item) in lines.by_ref() {
                let item = item.trim_end();
                let start = item.trim_start();
                if first && (start.starts_with("#[") || start.starts_with("//")) {
                    continue;
                }
                if (first && (item.ends_with(';') || item.ends_with('}'))) || item == close {
                    break;
                }
                first = false;
            }
            continue;
        }
        if HARNESS_ARMS.iter().any(|arm| line.contains(arm)) {
            hits.push((index + 1, line));
        }
    }
    hits
}
