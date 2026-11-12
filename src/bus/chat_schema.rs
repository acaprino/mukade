pub(super) const OBSERVATION_MIGRATION: &str = "
ALTER TABLE observe_cursors ADD COLUMN projection_version INTEGER NOT NULL DEFAULT 1;
CREATE TABLE chat_observe_cursors (
    observer_id TEXT NOT NULL REFERENCES members(chat_id) ON DELETE CASCADE,
    target_id TEXT NOT NULL REFERENCES members(chat_id) ON DELETE CASCADE,
    generation INTEGER NOT NULL, revision INTEGER NOT NULL DEFAULT 0,
    projection_version INTEGER NOT NULL, source_ref TEXT,
    seen_json TEXT NOT NULL, order_json TEXT NOT NULL,
    PRIMARY KEY(observer_id,target_id,generation)
);
INSERT INTO chat_observe_cursors(observer_id,target_id,generation,projection_version,source_ref,seen_json,order_json)
SELECT o.chat_id,t.chat_id,t.binding_generation,1,t.source_ref,c.seen_json,c.order_json
FROM observe_cursors c JOIN members o ON o.room=c.room AND o.name=c.observer
JOIN members t ON t.room=c.room AND t.name=c.target
WHERE c.binding=t.session_harness||':'||t.session_id AND o.left_at IS NULL AND t.left_at IS NULL;
CREATE TABLE chat_observe_batches (
    id TEXT PRIMARY KEY,
    observer_id TEXT NOT NULL REFERENCES members(chat_id) ON DELETE CASCADE,
    target_id TEXT NOT NULL REFERENCES members(chat_id) ON DELETE CASCADE,
    generation INTEGER NOT NULL, expected_revision INTEGER NOT NULL,
    projection_version INTEGER NOT NULL, source_ref TEXT NOT NULL,
    metadata_json TEXT NOT NULL, status TEXT NOT NULL CHECK(status IN ('pending','acknowledged','invalidated'))
);
CREATE UNIQUE INDEX chat_pending_observation ON chat_observe_batches(observer_id,target_id,generation) WHERE status='pending';
CREATE TRIGGER chat_invalidate_observation
AFTER UPDATE OF binding_generation,chat_handle,left_at ON members
WHEN NEW.binding_generation IS NOT OLD.binding_generation OR NEW.chat_handle IS NOT OLD.chat_handle OR NEW.left_at IS NOT OLD.left_at BEGIN
    UPDATE chat_observe_batches SET status='invalidated'
    WHERE status='pending' AND (observer_id=NEW.chat_id OR target_id=NEW.chat_id);
END;
";

pub(super) const MIGRATION: &str = "
ALTER TABLE rooms ADD COLUMN chat_id TEXT;
ALTER TABLE members ADD COLUMN chat_id TEXT;
ALTER TABLE members ADD COLUMN chat_handle TEXT;
ALTER TABLE members ADD COLUMN chat_cursor INTEGER NOT NULL DEFAULT 0;
ALTER TABLE members ADD COLUMN binding_generation INTEGER NOT NULL DEFAULT 0;
ALTER TABLE members ADD COLUMN source_ref TEXT;
ALTER TABLE events ADD COLUMN chat_actor_id TEXT;
ALTER TABLE events ADD COLUMN chat_actor_role TEXT;
UPDATE rooms SET chat_id=lower(hex(randomblob(16)));
UPDATE members SET chat_id=lower(hex(randomblob(16))),
    binding_generation=CASE WHEN session_id IS NULL THEN 0 ELSE 1 END;
UPDATE events SET
    chat_actor_id=(SELECT chat_id FROM members m WHERE m.room=events.room AND m.name=events.member),
    chat_actor_role=coalesce(json_extract(CASE WHEN json_valid(body_json) THEN body_json END,'$.role'),
        (SELECT role FROM members m WHERE m.room=events.room AND m.name=events.member));
CREATE UNIQUE INDEX rooms_chat_id ON rooms(chat_id);
CREATE UNIQUE INDEX members_chat_id ON members(chat_id);

