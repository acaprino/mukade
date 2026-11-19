mod support;
use mukade::{HarnessId, bus::Bus, roots::StoreRoots};
use serde_json::{Value, json};
use std::path::PathBuf;
use uuid::Uuid;

fn transcript(turns: usize, reply: &str) -> String {
    let mut rows = vec![
        json!({"type":"session","version":3,"id":support::source_id(HarnessId::Pi),"cwd":"/observe","timestamp":"2026-11-11T10:00:00Z"}),
    ];
    for i in 0..turns {
        rows.push(json!({"type":"message","id":format!("u{i}"),"parentId":if i==0 {None} else {Some(format!("a{}",i-1))},"message":{"role":"user","content":[{"type":"text","text":format!("prompt {i}")}],"timestamp":1794391200000i64}}));
        rows.push(json!({"type":"message","id":format!("a{i}"),"parentId":format!("u{i}"),"message":{"role":"assistant","content":[{"type":"text","text":reply}],"provider":"test","model":"test","api":"anthropic-messages","stopReason":"stop","timestamp":1794391200001i64}}));
    }
    rows.into_iter().map(|v| v.to_string() + "\n").collect()
}
struct Fixture {
    bus: Bus,
    file: PathBuf,
    roots: StoreRoots,
    room: Value,
    target: Value,
    observer: Value,
}
impl Fixture {
    fn new(tag: &str, turns: usize, reply: &str) -> Self {
        let root = support::scratch(HarnessId::Pi, tag);
        let roots = support::roots(&root);
        let file = root
            .join("home/sessions/project")
            .join(format!("live_{}.jsonl", support::source_id(HarnessId::Pi)));
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, transcript(turns, reply)).unwrap();
        let mut bus = Bus::open_in(&root.join("bus")).unwrap();
        let room = mutation(&mut bus, "open", json!({"name":"r"}), &roots)["room_id"].clone();
        let target = mutation(
            &mut bus,
            "join",
            json!({"room_id":room,"harness":"pi","role":"peer","name":"target","session_id":support::source_id(HarnessId::Pi)}),
            &roots,
        );
        let observer = mutation(
            &mut bus,
            "join",
            json!({"room_id":room,"harness":"codex","role":"observer","name":"observer"}),
            &roots,
        );
        Self {
            bus,
            file,
            roots,
            room,
            target,
            observer,
        }
    }
    fn args(&self) -> Value {
        json!({"room_id":self.room,"member_handle":self.observer["member_handle"],"target_member_id":self.target["member_id"]})
    }
    fn observe(&mut self) -> Value {
        self.bus
            .chat_call("observe", self.args(), &self.roots)
            .unwrap()
    }
    fn ack(&mut self, batch: &Value) -> Result<Value, mukade::bus::ChatError> {
        self.bus.chat_call("ack",json!({"room_id":self.room,"member_handle":self.observer["member_handle"],"batch_id":batch["batch_id"]}),&self.roots)
    }
    fn restart(&mut self) {
        let dir = self.bus.path().parent().unwrap().to_path_buf();
        self.bus = Bus::open_in(&dir).unwrap();
    }
}
fn mutation(bus: &mut Bus, op: &str, mut args: Value, roots: &StoreRoots) -> Value {
    args["request_id"] = json!(Uuid::new_v4().to_string());
    bus.chat_call(op, args, roots).unwrap()
}

#[test]
fn full_permitted_field_changes_emit_updates_after_receipt() {
    let old = format!("{}a", "x".repeat(1300));
    let new = format!("{}b", "x".repeat(1300));
    let mut f = Fixture::new("observe-full-field", 1, &old);
    let first = f.observe();
    assert_eq!(first["items"].as_array().unwrap().len(), 2);
    f.ack(&first).unwrap();
    std::fs::write(&f.file, transcript(1, &new)).unwrap();
    let changed = f.observe();
    assert_eq!(changed["items"].as_array().unwrap().len(), 1);
    assert_eq!(changed["items"][0]["kind"], "updated");
    assert_ne!(
        changed["items"][0]["item"]["fingerprint"],
        first["items"][1]["item"]["fingerprint"]
    );
    assert_eq!(
        changed["items"][0]["item"]["text"],
        first["items"][1]["item"]["text"]
    );
}

