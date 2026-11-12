//! The architecture rule: a decision that depends on which harness a session comes from or goes to
//! is made in that harness's adapter, never by a branch on `HarnessId` elsewhere.

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
/// harness, and the enum itself. A path ending in `/` is a folder; any other is one file.
const ADAPTER_FILES: &[&str] = &[
    "src/harnesses/",
    "src/reader/claude_code.rs",
    "src/reader/claude_code/",
    "src/reader/claude_history.rs",
    "src/reader/claude_live.rs",
    "src/reader/codex.rs",
    "src/reader/codex/",
    "src/reader/codex_state.rs",
    "src/reader/opencode.rs",
    "src/reader/pi.rs",
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

/// Whether `relative` is one of the adapter files.
fn is_adapter_file(relative: &str) -> bool {
    ADAPTER_FILES
        .iter()
        .any(|entry| match entry.strip_suffix('/') {
            Some(_) => relative.starts_with(entry),
            None => relative == *entry,
        })
}

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
    let mut adapter_files = Vec::new();
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
        if is_adapter_file(&relative) {
            adapter_hits += hits.len();
            adapter_files.push(relative);
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

    assert!(
        walked > 50,
        "only {walked} files walked: the search lost its folders"
    );
    assert!(
        checked > 30,
        "only {checked} files checked: the adapter list skips too much"
    );
    assert!(
        adapter_hits > 0,
        "the adapters name no harness: the search finds nothing"
    );
    let missing: Vec<&str> = ADAPTER_FILES
        .iter()
        .filter(|entry| !adapter_files.iter().any(|file| file.starts_with(*entry)))
        .copied()
        .collect();
    assert!(
        missing.is_empty(),
        "adapter files that no longer exist: {missing:?}"
    );
    let stale: Vec<&str> = ALLOWED
        .iter()
        .zip(&allowed_hits)
        .filter(|(_, hits)| **hits == 0)
        .map(|((path, _), _)| *path)
        .collect();
    assert!(
        stale.is_empty(),
        "allowed files that no longer name a harness: {stale:?}"
    );
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
/// the item after a `#[cfg(test)]` line (see [`test_item_end`]).
fn arms_outside_tests(text: &str) -> Vec<(usize, &str)> {
    let lines: Vec<&str> = text.lines().collect();
    let mut hits = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index];
        if line.trim() == "#[cfg(test)]" {
            index = test_item_end(&lines, index);
            continue;
        }
        if HARNESS_ARMS.iter().any(|arm| line.contains(arm)) {
            hits.push((index + 1, line));
        }
        index += 1;
    }
    hits
}

/// The index of the first line after the item that `lines[attribute]`, a `#[cfg(test)]` line,
/// marks. Brackets are counted outside strings, characters and comments. The item ends:
///
/// 1. before a line, outside a string or comment, indented less than the attribute;
/// 2. after a line that leaves no bracket open and ends with `;`, `,` or `}`;
/// 3. after a line at the attribute's indentation that starts with `}`, `]` or `)` and does not
///    end by opening one.
///
/// Where the rules are unsure, the item ends early: code after it is read, never hidden.
fn test_item_end(lines: &[&str], attribute: usize) -> usize {
    let indent = indentation(lines[attribute]);
    let mut lexer = Lexer::default();
    for (index, line) in lines.iter().enumerate().skip(attribute + 1) {
        let in_code = lexer.state == State::Code;
        if in_code && !line.trim().is_empty() && indentation(line) < indent {
            return index;
        }
        let last = lexer.read_line(line);
        let balanced_end = lexer.depth <= 0 && matches!(last, Some(';' | ',' | '}'));
        let closing_line = in_code
            && indentation(line) == indent
            && line.trim_start().starts_with(['}', ']', ')'])
            && !matches!(last, Some('{' | '[' | '('));
        if balanced_end || closing_line {
            return index + 1;
        }
    }
    lines.len()
}

/// The width of a line's leading whitespace.
fn indentation(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// Where a reading of Rust source stands between two characters.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum State {
    #[default]
    Code,
    /// Inside `"…"`.
    Str,
    /// Inside a raw string closed by `"` and this many `#`.
    RawStr(usize),
    /// Inside this many nested `/* */`.
    Comment(usize),
}

/// A line-by-line reading of Rust source that counts open brackets.
#[derive(Debug, Default)]
struct Lexer {
    depth: i64,
    state: State,
}

