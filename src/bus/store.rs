use super::BusError;
use rusqlite::{Connection, Transaction, TransactionBehavior};
use std::{
    fs::OpenOptions,
    path::{Path, PathBuf},
    time::Duration,
};

pub struct Bus {
    pub(crate) conn: Connection,
    path: PathBuf,
    size_notice_bytes: u64,
    pub(super) chat_snapshots: super::chat_query::Snapshots,
    pub(super) observation_cache: super::chat_observe::PayloadCache,
    #[cfg(test)]
    pub(super) chat_space_override: Option<u64>,
}

impl Bus {
    pub fn open() -> Result<Self, BusError> {
        let dir = crate::portable::bus_dir().ok_or_else(|| {
            BusError::Failed("cannot determine bus folder; MUKADE_DATA_DIR must be absolute".into())
        })?;
        let resolved = resolve_path(&dir)?;
        if resolved.ancestors().any(|p| p.join(".git").exists()) {
            return Err(BusError::Refused(format!(
                "bus folder {} is inside a project; set MUKADE_DATA_DIR to a shared folder outside the project",
                dir.display()
            )));
        }
        ensure_outside_stores(&dir, &crate::roots::StoreRoots::from_env())?;
        Self::open_in(&dir)
    }
    pub fn open_in(dir: &Path) -> Result<Self, BusError> {
        let init = || -> Result<Self, BusError> {
            std::fs::create_dir_all(dir)?;

            // Serialize schema creation across processes, including the pre-schema PRAGMAs.
            let lock = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(dir.join("schema.lock"))?;
            lock.lock()?;
            let path = dir.join("bus.db");
            let mut conn = Connection::open(&path)?;
            conn.busy_timeout(Duration::from_millis(5000))?;
            let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
            if version > 3 {
                return Err(BusError::Failed(format!(
                    "unsupported bus schema {version}; this build supports 3"
                )));
            }
            if version < 2 {
                check_space(dir)?;
            }
            if version == 0 {
                conn.pragma_update(None, "auto_vacuum", "INCREMENTAL")?;
            }
            conn.pragma_update(None, "journal_mode", "WAL")?;
            conn.pragma_update(None, "foreign_keys", "ON")?;
            conn.pragma_update(None, "synchronous", "FULL")?;
            if version < 3 {
                let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
                if version == 0 {
                    tx.execute_batch(SCHEMA)?;
                }
                if version < 2 {
                    tx.execute_batch(super::chat_schema::MIGRATION)?;
                }
                tx.execute_batch(super::chat_schema::OBSERVATION_MIGRATION)?;
                tx.pragma_update(None, "user_version", 3)?;
                tx.commit()?;
            }
            Ok(Self {
                conn,
                path,
                chat_snapshots: Default::default(),
                observation_cache: Default::default(),
                size_notice_bytes: 200 * 1024 * 1024,
                #[cfg(test)]
                chat_space_override: None,
            })
        };
        init().map_err(|e| match e {
            BusError::Refused(s) => BusError::Refused(format!("bus folder {}: {s}",dir.display())),
            BusError::Failed(s) => BusError::Failed(format!("bus folder {}: {s}; grant this folder write access or set MUKADE_DATA_DIR to a writable absolute folder",dir.display())),
        })
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub(crate) fn write<T>(
        &mut self,
        f: impl FnOnce(&Transaction<'_>) -> Result<T, BusError>,
    ) -> Result<T, BusError> {
        check_space(self.path.parent().expect("database folder"))?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let value = f(&tx)?;
        tx.commit()?;
        Ok(value)
    }
    pub fn set_chat_snapshot_lifetime(&mut self, lifetime: Duration) {
        self.chat_snapshots.lifetime = lifetime;
    }

    pub fn set_chat_size_notice_bytes(&mut self, threshold: u64) {
        self.size_notice_bytes = threshold;
    }

    pub fn size_notice(&self) -> Option<String> {
        let bytes = std::fs::metadata(&self.path).ok()?.len()
            + std::fs::metadata(self.path.with_file_name("bus.db-wal"))
                .map(|m| m.len())
                .unwrap_or(0);
        if bytes < self.size_notice_bytes {
            return None;
        }
        let mut q=self.conn.prepare("SELECT room FROM events GROUP BY room ORDER BY SUM(length(body_json)) DESC LIMIT 3").ok()?;
        let names = q
            .query_map([], |r| r.get::<_, String>(0))
            .ok()?
            .filter_map(Result::ok)
            .collect::<Vec<_>>();
        Some(format!(
            "bus uses {} MiB; largest rooms: {}; export, close, then explicitly remove an unwanted room",
            bytes / 1024 / 1024,
            names.join(", ")
        ))
    }
    pub(crate) fn reclaim(&self) {
        // Reclamation is best effort after a successful logical deletion; readers may pin WAL.
        let _ = self
            .conn
            .execute_batch("PRAGMA incremental_vacuum; PRAGMA wal_checkpoint(TRUNCATE);");
    }
}

fn resolve_path(path: &Path) -> Result<PathBuf, BusError> {
    let absolute = std::path::absolute(path)?;
    let mut ancestor = absolute.as_path();
    let mut tail = Vec::new();
    while !ancestor.exists() {
        if let Some(name) = ancestor.file_name() {
            tail.push(name.to_os_string());
        }
        ancestor = ancestor
            .parent()
            .ok_or_else(|| BusError::Failed(format!("cannot resolve {}", path.display())))?;
    }
    let mut resolved = ancestor.canonicalize()?;
    for part in tail.into_iter().rev() {
        resolved.push(part);
    }
    Ok(resolved)
}
pub(crate) fn ensure_outside_stores(
    path: &Path,
    roots: &crate::roots::StoreRoots,
) -> Result<(), BusError> {
    let resolved = resolve_path(path)?;
    let vars = crate::catalog::Vars::from_env();
    for h in crate::HarnessId::ALL {
        let root = roots
            .root(h)
            .map(Path::to_path_buf)
            .or_else(|| crate::catalog::store_default_with(h, &vars));
        if let Some(root) = root {
            let root = crate::harnesses::get(h).store_folder(&root);
            if let Ok(native) = resolve_path(&root) {
                if resolved.starts_with(&native) {
                    return Err(BusError::Refused(format!(
                        "destination {} is inside the {h} native store; choose another folder",
                        path.display()
                    )));
                }
            }
        }
    }
    Ok(())
}
pub(crate) fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
pub(crate) fn event(
    tx: &Transaction<'_>,
    room: &str,
    kind: &str,
    member: Option<&str>,
    body: serde_json::Value,
) -> Result<(), BusError> {
    tx.execute(
        "INSERT INTO events(room,at,kind,member,body_json) VALUES(?1,?2,?3,?4,?5)",
        rusqlite::params![room, now(), kind, member, body.to_string()],
    )?;
    Ok(())
}

pub(super) fn check_space(dir: &Path) -> Result<(), BusError> {
    if free_space(dir).is_some_and(|n| n < 1024 * 1024 * 1024) {
        return Err(BusError::Refused(format!(
            "less than 1 GiB free in bus folder {}",
            dir.display()
        )));
    }
    Ok(())
}

#[cfg(windows)]
fn free_space(dir: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetDiskFreeSpaceExW(
            path: *const u16,
            available: *mut u64,
            total: *mut u64,
            free: *mut u64,
        ) -> i32;
    }
    let path: Vec<u16> = dir.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut available = 0;
    // The NUL-terminated path and writable u64 remain alive for this synchronous call.
    (unsafe {
        GetDiskFreeSpaceExW(
            path.as_ptr(),
            &mut available,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    } != 0)
        .then_some(available)
}
#[cfg(not(windows))]
fn free_space(_dir: &Path) -> Option<u64> {
    // std exposes no portable free-space query. A platform without a supported query does not
    // refuse a write on guessed capacity; SQLite/I/O errors are still propagated.
    None
}

const SCHEMA:&str = "
CREATE TABLE rooms(name TEXT PRIMARY KEY, opened_at TEXT NOT NULL, closed_at TEXT, opened_by TEXT, kept INTEGER NOT NULL DEFAULT 0);
CREATE TABLE events(seq INTEGER PRIMARY KEY AUTOINCREMENT, room TEXT NOT NULL REFERENCES rooms(name) ON DELETE CASCADE, at TEXT NOT NULL, kind TEXT NOT NULL, member TEXT, body_json TEXT NOT NULL);
CREATE INDEX events_room_seq ON events(room,seq);
CREATE TABLE members(room TEXT NOT NULL REFERENCES rooms(name) ON DELETE CASCADE, name TEXT NOT NULL, harness TEXT NOT NULL, role TEXT NOT NULL, joined_at TEXT NOT NULL, left_at TEXT, last_command_at TEXT, session_harness TEXT, session_id TEXT, session_cwd TEXT, PRIMARY KEY(room,name));
CREATE UNIQUE INDEX members_binding ON members(room,session_harness,session_id) WHERE session_id IS NOT NULL AND left_at IS NULL;
CREATE TABLE observe_cursors(room TEXT NOT NULL REFERENCES rooms(name) ON DELETE CASCADE, observer TEXT NOT NULL, target TEXT NOT NULL, position INTEGER NOT NULL, last_entry_id TEXT, last_fingerprint TEXT, updated_at TEXT NOT NULL, binding TEXT NOT NULL, seen_json TEXT NOT NULL DEFAULT '{}', order_json TEXT NOT NULL DEFAULT '[]', PRIMARY KEY(room,observer,target));
CREATE TABLE observations(id INTEGER PRIMARY KEY AUTOINCREMENT, room TEXT NOT NULL REFERENCES rooms(name) ON DELETE CASCADE, observer TEXT NOT NULL, target TEXT NOT NULL, at TEXT NOT NULL, entries_shown INTEGER NOT NULL, entries_left_out INTEGER NOT NULL);
";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::roots::StoreRoots;
    use serde_json::{Value, json};
    use uuid::Uuid;

    fn scratch() -> PathBuf {
        let dir = std::env::var_os("MUKADE_TEST_SCRATCH_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap().join("target/tmp"))
            .join(format!("chat-schema-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn legacy_db(dir: &Path) -> Connection {
        let conn = Connection::open(dir.join("bus.db")).unwrap();
        conn.execute_batch(SCHEMA).unwrap();
        conn.execute_batch(r#"
            PRAGMA user_version=1;
            INSERT INTO rooms(name,opened_at) VALUES('legacy','old');
            INSERT INTO members(room,name,harness,role,joined_at) VALUES('legacy','observer','pi','observer','old');
            INSERT INTO members(room,name,harness,role,joined_at,session_harness,session_id)
                VALUES('legacy','peer','pi','peer','old','pi','full-native-id');
            INSERT INTO events(room,at,kind,member,body_json)
                VALUES('legacy','old','join','peer','{"role":"peer"}');
            INSERT INTO observe_cursors(room,observer,target,position,updated_at,binding,seen_json,order_json)
                VALUES('legacy','observer','peer',1,'old','pi:full-native-id','{"native-entry":"fingerprint"}','["native-entry"]');
            INSERT INTO observations(room,observer,target,at,entries_shown,entries_left_out)
                VALUES('legacy','observer','peer','old',1,0);
        "#).unwrap();
        conn
    }

    fn call(bus: &mut Bus, operation: &str, args: Value) -> Value {
        bus.chat_call(operation, args, &StoreRoots::default())
            .unwrap()
    }

    fn mutate(bus: &mut Bus, operation: &str, mut args: Value) -> Value {
        args["request_id"] = json!(Uuid::new_v4().to_string());
        call(bus, operation, args)
    }

    #[test]
    fn v1_migration_preserves_events_members_and_acknowledged_observations() {
        let dir = scratch();
        drop(legacy_db(&dir));
        let mut bus = Bus::open_in(&dir).unwrap();
        let id = call(&mut bus, "rooms", json!({}))["items"][0]["room_id"].clone();
        let page = call(&mut bus, "export", json!({"room_id":id}));
        let items = page["items"].as_array().unwrap();
        let peer = items
            .iter()
            .find(|r| r["member"]["name"] == "peer")
            .unwrap();
        assert_eq!(peer["member"]["session"]["id"], "full-native-id");
        let peer_id = &peer["member"]["member_id"];
        let event = items.iter().find(|r| r["type"] == "event").unwrap();
        assert_eq!(event["event"]["event_id"], "1");
        assert_eq!(event["event"]["actor"]["member_id"], *peer_id);
        assert_eq!(event["event"]["actor"]["role"], "peer");
        assert!(items.iter().any(|r| r["observation"]["entries_shown"] == 1));
        let cursor: String = bus
            .conn
            .query_row("SELECT seen_json FROM observe_cursors", [], |r| r.get(0))
            .unwrap();
        assert_eq!(cursor, r#"{"native-entry":"fingerprint"}"#);
        drop(bus);
        let mut bus = Bus::open_in(&dir).unwrap();
        assert_eq!(
            call(&mut bus, "members", json!({"room_id":id}))["items"][1]["member_id"],
            *peer_id
        );
    }

    #[test]
    fn v1_migration_error_rolls_back_columns_data_and_version() {
        let dir = scratch();
        let old = legacy_db(&dir);
        // The conflicting table is encountered after the ALTERs and backfill.
        old.execute_batch(
            "CREATE TABLE chat_service (sentinel TEXT); INSERT INTO chat_service VALUES('keep')",
        )
        .unwrap();
        drop(old);
        assert!(Bus::open_in(&dir).is_err());
        let old = Connection::open(dir.join("bus.db")).unwrap();
        assert_eq!(
            old.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert!(old.prepare("SELECT chat_id FROM rooms").is_err());
        assert_eq!(
            old.query_row("SELECT sentinel FROM chat_service", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "keep"
        );
        assert_eq!(
            old.query_row("SELECT count(*) FROM events", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
        old.execute("DROP TABLE chat_service", []).unwrap();
        assert!(Bus::open_in(&dir).is_ok());
    }

    #[test]
    fn v2_observation_migration_preserves_acknowledged_metadata_and_rolls_back_failure() {
        let dir = scratch();
        let old = legacy_db(&dir);
        old.execute_batch(super::super::chat_schema::MIGRATION)
            .unwrap();
        old.pragma_update(None, "user_version", 2).unwrap();
        let old_ids:(String,String)=old.query_row("SELECT r.chat_id,m.chat_id FROM rooms r JOIN members m ON m.room=r.name WHERE m.name='peer'",[],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
        old.execute_batch("CREATE TABLE chat_observe_batches(sentinel TEXT); INSERT INTO chat_observe_batches VALUES('keep');").unwrap();
        drop(old);
        assert!(Bus::open_in(&dir).is_err());
        let old = Connection::open(dir.join("bus.db")).unwrap();
        assert_eq!(
            old.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                .unwrap(),
            2
        );
        assert!(
            old.prepare("SELECT projection_version FROM observe_cursors")
                .is_err()
        );
        assert!(old.prepare("SELECT * FROM chat_observe_cursors").is_err());
        assert_eq!(
            old.query_row("SELECT sentinel FROM chat_observe_batches", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "keep"
        );
        old.execute_batch("DROP TABLE chat_observe_batches")
            .unwrap();
        drop(old);
        let bus = Bus::open_in(&dir).unwrap();
        assert_eq!(
            bus.conn
                .pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                .unwrap(),
            3
        );
        let migrated: (String, String, u32, i64) = bus
            .conn
            .query_row(
                "SELECT seen_json,order_json,projection_version,revision FROM chat_observe_cursors",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap();
        assert_eq!(
            migrated,
            (
                r#"{"native-entry":"fingerprint"}"#.into(),
                r#"["native-entry"]"#.into(),
                1,
                0
            )
        );
        let new_ids:(String,String)=bus.conn.query_row("SELECT r.chat_id,m.chat_id FROM rooms r JOIN members m ON m.room=r.name WHERE m.name='peer'",[],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
        assert_eq!(new_ids, old_ids);
    }

    #[test]
    fn v1_failure_in_final_migration_rolls_back_entire_upgrade() {
        let dir = scratch();
        let old = legacy_db(&dir);
        old.execute_batch("CREATE TABLE chat_observe_batches(sentinel TEXT); INSERT INTO chat_observe_batches VALUES('keep');").unwrap();
        drop(old);
        assert!(Bus::open_in(&dir).is_err());
        let old = Connection::open(dir.join("bus.db")).unwrap();
        assert_eq!(
            old.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert!(old.prepare("SELECT chat_id FROM rooms").is_err());
        assert!(
            old.prepare("SELECT projection_version FROM observe_cursors")
                .is_err()
        );
        assert!(old.prepare("SELECT * FROM chat_service").is_err());
        assert_eq!(
            old.query_row("SELECT sentinel FROM chat_observe_batches", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "keep"
        );
        assert_eq!(
            old.query_row("SELECT seen_json FROM observe_cursors", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            r#"{"native-entry":"fingerprint"}"#
        );
        old.execute_batch("DROP TABLE chat_observe_batches")
            .unwrap();
        drop(old);
        assert!(Bus::open_in(&dir).is_ok());
    }

    #[test]
    fn migrated_prefix_fingerprints_upgrade_before_later_full_field_changes() {
        use crate::{bus::view, reader};
        for version in [1, 2] {
            let dir = scratch();
            let native = dir.join("native");
            let file = native.join("sessions/project/live_full-native-id.jsonl");
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            let content = |suffix: &str| {
                [json!({"type":"session","version":3,"id":"full-native-id","cwd":"/observe"}),
                 json!({"type":"message","id":"native-entry","parentId":null,"message":{"role":"user","content":[{"type":"text","text":format!("{}{suffix}","p".repeat(1300))}],"timestamp":1794391200000i64}})]
                .into_iter().map(|v|v.to_string()+"\n").collect::<String>()
            };
            std::fs::write(&file, content("a")).unwrap();
            let item = view::build(&reader::pi::read_file(&file).unwrap()).remove(0);
            let old = legacy_db(&dir);
            old.execute(
                "UPDATE observe_cursors SET seen_json=?1",
                [json!({"native-entry":view::legacy_fingerprint(&item)}).to_string()],
            )
            .unwrap();
            if version == 2 {
                old.execute_batch(super::super::chat_schema::MIGRATION)
                    .unwrap();
                old.pragma_update(None, "user_version", 2).unwrap();
            }
            drop(old);
            let mut bus = Bus::open_in(&dir).unwrap();
            let room = call(&mut bus, "rooms", json!({}))["items"][0]["room_id"].clone();
            let members = call(&mut bus, "members", json!({"room_id":room}));
            let observer = members["items"]
                .as_array()
                .unwrap()
                .iter()
                .find(|m| m["name"] == "observer")
                .unwrap();
            let target = members["items"]
                .as_array()
                .unwrap()
                .iter()
                .find(|m| m["name"] == "peer")
                .unwrap();
            let recovered = mutate(
                &mut bus,
                "rejoin",
                json!({"room_id":room,"member_id":observer["member_id"]}),
            );
            let roots = StoreRoots {
                pi_agent_dir: Some(native),
                ..Default::default()
            };
            let args = json!({"room_id":room,"member_handle":recovered["member_handle"],"target_member_id":target["member_id"]});
            let empty = bus.chat_call("observe", args.clone(), &roots).unwrap();
            assert_eq!(empty["batch_id"], Value::Null);
            std::fs::write(&file, content("b")).unwrap();
            let update = bus.chat_call("observe", args, &roots).unwrap();
            assert_eq!(update["items"][0]["kind"], "updated");
        }
    }

    #[test]
    fn low_space_refuses_content_and_permits_receipts_and_cleanup() {
        let dir = scratch();
        let mut bus = Bus::open_in(&dir).unwrap();
        let room = mutate(&mut bus, "open", json!({"name":"disk"}));
        let id = &room["room_id"];
        let peer = mutate(
            &mut bus,
            "join",
            json!({"room_id":id,"harness":"pi","role":"peer"}),
        );
        let handle = &peer["member_handle"];
        let first = json!({"room_id":id,"member_handle":handle,"text":"already committed","request_id":Uuid::new_v4().to_string()});
        let committed = call(&mut bus, "send", first.clone());
        bus.chat_space_override = Some(0);
        assert_eq!(call(&mut bus, "send", first), committed);
        let new = json!({"room_id":id,"member_handle":handle,"text":"new content","request_id":Uuid::new_v4().to_string()});
        let error = bus
            .chat_call("send", new, &StoreRoots::default())
            .unwrap_err();
        assert_eq!(error.code, "storage_low");
        let batch = call(
            &mut bus,
            "read",
            json!({"room_id":id,"member_handle":handle}),
        );
        call(
            &mut bus,
            "ack",
            json!({"room_id":id,"member_handle":handle,"batch_id":batch["batch_id"]}),
        );
        mutate(&mut bus, "close", json!({"room_id":id}));
        mutate(
            &mut bus,
            "unbind",
            json!({"room_id":id,"member_handle":handle}),
        );
        mutate(
            &mut bus,
            "leave",
            json!({"room_id":id,"member_handle":handle}),
        );
        let rooms = call(&mut bus, "rooms", json!({}));
        assert_eq!(
            mutate(
                &mut bus,
                "delete",
                json!({"room_id":id,"expected_revision":rooms["items"][0]["revision"],"confirm":true})
            )["deleted"],
            true
        );
    }
}
