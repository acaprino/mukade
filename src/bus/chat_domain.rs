use crate::roots::StoreRoots;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::fmt;
use uuid::Uuid;

use super::{Bus, BusError, chat_delivery, chat_mutation, chat_query, chat_tools};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct ChatError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ack_batch_id: Option<String>,
}

impl ChatError {
    pub(crate) fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            retryable: false,
            ack_batch_id: None,
        }
    }

    pub(super) fn invalid(message: impl Into<String>) -> Self {
        Self::new("invalid_arguments", message)
    }

    pub(super) fn storage() -> Self {
        Self {
            retryable: true,
            ..Self::new("storage_failure", "The chat database operation failed")
        }
    }

    pub fn exit_code(&self) -> i32 {
        match self.code.as_str() {
            "storage_failure" | "storage_low" | "not_implemented" | "server_stopping"
            | "transport_failure" | "protocol_failure" => 4,
            _ => 2,
        }
    }
}

impl fmt::Display for ChatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ChatError {}

impl From<rusqlite::Error> for ChatError {
    fn from(_: rusqlite::Error) -> Self {
        Self::storage()
    }
}

impl From<serde_json::Error> for ChatError {
    fn from(_: serde_json::Error) -> Self {
        Self::storage()
    }
}

impl From<BusError> for ChatError {
    /// A database failure keeps its details out of the answer; a refusal for low space keeps its
    /// reason, which names the folder and what to do.
    fn from(error: BusError) -> Self {
        match error {
            BusError::Refused(reason) if reason.contains(super::store::LOW_SPACE) => Self {
                retryable: true,
                ..Self::new("storage_low", reason)
            },
            _ => Self::storage(),
        }
    }
}

pub(super) type Result<T> = std::result::Result<T, ChatError>;

impl Bus {
    pub fn service_id(&self) -> Result<String> {
        service_id(&self.conn)
    }

    /// Identify the durable stream for the transport's in-flight read guard.
    pub fn chat_stream_key(&self, operation: &str, arguments: &Value) -> Result<Option<String>> {
        let operation = operation.strip_prefix("chat_").unwrap_or(operation);
        chat_tools::validate(operation, arguments)?;
        if !matches!(operation, "read" | "wait" | "observe") {
            return Ok(None);
        }
        let tx = self.conn.unchecked_transaction()?;
        let room = room(&tx, required(arguments, "room_id"))?;
        let member = member(&tx, &room, required(arguments, "member_handle"))?;
        if operation != "observe" {
            return Ok(Some(room_stream(&member.id)));
        }
        require_role(&member.role, &["observer"])?;
        let target = required(arguments, "target_member_id");
        if target == member.id {
            return Err(ChatError::new(
                "target_refused",
                "An observer must select a different room member",
            ));
        }
        let generation: i64 = tx.query_row(
            "SELECT binding_generation FROM members WHERE room=?1 AND chat_id=?2 AND left_at IS NULL AND session_id IS NOT NULL",
            params![room.name,target], |r|r.get(0),
        ).optional()?.ok_or_else(||ChatError::new("target_unbound","The target must be an active member with a native session binding"))?;
        Ok(Some(format!(
            "observation:{}:{target}:{generation}",
            member.id
        )))
    }

    pub fn chat_call(
        &mut self,
        operation: &str,
        arguments: Value,
        roots: &StoreRoots,
    ) -> Result<Value> {
        let operation = operation.strip_prefix("chat_").unwrap_or(operation);
        let operation = chat_tools::validate(operation, &arguments)?;
        let mut args = arguments;
        if let Some(id) = arg(&args, "request_id") {
            args["request_id"] = json!(
                Uuid::parse_str(id)
                    .map_err(|_| ChatError::invalid("request_id must be a UUID"))?
                    .to_string()
            );
        }
        match operation.name {
            "read" | "peek" | "wait" | "ack" => chat_delivery::call(self, operation.name, &args),
            "rooms" | "members" | "ledger" | "export" | "candidates" => {
                chat_query::call(self, operation.name, &args, roots)
            }
            "observe" => super::chat_observe::call(self, &args, roots),
            name => chat_mutation::call(self, name, &args, roots),
        }
    }
}

pub(super) fn arg<'a>(args: &'a Value, name: &str) -> Option<&'a str> {
    args.get(name).and_then(Value::as_str)
}

pub(super) fn required<'a>(args: &'a Value, name: &str) -> &'a str {
    arg(args, name).expect("validated required string")
}

pub(super) fn number(args: &Value, name: &str, default: u64) -> u64 {
    args.get(name).and_then(Value::as_u64).unwrap_or(default)
}

pub(super) fn service_id(conn: &Connection) -> Result<String> {
    Ok(
        conn.query_row("SELECT id FROM chat_service WHERE singleton=1", [], |r| {
            r.get(0)
        })?,
    )
}