#[test]
fn cached_replay_ack_commits_only_staged_delta_without_source_read() {
    let mut f = Fixture::new("observe-cached", 1, "old reply");
    let first = f.observe();
    std::fs::write(&f.file, transcript(2, "changed reply")).unwrap();
    assert_eq!(f.observe(), first);
    std::fs::remove_file(&f.file).unwrap();
    f.restart();
    f.ack(&first).unwrap();
    std::fs::write(&f.file, transcript(2, "changed reply")).unwrap();
    let next = f.observe();
    assert_eq!(next["items"].as_array().unwrap().len(), 3);
    assert_eq!(next["items"][0]["kind"], "updated");
    f.ack(&first).unwrap();
    assert_eq!(f.observe(), next);
}

#[test]
fn restart_reconstructs_selected_range_despite_appended_entries() {
    let mut f = Fixture::new("observe-append", 1, "old reply");
    let first = f.observe();
    f.restart();
    std::fs::write(&f.file, transcript(2, "old reply")).unwrap();
    assert_eq!(f.observe(), first);
    f.ack(&first).unwrap();
    assert_eq!(f.observe()["items"].as_array().unwrap().len(), 2);
}

#[test]
fn changed_selected_range_invalidates_batch_and_late_ack() {
    let mut f = Fixture::new("observe-invalidate", 1, "old reply");
    let first = f.observe();
    f.restart();
    std::fs::write(&f.file, transcript(1, "new reply")).unwrap();
    let next = f.observe();
    assert_ne!(first["batch_id"], next["batch_id"]);
    assert!(
        next["notices"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n.as_str().unwrap().contains("invalidated"))
    );
    assert_eq!(f.ack(&first).unwrap_err().code, "batch_invalidated");
    assert_eq!(f.observe(), next);
}

#[test]
fn closed_room_observation_and_revocation_fence_late_receipts() {
    for op in ["unbind", "leave"] {
        let mut f = Fixture::new(op, 1, "reply");
        mutation(&mut f.bus, "close", json!({"room_id":f.room}), &f.roots);
        let batch = f.observe();
        mutation(
            &mut f.bus,
            op,
            json!({"room_id":f.room,"member_handle":f.target["member_handle"]}),
            &f.roots,
        );
        assert_eq!(f.ack(&batch).unwrap_err().code, "batch_invalidated");
        assert_eq!(
            f.bus
                .chat_call("observe", f.args(), &f.roots)
                .unwrap_err()
                .code,
            "target_unbound"
        );
    }
}

#[test]
fn filters_report_skips_and_commit_only_on_receipt() {
    let mut f = Fixture::new("observe-filters", 3, "reply");
    let mut args = f.args();
    args["last_turns"] = json!(1);
    let batch = f.bus.chat_call("observe", args, &f.roots).unwrap();
    assert_eq!(batch["skipped"], 4);
    assert_eq!(batch["items"][0]["item"]["id"], "u2");
    assert_eq!(f.observe(), batch);
    f.restart();
    assert_eq!(f.observe(), batch);
    f.ack(&batch).unwrap();
    assert_eq!(f.observe()["batch_id"], Value::Null);
}

#[test]
fn unavailable_source_preserves_pending_receipt() {
    let mut f = Fixture::new("observe-unavailable", 1, "reply");
    let first = f.observe();
    f.restart();
    std::fs::remove_file(&f.file).unwrap();
    assert_eq!(
        f.bus
            .chat_call("observe", f.args(), &f.roots)
            .unwrap_err()
            .code,
        "source_unavailable"
    );
    std::fs::write(&f.file, transcript(1, "reply")).unwrap();
    assert_eq!(f.observe(), first);
}

#[test]
fn exact_binding_and_selected_duplicate_source_are_honored_after_restart() {
    let mut f = Fixture::new("observe-source-selection", 1, "selected original");
    let duplicate = f
        .file
        .parent()
        .unwrap()
        .join(format!("other_{}.jsonl", support::source_id(HarnessId::Pi)));
    std::fs::write(&duplicate, transcript(1, "different copy")).unwrap();
    let mut args = json!({"room_id":f.room,"member_handle":f.target["member_handle"],"harness":"pi","session_id":&support::source_id(HarnessId::Pi)[..8],"request_id":Uuid::new_v4().to_string()});
    assert_eq!(
        f.bus
            .chat_call("bind", args.clone(), &f.roots)
            .unwrap_err()
            .code,
        "session_not_found"
    );
    args["session_id"] = json!(support::source_id(HarnessId::Pi));
    assert_eq!(
        f.bus.chat_call("bind", args, &f.roots).unwrap_err().code,
        "session_ambiguous"
    );
    let batch = f.observe();
    assert_eq!(batch["items"][1]["item"]["text"], "selected original");
    f.restart();
    assert_eq!(f.observe(), batch);
    std::fs::remove_file(&f.file).unwrap();
    f.restart();
    assert_eq!(
        f.bus
            .chat_call("observe", f.args(), &f.roots)
            .unwrap_err()
            .code,
        "source_unavailable"
    );
    f.ack(&batch).unwrap();
}