impl Lexer {
    /// Read one line; its last character outside strings and comments, if any.
    fn read_line(&mut self, line: &str) -> Option<char> {
        let chars: Vec<char> = line.chars().collect();
        let mut last = None;
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            let next = chars.get(i + 1).copied();
            match self.state {
                State::Str => match c {
                    '\\' => i += 1,
                    '"' => {
                        self.state = State::Code;
                        last = Some(c);
                    }
                    _ => {}
                },
                State::RawStr(hashes) => {
                    if c == '"' && (1..=hashes).all(|k| chars.get(i + k) == Some(&'#')) {
                        i += hashes;
                        self.state = State::Code;
                        last = Some(c);
                    }
                }
                State::Comment(depth) => {
                    if c == '*' && next == Some('/') {
                        i += 1;
                        self.state = match depth {
                            1 => State::Code,
                            _ => State::Comment(depth - 1),
                        };
                    } else if c == '/' && next == Some('*') {
                        i += 1;
                        self.state = State::Comment(depth + 1);
                    }
                }
                State::Code => match c {
                    '/' if next == Some('/') => break,
                    '/' if next == Some('*') => {
                        i += 1;
                        self.state = State::Comment(1);
                    }
                    '"' => {
                        self.state = State::Str;
                        last = Some(c);
                    }
                    'r' if let Some(hashes) = raw_string_hashes(&chars, i) => {
                        i += hashes + 1;
                        self.state = State::RawStr(hashes);
                        last = Some('"');
                    }
                    '\'' => {
                        // A character literal is skipped; a lifetime or a label is one quote.
                        if next == Some('\\') {
                            i += 2;
                            while i < chars.len() && chars[i] != '\'' {
                                i += 1;
                            }
                        } else if chars.get(i + 2) == Some(&'\'') {
                            i += 2;
                        }
                        last = Some('\'');
                    }
                    _ => {
                        match c {
                            '{' | '[' | '(' => self.depth += 1,
                            '}' | ']' | ')' => self.depth -= 1,
                            _ => {}
                        }
                        if !c.is_whitespace() {
                            last = Some(c);
                        }
                    }
                },
            }
            i += 1;
        }
        last
    }
}

/// The number of `#` of a raw string that opens at `chars[i]` (`r"`, `r#"`, `br#"`), if one does.
fn raw_string_hashes(chars: &[char], i: usize) -> Option<usize> {
    let ident = |k: usize| chars[k].is_alphanumeric() || chars[k] == '_';
    let starts_a_token =
        i == 0 || !ident(i - 1) || (chars[i - 1] == 'b' && (i == 1 || !ident(i - 2)));
    if !starts_a_token {
        return None;
    }
    let hashes = chars[i + 1..].iter().take_while(|c| **c == '#').count();
    (chars.get(i + 1 + hashes) == Some(&'"')).then_some(hashes)
}

/// The line numbers `arms_outside_tests` reports in `text`.
fn reported(text: &str) -> Vec<usize> {
    arms_outside_tests(text)
        .into_iter()
        .map(|(line, _)| line)
        .collect()
}

#[test]
fn a_multi_line_test_use_ends_at_its_own_closing_line() {
    let text = "\
#[cfg(test)]
use crate::{
    a,
    b,
};
const P: HarnessId = HarnessId::Pi;
";
    assert_eq!(reported(text), [6]);
}

#[test]
fn a_test_field_ends_at_its_own_line() {
    let text = "\
struct S {
    #[cfg(test)]
    a: u8,
    b: Vec<u8>,
}
fn f() -> HarnessId {
    HarnessId::Pi
}
";
    assert_eq!(reported(text), [7]);
}

#[test]
fn a_multi_line_test_const_ends_at_its_own_closing_line() {
    let text = "\
#[cfg(test)]
const X: &[&str] = &[
    \"a\",
    \"b\",
];
fn g() -> HarnessId {
    HarnessId::Pi
}
";
    assert_eq!(reported(text), [7]);
}

#[test]
fn a_test_match_arm_ends_at_its_own_line() {
    let text = "\
fn h(n: u8) -> HarnessId {
    match n {
        #[cfg(test)]
        0 => HarnessId::Codex,
        _ => HarnessId::Pi,
    }
}
";
    assert_eq!(reported(text), [5]);
}

#[test]
fn a_test_statement_ends_at_its_closing_line_inside_a_block() {
    let text = "\
fn k(n: u8) {
    #[cfg(test)]
    let x = (
        n
    )
}
const P: HarnessId = HarnessId::Pi;
";
    assert_eq!(reported(text), [7]);
}

/// A test module and a test function whose signature spans lines are skipped whole, braces in
/// their strings included; the code after them is read.
#[test]
fn test_items_are_skipped_whole() {
    let text = "\
#[cfg(test)]
fn pick(
    explicit: Option<&str>,
) -> HarnessId {
    let _ = \"{\";
    HarnessId::Pi
}

fn pick_source() -> HarnessId {
    HarnessId::Codex
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t() {
        let _ = r#\"{ \"open\": [\"#;
        let _ = '{';
        assert_eq!(pick(None), HarnessId::Pi);
    }
}
const P: HarnessId = HarnessId::Pi;
";
    assert_eq!(reported(text), [10, 24]);
}

#[test]
fn a_test_parameter_ends_before_the_shallower_line() {
    let text = "\
fn k(
    a: u8,
    #[cfg(test)]
    b: u8
) -> HarnessId {
    HarnessId::Pi
}
";
    assert_eq!(reported(text), [6]);
}

/// A string that spans lines is read as a string: its braces count for nothing and its lines at
/// column 0 do not end the item.
#[test]
fn a_test_string_spanning_lines_stays_inside_its_item() {
    let text = "\
impl S {
    #[cfg(test)]
    const DOC: &str = r#\"
{
HarnessId::Pi
\"#;
    fn f() -> HarnessId {
        HarnessId::Codex
    }
}
";
    assert_eq!(reported(text), [8]);
}
