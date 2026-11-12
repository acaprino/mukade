use super::chat_domain::{ChatError, Result};
use serde_json::{Map, Value, json};
use uuid::Uuid;

#[derive(Clone, Copy)]
enum Kind {
    Plain,
    Uuid,
    Id,
    Event,
    Bool,
    Confirm,
    Integer(u64, u64),
    Choice(&'static [&'static str]),
    Names,
    RoomName,
    MemberName,
    Harness,
    Session,
    Message,
    Reason,
}

#[derive(Clone, Copy)]
struct Field {
    name: &'static str,
    kind: Kind,
    required: bool,
}

const fn field(name: &'static str, kind: Kind) -> Field {
    Field {
        name,
        kind,
        required: true,
    }
}

const fn optional(name: &'static str, kind: Kind) -> Field {
    Field {
        name,
        kind,
        required: false,
    }
}

use Kind::*;
const ROLES: &[&str] = &["observer", "peer", "lead", "worker"];
const BUDGET: Kind = Integer(1, 1_048_576);
const LIMIT: Kind = Integer(1, 500);

pub(super) struct Operation {
    pub(super) name: &'static str,
    description: &'static str,
    fields: &'static [Field],
    read_only: bool,
    deserialize: fn(&Value) -> bool,
}

// Declare the serde argument type and its tool schema together. Both parsing and the
// advertised schema reject fields outside the specific operation's argument structure.
macro_rules! tool {
    ($name:literal, $description:literal, $read_only:literal,
        { $($required:ident : $required_type:ty => $required_kind:expr),* $(,)? },
        { $($optional:ident : $optional_type:ty => $optional_kind:expr),* $(,)? }
    ) => {
        Operation {
            name: $name,
            description: $description,
            read_only: $read_only,
            fields: &[
                $(field(stringify!($required), $required_kind),)*
                $(optional(stringify!($optional), $optional_kind),)*
            ],
            deserialize: |value| {
                #[derive(serde::Deserialize)]
                #[serde(deny_unknown_fields)]
                #[allow(dead_code)]
                struct Input {
                    $($required: $required_type,)*
                    $($optional: Option<$optional_type>,)*
                }
                <Input as serde::Deserialize>::deserialize(value).is_ok()
            },
        }
    };
}

