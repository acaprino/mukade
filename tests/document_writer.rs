//! The interchange document written one message at a time (`wire::DocumentWriter`): the bytes
//! `to_document` gives, without holding the session (docs/adr/ADR-0004-size-agnostic-session-reads.md).

use mukade::model::{Entry, HarnessId, Kind, Meta, Session};
use mukade::wire::{DocumentWriter, to_document};
use serde_json::Value;
use std::path::PathBuf;

mod support;

/// What `to_document` serializes to, compact.
fn reference(session: &Session) -> Vec<u8> {
    let mut out = Vec::new();
    serde_json::to_writer(&mut out, &to_document(session)).unwrap();
    out
}

/// The document of `session` written entry by entry.
fn streamed(session: &Session) -> Vec<u8> {
    let mut writer = DocumentWriter::new(Vec::new(), &session.meta).unwrap();
    for entry in &session.entries {
        writer.entry(entry).unwrap();
    }
    writer
        .finish_with_branches(&session.meta, &session.branches)
        .unwrap()
}

fn assert_same_bytes(session: &Session, what: &str) {
    let (want, got) = (reference(session), streamed(session));
    assert!(
        want == got,
        "{what}: {} bytes against {}; first difference at byte {:?}",
        got.len(),
        want.len(),
        want.iter().zip(&got).position(|(a, b)| a != b)
    );
}

