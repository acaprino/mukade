//! The manifest: what a hop did, what it carried and what it lost (§6.3).
//!
//! One JSON object written beside (or instead of) the destination session, with the same keys the
//! spec names, so a caller can decide whether the result is worth resuming without re-reading the
//! session. Every loss is a line in `losses`: the spec's rule 5 is that **no loss is silent**.

use crate::writer::plan::Counts;
use serde_json::{Map, Value, json};

/// One hop's record.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Manifest {
    pub source_harness: String,
    pub source_version: Option<String>,
    pub source_session_id: String,
    pub source_content_sha256: String,
    pub dest_harness: String,
    pub dest_version: Option<String>,
    pub dest_session_id: String,
    pub dest_location: String,
    /// `reader` / `writer` as `harness@version`, the pair that actually did the work.
    pub reader: String,
    pub writer: String,
    pub compaction: String,
    pub reasoning: String,
    pub resume_prompt: bool,
    pub counts: Counts,
    pub losses: Vec<String>,
    pub strict_checked: bool,
    pub strict_violations: Vec<String>,
}

impl Manifest {
    /// The manifest as the spec writes it.
    pub fn to_json(&self) -> Value {
        let mut counts = Map::new();
        counts.insert("messages_in".into(), json!(self.counts.messages_in));
        counts.insert("messages_out".into(), json!(self.counts.messages_out));
        counts.insert("tool_calls".into(), json!(self.counts.tools_translated));
        counts.insert("tools_text_form".into(), json!(self.counts.tools_text_form));
        counts.insert(
            "reasoning_dropped".into(),
            json!(self.counts.reasoning_dropped),
        );
        counts.insert(
            "dropped_orphan_results".into(),
            json!(self.counts.dropped_orphan_results),
        );
        counts.insert(
            "dropped_duplicate_results".into(),
            json!(self.counts.dropped_duplicate_results),
        );
        counts.insert(
            "rewritten_call_ids".into(),
            json!(self.counts.rewritten_call_ids),
        );
        counts.insert(
            "dropped_off_branch".into(),
            json!(self.counts.dropped_off_branch),
        );
        counts.insert(
            "dropped_unfinished_lines".into(),
            json!(self.counts.dropped_unfinished_lines),
        );
        counts.insert(
            "unanswered_calls".into(),
            json!(self.counts.unanswered_calls),
        );
        counts.insert(
            "dropped_empty_steps".into(),
            json!(self.counts.dropped_empty_steps),
        );
        counts.insert(
            "dropped_attachments".into(),
            json!(self.counts.dropped_attachments),
        );
        counts.insert(
            "dropped_result_blocks".into(),
            json!(self.counts.dropped_result_blocks),
        );

        json!({
            "source": {
                "harness": self.source_harness,
                "version": self.source_version,
                "session_id": self.source_session_id,
                "content_sha256": self.source_content_sha256,
            },
            "dest": {
                "harness": self.dest_harness,
                "version": self.dest_version,
                "session_id": self.dest_session_id,
                "location": self.dest_location,
            },
            "library": {
                "name": crate::wire::LIBRARY,
                "version": env!("CARGO_PKG_VERSION"),
                "reader": self.reader,
                "writer": self.writer,
            },
            "options": {
                "compaction": self.compaction,
                "reasoning": self.reasoning,
                "resume_prompt": self.resume_prompt,
            },
            "counts": Value::Object(counts),
            "losses": self.losses,
            "strict": {"checked": self.strict_checked, "violations": self.strict_violations},
        })
    }

