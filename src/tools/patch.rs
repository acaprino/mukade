//! Codex's `apply_patch` input read as the write or edit it makes (tables §4.3, §5).
//!
//! The call's recorded input stays the patch (`ToolCall::input`); this reading is made when the
//! call is translated. Only a patch of exactly one plain file section is read: `*** Add File`
//! becomes a write, `*** Update File` with one hunk an edit, with several hunks a multi-edit.
//! A delete, a move, an end-of-file anchor, several files or anything unexpected is not read,
//! and the call keeps travelling as text form.

use crate::model::CanonicalTool;
use serde_json::{Value, json};

/// The canonical call a Codex `apply_patch` input makes, with the input in canonical spelling
/// (`file_path`, `content`, `old_string`, `new_string`, `edits`). The input is the patch string
/// itself, or an object holding it under `input`.
pub(crate) fn view(input: &Value) -> Option<(CanonicalTool, Value)> {
    let text = match input {
        Value::String(s) => s.as_str(),
        Value::Object(o) => o.get("input")?.as_str()?,
        _ => return None,
    };
    let mut lines: Vec<&str> = text.split('\n').collect();
    while lines.last().is_some_and(|l| l.trim().is_empty()) {
        lines.pop();
    }
    if lines.first().map(|l| l.trim_end()) != Some("*** Begin Patch")
        || lines.last().map(|l| l.trim_end()) != Some("*** End Patch")
        || lines.len() < 3
    {
        return None;
    }
    let (header, body) = lines[1..lines.len() - 1].split_first()?;
    // A second section, a move or an end-of-file anchor is another `***` line.
    if body.iter().any(|l| l.starts_with("***")) {
        return None;
    }
    if let Some(path) = section_path(header, "*** Add File: ") {
        let content: Vec<&str> = body
            .iter()
            .map(|l| l.strip_prefix('+'))
            .collect::<Option<_>>()?;
        let mut content = content.join("\n");
        content.push('\n');
        return Some((
            CanonicalTool::Write,
            json!({"file_path": path, "content": content}),
        ));
    }
    let path = section_path(header, "*** Update File: ")?;
    let mut hunks = hunks(body)?;
    if hunks.len() == 1 {
        let (old, new) = hunks.remove(0);
        return Some((
            CanonicalTool::Edit,
            json!({"file_path": path, "old_string": old, "new_string": new}),
        ));
    }
    let edits: Vec<Value> = hunks
        .into_iter()
        .map(|(old, new)| json!({"old_string": old, "new_string": new}))
        .collect();
    Some((
        CanonicalTool::MultiEdit,
        json!({"file_path": path, "edits": edits}),
    ))
}

fn section_path<'a>(header: &'a str, marker: &str) -> Option<&'a str> {
    let path = header.strip_prefix(marker)?.trim();
    (!path.is_empty()).then_some(path)
}