#[test]
fn rebind_and_rejoin_invalidate_pending_batches_transactionally() {
    for op in ["bind", "target_rejoin", "observer_rejoin"] {
        let mut f = Fixture::new(op, 1, "reply");
        let batch = f.observe();
        if op == "bind" {
            mutation(
                &mut f.bus,
                "bind",
                json!({"room_id":f.room,"member_handle":f.target["member_handle"],"harness":"pi","session_id":support::source_id(HarnessId::Pi)}),
                &f.roots,
            );
        } else {
            let member = if op == "target_rejoin" {
                &f.target
            } else {
                &f.observer
            };
            let result = mutation(
                &mut f.bus,
                "rejoin",
                json!({"room_id":f.room,"member_id":member["member_id"]}),
                &f.roots,
            );
            if op == "observer_rejoin" {
                f.observer = result;
            }
        }
        assert_eq!(f.ack(&batch).unwrap_err().code, "batch_invalidated", "{op}");
    }
}

#[test]
fn rewind_notice_restarts_from_last_acknowledged_branch() {
    let mut f = Fixture::new("observe-rewind", 2, "reply");
    let batch = f.observe();
    f.ack(&batch).unwrap();
    std::fs::write(&f.file, transcript(1, "reply")).unwrap();
    let next = f.observe();
    assert_eq!(next["items"].as_array().unwrap().len(), 2);
    assert!(
        next["notices"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s.as_str().unwrap().contains("rewound"))
    );
    f.ack(&next).unwrap();
    assert_eq!(f.observe()["batch_id"], Value::Null);
}

#[test]
fn latest_reports_skips_and_pending_replays_before_incompatible_options() {
    let mut f = Fixture::new("observe-latest", 3, "reply");
    let mut args = f.args();
    args["latest"] = json!(true);
    args["budget_chars"] = json!(1);
    let batch = f.bus.chat_call("observe", args.clone(), &f.roots).unwrap();
    assert_eq!(batch["items"].as_array().unwrap().len(), 1);
    assert_eq!(batch["skipped"], 5);
    assert_eq!(batch["remaining"], 0);
    assert_eq!(batch["selected_range"]["first_item_id"], "a2");
    args["last_turns"] = json!(1);
    assert_eq!(
        f.bus.chat_call("observe", args.clone(), &f.roots).unwrap(),
        batch
    );
    f.ack(&batch).unwrap();
    assert_eq!(
        f.bus.chat_call("observe", args, &f.roots).unwrap_err().code,
        "invalid_arguments"
    );
    assert_eq!(f.observe()["batch_id"], Value::Null);
}

#[test]
fn included_receipt_survives_next_read_error_and_does_not_cross_streams() {
    let mut f = Fixture::new("observe-included-ack", 1, "reply");
    let batch = f.observe();
    let room_batch = f
        .bus
        .chat_call(
            "read",
            json!({"room_id":f.room,"member_handle":f.observer["member_handle"]}),
            &f.roots,
        )
        .unwrap();
    let mut args = f.args();
    args["ack_batch_id"] = room_batch["batch_id"].clone();
    assert_eq!(
        f.bus
            .chat_call("observe", args.clone(), &f.roots)
            .unwrap_err()
            .code,
        "batch_not_found"
    );
    args["ack_batch_id"] = batch["batch_id"].clone();
    std::fs::remove_file(&f.file).unwrap();
    let err = f.bus.chat_call("observe", args, &f.roots).unwrap_err();
    assert_eq!(err.code, "source_unavailable");
    assert_eq!(err.ack_batch_id.as_deref(), batch["batch_id"].as_str());
    std::fs::write(&f.file, transcript(1, "reply")).unwrap();
    assert_eq!(f.observe()["batch_id"], Value::Null);
}

