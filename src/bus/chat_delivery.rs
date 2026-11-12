use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde_json::{Value, json};
use uuid::Uuid;

use super::{
    Bus,
    chat_domain::{self as domain, ChatError, EVENT_SELECT, Member, Result, Room, arg, required},
};

pub(super) fn call(bus: &mut Bus, op: &str, args: &Value) -> Result<Value> {
    let id = required(args, "room_id");
    let handle = required(args, "member_handle");
    if op == "ack" {
        if super::chat_observe::owns_batch(&bus.conn, required(args, "batch_id"))? {
            return super::chat_observe::acknowledge(
                bus,
                id,
                handle,
                required(args, "batch_id"),
                None,
            );
        }
        return acknowledge(bus, id, handle, required(args, "batch_id"));
    }

    // Commit an included acknowledgement independently of staging the next batch.
    let included = arg(args, "ack_batch_id");
    if let Some(batch) = included {
        acknowledge(bus, id, handle, batch)?;
    }
    stage(bus, op, args, id, handle, included).map_err(|mut error| {
        error.ack_batch_id = included.map(str::to_owned);
        error
    })
}

fn acknowledge(bus: &mut Bus, room_id: &str, handle: &str, id: &str) -> Result<Value> {
    let tx = bus
        .conn
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    let room = domain::room(&tx, room_id)?;
    let member = domain::member(&tx, &room, handle)?;
    let (expected, last, acknowledged): (i64,i64,bool) = tx.query_row(
        "SELECT expected_cursor,last_event,acknowledged FROM chat_room_batches WHERE id=?1 AND member_id=?2",
        params![id,member.id], |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)),
    ).optional()?.ok_or_else(||ChatError::new("batch_not_found","No room batch with that ID belongs to this member"))?;
    if !acknowledged {
        if member.cursor != expected {
            return Err(ChatError::new(
                "batch_invalidated",
                "This batch no longer matches the committed room cursor",
            ));
        }
        tx.execute(
            "UPDATE members SET chat_cursor=?2,last_command_at=?3 WHERE chat_id=?1",
            params![member.id, last, super::store::now()],
        )?;
        tx.execute(
            "UPDATE chat_room_batches SET acknowledged=1 WHERE id=?1",
            [id],
        )?;
    }
    let answer = json!({"status":"ok","service_id":domain::service_id(&tx)?,
        "room_id":room.id,"member_id":member.id,"stream_id":domain::room_stream(&member.id),
        "ack_batch_id":id,"acknowledged_through":last.to_string()});
    tx.commit()?;
    Ok(answer)
}

struct Pending {
    id: String,
    last: i64,
    high: i64,
    trigger: Option<i64>,
    remaining: i64,
}

fn pending(conn: &Connection, member: &Member) -> Result<Option<Pending>> {
    Ok(conn.query_row(
        "SELECT id,last_event,high_water,trigger_event,remaining FROM chat_room_batches WHERE member_id=?1 AND acknowledged=0",
        [&member.id],
        |r| Ok(Pending { id:r.get(0)?,last:r.get(1)?,high:r.get(2)?,trigger:r.get(3)?,remaining:r.get(4)? }),
    ).optional()?)
}

