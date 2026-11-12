mod support;
use mukade::{
    HarnessId,
    bus::{Bus, JoinRequest, ObserveOpts, ObservedKind, Role},
    roots::StoreRoots,
};
use serde_json::json;
use std::path::PathBuf;
fn transcript(turns: usize, changed: Option<(usize, &str)>) -> String {
    let mut rows = vec![
        json!({"type":"session","version":3,"id":support::source_id(HarnessId::Pi),"cwd":"/bus-project","timestamp":"2026-11-11T10:00:00Z"}),
    ];
    for i in 0..turns {
        rows.push(json!({"type":"message","id":format!("u{i}"),"parentId":if i==0 {None} else {Some(format!("a{}",i-1))},"message":{"role":"user","content":[{"type":"text","text":format!("prompt {i}")}],"timestamp":1794391200000i64}}));
        let text = changed
            .filter(|(n, _)| *n == i)
            .map(|(_, s)| s.to_string())
            .unwrap_or_else(|| format!("reply {i} {}", "word ".repeat(40)));
        rows.push(json!({"type":"message","id":format!("a{i}"),"parentId":format!("u{i}"),"message":{"role":"assistant","content":[{"type":"text","text":text}],"provider":"test","model":"test","api":"anthropic-messages","stopReason":"stop","timestamp":1794391200001i64}}));
    }
    rows.into_iter().map(|v| v.to_string() + "\n").collect()
}
fn setup(tag: &str, turns: usize) -> (Bus, PathBuf, StoreRoots) {
    let root = support::scratch(HarnessId::Pi, tag);
    let roots = support::roots(&root);
    let file = root
        .join("home/sessions/project")
        .join(format!("live_{}.jsonl", support::source_id(HarnessId::Pi)));
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, transcript(turns, None)).unwrap();
    let mut b = Bus::open_in(&root.join("bus")).unwrap();
    b.open_room("r").unwrap();
    for (harness, role) in [("pi", Role::Peer), ("codex", Role::Observer)] {
        b.join(&JoinRequest {
            room: "r".into(),
            harness: harness.into(),
            role,
            name: None,
            rejoin: None,
        })
        .unwrap();
    }
    b.bind(
        "r",
        "pi-1",
        HarnessId::Pi,
        support::source_id(HarnessId::Pi),
        &roots,
    )
    .unwrap();
    (b, file, roots)
}
#[test]
fn only_an_observer_may_observe() {
    let (mut b, _, roots) = setup("observe-roles", 1);
    for role in [Role::Peer, Role::Lead, Role::Worker] {
        let m = b
            .join(&JoinRequest {
                room: "r".into(),
                harness: "reviewbot".into(),
                role,
                name: None,
                rejoin: None,
            })
            .unwrap();
        assert!(
            b.observe("r", &m.name, "pi-1", &roots, &ObserveOpts::default())
                .unwrap_err()
                .to_string()
                .contains("observer")
        );
    }
}
#[test]
fn an_observer_cannot_observe_a_member_that_left_or_has_no_session() {
    let (mut b, _, roots) = setup("observe-no-bind", 1);
    b.join(&JoinRequest {
        room: "r".into(),
        harness: "pi".into(),
        role: Role::Peer,
        name: None,
        rejoin: None,
    })
    .unwrap();
    assert!(
        b.observe("r", "codex-1", "pi-2", &roots, &ObserveOpts::default())
            .unwrap_err()
            .to_string()
            .contains("bind")
    );
    b.leave("r", "pi-1").unwrap();
    assert!(
        b.observe("r", "codex-1", "pi-1", &roots, &ObserveOpts::default())
            .is_err()
    );
}
#[test]
fn first_call_shows_from_the_start_and_second_shows_only_news() {
    let (mut b, file, roots) = setup("observe-grow", 1);
    let opts = ObserveOpts::default();
    assert_eq!(
        b.observe("r", "codex-1", "pi-1", &roots, &opts)
            .unwrap()
            .shown
            .len(),
        2
    );
    assert!(
        b.observe("r", "codex-1", "pi-1", &roots, &opts)
            .unwrap()
            .shown
            .is_empty()
    );
    std::fs::write(file, transcript(2, None)).unwrap();
    let next = b.observe("r", "codex-1", "pi-1", &roots, &opts).unwrap();
    assert_eq!(next.shown.len(), 2);
    assert_eq!(next.shown[0].item.text, "prompt 1");
}
#[test]
fn a_grown_reply_is_shown_again_as_updated() {
    let (mut b, file, roots) = setup("observe-update", 1);
    let opts = ObserveOpts::default();
    b.observe("r", "codex-1", "pi-1", &roots, &opts).unwrap();
    std::fs::write(file, transcript(1, Some((0, "longer updated reply")))).unwrap();
    let next = b.observe("r", "codex-1", "pi-1", &roots, &opts).unwrap();
    assert_eq!(next.shown.len(), 1);
    assert_eq!(next.shown[0].kind, ObservedKind::Updated);
}
#[test]
fn an_earlier_consumed_entry_can_grow_after_the_cursor_passed_it() {
    let (mut b, file, roots) = setup("observe-old-update", 3);
    let opts = ObserveOpts::default();
    b.observe("r", "codex-1", "pi-1", &roots, &opts).unwrap();
    std::fs::write(file, transcript(3, Some((0, "earlier update")))).unwrap();
    let next = b.observe("r", "codex-1", "pi-1", &roots, &opts).unwrap();
    assert_eq!(next.shown.len(), 1);
    assert_eq!(next.shown[0].item.id, "a0");
    assert_eq!(next.shown[0].kind, ObservedKind::Updated);
}
#[test]
fn a_budget_pages_without_losing_anything() {
    let (mut b, _, roots) = setup("observe-pages", 6);
    let opts = ObserveOpts {
        budget_chars: 300,
        ..Default::default()
    };
    let mut ids = vec![];
    let mut previous = usize::MAX;
    loop {
        let next = b.observe("r", "codex-1", "pi-1", &roots, &opts).unwrap();
        assert!(next.remaining < previous);
        previous = next.remaining;
        ids.extend(next.shown.into_iter().map(|s| s.item.id));
        if previous == 0 {
            break;
        }
    }
    assert_eq!(
        ids,
        (0..6)
            .flat_map(|i| [format!("u{i}"), format!("a{i}")])
            .collect::<Vec<_>>()
    );
}
#[test]
fn latest_reports_how_many_it_left_out() {
    let (mut b, _, roots) = setup("observe-latest", 6);
    let next = b
        .observe(
            "r",
            "codex-1",
            "pi-1",
            &roots,
            &ObserveOpts {
                budget_chars: 300,
                latest: true,
                last_turns: None,
            },
        )
        .unwrap();
    assert!(next.left_out > 0);
    assert_eq!(next.left_out + next.shown.len(), 12);
    assert_eq!(next.remaining, 0);
}
#[test]
fn a_rewound_conversation_restarts_with_a_notice() {
    let (mut b, file, roots) = setup("observe-rewind", 3);
    b.observe("r", "codex-1", "pi-1", &roots, &ObserveOpts::default())
        .unwrap();
    std::fs::write(file, transcript(1, None)).unwrap();
    let next = b
        .observe("r", "codex-1", "pi-1", &roots, &ObserveOpts::default())
        .unwrap();
    assert!(next.notices.iter().any(|s| s.contains("restarting")));
    assert_eq!(next.shown.len(), 2);
}
#[test]
fn a_half_written_last_line_is_not_an_error() {
    use std::io::Write;
    let (mut b, file, roots) = setup("observe-partial", 1);
    std::fs::OpenOptions::new()
        .append(true)
        .open(file)
        .unwrap()
        .write_all(b"{\"type\":")
        .unwrap();
    assert_eq!(
        b.observe("r", "codex-1", "pi-1", &roots, &ObserveOpts::default())
            .unwrap()
            .shown
            .len(),
        2
    );
}
#[test]
fn a_deleted_session_file_is_reported_not_a_crash() {
    let (mut b, file, roots) = setup("observe-deleted", 1);
    std::fs::remove_file(file).unwrap();
    assert!(
        b.observe("r", "codex-1", "pi-1", &roots, &ObserveOpts::default())
            .unwrap_err()
            .to_string()
            .contains(support::source_id(HarnessId::Pi))
    );
}
#[test]
fn a_newer_session_in_the_folder_is_reported() {
    let (mut b, file, roots) = setup("observe-newer", 1);
    b.observe("r", "codex-1", "pi-1", &roots, &ObserveOpts::default())
        .unwrap();
    let other = "11111111-2222-4333-8444-555555555555";
    let text = transcript(1, None)
        .replace(support::source_id(HarnessId::Pi), other)
        .replace("2026-11-11T10:00:00Z", "2026-11-12T10:00:00Z")
        .replace("1794391200000", "1794477600000")
        .replace("1794391200001", "1794477600001");
    std::fs::write(file.with_file_name(format!("live_{other}.jsonl")), text).unwrap();
    let next = b
        .observe("r", "codex-1", "pi-1", &roots, &ObserveOpts::default())
        .unwrap();
    assert!(next.notices.iter().any(|s| s.contains("newer")));
}
#[test]
fn two_concurrent_observes_of_one_pair_never_show_the_same_item_to_both_as_new() {
    let (b, _, roots) = setup("observe-concurrent", 4);
    let dir = b.path().parent().unwrap().to_path_buf();
    drop(b);
    let handles: Vec<_> = (0..2)
        .map(|_| {
            let dir = dir.clone();
            let roots = roots.clone();
            std::thread::spawn(move || {
                Bus::open_in(&dir).unwrap().observe(
                    "r",
                    "codex-1",
                    "pi-1",
                    &roots,
                    &ObserveOpts::default(),
                )
            })
        })
        .collect();
    let mut ids = vec![];
    for h in handles {
        match h.join().unwrap() {
            Ok(o) => ids.extend(o.shown.into_iter().map(|s| s.item.id)),
            Err(e) => assert!(e.to_string().contains("active reader"), "{e}"),
        }
    }
    assert_eq!(ids.len(), 8);
    assert_eq!(
        ids.iter().collect::<std::collections::HashSet<_>>().len(),
        8
    );
}
#[test]
fn observation_rows_hold_counts_not_text() {
    let (mut b, _, roots) = setup("observe-counts", 1);
    b.observe("r", "codex-1", "pi-1", &roots, &ObserveOpts::default())
        .unwrap();
    let c = rusqlite::Connection::open(b.path()).unwrap();
    let count: i64 = c
        .query_row("SELECT entries_shown FROM observations", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 2);
    let schema: String = c
        .query_row(
            "SELECT sql FROM sqlite_master WHERE name='observations'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(!schema.contains("body"));
}
#[test]
fn observing_never_touches_the_source_file() {
    let (mut b, file, roots) = setup("observe-source", 1);
    let before = std::fs::read(&file).unwrap();
    b.observe("r", "codex-1", "pi-1", &roots, &ObserveOpts::default())
        .unwrap();
    assert_eq!(before, std::fs::read(file).unwrap());
}
#[test]
fn failed_output_does_not_move_the_cursor() {
    let (mut b, _, roots) = setup("observe-output", 1);
    let opts = ObserveOpts::default();
    assert!(
        b.observe_with("r", "codex-1", "pi-1", &roots, &opts, |_| Err(
            mukade::bus::BusError::Failed("broken pipe".into())
        ))
        .is_err()
    );
    assert_eq!(
        b.observe("r", "codex-1", "pi-1", &roots, &opts)
            .unwrap()
            .shown
            .len(),
        2
    );
}
#[test]
fn last_turns_applies_only_to_first_observation() {
    let (mut b, _, roots) = setup("observe-last", 4);
    let opts = ObserveOpts {
        last_turns: Some(1),
        ..Default::default()
    };
    let next = b.observe("r", "codex-1", "pi-1", &roots, &opts).unwrap();
    assert_eq!(next.left_out, 6);
    assert_eq!(next.shown[0].item.text, "prompt 3");
    assert!(
        b.observe("r", "codex-1", "pi-1", &roots, &opts)
            .unwrap()
            .shown
            .is_empty()
    );
}