const OPERATIONS: &[Operation] = &[
    tool!("open", "Open a named chat room and return its immutable ID.", false,
        {request_id: String => Uuid, name: String => RoomName}, {}),
    tool!("close", "Close an open room while keeping its history and deliveries.", false,
        {request_id: String => Uuid, room_id: String => Id}, {}),
    tool!("reopen", "Reopen an explicitly selected closed room.", false,
        {request_id: String => Uuid, room_id: String => Id}, {}),
    tool!("delete", "Delete a closed room matching the reviewed revision.", false,
        {request_id: String => Uuid, room_id: String => Id, expected_revision: String => Event,
         confirm: bool => Confirm}, {}),
    tool!("rooms", "List chat rooms and their current revisions.", true,
        {}, {cursor: String => Uuid, limit: u64 => LIMIT}),
    tool!("join", "Join a room with a fixed role and optional exact native session ID.", false,
        {request_id: String => Uuid, room_id: String => Id, harness: String => Harness,
         role: String => Choice(ROLES)}, {name: String => MemberName, session_id: String => Session}),
    tool!("rejoin", "Recover a member and rotate its handle.", false,
        {request_id: String => Uuid, room_id: String => Id, member_id: String => Id},
        {session_id: String => Session}),
    tool!("leave", "Leave a room and revoke this member handle and binding.", false,
        {request_id: String => Uuid, room_id: String => Id, member_handle: String => Uuid}, {}),
    tool!("members", "List historical and active members of a room.", true,
        {room_id: String => Id}, {cursor: String => Uuid, limit: u64 => LIMIT}),
    tool!("candidates", "List native sessions from the configured store with exact IDs.", true,
        {harness: String => Choice(&["claude_code","claude-code","claude","codex","opencode","pi"])},
        {cwd: String => Plain, cursor: String => Uuid, limit: u64 => LIMIT}),
    tool!("bind", "Bind your member to an exact native session in a configured store.", false,
        {request_id: String => Uuid, room_id: String => Id, member_handle: String => Uuid,
         harness: String => Harness, session_id: String => Session}, {source_ref: String => Id}),
    tool!("unbind", "Revoke your member's native binding, even in a closed room.", false,
        {request_id: String => Uuid, room_id: String => Id, member_handle: String => Uuid}, {}),
    tool!("send", "Send a room message as a peer or lead. Use explicit recipients or inline mentions.", false,
        {request_id: String => Uuid, room_id: String => Id, member_handle: String => Uuid,
         text: String => Message}, {recipients: Vec<String> => Names}),
    tool!("signal", "Report done or blocked as a worker against a lead's addressed instruction.", false,
        {request_id: String => Uuid, room_id: String => Id, member_handle: String => Uuid,
         instruction_event_id: String => Event, signal: String => Choice(&["done","blocked"])},
        {reason: String => Reason}),
    tool!("read", "Stage unread events. Acknowledge the returned batch after delivery.", false,
        {room_id: String => Id, member_handle: String => Uuid},
        {budget_chars: u64 => BUDGET, ack_batch_id: String => Uuid}),
    tool!("peek", "Preview unread room events without staging or consuming them.", true,
        {room_id: String => Id, member_handle: String => Uuid}, {budget_chars: u64 => BUDGET}),
    tool!("wait", "Wait for another member's message or signal and stage unread events.", false,
        {room_id: String => Id, member_handle: String => Uuid},
        {budget_chars: u64 => BUDGET, ack_batch_id: String => Uuid, addressed_only: bool => Bool,
         timeout_seconds: u64 => Integer(0,300)}),
    tool!("ack", "Acknowledge receipt of a staged batch by its ID.", false,
        {room_id: String => Id, member_handle: String => Uuid, batch_id: String => Uuid}, {}),
    tool!("observe", "Stage a reduced view of another member's explicitly bound native session as an observer.", false,
        {room_id: String => Id, member_handle: String => Uuid, target_member_id: String => Id},
        {budget_chars: u64 => BUDGET, ack_batch_id: String => Uuid, latest: bool => Bool,
         last_turns: u64 => Integer(1,500)}),
    tool!("ledger", "Search a fixed snapshot of room events.", true,
        {room_id: String => Id},
        {member_handle: String => Uuid, actor: String => Plain, kind: String => Plain,
         since: String => Plain, until: String => Plain, mention: String => Plain,
         text: String => Plain, cursor: String => Uuid, limit: u64 => LIMIT}),
    tool!("export", "Page through a fixed snapshot of the room, members and events.", true,
        {room_id: String => Id}, {member_handle: String => Uuid, cursor: String => Uuid, limit: u64 => LIMIT}),
];

impl Kind {
    fn schema(self) -> Value {
        match self {
            Plain => json!({"type":"string"}),
            Uuid => json!({"type":"string","format":"uuid"}),
            Id => json!({"type":"string","minLength":1,"maxLength":128}),
            Event => json!({"type":"string","pattern":"^(0|[1-9][0-9]{0,18})$"}),
            Bool => json!({"type":"boolean"}),
            Confirm => json!({"type":"boolean","const":true}),
            Integer(min, max) => json!({"type":"integer","minimum":min,"maximum":max}),
            Choice(choices) => json!({"type":"string","enum":choices}),
            RoomName => json!({"type":"string","pattern":"^[a-z0-9][a-z0-9_-]{0,63}$"}),
            MemberName => {
                json!({"type":"string","pattern":"^[a-z0-9][a-z0-9_-]{0,31}$","not":{"const":"all"}})
            }
            Harness => json!({"type":"string","pattern":"^[a-z0-9][a-z0-9_-]{0,23}$"}),
            Session => {
                json!({"type":"string","minLength":1,"description":"The full native session ID from the configured store."})
            }
            Message => {
                json!({"type":"string","minLength":1,"maxLength":65536,"description":"Limit is 65,536 UTF-8 bytes."})
            }
            Reason => {
                json!({"type":"string","minLength":1,"maxLength":200,"description":"One line required for blocked. Omit for done."})
            }
            Names => json!({"type":"array","items":{"type":"string","minLength":1,"maxLength":32}}),
        }
    }