#[test]
fn sqlite_and_export_never_store_native_payload_text() {
    let marker = "NATIVE_ONLY_observation_payload_marker_729824";
    let mut f = Fixture::new("observe-no-persistence", 1, marker);
    let batch = f.observe();
    f.ack(&batch).unwrap();
    let export = f
        .bus
        .chat_call("export", json!({"room_id":f.room}), &f.roots)
        .unwrap()
        .to_string();
    assert!(!export.contains(marker));
    let db = f.bus.path().to_path_buf();
    for path in [db.clone(), db.with_file_name("bus.db-wal")] {
        let bytes = std::fs::read(path).unwrap();
        assert!(!bytes.windows(marker.len()).any(|w| w == marker.as_bytes()));
    }
}

#[test]
fn opencode_observation_reads_selected_database_without_mutation_or_native_process() {
    let mut f = Fixture::new("observe-opencode", 1, "reply");
    let db = f.file.parent().unwrap().join("selected.db");
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute_batch("CREATE TABLE session_v2(id TEXT PRIMARY KEY,parent_id TEXT,title TEXT,directory TEXT,time_updated INTEGER,time_archived INTEGER); CREATE TABLE session_message(id TEXT PRIMARY KEY,session_id TEXT,type TEXT,seq INTEGER,data TEXT);").unwrap();
    let sid = "ses_exact_observation_source";
    conn.execute(
        "INSERT INTO session_v2(id,title,directory,time_updated) VALUES(?1,'test','/observe',1)",
        [sid],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO session_message VALUES('u',?1,'user',1,?2)",
        rusqlite::params![
            sid,
            json!({"text":"read only native prompt","files":[]}).to_string()
        ],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO session_message VALUES('a',?1,'assistant',2,?2)",
        rusqlite::params![
            sid,
            json!({"content":[{"type":"text","text":"read only reply"}],"finish":"stop"})
                .to_string()
        ],
    )
    .unwrap();
    drop(conn);
    let before = std::fs::read(&db).unwrap();
    f.roots.opencode_db = Some(db.clone());
    f.target = mutation(
        &mut f.bus,
        "join",
        json!({"room_id":f.room,"harness":"opencode","role":"peer","session_id":sid}),
        &f.roots,
    );
    let batch = f.observe();
    assert_eq!(batch["items"][0]["item"]["text"], "read only native prompt");
    assert_eq!(batch["items"][1]["item"]["text"], "read only reply");
    assert_eq!(std::fs::read(db).unwrap(), before);
    f.ack(&batch).unwrap();
}

#[test]
fn configured_file_readers_stage_native_fixtures_through_public_bus() {
    for harness in [HarnessId::ClaudeCode, HarnessId::Codex, HarnessId::Pi] {
        let root = support::scratch(harness, "chat-observe-native");
        support::stage_fixture(harness, &root);
        let roots = support::roots(&root);
        let mut bus = Bus::open_in(&root.join("bus")).unwrap();
        let room = mutation(&mut bus, "open", json!({"name":"r"}), &roots)["room_id"].clone();
        let target = mutation(
            &mut bus,
            "join",
            json!({"room_id":room,"harness":harness.as_str(),"role":"peer","session_id":support::source_id(harness)}),
            &roots,
        );
        let observer = mutation(
            &mut bus,
            "join",
            json!({"room_id":room,"harness":"review","role":"observer"}),
            &roots,
        );
        let args = json!({"room_id":room,"member_handle":observer["member_handle"],"target_member_id":target["member_id"]});
        let result = bus.chat_call("observe", args.clone(), &roots).unwrap();
        assert!(!result["items"].as_array().unwrap().is_empty());
        drop(bus);
        let mut bus = Bus::open_in(&root.join("bus")).unwrap();
        assert_eq!(bus.chat_call("observe", args, &roots).unwrap(), result);
    }
}