/// The `(old, new)` text of each `@@` hunk: context and removed lines make the old text, context
/// and added lines the new one. A hunk that removes nothing and has no context is an insertion
/// with no anchor, which no edit tool can express.
fn hunks(body: &[&str]) -> Option<Vec<(String, String)>> {
    let mut hunks: Vec<(Vec<&str>, Vec<&str>)> = Vec::new();
    for line in body {
        if line.starts_with("@@") {
            hunks.push((Vec::new(), Vec::new()));
            continue;
        }
        let (old, new) = hunks.last_mut()?;
        if line.is_empty() {
            old.push("");
            new.push("");
        } else if let Some(rest) = line.strip_prefix(' ') {
            old.push(rest);
            new.push(rest);
        } else if let Some(rest) = line.strip_prefix('-') {
            old.push(rest);
        } else if let Some(rest) = line.strip_prefix('+') {
            new.push(rest);
        } else {
            return None;
        }
    }
    if hunks.is_empty() || hunks.iter().any(|(old, _)| old.is_empty()) {
        return None;
    }
    Some(
        hunks
            .into_iter()
            .map(|(old, new)| (old.join("\n"), new.join("\n")))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn patch(body: &str) -> String {
        format!("*** Begin Patch\n{body}\n*** End Patch")
    }

    #[test]
    fn an_added_file_is_a_write() {
        let got = view(&json!(patch("*** Add File: D:/w/a.txt\n+L1\n+L2")));
        assert_eq!(
            got,
            Some((
                CanonicalTool::Write,
                json!({"file_path": "D:/w/a.txt", "content": "L1\nL2\n"})
            ))
        );
    }

    #[test]
    fn one_hunk_is_an_edit() {
        let got = view(&json!(patch(
            "*** Update File: D:/w/a.txt\n@@ fn anchor()\n ctx\n-old\n+new\n ctx2"
        )));
        assert_eq!(
            got,
            Some((
                CanonicalTool::Edit,
                json!({"file_path": "D:/w/a.txt", "old_string": "ctx\nold\nctx2", "new_string": "ctx\nnew\nctx2"})
            ))
        );
    }

    #[test]
    fn an_empty_line_in_a_hunk_is_an_empty_context_line() {
        let got = view(&json!(patch(
            "*** Update File: D:/w/a.txt\n@@\n a\n\n-b\n+c"
        )));
        assert_eq!(got.unwrap().1["old_string"], "a\n\nb");
    }

    #[test]
    fn several_hunks_are_a_multi_edit() {
        let got = view(&json!(patch(
            "*** Update File: D:/w/a.txt\n@@\n-a\n+b\n@@\n x\n-c\n+d"
        )));
        assert_eq!(
            got,
            Some((
                CanonicalTool::MultiEdit,
                json!({"file_path": "D:/w/a.txt", "edits": [
                    {"old_string": "a", "new_string": "b"},
                    {"old_string": "x\nc", "new_string": "x\nd"},
                ]})
            ))
        );
    }

    #[test]
    fn the_object_shape_reads_too() {
        let body = patch("*** Add File: D:/w/a.txt\n+L1");
        assert_eq!(view(&json!({"input": body})), view(&json!(body)));
        assert!(view(&json!({"input": body})).is_some());
    }

    #[test]
    fn a_pure_insertion_hunk_is_not_an_edit() {
        assert_eq!(
            view(&json!(patch("*** Update File: D:/w/a.txt\n@@\n+new"))),
            None
        );
    }

    #[test]
    fn what_is_not_one_plain_file_is_not_read() {
        for input in [
            json!(patch("*** Delete File: D:/w/a.txt")),
            json!(patch(
                "*** Update File: D:/w/a.txt\n*** Move to: D:/w/b.txt\n@@\n-a\n+b"
            )),
            json!(patch(
                "*** Update File: D:/w/a.txt\n@@\n-a\n+b\n*** End of File"
            )),
            json!(patch(
                "*** Add File: D:/w/a.txt\n+L1\n*** Add File: D:/w/b.txt\n+L2"
            )),
            json!(patch(
                "*** Update File: D:/w/a.txt\n@@\n-a\n+b\n*** Update File: D:/w/b.txt\n@@\n-c\n+d"
            )),
            json!(patch("*** Update File: D:/w/a.txt")),
            json!(patch("*** Update File: D:/w/a.txt\n-a\n+b")),
            json!(patch("*** Update File: D:/w/a.txt\n@@\n-a\n?b")),
            json!(patch("*** Add File: D:/w/a.txt\n L1")),
            json!(patch("*** Add File: \n+L1")),
            json!("*** Add File: D:/w/a.txt\n+L1\n*** End Patch"),
            json!("*** Begin Patch\n*** Add File: D:/w/a.txt\n+L1"),
            json!(42),
            json!({"patch": "x"}),
        ] {
            assert_eq!(view(&input), None, "{input}");
        }
    }
}