    fn accepts(self, value: &Value) -> bool {
        match self {
            RoomName | MemberName | Harness => value.as_str().is_some_and(|name| {
                let limit = match self {
                    RoomName => 64,
                    MemberName => 32,
                    _ => 24,
                };
                name.len() <= limit
                    && (!matches!(self, MemberName) || name != "all")
                    && name
                        .bytes()
                        .next()
                        .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
                    && name
                        .bytes()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"_-".contains(&c))
            }),
            Session => value.as_str().is_some_and(|s| !s.is_empty()),
            Message => value
                .as_str()
                .is_some_and(|s| !s.trim().is_empty() && s.len() <= 65_536 && !s.contains('\0')),
            Reason => value.as_str().is_some_and(|s| {
                !s.trim().is_empty()
                    && s.chars().count() <= 200
                    && !s.contains(['\n', '\r', '\0', '\u{2028}', '\u{2029}'])
            }),
            Plain => value.is_string(),
            Uuid => value.as_str().is_some_and(|s| Uuid::parse_str(s).is_ok()),
            Id => value
                .as_str()
                .is_some_and(|s| !s.is_empty() && s.chars().count() <= 128),
            Event => value
                .as_str()
                .is_some_and(|s| s.parse::<i64>().is_ok_and(|n| n >= 0 && s == n.to_string())),
            Bool => value.is_boolean(),
            Confirm => value.as_bool() == Some(true),
            Integer(min, max) => value.as_u64().is_some_and(|n| (min..=max).contains(&n)),
            Choice(choices) => value.as_str().is_some_and(|s| choices.contains(&s)),
            Names => value.as_array().is_some_and(|xs| {
                xs.iter().all(|x| {
                    x.as_str()
                        .is_some_and(|s| (1..=32).contains(&s.chars().count()))
                })
            }),
        }
    }
}

pub(super) fn validate(name: &str, arguments: &Value) -> Result<&'static Operation> {
    let op = OPERATIONS
        .iter()
        .find(|op| op.name == name)
        .ok_or_else(|| ChatError::new("unknown_operation", "Unknown chat operation"))?;
    let object = arguments
        .as_object()
        .ok_or_else(|| ChatError::invalid("Arguments must be a JSON object"))?;
    if object
        .keys()
        .any(|key| !op.fields.iter().any(|f| f.name == key))
    {
        return Err(ChatError::invalid("Arguments contain an unknown field"));
    }
    for field in op.fields {
        match object.get(field.name) {
            None | Some(Value::Null) if !field.required => {}
            Some(value) if field.kind.accepts(value) => {}
            _ => {
                return Err(ChatError::invalid(format!(
                    "Invalid or missing {}",
                    field.name
                )));
            }
        }
    }
    if !(op.deserialize)(arguments) {
        return Err(ChatError::invalid(
            "Arguments do not match the operation schema",
        ));
    }
    Ok(op)
}

