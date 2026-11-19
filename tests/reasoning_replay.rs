//! Gateway/OpenRouter reasoning survives a session conversion, not only its visible summary.
use mukade::model::{HarnessId, Kind, Protocol};
use mukade::policy::{Clock, ReasoningPolicy};
use mukade::writer::{WriteOpts, opencode::document_for, plan};
use serde_json::{Value, json};

fn fixture(provider: &str, format: &str) -> (Value, Value) {
    let details = json!([
        {"type":"reasoning.summary","summary":"Read the two files.","id":"rs_a","format":format,"index":0},
        {"type":"reasoning.encrypted","data":"opaque-A+/==","id":"rs_a","format":format,"index":0,"extension":{"keep":true}},
        {"type":"reasoning.encrypted","data":"opaque-B+/==","id":"rs_b","format":format,"index":1}
    ]);
    let doc = json!({"info":{"id":"ses_proof","directory":"D:/fixture","time":{"created":1,"updated":2}},
    "messages":[
        {"id":"msg_u","type":"user","text":"Read two files.","time":{"created":1}},
        {"id":"msg_a","type":"assistant","finish":"tool-calls","model":{"providerID":provider,"id":"gpt-6.1-sol"},
         "time":{"created":2},"content":[
             {"type":"reasoning","text":"Read the two files.","native":{"reasoningDetails":details}},
             {"type":"tool","id":"read_a","name":"read","state":{"status":"completed","input":{"path":"a.txt"},"content":[{"type":"text","text":"Read b.txt next."}]}},
             {"type":"tool","id":"read_b","name":"read","state":{"status":"completed","input":{"path":"b.txt"},"content":[{"type":"text","text":"Done."}]}}
         ]}
    ]});
    (doc, details)
}

fn opts(provider: &str) -> WriteOpts {
    WriteOpts {
        model: Some("gpt-6.1-sol".into()),
        model_provider: Some(provider.into()),
        project_id: Some("global".into()),
        dest_protocol: Some(Protocol::OpenaiResponses),
        ..Default::default()
    }
}

fn replay(session: &mukade::model::Session, opts: &WriteOpts) -> Value {
    document_for(
        session,
        "ses_replay",
        "D:/fixture",
        opts,
        &mut Clock::new(Some("2026-11-02T00:00:00Z")),
    )
    .unwrap()
}

fn details(doc: &Value) -> Vec<Value> {
    doc["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|m| m["content"].as_array())
        .flatten()
        .filter_map(|p| {
            p.pointer("/native/reasoningDetails")
                .and_then(Value::as_array)
        })
        .flatten()
        .cloned()
        .collect()
}

#[test]
fn native_openai_format_is_recognized_after_summary_details() {
    let (doc, _) = fixture("tri-llm-gateway", "openai-native-responses-v1");
    let session = mukade::reader::opencode::parse(&doc, None).unwrap();
    let opaque = session
        .conversation()
        .find_map(|e| match &e.kind {
            Kind::Step { reasoning, .. } => reasoning.first().and_then(|r| r.opaque.as_ref()),
            _ => None,
        })
        .unwrap();
    assert_eq!(opaque.protocol, Protocol::OpenaiResponses);
    assert_eq!(opaque.encrypted.as_deref(), Some("opaque-A+/=="));
}

#[test]
fn gateway_proof_array_survives_document_and_repeated_native_roundtrips() {
    roundtrip("tri-llm-gateway", "openai-native-responses-v1");
}

#[test]
fn openrouter_proof_array_survives_document_and_repeated_native_roundtrips() {
    roundtrip("openrouter", "openai-responses-v1");
}

fn roundtrip(provider: &str, format: &str) {
    let (mut doc, expected) = fixture(provider, format);
    for _ in 0..3 {
        let session = mukade::reader::opencode::parse(&doc, None).unwrap();
        let document = mukade::wire::to_document(&session);
        let session = mukade::wire::from_document(&document).unwrap();
        let planned = plan::plan(&session, HarnessId::OpenCode, &opts(provider));
        assert_eq!(planned.counts.reasoning_dropped, 0);
        doc = replay(&session, &opts(provider));
        assert_eq!(
            serde_json::to_string(&details(&doc)).unwrap(),
            serde_json::to_string(&expected).unwrap(),
            "IDs, indices, opaque data, format, extensions and ordering must remain exact"
        );
        let calls: Vec<_> = doc["messages"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|m| m["content"].as_array())
            .flatten()
            .filter(|p| p["type"] == "tool")
            .collect();
        assert_eq!(calls.len(), 2, "reasoning keeps its tool siblings");
    }
}

#[test]
fn native_gateway_proof_is_not_relabelled_for_openrouter() {
    let (doc, _) = fixture("tri-llm-gateway", "openai-native-responses-v1");
    let session = mukade::reader::opencode::parse(&doc, None).unwrap();
    let mut destination = opts("openrouter");
    let planned = plan::plan(&session, HarnessId::OpenCode, &destination);
    assert_eq!(planned.counts.reasoning_dropped, 1);
    assert_eq!(
        planned.counts.reasoning_dropped_why[0].0,
        mukade::policy::ReasoningDrop::NoPlaceInFormat
    );
    assert!(details(&replay(&session, &destination)).is_empty());
    destination.reasoning = ReasoningPolicy::AsText;
    let rendered = serde_json::to_string(&replay(&session, &destination)).unwrap();
    assert!(rendered.contains("[earlier reasoning, not replayable]"));
    assert!(!rendered.contains("opaque-A"));
}

#[test]
fn an_unknown_destination_still_reports_the_loss() {
    let (doc, _) = fixture("tri-llm-gateway", "openai-native-responses-v1");
    let session = mukade::reader::opencode::parse(&doc, None).unwrap();
    let mut destination = opts("tri-llm-gateway");
    destination.dest_protocol = None;
    let planned = plan::plan(&session, HarnessId::OpenCode, &destination);
    assert_eq!(planned.counts.reasoning_dropped, 1);
    assert_eq!(
        planned.counts.reasoning_dropped_why[0].0,
        mukade::policy::ReasoningDrop::UnknownProtocol
    );
}

#[test]
fn mixed_or_unknown_detail_formats_are_not_guessed_from_the_first_item() {
    for format in [
        "anthropic-claude-v1",
        "openai-responses-v1",
        "future-provider-v9",
    ] {
        let (mut doc, _) = fixture("tri-llm-gateway", "openai-native-responses-v1");
        doc["messages"][1]["content"][0]["native"]["reasoningDetails"][2]["format"] = json!(format);
        let session = mukade::reader::opencode::parse(&doc, None).unwrap();
        let planned = plan::plan(&session, HarnessId::OpenCode, &opts("tri-llm-gateway"));
        assert_eq!(planned.counts.reasoning_dropped, 1);
        assert_eq!(
            planned.counts.reasoning_dropped_why[0].0,
            mukade::policy::ReasoningDrop::ForeignProtocol
        );
    }
}

#[test]
fn a_multi_item_chat_envelope_is_not_silently_reduced_to_one_codex_payload() {
    let (doc, _) = fixture("tri-llm-gateway", "openai-native-responses-v1");
    let session = mukade::reader::opencode::parse(&doc, None).unwrap();
    let planned = plan::plan(&session, HarnessId::Codex, &WriteOpts::default());
    assert_eq!(planned.counts.reasoning_dropped, 1);
    assert_eq!(
        planned.counts.reasoning_dropped_why[0].0,
        mukade::policy::ReasoningDrop::NoPlaceInFormat
    );
}