#[test]
fn unknown_turn_boundaries_are_refused_and_partial_final_records_are_tolerated() {
    let mut f = Fixture::new("observe-unknown-boundary", 1, "reply");
    let mut rows: Vec<Value> = transcript(1, "reply")
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    rows[1]["type"] = json!("custom_message");
    rows[1]["customType"] = json!("extension");
    let source: String = rows.into_iter().map(|v| v.to_string() + "\n").collect();
    std::fs::write(&f.file, source + "{\"type\":").unwrap();
    let mut args = f.args();
    args["last_turns"] = json!(1);
    assert_eq!(
        f.bus.chat_call("observe", args, &f.roots).unwrap_err().code,
        "turn_boundaries_unknown"
    );
    assert!(!f.observe()["items"].as_array().unwrap().is_empty());
}

#[test]
fn selected_change_remains_invalidated_even_when_replacement_filter_is_refused() {
    let mut f = Fixture::new("observe-invalid-filter", 1, "old reply");
    let batch = f.observe();
    f.restart();
    std::fs::write(&f.file, transcript(1, "new reply")).unwrap();
    let mut args = f.args();
    args["latest"] = json!(true);
    args["last_turns"] = json!(1);
    assert_eq!(
        f.bus.chat_call("observe", args, &f.roots).unwrap_err().code,
        "invalid_arguments"
    );
    assert_eq!(f.ack(&batch).unwrap_err().code, "batch_invalidated");
    f.restart();
    let replacement = f.observe();
    assert!(
        replacement["notices"]
            .as_array()
            .unwrap()
            .iter()
            .any(|notice| notice.as_str().unwrap().contains("invalidated"))
    );
    assert_ne!(replacement["batch_id"], batch["batch_id"]);
    f.restart();
    assert_eq!(f.observe(), replacement);
    assert_eq!(f.ack(&batch).unwrap_err().code, "batch_invalidated");
    f.ack(&replacement).unwrap();
    f.restart();
    assert!(f.observe()["notices"].as_array().unwrap().is_empty());
}

#[test]
fn invalidation_notice_survives_unknown_turns_and_repeated_replacement_changes() {
    let mut f = Fixture::new("observe-notice-unknown-turns", 1, "old reply");
    let old = f.observe();
    f.restart();
    let mut rows: Vec<Value> = transcript(1, "new reply")
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    rows[1]["type"] = json!("custom_message");
    rows[1]["customType"] = json!("extension");
    std::fs::write(
        &f.file,
        rows.into_iter()
            .map(|v| v.to_string() + "\n")
            .collect::<String>(),
    )
    .unwrap();
    let mut args = f.args();
    args["last_turns"] = json!(1);
    assert_eq!(
        f.bus
            .chat_call("observe", args.clone(), &f.roots)
            .unwrap_err()
            .code,
        "turn_boundaries_unknown"
    );
    f.restart();
    assert_eq!(
        f.bus.chat_call("observe", args, &f.roots).unwrap_err().code,
        "turn_boundaries_unknown"
    );
    let replacement = f.observe();
    assert!(
        replacement["notices"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s.as_str().unwrap().contains("invalidated"))
    );
    f.restart();
    std::fs::write(&f.file, transcript(1, "changed again")).unwrap();
    let final_batch = f.observe();
    assert!(
        final_batch["notices"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s.as_str().unwrap().contains("invalidated"))
    );
    assert_eq!(f.ack(&old).unwrap_err().code, "batch_invalidated");
    assert_eq!(f.ack(&replacement).unwrap_err().code, "batch_invalidated");
    f.ack(&final_batch).unwrap();
    f.restart();
    assert!(f.observe()["notices"].as_array().unwrap().is_empty());
}

#[test]
fn ordinary_unicode_budget_pages_oldest_first_without_skip_and_includes_receipt_envelope() {
    for budget in [1000, 1500, 2300] {
        let mut f = Fixture::new("observe-budget", 4, &"界".repeat(120));
        let mut args = f.args();
        args["budget_chars"] = json!(budget);
        let mut shown = Vec::new();
        loop {
            let page = f.bus.chat_call("observe", args.clone(), &f.roots).unwrap();
            assert_eq!(page["skipped"], 0);
            let items = page["items"].as_array().unwrap();
            assert!(items.len() <= 1 || page.to_string().chars().count() <= budget as usize);
            shown.extend(
                items
                    .iter()
                    .map(|v| v["item"]["id"].as_str().unwrap().to_owned()),
            );
            if page["batch_id"].is_null() {
                break;
            }
            args["ack_batch_id"] = page["batch_id"].clone();
        }
        assert_eq!(shown, vec!["u0", "a0", "u1", "a1", "u2", "a2", "u3", "a3"]);
    }
}
