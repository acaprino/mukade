mod support;
use mukade::{
    HarnessId,
    bus::{Bus, JoinRequest, Role},
};
use rusqlite::Connection;
fn setup(tag: &str) -> (Bus, std::path::PathBuf) {
    let root = support::scratch(HarnessId::Pi, tag);
    (Bus::open_in(&root.join("bus")).unwrap(), root)
}
fn req(room: &str, harness: &str) -> JoinRequest {
    JoinRequest {
        room: room.into(),
        harness: harness.into(),
        role: Role::Peer,
        name: None,
        rejoin: None,
    }
}

#[test]
fn names_are_harness_dash_n_and_never_reused() {
    let (mut b, _) = setup("bus-names");
    b.open_room("r").unwrap();
    assert_eq!(b.join(&req("r", "codex")).unwrap().name, "codex-1");
    b.leave("r", "codex-1").unwrap();
    assert_eq!(b.join(&req("r", "codex")).unwrap().name, "codex-2");
    assert_eq!(b.join(&req("r", "claude_code")).unwrap().name, "claude-1");
}
#[test]
fn a_chosen_name_is_validated_and_unique() {
    let (mut b, _) = setup("bus-custom");
    b.open_room("r").unwrap();
    for name in ["all", "Bad Name", "", "../escape"] {
        let mut r = req("r", "codex");
        r.name = Some(name.into());
        assert!(b.join(&r).is_err());
    }
    let mut r = req("r", "reviewbot");
    r.name = Some("reviewer".into());
    b.join(&r).unwrap();
    assert!(b.join(&r).is_err());
}
#[test]
fn join_in_a_closed_or_missing_room_is_refused() {
    let (mut b, _) = setup("bus-closed");
    assert!(b.join(&req("r", "pi")).is_err());
    b.open_room("r").unwrap();
    b.close_room("r").unwrap();
    assert!(b.join(&req("r", "pi")).is_err());
    b.reopen_room("r").unwrap();
    b.join(&req("r", "pi")).unwrap();
}
#[test]
fn rejoin_restores_the_name_and_clears_the_old_binding() {
    let (mut b, root) = setup("bus-rejoin");
    let id = support::stage_fixture(HarnessId::Pi, &root);
    b.open_room("r").unwrap();
    b.join(&req("r", "pi")).unwrap();
    b.bind("r", "pi-1", HarnessId::Pi, &id, &support::roots(&root))
        .unwrap();
    b.leave("r", "pi-1").unwrap();
    let mut r = req("r", "pi");
    r.rejoin = Some("pi-1".into());
    let m = b.join(&r).unwrap();
    assert_eq!(m.name, "pi-1");
    assert!(m.session.is_none());
    assert!(m.left_at.is_none());
    assert!(b.join(&r).is_err());
}
#[test]
fn bind_refuses_an_unknown_session_and_never_guesses() {
    let (mut b, root) = setup("bus-bind");
    support::stage_fixture(HarnessId::Pi, &root);
    b.open_room("r").unwrap();
    b.join(&req("r", "pi")).unwrap();
    assert!(
        b.bind("r", "pi-1", HarnessId::Pi, "absent", &support::roots(&root))
            .is_err()
    );
    assert!(b.members("r").unwrap()[0].session.is_none());
    assert_eq!(
        Bus::candidates(HarnessId::Pi, None, &support::roots(&root))
            .unwrap()
            .len(),
        1
    );
}
#[test]
fn a_session_bound_to_one_member_is_refused_for_another() {
    let (mut b, root) = setup("bus-exclusive");
    let id = support::stage_fixture(HarnessId::Pi, &root);
    b.open_room("r").unwrap();
    b.join(&req("r", "pi")).unwrap();
    b.join(&req("r", "pi")).unwrap();
    let session = b
        .bind("r", "pi-1", HarnessId::Pi, &id, &support::roots(&root))
        .unwrap();
    assert_eq!(session.id, support::source_id(HarnessId::Pi));
    assert!(
        b.bind("r", "pi-2", HarnessId::Pi, &id, &support::roots(&root))
            .is_err()
    );
}
#[test]
fn delete_room_needs_the_room_closed_and_removes_everything() {
    let (mut b, _) = setup("bus-delete");
    b.open_room("r").unwrap();
    b.join(&req("r", "pi")).unwrap();
    assert!(b.delete_room("r").is_err());
    b.close_room("r").unwrap();
    let summary = b.delete_room("r").unwrap();
    assert_eq!(summary.members, 1);
    let c = Connection::open(b.path()).unwrap();
    for table in [
        "rooms",
        "members",
        "events",
        "observe_cursors",
        "observations",
    ] {
        let count: i64 = c
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }
}
#[test]
fn every_change_is_an_event() {
    let (mut b, _) = setup("bus-events");
    b.open_room("r").unwrap();
    b.join(&req("r", "pi")).unwrap();
    b.leave("r", "pi-1").unwrap();
    b.close_room("r").unwrap();
    b.reopen_room("r").unwrap();
    let c = Connection::open(b.path()).unwrap();
    let rows: Vec<(i64, String)> = c
        .prepare("SELECT seq,kind FROM events ORDER BY seq")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        rows.iter().map(|r| r.1.as_str()).collect::<Vec<_>>(),
        ["open", "join", "leave", "close", "reopen"]
    );
    assert!(rows.windows(2).all(|w| w[0].0 < w[1].0));
}
#[test]
fn a_second_codex_with_a_new_session_does_not_inherit_the_first_binding() {
    let (mut b, root) = setup("bus-codex");
    let id = support::stage_fixture(HarnessId::Codex, &root);
    b.open_room("r").unwrap();
    b.join(&req("r", "codex")).unwrap();
    b.bind(
        "r",
        "codex-1",
        HarnessId::Codex,
        &id,
        &support::roots(&root),
    )
    .unwrap();
    assert!(b.join(&req("r", "codex")).unwrap().session.is_none());
}