fn stage(
    bus: &mut Bus,
    op: &str,
    args: &Value,
    room_id: &str,
    handle: &str,
    ack: Option<&str>,
) -> Result<Value> {
    // A deferred transaction suffices for peeks and waiting with an empty backlog.
    let behavior = if op == "peek" {
        TransactionBehavior::Deferred
    } else {
        TransactionBehavior::Immediate
    };
    let tx = bus.conn.transaction_with_behavior(behavior)?;
    let room = domain::room(&tx, room_id)?;
    let member = domain::member(&tx, &room, handle)?;
    let batch = if op == "peek" {
        None
    } else {
        pending(&tx, &member)?
    };
    let mut response = domain::result(&tx, "ok")?;
    response["room_id"] = json!(room_id);
    response["member_id"] = json!(member.id);
    response["stream_id"] = json!(domain::room_stream(&member.id));
    response["batch_id"] = Value::Null;
    response["notices"] = json!([]);
    response["skipped"] = json!(0);
    response["items"] = json!([]);
    if let Some(ack) = ack {
        response["ack_batch_id"] = json!(ack);
    }
    if let Some(batch) = batch {
        response["batch_id"] = json!(batch.id);
        response["high_water"] = json!(batch.high.to_string());
        response["trigger_event_id"] = json!(batch.trigger.map(|n| n.to_string()));
        response["remaining"] = json!(batch.remaining);
        response["items"] = json!(collect_range(&tx, &room, member.cursor, batch.last)?);
        return Ok(response);
    }

    let high = domain::high_water(&tx, &room)?;
    let count: i64 = tx.query_row(
        "SELECT count(*) FROM events WHERE room=?1 AND seq>?2 AND seq<=?3",
        params![room.name, member.cursor, high],
        |r| r.get(0),
    )?;
    response["high_water"] = json!(high.to_string());
    response["remaining"] = json!(count);
    response["trigger_event_id"] = Value::Null;
    let mut trigger = None;
    if op == "wait" {
        let addressed = args["addressed_only"].as_bool().unwrap_or(false);
        trigger = tx.query_row(
            "SELECT seq FROM events e WHERE room=?1 AND seq>?2 AND seq<=?3
            AND kind IN ('message','signal') AND chat_actor_id!=?4
            AND (NOT ?5 OR EXISTS(SELECT 1 FROM chat_recipients r WHERE r.event=e.seq AND r.member_id=?4))
            ORDER BY seq LIMIT 1",
            params![room.name,member.cursor,high,member.id,addressed], |r|r.get::<_,i64>(0),
        ).optional()?;
        if trigger.is_none() {
            response["status"] = json!(if room.closed.is_some() {
                "room_closed"
            } else {
                "waiting"
            });
            return Ok(response);
        }
        response["trigger_event_id"] = json!(trigger.map(|id| id.to_string()));
    }
    if count == 0 {
        return Ok(response);
    }

    let batch_id = (op != "peek").then(|| Uuid::new_v4().to_string());
    response["batch_id"] = json!(batch_id);
    let (items, last) = budgeted_range(
        &tx,
        &room,
        member.cursor,
        high,
        count,
        &response,
        domain::number(args, "budget_chars", 8_000) as usize,
    )?;
    let remaining = count - items.len() as i64;
    response["remaining"] = json!(remaining);
    response["items"] = json!(items);
    if let Some(id) = batch_id {
        tx.execute("INSERT INTO chat_room_batches(id,member_id,expected_cursor,last_event,high_water,trigger_event,remaining) VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![id,member.id,member.cursor,last,high,trigger,remaining])?;
        domain::touch(&tx, &member.id)?;
    }
    tx.commit()?;
    Ok(response)
}

fn collect_range(conn: &Connection, room: &Room, after: i64, through: i64) -> Result<Vec<Value>> {
    let mut stmt = conn.prepare(&format!(
        "{EVENT_SELECT} WHERE e.room=?1 AND e.seq>?2 AND e.seq<=?3 ORDER BY e.seq"
    ))?;
    let mut rows = stmt.query(params![room.name, after, through])?;
    let mut items = Vec::new();
    while let Some(row) = rows.next()? {
        items.push(domain::event_value(row, &room.id)?);
    }
    Ok(items)
}

fn budgeted_range(
    conn: &Connection,
    room: &Room,
    after: i64,
    through: i64,
    count: i64,
    response: &Value,
    budget: usize,
) -> Result<(Vec<Value>, i64)> {
    let mut stmt = conn.prepare(&format!(
        "{EVENT_SELECT} WHERE e.room=?1 AND e.seq>?2 AND e.seq<=?3 ORDER BY e.seq"
    ))?;
    let mut rows = stmt.query(params![room.name, after, through])?;
    let overhead = response.to_string().chars().count() - count.to_string().len();
    let mut used = 0;
    let mut items = Vec::new();
    let mut last = after;
    while let Some(row) = rows.next()? {
        let item = domain::event_value(row, &room.id)?;
        let size = item.to_string().chars().count() + usize::from(!items.is_empty());
        let left = count - items.len() as i64 - 1;
        if !items.is_empty() && overhead + used + size + left.to_string().len() > budget {
            break;
        }
        used += size;
        last = row.get(0)?;
        items.push(item);
    }
    Ok((items, last))
}