pub fn chat_tool_definitions() -> Vec<Value> {
    OPERATIONS.iter().map(|op| {
        let properties: Map<_,_> = op.fields.iter().map(|f| {
            let schema = f.kind.schema();
            (f.name.to_owned(), if f.required { schema } else { json!({"anyOf":[schema,{"type":"null"}]}) })
        }).collect();
        let required: Vec<_> = op.fields.iter().filter(|f| f.required).map(|f| f.name).collect();
        let mut input = json!({"type":"object","properties":properties,"required":required,"additionalProperties":false});
        if op.name == "signal" {
            input["if"] = json!({"properties":{"signal":{"const":"blocked"}},"required":["signal"]});
            input["then"] = json!({"required":["reason"],"properties":{"reason":Reason.schema()}});
            input["else"] = json!({"properties":{"reason":{"type":"null"}}});
        }
        json!({
            "name":format!("chat_{}",op.name),"description":op.description,
            "inputSchema":input,
            "outputSchema":output_schema(op),
            "annotations":{"readOnlyHint":op.read_only,"destructiveHint":matches!(op.name,"delete"|"leave"|"unbind"|"rejoin"),"idempotentHint":op.fields.iter().any(|f|f.name=="request_id") || op.name=="ack","openWorldHint":false}
        })
    }).collect()
}

fn object(properties: Value, required: &[&str]) -> Value {
    json!({"type":"object","properties":properties,"required":required})
}

fn nullable(schema: Value) -> Value {
    json!({"anyOf":[schema,{"type":"null"}]})
}

fn array(items: Value) -> Value {
    json!({"type":"array","items":items})
}

fn reference(name: &str) -> Value {
    json!({"$ref":format!("#/$defs/{name}")})
}

