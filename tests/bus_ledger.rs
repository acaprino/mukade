mod support;
use mukade::{
    HarnessId,
    bus::{Bus, HistoryFilter, JoinRequest, Role},
};
fn setup(tag: &str) -> Bus {
    let root = support::scratch(HarnessId::Pi, tag);
    let mut b = Bus::open_in(&root.join("bus")).unwrap();
    b.open_room("r").unwrap();
    b.join(&JoinRequest {
        room: "r".into(),
        harness: "pi".into(),
        role: Role::Peer,
        name: None,
        rejoin: None,
    })
    .unwrap();
    b.leave("r", "pi-1").unwrap();
    b
}
#[test]
fn rooms_list_open_and_closed_with_counts() {
    let mut b = setup("ledger-rooms");
    b.open_room("other").unwrap();
    b.close_room("r").unwrap();
    let rooms = b.ledger_rooms().unwrap();
    let r = rooms.iter().find(|r| r.name == "r").unwrap();
    assert_eq!(r.members, 1);
    assert_eq!(r.events, 4);
    assert!(r.closed_at.is_some());
    assert!(
        rooms
            .iter()
            .find(|r| r.name == "other")
            .unwrap()
            .closed_at
            .is_none()
    );
}
#[test]
fn members_include_those_who_left_with_dates() {
    let b = setup("ledger-members");
    let m = &b.ledger_members("r").unwrap()[0];
    assert_eq!(m.name, "pi-1");
    assert!(m.left_at.is_some());
    assert!(!m.joined_at.is_empty());
}
#[test]
fn history_filters_by_member_kind_and_time() {
    let b = setup("ledger-filters");
    let h = b
        .ledger_history(
            "r",
            &HistoryFilter {
                member: Some("pi-1".into()),
                kind: Some("join".into()),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(h.len(), 1);
    assert_eq!(h[0].kind, "join");
    assert!(
        b.ledger_history(
            "r",
            &HistoryFilter {
                since: Some("9999-01-01".into()),
                ..Default::default()
            }
        )
        .unwrap()
        .is_empty()
    );
    assert!(
        b.ledger_history(
            "r",
            &HistoryFilter {
                until: Some("0001-01-01".into()),
                ..Default::default()
            }
        )
        .unwrap()
        .is_empty()
    );
    assert!(
        b.ledger_history(
            "r",
            &HistoryFilter {
                mentions: Some("pi-1".into()),
                ..Default::default()
            }
        )
        .unwrap()
        .is_empty()
    );
    assert!(
        b.ledger_history(
            "r",
            &HistoryFilter {
                text: Some("pi".into()),
                ..Default::default()
            }
        )
        .unwrap()
        .is_empty()
    );
}
#[test]
fn history_from_the_end_with_a_limit() {
    let b = setup("ledger-tail");
    let h = b
        .ledger_history(
            "r",
            &HistoryFilter {
                from_end: true,
                limit: Some(2),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(
        h.iter().map(|r| r.kind.as_str()).collect::<Vec<_>>(),
        ["join", "leave"]
    );
    assert!(h[0].seq < h[1].seq);
}
fn snapshot(b: &Bus) -> String {
    let c = rusqlite::Connection::open(b.path()).unwrap();
    let mut out = String::new();
    for table in [
        "rooms",
        "members",
        "events",
        "observations",
        "observe_cursors",
    ] {
        let mut q = c
            .prepare(&format!("SELECT * FROM {table} ORDER BY rowid"))
            .unwrap();
        let columns = q.column_count();
        let mut rows = q.query([]).unwrap();
        while let Some(row) = rows.next().unwrap() {
            for i in 0..columns {
                out.push_str(&format!("{:?}", row.get_ref(i).unwrap()));
            }
        }
    }
    out
}
#[test]
fn reading_the_ledger_changes_no_row() {
    let b = setup("ledger-pure");
    let before = snapshot(&b);
    b.ledger_rooms().unwrap();
    b.ledger_members("r").unwrap();
    b.ledger_history("r", &HistoryFilter::default()).unwrap();
    b.ledger_observations("r").unwrap();
    b.export_room("r", &mut vec![]).unwrap();
    assert_eq!(before, snapshot(&b));
}
#[test]
fn export_is_one_json_object_per_line_and_has_no_view_text() {
    let b = setup("ledger-export");
    let mut bytes = vec![];
    b.export_room("r", &mut bytes).unwrap();
    let text = String::from_utf8(bytes).unwrap();
    let rows: Vec<serde_json::Value> = text
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(rows[0]["type"], "room");
    assert_eq!(rows[1]["type"], "member");
    assert_eq!(rows.len(), 5);
    assert!(!text.contains("fingerprint"));
}
#[test]
fn a_closed_room_is_still_in_the_ledger() {
    let mut b = setup("ledger-closed");
    b.close_room("r").unwrap();
    assert_eq!(
        b.ledger_history("r", &HistoryFilter::default())
            .unwrap()
            .len(),
        4
    );
    assert!(
        b.ledger_history("missing", &HistoryFilter::default())
            .is_err()
    );
}