pub(super) fn deleted() -> ChatError {
    ChatError::new("resource_deleted", "The room was deleted")
}

pub(super) fn stale() -> ChatError {
    ChatError::new(
        "stale_member",
        "The member handle is stale; recover the member with chat_rejoin",
    )
}

pub(super) fn require_role(actual: &str, roles: &[&str]) -> Result<()> {
    if roles.contains(&actual) {
        Ok(())
    } else {
        Err(ChatError::new(
            "role_refused",
            format!("This operation requires the {} role", roles.join(" or ")),
        ))
    }
}

#[derive(Debug)]
pub(super) struct Room {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) closed: Option<String>,
}

impl Room {
    pub(super) fn require_open(&self) -> Result<()> {
        if self.closed.is_some() {
            return Err(ChatError::new(
                "room_closed",
                format!("Room {} is closed; use chat_reopen", self.id),
            ));
        }
        Ok(())
    }
}

pub(super) fn room(conn: &Connection, id: &str) -> Result<Room> {
    if let Some(room) = conn
        .query_row(
            "SELECT chat_id,name,closed_at FROM rooms WHERE chat_id=?1",
            [id],
            |r| {
                Ok(Room {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    closed: r.get(2)?,
                })
            },
        )
        .optional()?
    {
        return Ok(room);
    }
    if conn
        .query_row(
            "SELECT 1 FROM chat_deleted_rooms WHERE room_id=?1",
            [id],
            |_| Ok(()),
        )
        .optional()?
        .is_some()
    {
        Err(deleted())
    } else {
        Err(ChatError::new("room_not_found", "No room has that ID"))
    }
}

#[derive(Debug)]
pub(super) struct Member {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) harness: String,
    pub(super) role: String,
    pub(super) cursor: i64,
}

pub(super) fn member(conn: &Connection, room: &Room, handle: &str) -> Result<Member> {
    conn.query_row(
        "SELECT chat_id,name,harness,role,chat_cursor FROM members WHERE room=?1 AND chat_handle=?2 AND left_at IS NULL",
        params![room.name, handle],
        |r| Ok(Member { id: r.get(0)?, name: r.get(1)?, harness: r.get(2)?, role: r.get(3)?, cursor: r.get(4)? }),
    ).optional()?.ok_or_else(stale)
}

pub(super) fn high_water(conn: &Connection, room: &Room) -> Result<i64> {
    Ok(conn.query_row(
        "SELECT coalesce(max(seq),0) FROM events WHERE room=?1",
        [&room.name],
        |r| r.get(0),
    )?)
}

pub(super) fn result(conn: &Connection, status: &str) -> Result<Value> {
    Ok(json!({"service_id":service_id(conn)?,"status":status}))
}

pub(super) fn touch(conn: &Connection, member_id: &str) -> Result<()> {
    conn.execute(
        "UPDATE members SET last_command_at=?2 WHERE chat_id=?1",
        params![member_id, super::store::now()],
    )?;
    Ok(())
}

pub(super) const EVENT_SELECT: &str = "SELECT e.seq,e.at,e.kind,e.member,e.chat_actor_id,e.chat_actor_role,e.body_json,
    coalesce((SELECT json_group_array(member_id) FROM (SELECT member_id FROM chat_recipients WHERE event=e.seq ORDER BY member_id)),'[]')
    FROM events e";

pub(super) fn event_value(row: &rusqlite::Row<'_>, room_id: &str) -> Result<Value> {
    let id: i64 = row.get(0)?;
    let at: String = row.get(1)?;
    let kind: String = row.get(2)?;
    let name: Option<String> = row.get(3)?;
    let actor_id: Option<String> = row.get(4)?;
    let role: Option<String> = row.get(5)?;
    let body: Value = serde_json::from_str(&row.get::<_, String>(6)?)?;
    let recipients: Value = serde_json::from_str(&row.get::<_, String>(7)?)?;
    let actor = match name {
        Some(name) => json!({"type":"member","member_id":actor_id,"name":name,"role":role}),
        None => json!({"type":"local_admin"}),
    };
    Ok(
        json!({"event_id":id.to_string(),"room_id":room_id,"timestamp":at,"kind":kind,
        "actor":actor,"payload":body,"recipient_ids":recipients}),
    )
}

pub(super) fn event_result(conn: &Connection, room: &Room, id: i64) -> Result<Value> {
    let mut stmt = conn.prepare(&format!("{EVENT_SELECT} WHERE e.room=?1 AND e.seq=?2"))?;
    let mut rows = stmt.query(params![room.name, id])?;
    let event = event_value(rows.next()?.ok_or_else(ChatError::storage)?, &room.id)?;
    let mut response = result(conn, "ok")?;
    response["room_id"] = json!(room.id);
    response["event_id"] = event["event_id"].clone();
    response["event"] = event;
    Ok(response)
}

pub(super) fn room_stream(member_id: &str) -> String {
    format!("room:{member_id}")
}