fn output_schema(op: &Operation) -> Value {
    let text = Plain.schema();
    let id = Id.schema();
    let event_id = Event.schema();
    let uuid = Uuid.schema();
    let count = json!({"type":"integer","minimum":0});
    let mut room = object(
        json!({"room_id":id,"name":text,"opened_at":text,"closed_at":nullable(text.clone()),
            "opened_by":nullable(text.clone()),"kept":Bool.schema(),"members":count,
            "events":count,"last_activity":nullable(text.clone()),"revision":event_id}),
        &[
            "room_id",
            "name",
            "opened_at",
            "closed_at",
            "opened_by",
            "kept",
            "members",
            "events",
            "last_activity",
            "revision",
        ],
    );
    let binding = object(
        json!({"harness":text,"id":text,"cwd":nullable(text.clone()),"source_ref":nullable(id.clone())}),
        &["harness", "id", "cwd"],
    );
    let mut export_member = object(
        json!({"room_id":id,"member_id":id,"name":text,"harness":text,"role":Choice(ROLES).schema(),
            "joined_at":text,"left_at":nullable(text.clone()),"last_command_at":nullable(text.clone()),
            "session":nullable(reference("binding")),"binding_generation":count}),
        &[
            "room_id",
            "member_id",
            "name",
            "harness",
            "role",
            "joined_at",
            "left_at",
            "last_command_at",
            "session",
            "binding_generation",
        ],
    );
    export_member["additionalProperties"] = json!(false);
    let mut member = export_member.clone();
    member["properties"]["read_cursor"] = event_id.clone();
    member["properties"]["last_signal"] = nullable(object(
        json!({"instruction_event_id":event_id,"event_id":event_id,"state":Choice(&["done","blocked"]).schema(),
            "reason":nullable(text.clone())}),
        &["instruction_event_id", "event_id", "state", "reason"],
    ));
    member["required"]
        .as_array_mut()
        .unwrap()
        .extend([json!("read_cursor"), json!("last_signal")]);
    let candidate = object(
        json!({"harness":text,"session_id":text,"cwd":nullable(text.clone()),"updated":nullable(text.clone()),
            "archived":Bool.schema(),"readable":Bool.schema(),"source_ref":id}),
        &[
            "harness",
            "session_id",
            "cwd",
            "updated",
            "archived",
            "readable",
            "source_ref",
        ],
    );
    let observation = object(
        json!({"id":event_id,"room_id":id,"observer":text,"target":text,"at":text,"entries_shown":count,"entries_left_out":count}),
        &[
            "id",
            "room_id",
            "observer",
            "target",
            "at",
            "entries_shown",
            "entries_left_out",
        ],
    );
    let export_variants: Vec<Value> = [
        ("room", "room"),
        ("member", "export_member"),
        ("observation", "observation"),
        ("event", "event"),
    ]
    .into_iter()
    .map(|(name, definition)| {
        object(
            json!({"type":{"const":name},(name):reference(definition)}),
            &["type", name],
        )
    })
    .collect();
    let statuses: &[&str] = if op.name == "wait" {
        &["ok", "waiting", "timeout", "room_closed", "server_stopping"]
    } else {
        &["ok"]
    };

    let mut output = match op.name {
        "open" | "close" | "reopen" => room.clone(),
        "delete" => object(
            json!({"room_id":id,"revision":event_id,"deleted":Confirm.schema()}),
            &["room_id", "revision", "deleted"],
        ),
        "join" | "rejoin" => object(
            json!({"room_id":id,"member_id":id,"member_handle":uuid,"name":text,"harness":text,
                "role":Choice(ROLES).schema(),"session":nullable(reference("binding")),
                "instructions":text,"candidates":array(reference("candidate")),"candidates_remaining":count,
                "notices":array(text.clone()),"endpoint":{"type":"string","format":"uri"},"data_dir":text}),
            &[
                "room_id",
                "member_id",
                "member_handle",
                "name",
                "harness",
                "role",
                "session",
                "instructions",
            ],
        ),
        "bind" | "unbind" | "leave" => object(
            json!({"room_id":id,"member_id":id,"session":nullable(reference("binding")),"binding_generation":count}),
            &["room_id", "member_id", "session", "binding_generation"],
        ),
        "send" | "signal" => object(
            json!({"room_id":id,"event_id":event_id,"event":reference("event")}),
            &["room_id", "event_id", "event"],
        ),
        "read" | "peek" | "wait" => object(
            json!({"room_id":id,"member_id":id,"stream_id":text,"batch_id":nullable(uuid.clone()),
                "ack_batch_id":uuid,"high_water":event_id,"trigger_event_id":nullable(event_id.clone()),
                "remaining":count,"items":array(reference("event")),"notices":array(text.clone()),"skipped":count}),
            &[
                "room_id",
                "member_id",
                "stream_id",
                "batch_id",
                "high_water",
                "trigger_event_id",
                "remaining",
                "items",
                "notices",
                "skipped",
            ],
        ),
        "ack" => object(
            json!({"room_id":id,"member_id":id,"target_member_id":id,"binding_generation":count,"stream_id":text,"ack_batch_id":uuid,"acknowledged_through":event_id}),
            &[
                "room_id",
                "member_id",
                "stream_id",
                "ack_batch_id",
                "acknowledged_through",
            ],
        ),
        "observe" => object(
            json!({"room_id":id,"member_id":id,"target_member_id":id,"stream_id":text,"batch_id":nullable(uuid.clone()),
                "binding_generation":count,"projection_version":{"const":super::view::PROJECTION_VERSION},
                "ack_batch_id":uuid,"items":array(object(json!({
                    "kind":{"enum":["new","updated"]},"target_member_id":id,"binding_generation":count,
                    "projection_version":{"const":super::view::PROJECTION_VERSION},
                    "item":object(json!({"id":text,"kind":{"enum":["prompt","injected","shell_command","reply","tool_use","tool_outcome","notice"]},"text":text,"full_len":count,"fingerprint":id}), &["id","kind","text","full_len","fingerprint"])
                }), &["kind","item","target_member_id","binding_generation","projection_version"])),"notices":array(text.clone()),
                "remaining":count,"skipped":count,"selected_range":object(json!({"first_item_id":nullable(text.clone()),"last_item_id":nullable(text.clone())}), &["first_item_id","last_item_id"])}),
            &[
                "room_id",
                "member_id",
                "target_member_id",
                "binding_generation",
                "projection_version",
                "stream_id",
                "batch_id",
                "items",
                "notices",
                "remaining",
                "skipped",
                "selected_range",
            ],
        ),
        "rooms" | "members" | "candidates" | "ledger" | "export" => {
            let items = match op.name {
                "rooms" => reference("room"),
                "members" => reference("member"),
                "candidates" => reference("candidate"),
                "ledger" => reference("event"),
                _ => json!({"oneOf":export_variants}),
            };
            let fields = op
                .fields
                .iter()
                .filter(|f| !matches!(f.name, "cursor" | "limit" | "member_handle"));
            let filter_properties: Map<_, _> = fields
                .clone()
                .map(|f| (f.name.to_owned(), f.kind.schema()))
                .collect();
            let filter_required: Vec<_> = fields.filter(|f| f.required).map(|f| f.name).collect();
            let mut filters = object(json!(filter_properties), &filter_required);
            filters["additionalProperties"] = json!(false);
            let mut required = vec!["items", "snapshot_id", "filters", "next_cursor"];
            if matches!(op.name, "members" | "ledger" | "export") {
                required.extend(["room_id", "high_water"]);
            }
            object(
                json!({"room_id":id,"high_water":event_id,"snapshot_id":uuid,"next_cursor":nullable(uuid.clone()),
                "filters":filters,"items":array(items)}),
                &required,
            )
        }
        _ => unreachable!("registered tool"),
    };
    output["properties"]["status"] = Choice(statuses).schema();
    output["properties"]["service_id"] = id;
    output["required"]
        .as_array_mut()
        .unwrap()
        .extend([json!("status"), json!("service_id")]);
    room["additionalProperties"] = json!(false);
    output["$defs"] = json!({"room":room,"binding":binding,"candidate":candidate,
        "observation":observation,"event":event_schema()});
    if op.name == "members" {
        output["$defs"]["member"] = member;
    } else if op.name == "export" {
        output["$defs"]["export_member"] = export_member;
    }
    output
}

