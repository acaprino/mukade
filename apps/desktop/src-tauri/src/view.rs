//! The receipt of a hop or an import, as the window shows it (spec F6): plain records the
//! TypeScript side mirrors by hand (`src/types.ts`). Pure, so the tests need no window.

use crate::locations::Location;
use mukade::reader::SessionInfo;
use serde::Serialize;
use serde_json::Value;

/// A hop's or an import's result, as the receipt dialog shows it (spec F6).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ReceiptView {
    pub dest_id: String,
    pub dest_label: String,
    /// The destination installation's key, to show the new session in the tree.
    pub dest_key: String,
    pub location: String,
    pub headless: Vec<String>,
    /// The command that opens it in the destination's own interface; empty with `refusal`.
    pub interactive: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refusal: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest_path: Option<String>,
    pub losses: Vec<String>,
    pub children: Vec<ChildLine>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply: Option<String>,
    /// The session was written but something after it failed (the resume prompt, a subagent).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ChildLine {
    pub source_id: String,
    pub dest_id: String,
    pub linked: bool,
}

pub fn receipt(
    receipt: &mukade::Receipt,
    dest_key: &str,
    dest: &Location,
    manifest_path: Option<String>,
    warning: Option<String>,
) -> ReceiptView {
    let info = SessionInfo {
        id: receipt.dest_id.clone(),
        path: receipt.location.clone(),
        ..Default::default()
    };
    let refusal = mukade::harnesses::get(dest.harness).resume_refusal(&info);
    let interactive = if refusal.is_some() {
        Vec::new()
    } else {
        mukade::catalog::interactive_resume(
            dest.harness,
            &dest.engine(),
            &info.id,
            Some(std::path::Path::new(&info.path)),
        )
    };
    let losses = receipt
        .manifest
        .get("losses")
        .and_then(Value::as_array)
        .map(|losses| {
            losses
                .iter()
                .map(|loss| {
                    loss.as_str()
                        .map(str::to_string)
                        .unwrap_or_else(|| serde_json::to_string(loss).unwrap_or_default())
                })
                .collect()
        })
        .unwrap_or_default();
    ReceiptView {
        dest_id: receipt.dest_id.clone(),
        dest_label: dest.label.clone(),
        dest_key: dest_key.to_string(),
        location: receipt.location.clone(),
        headless: receipt.resume_command.clone(),
        interactive,
        refusal,
        manifest_path,
        losses,
        children: child_lines(&receipt.manifest),
        reply: receipt.resume_output.clone(),
        warning,
    }
}

/// What became of each subagent session a hop carried (the manifest's `children`).
pub fn child_lines(manifest: &Value) -> Vec<ChildLine> {
    manifest
        .get("children")
        .and_then(Value::as_array)
        .map(|children| {
            children
                .iter()
                .map(|child| {
                    let text = |key: &str| {
                        child
                            .get(key)
                            .and_then(Value::as_str)
                            .unwrap_or("?")
                            .to_string()
                    };
                    ChildLine {
                        source_id: text("source_id"),
                        dest_id: text("dest_id"),
                        linked: child.get("link").and_then(Value::as_str) == Some("native"),
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn child_lines_say_whether_the_destination_linked_each_child() {
        let manifest = json!({"children": [
            {"source_id": "a", "dest_id": "x", "link": "native"},
            {"source_id": "b", "dest_id": "y", "link": "none"},
        ]});
        assert_eq!(
            child_lines(&manifest),
            [
                ChildLine {
                    source_id: "a".into(),
                    dest_id: "x".into(),
                    linked: true
                },
                ChildLine {
                    source_id: "b".into(),
                    dest_id: "y".into(),
                    linked: false
                },
            ]
        );
        assert!(child_lines(&json!({})).is_empty());
    }
}