fn all_fixtures() -> Vec<(String, Session)> {
    let mut sessions = Vec::new();
    for harness in HarnessId::ALL {
        sessions.push((harness.to_string(), support::read_source(harness)));
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/claude_code/subagents");
    for path in support::files_under(&root) {
        if path.extension().is_some_and(|e| e == "jsonl")
            && let Ok(session) = mukade::reader::claude_code::read_file(&path)
        {
            sessions.push((path.display().to_string(), session));
        }
    }
    // Every branch the source keeps, as `branches`.
    for (harness, path) in [
        (
            HarnessId::ClaudeCode,
            support::source_path(HarnessId::ClaudeCode),
        ),
        (HarnessId::Codex, support::source_path(HarnessId::Codex)),
        (HarnessId::Pi, support::source_path(HarnessId::Pi)),
    ] {
        let adapter = mukade::harnesses::get(harness);
        if let Some(Ok(session)) = adapter.read_all_branches(&path) {
            sessions.push((format!("{harness} (all branches)"), session));
        }
    }
    sessions
}

#[test]
fn the_streamed_document_is_the_bytes_to_document_gives_for_every_fixture() {
    let sessions = all_fixtures();
    assert!(sessions.len() >= 9, "{}", sessions.len());
    for (what, session) in &sessions {
        assert_same_bytes(session, what);
    }
}

#[test]
fn a_session_with_branches_and_a_compaction_is_written_alike() {
    // The fixtures carry compactions (Claude, Codex); a hand-made one carries branches whatever
    // the fixtures hold.
    let mut session = support::read_source(HarnessId::ClaudeCode);
    assert!(
        session
            .entries
            .iter()
            .any(|e| matches!(e.kind, Kind::Compaction { .. })),
        "a fixture with a compaction"
    );
    let branch = mukade::model::Branch {
        from: session.entries.first().map(|e| e.id.clone()),
        entries: session.entries.iter().skip(3).take(4).cloned().collect(),
    };
    session.branches = vec![
        branch.clone(),
        mukade::model::Branch {
            from: None,
            entries: vec![],
        },
    ];
    assert_same_bytes(&session, "branches");
    session.branches.clear();
    assert_same_bytes(&session, "no branches");
}

#[test]
fn a_session_without_conversation_is_an_empty_list() {
    let mut session = support::read_source(HarnessId::Pi);
    session.entries.clear();
    assert_same_bytes(&session, "empty");
    session.entries = support::read_source(HarnessId::Pi)
        .entries
        .into_iter()
        .filter(|e| matches!(e.kind, Kind::Meta { .. }))
        .collect();
    assert_same_bytes(&session, "meta only");
}

#[test]
fn the_document_a_streamed_read_feeds_is_the_documents_of_the_full_read() {
    let dir = support::scratch(HarnessId::ClaudeCode, "document-writer");
    for harness in [HarnessId::ClaudeCode, HarnessId::Pi, HarnessId::Codex] {
        support::stage_fixture(harness, &dir);
    }
    let roots = support::roots(&dir);
    for harness in [HarnessId::ClaudeCode, HarnessId::Pi, HarnessId::Codex] {
        let reader = mukade::reader::for_harness(harness).unwrap();
        let id = support::source_id(harness);
        let full = reader.read(id, &roots).unwrap();
        let writer: std::cell::RefCell<Option<DocumentWriter<Vec<u8>>>> = Default::default();
        let meta = reader
            .read_streamed_with_meta(
                id,
                &roots,
                &mut |meta| {
                    assert!(writer.borrow().is_none(), "the meta is handed over once");
                    *writer.borrow_mut() = Some(DocumentWriter::new(Vec::new(), meta)?);
                    Ok(())
                },
                &mut |entry| {
                    writer
                        .borrow_mut()
                        .as_mut()
                        .expect("the meta came first")
                        .entry(&entry)
                },
            )
            .unwrap();
        let bytes = writer.into_inner().unwrap().finish(&meta).unwrap();
        assert!(bytes == reference(&full), "{harness}");
    }
}

#[test]
fn a_document_is_refused_when_the_meta_it_opened_with_was_not_the_final_one() {
    let session = support::read_source(HarnessId::Pi);
    let writer = DocumentWriter::new(Vec::new(), &session.meta).unwrap();
    let mut other: Meta = session.meta.clone();
    other.title = Some("another title".into());
    let error = writer.finish(&other).err().expect("refused");
    assert!(error.to_string().contains("meta changed"), "{error}");
}

#[test]
fn a_message_can_be_changed_before_it_is_written() {
    // What an exporter does per message: drop the reasoning, redact.
    let session = support::read_source(HarnessId::ClaudeCode);
    let change = |message: &mut Value| -> anyhow::Result<()> {
        if let Some(content) = message["content"].as_array_mut() {
            content.retain(|block| block["type"] != "thinking");
            for block in content {
                if let Some(text) = block.get("text").and_then(Value::as_str).map(str::to_owned) {
                    block["text"] = Value::String(text.replace("alpha", "[redacted]"));
                }
            }
        }
        Ok(())
    };
    let mut writer = DocumentWriter::new(Vec::new(), &session.meta).unwrap();
    for entry in &session.entries {
        writer.entry_with(entry, &mut |m| change(m)).unwrap();
    }
    let got: Value = serde_json::from_slice(&writer.finish(&session.meta).unwrap()).unwrap();
    let mut want = to_document(&session);
    for message in want["messages"].as_array_mut().unwrap() {
        change(message).unwrap();
    }
    assert_eq!(got, want);
    assert!(!got.to_string().contains("\"thinking\""));
}

#[test]
fn the_lower_level_writes_the_messages_it_is_given() {
    let session = support::read_source(HarnessId::Pi);
    let doc = to_document(&session);
    let mut writer = DocumentWriter::new(Vec::new(), &session.meta).unwrap();
    for message in doc["messages"].as_array().unwrap() {
        writer.message(message).unwrap();
    }
    let bytes = writer.finish(&session.meta).unwrap();
    assert!(bytes == reference(&session));
}

/// Counts what reaches it.
struct Counting(std::rc::Rc<std::cell::Cell<usize>>);

impl std::io::Write for Counting {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.set(self.0.get() + bytes.len());
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn the_writer_holds_no_message_it_has_been_given() {
    // 400 results of 250 KB, each built and dropped by the caller: every message is on its way to
    // the output when `entry` returns, so the writer keeps none (it has no buffer of its own).
    let session = support::read_source(HarnessId::ClaudeCode);
    let written = std::rc::Rc::new(std::cell::Cell::new(0));
    let mut writer = DocumentWriter::new(Counting(written.clone()), &session.meta).unwrap();
    let opened = written.get();
    assert!(opened > 0);
    let text = "x".repeat(250_000);
    for n in 0..400 {
        let entry = Entry {
            id: format!("big-{n}"),
            kind: Kind::User {
                text: text.clone(),
                attachments: Vec::new(),
            },
            raw: Vec::new(),
        };
        let before = written.get();
        writer.entry(&entry).unwrap();
        assert!(
            written.get() >= before + text.len(),
            "entry {n} was not written through"
        );
    }
    writer.finish(&session.meta).unwrap();
    assert!(written.get() > 400 * 250_000);
    assert_eq!(std::mem::size_of::<DocumentWriter<Counting>>() < 256, true);
}
