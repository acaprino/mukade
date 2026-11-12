mod support;
use mukade::{HarnessId, bus::Bus};
use rusqlite::Connection;

#[test]
fn open_creates_the_folder_and_a_wal_database() {
    let root = support::scratch(HarnessId::Pi, "bus-store").join("bus");
    let bus = Bus::open_in(&root).unwrap();
    assert_eq!(bus.path(), root.join("bus.db"));
    let conn = Connection::open(bus.path()).unwrap();
    assert_eq!(
        conn.query_row("PRAGMA journal_mode", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "wal"
    );
    assert_eq!(
        conn.query_row("PRAGMA auto_vacuum", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        2
    );
}

#[test]
fn an_unwritable_data_folder_names_the_folder_and_writes_nothing_else() {
    let root = support::scratch(HarnessId::Pi, "bus-bad-dir").join("blocked");
    std::fs::write(&root, "keep").unwrap();
    let error = Bus::open_in(&root).err().unwrap();
    assert!(error.to_string().contains(root.to_str().unwrap()));
    assert_eq!(error.exit_code(), 4);
    assert_eq!(std::fs::read_to_string(&root).unwrap(), "keep");
}

#[test]
fn schema_version_is_3() {
    let root = support::scratch(HarnessId::Pi, "bus-version").join("bus");
    let bus = Bus::open_in(&root).unwrap();
    let conn = Connection::open(bus.path()).unwrap();
    assert_eq!(
        conn.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        3
    );
    drop(bus);
    conn.pragma_update(None, "user_version", 99).unwrap();
    assert!(
        Bus::open_in(&root)
            .err()
            .unwrap()
            .to_string()
            .contains("99")
    );
}

#[test]
fn two_processes_writing_at_once_lose_nothing() {
    let root = support::scratch(HarnessId::Pi, "bus-concurrent").join("bus");
    let handles: Vec<_> = (0..2)
        .map(|n| {
            let root = root.clone();
            std::thread::spawn(move || {
                let mut bus = Bus::open_in(&root).unwrap();
                for i in 0..200 {
                    bus.open_room(&format!("room-{n}-{i}")).unwrap();
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    let conn = Connection::open(root.join("bus.db")).unwrap();
    assert_eq!(
        conn.query_row("SELECT COUNT(DISTINCT seq) FROM events", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        400
    );
}

#[test]
fn a_write_that_returns_an_error_leaves_nothing() {
    let root = support::scratch(HarnessId::Pi, "bus-rollback").join("bus");
    let mut bus = Bus::open_in(&root).unwrap();
    bus.open_room("one").unwrap();
    assert!(bus.open_room("one").is_err());
    let conn = Connection::open(bus.path()).unwrap();
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM events", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn killed_writer_child() {
    let Some(dir) = std::env::var_os("MUKADE_BUS_KILL_TEST") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    let bus = Bus::open_in(&dir).unwrap();
    let conn = Connection::open(bus.path()).unwrap();
    conn.execute_batch(
        "BEGIN IMMEDIATE; INSERT INTO rooms(name,opened_at) VALUES('uncommitted','now');",
    )
    .unwrap();
    std::fs::write(dir.join("ready"), "ready").unwrap();
    loop {
        std::thread::park_timeout(std::time::Duration::from_millis(100));
    }
}

#[test]
fn a_killed_writer_leaves_the_next_open_consistent() {
    let root = support::scratch(HarnessId::Pi, "bus-killed").join("bus");
    drop(Bus::open_in(&root).unwrap());
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "killed_writer_child", "--nocapture"])
        .env("MUKADE_BUS_KILL_TEST", &root)
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let until = std::time::Instant::now() + std::time::Duration::from_secs(15);
    while !root.join("ready").exists() && std::time::Instant::now() < until {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let ready = root.join("ready").exists();
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(ready);
    let bus = Bus::open_in(&root).unwrap();
    let conn = Connection::open(bus.path()).unwrap();
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM rooms", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
}