    /// The losses the plan implies, in the order the spec lists them.
    pub fn losses_for(
        counts: &Counts,
        interrupted_last_turn: bool,
        compaction_carried: bool,
    ) -> Vec<String> {
        let mut losses = Vec::new();
        if counts.reasoning_dropped > 0 {
            losses.push(format!(
                "reasoning dropped: foreign provider or no opaque payload ({})",
                counts.reasoning_dropped
            ));
        }
        if counts.tools_text_form > 0 {
            losses.push(format!(
                "tool calls written as text, not translated ({}, {})",
                counts.tools_text_form,
                counts.text_form_tools.join(", ")
            ));
        }
        if counts.unanswered_calls > 0 {
            losses.push(format!(
                "unanswered tool calls carried as text ({})",
                counts.unanswered_calls
            ));
        }
        if counts.dropped_orphan_results > 0 {
            losses.push(format!(
                "orphan tool results dropped ({})",
                counts.dropped_orphan_results
            ));
        }
        if counts.dropped_duplicate_results > 0 {
            losses.push(format!(
                "duplicate tool results dropped, the first answer kept ({})",
                counts.dropped_duplicate_results
            ));
        }
        if counts.rewritten_call_ids > 0 {
            losses.push(format!(
                "tool call ids rewritten to the destination's id pattern ({})",
                counts.rewritten_call_ids
            ));
        }
        if counts.dropped_off_branch > 0 {
            losses.push(format!(
                "records of an abandoned branch (a rewind or an edited prompt) left behind ({})",
                counts.dropped_off_branch
            ));
        }
        if counts.dropped_unfinished_lines > 0 {
            losses.push(format!(
                "an unfinished last line of the source left unread ({})",
                counts.dropped_unfinished_lines
            ));
        }
        if counts.dropped_empty_steps > 0 {
            losses.push(format!(
                "empty steps dropped ({})",
                counts.dropped_empty_steps
            ));
        }
        if counts.dropped_attachments > 0 {
            losses.push(format!(
                "user attachments dropped ({})",
                counts.dropped_attachments
            ));
        }
        if counts.dropped_result_blocks > 0 {
            losses.push(format!(
                "structured tool-result blocks flattened to text ({})",
                counts.dropped_result_blocks
            ));
        }
        if interrupted_last_turn {
            losses.push("interrupted_last_turn".into());
        }
        if compaction_carried {
            losses.push("the source's compaction summary travels as one user message".into());
        }
        losses
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_manifest_has_the_keys_the_spec_names() {
        let counts = Counts {
            messages_in: 18,
            messages_out: 9,
            reasoning_dropped: 1,
            ..Counts::default()
        };
        let manifest = Manifest {
            source_harness: "codex".into(),
            source_version: Some("0.164.0".into()),
            source_session_id: "01a1".into(),
            source_content_sha256: "ab".repeat(32),
            dest_harness: "claude_code".into(),
            dest_version: Some("2.1.312".into()),
            dest_session_id: "d863".into(),
            dest_location: "C:/x.jsonl".into(),
            reader: "codex@0.164.0".into(),
            writer: "claude_code@2.1.312".into(),
            compaction: "none".into(),
            reasoning: "drop_foreign".into(),
            resume_prompt: false,
            counts: counts.clone(),
            losses: vec!["reasoning dropped: foreign provider (1)".into()],
            strict_checked: true,
            strict_violations: Vec::new(),
        };
        let json = manifest.to_json();
        for key in [
            "source", "dest", "library", "options", "counts", "losses", "strict",
        ] {
            assert!(json.get(key).is_some(), "missing {key}");
        }
        assert_eq!(json["source"]["harness"], "codex");
        assert_eq!(json["dest"]["session_id"], "d863");
        assert_eq!(json["options"]["compaction"], "none");
        assert_eq!(json["counts"]["messages_in"], 18);
        assert_eq!(json["strict"]["checked"], true);
        assert_eq!(json["losses"].as_array().unwrap().len(), 1);
        assert_eq!(json["library"]["name"], "mukade");
    }

    #[test]
    fn a_loss_line_appears_for_every_counted_loss() {
        let counts = Counts {
            reasoning_dropped: 3,
            tools_text_form: 1,
            text_form_tools: vec!["Glob".into()],
            dropped_orphan_results: 2,
            unanswered_calls: 1,
            dropped_empty_steps: 0,
            ..Counts::default()
        };
        let losses = Manifest::losses_for(&counts, true, true);
        assert_eq!(losses.len(), 6, "{losses:?}");
        assert!(losses.iter().any(|l| l.contains("Glob")));
        assert!(losses.iter().any(|l| l == "interrupted_last_turn"));
        assert!(
            !losses.iter().any(|l| l.contains("empty steps")),
            "zero losses are not reported"
        );
    }
}