CREATE TABLE chat_service (singleton INTEGER PRIMARY KEY CHECK(singleton=1), id TEXT NOT NULL);
INSERT INTO chat_service VALUES(1, lower(hex(randomblob(16))));
CREATE TABLE chat_deleted_rooms (room_id TEXT PRIMARY KEY);
CREATE TABLE chat_requests (
    operation TEXT NOT NULL, scope TEXT NOT NULL, request_id TEXT NOT NULL,
    room_id TEXT NOT NULL, args_hash TEXT NOT NULL, result TEXT, handle_hash TEXT,
    PRIMARY KEY(operation, scope, request_id)
);
CREATE INDEX chat_request_room ON chat_requests(room_id);
CREATE INDEX chat_request_leave_handle ON chat_requests(room_id,request_id,handle_hash)
    WHERE operation='leave';
CREATE TABLE chat_room_batches (
    id TEXT PRIMARY KEY,
    member_id TEXT NOT NULL REFERENCES members(chat_id) ON DELETE CASCADE,
    expected_cursor INTEGER NOT NULL, last_event INTEGER NOT NULL, high_water INTEGER NOT NULL,
    trigger_event INTEGER, remaining INTEGER NOT NULL, acknowledged INTEGER NOT NULL DEFAULT 0
);
CREATE UNIQUE INDEX chat_pending_room_batch ON chat_room_batches(member_id) WHERE acknowledged=0;
CREATE TABLE chat_recipients (
    event INTEGER NOT NULL REFERENCES events(seq) ON DELETE CASCADE,
    member_id TEXT NOT NULL REFERENCES members(chat_id) ON DELETE CASCADE,
    PRIMARY KEY(event,member_id)
);
CREATE INDEX chat_member_recipients ON chat_recipients(member_id,event);
CREATE TABLE chat_signals (
    instruction INTEGER NOT NULL REFERENCES events(seq) ON DELETE CASCADE,
    worker TEXT NOT NULL REFERENCES members(chat_id) ON DELETE CASCADE,
    signal TEXT NOT NULL, reason TEXT, event INTEGER NOT NULL REFERENCES events(seq) ON DELETE CASCADE,
    PRIMARY KEY(instruction,worker)
);
CREATE INDEX chat_latest_worker_signal ON chat_signals(worker,event DESC);

CREATE TRIGGER chat_id_room AFTER INSERT ON rooms WHEN NEW.chat_id IS NULL BEGIN
    UPDATE rooms SET chat_id=lower(hex(randomblob(16))) WHERE name=NEW.name;
END;
CREATE TRIGGER chat_id_member AFTER INSERT ON members WHEN NEW.chat_id IS NULL BEGIN
    UPDATE members SET chat_id=lower(hex(randomblob(16))),
        binding_generation=CASE WHEN NEW.session_id IS NULL THEN 0 ELSE 1 END WHERE rowid=NEW.rowid;
END;
CREATE TRIGGER chat_event_actor AFTER INSERT ON events WHEN NEW.member IS NOT NULL BEGIN
    UPDATE events SET
        chat_actor_id=(SELECT chat_id FROM members m WHERE m.room=NEW.room AND m.name=NEW.member),
        chat_actor_role=(SELECT role FROM members m WHERE m.room=NEW.room AND m.name=NEW.member)
    WHERE seq=NEW.seq;
END;
CREATE TRIGGER chat_revoke_legacy_member
AFTER UPDATE OF left_at,session_id,session_harness ON members
WHEN NEW.binding_generation=OLD.binding_generation BEGIN
    UPDATE members SET binding_generation=OLD.binding_generation+1, source_ref=NULL,
        chat_handle=CASE WHEN NEW.left_at IS NOT OLD.left_at THEN NULL ELSE chat_handle END
    WHERE rowid=NEW.rowid;
END;
CREATE TRIGGER chat_tombstone_room BEFORE DELETE ON rooms BEGIN
    INSERT INTO chat_deleted_rooms VALUES(OLD.chat_id);
    UPDATE chat_requests SET result=NULL,handle_hash=NULL,args_hash='' WHERE room_id=OLD.chat_id AND operation!='delete';
END;
";