fn event_schema() -> Value {
    let text = Plain.schema();
    let actor = json!({"oneOf":[
        object(json!({"type":{"const":"local_admin"}}),&["type"]),
        object(json!({"type":{"const":"member"},"member_id":nullable(Id.schema()),"name":text,"role":nullable(Choice(ROLES).schema())}),&["type","member_id","name","role"])
    ]});
    let event_id = Event.schema();
    const KINDS: &[&str] = &[
        "open", "close", "reopen", "join", "rejoin", "bind", "unbind", "leave", "message",
        "signal", "observe",
    ];
    let mut schema = object(
        json!({"room_id":Id.schema(),"event_id":event_id,"timestamp":text,"actor":actor,
            "kind":Choice(KINDS).schema(),"payload":{"type":"object"},"recipient_ids":array(Id.schema())}),
        &[
            "room_id",
            "event_id",
            "timestamp",
            "actor",
            "kind",
            "payload",
            "recipient_ids",
        ],
    );
    let variants = KINDS.iter().copied().map(|kind| {
        let payload = match kind {
            "open" => object(json!({"name":text}),&["name"]),
            "join" | "rejoin" => object(json!({"harness":text,"role":Choice(ROLES).schema(),"session":nullable(reference("binding"))}),&["harness","role"]),
            "bind" | "unbind" | "leave" => object(
                json!({"session":nullable(reference("binding")),"binding_generation":{"type":"integer","minimum":0}}),
                &[]
            ),
            "message" => object(json!({"text":text,"recipients":Names.schema(),"mentions":Names.schema(),"historical_mentions":Names.schema()}),
                &["text","recipients","mentions","historical_mentions"]),
            "signal" => object(json!({"instruction_event_id":event_id,"signal":Choice(&["done","blocked"]).schema(),"reason":nullable(text.clone())}),
                &["instruction_event_id","signal","reason"]),
            _ => object(json!({}),&[]),
        };
        json!({"properties":{"kind":{"const":kind},"payload":payload}})
    }).collect::<Vec<_>>();
    schema["oneOf"] = json!(variants);
    schema
}
