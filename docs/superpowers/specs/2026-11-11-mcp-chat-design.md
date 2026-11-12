# Mukade chat: a shared local MCP server

Date: 2026-11-11. Status: design; the server is not implemented.
This is the authoritative specification for chat, dialogue, supervision and observation.
Decisions labelled **Builder default** are implementation choices, not additional user requests.
Contract clarifications added during review are marked the same way when they settle a product
choice. Protocol references below are pinned to dated revisions.

## Intent and agreed interface

The user runs several local coding instances, including multiple instances of one harness.
They need to discuss a problem, do pair programming, direct workers, and explain another
instance's progress without passing its raw session content around.

Mukade supplies one persistent local MCP service, shared by those instances. The user starts
and controls it with:

```text
mukade chat start
mukade chat status
mukade chat stop
```

`start` runs the service in the background. All participating clients connect to its displayed
MCP endpoint. Rooms, messages and progress survive `stop`, a crash and a later `start`.
Starting the service does not start agents, join them to rooms, bind their sessions, install
plugins, or change harness configuration.

The existing product description stays "harness sessions manager". Session conversion,
installation discovery and the TUI keep their existing contracts.

## Architecture and alternatives

The chosen design is an independent service using **Streamable HTTP on loopback**, with the
domain logic in the Rust library and a thin MCP interface. The ordinary chat CLI is a client
of that service. The service owns persistent chat storage; clients never open its database.

Alternatives considered were commands coordinating directly through SQLite, and a separate
stdio server launched by each harness. The first spreads coordination and storage permissions
across clients; the second creates multiple server processes. The user chose one background
service. A stdio bridge can be added only if compatibility testing identifies a client that
needs it; it must connect to the same service and must not own another database.

The same mukade executable contains lifecycle commands and the service entry point. A maintained
Rust MCP SDK and its necessary HTTP/runtime dependencies are permitted. Choose and pin them
in the implementation plan after checking the installed harnesses' protocol support; do not
implement HTTP or MCP framing by hand to avoid a dependency. No separate application server,
Python runtime or external broker is required at deployment.

## Service lifecycle

One service may own a canonical data directory at a time, enforced by an OS-held exclusive
lock. It remains held for the process lifetime and is released by the OS after a crash.

| Command | Required behaviour |
|---|---|
| `chat start` | Start a hidden background process, wait for readiness, then report instance ID, endpoint and data directory. Return success only after the child is ready. |
| `chat start` while running | Report the existing healthy instance; create no second process or listener. |
| `chat status` | Inspect the recorded instance and check its authenticated health endpoint. Report running, stopped or unreachable distinctly, without creating files or a database. |
| `chat stop` | Request authenticated graceful shutdown of the identified instance; wait for exit. If already stopped, succeed without side effects. Never delete history. |
| `chat serve` | Run the same service in the foreground for diagnostics; used internally by `start` and subject to the same exclusive lock. |

**Builder defaults:** startup and graceful-stop deadlines are 10 seconds. On first start an
automatically selected loopback port is persisted; subsequent starts reuse it so installed
MCP URLs remain valid. `--port` explicitly changes it and requires clients to be reconfigured.
A collision is an error, never a silent port change. Each lifecycle command supports `--json`.
An already-running instance accepts `start` only with compatible configuration. A conflicting
`--port` returns a configuration conflict and leaves the running instance intact.

Readiness metadata includes the canonical directory, endpoint, PID, random instance ID and
server version. It is published atomically only after configuration, migrations and listener
initialization succeed. A PID alone is never proof of identity. Stale metadata can be replaced
only after obtaining the instance lock; an unhealthy but locked instance is reported, not
duplicated or killed. `stop` never kills an unrelated process or falls back to deleting a lock.
Startup failure or timeout must terminate/reap the child created by that invocation and remove
only its own incomplete metadata. Parent/child readiness handshaking prevents an orphan child
from becoming a service after `start` reported failure.

Persist a `service_id` for the lifetime of the data directory, alongside the per-process
`instance_id`. Clients pin the service identity during connection setup and validate it on
reconnect. A restart changes the instance ID and preserves the service ID. Replacing the
database with a different service requires explicit reconnection. Shutdown requests include
the expected instance ID so a delayed stop cannot stop its replacement. Missing metadata with
a held lock means starting/unreachable, never stopped.

Shutdown rejects new mutations, resolves outstanding waits with `server_stopping`, drains
accepted work to the deadline, and closes the listener. Unacknowledged deliveries remain
pending. A forced process exit cannot turn an uncommitted operation into a successful one.

## Data and deployment

Keep the existing portable data-root policy: absolute `MUKADE_DATA_DIR`, otherwise
`mukade-data` beside the executable. No fallback to another folder when access fails.
The service refuses a data directory inside a project or native harness store, including
resolved links and relative components. The root must be on a local, unsynchronized disk.
Check existing ancestors before creating directories. Detect projects through repository
markers and configured workspace roots, and reject known network/synchronized locations.
Document platform detection limits. Installing the executable under a repository therefore
requires an explicit external `MUKADE_DATA_DIR` for chat.

The library's `bus/bus.db` remains the internal SQLite ledger path. Naming this internal
storage a bus does not create another public CLI or another service. Service metadata,
configuration, instance lock and protected credential files also live beneath `bus/`.
They are operational files, separate from room events and exports.

Different explicitly selected data directories identify different isolated services. Copies
of the executable must use the same `MUKADE_DATA_DIR` to share one service. Clients discover
the endpoint from that directory's metadata or an explicitly configured URL. They must not
silently connect to another instance, start a service, or open SQLite when it is unavailable.

Only the server process needs database write access and read access to the configured native
stores. Clients need access to the endpoint and its credential. Connecting from a sandbox
may require permission to reach loopback; this must be tested, not assumed away.

SQLite uses WAL, foreign keys, ordered schema migrations and short write transactions.
Neither a network wait nor a transcript read holds a write transaction. An unsupported newer
schema is refused. Failed migrations leave the prior schema intact. No persisted user data
is discarded to make a schema change succeed.
Select durability settings that preserve acknowledged commits across process and OS crashes
on supported local filesystems, and verify recovery with fault injection. Every migration
from the existing schema preserves room/member IDs, events and acknowledged observation state.

## Local endpoint boundary

Listen only on loopback; no LAN or public binding is part of this feature. Validate `Origin`
and request authority/host according to the chosen MCP transport revision. Browser-originated
requests are denied unless explicitly allowed. Require a randomly generated local bearer
credential, stored with owner-only permissions and never printed in status, URLs, logs, room
events or exports. A failed secure credential-file setup prevents startup.
Protect the database, WAL, configuration and backups with the same account boundary. Apply
POSIX modes or Windows ACLs as appropriate. Local clients bypass HTTP proxies and refuse
redirects so the bearer credential stays at the configured loopback endpoint. Bound request
bodies, concurrent waits and response writes, releasing resources on expiry.

An explicit connection-configuration command may write the credential into the selected
harness's protected configuration. It must not display it. The credential survives ordinary
restart so clients can reconnect. No credential or configuration is installed by `start`.

This boundary protects the local HTTP service from unintended callers; it is not isolation
between malicious agents sharing an OS account. Roles are declared by cooperating members.
Neither MCP nor mukade prevents those agents from reading files they can already access.

## Rooms, members and native identity

A room has an immutable internal ID, explicit name, open/closed state and ordered durable event
log. Reusing a deleted room's name creates a different ID and cannot attach its old handles or
receipts. No inactivity timeout closes a room or removes its history. A closed room remains
readable and exportable.
Reopening is explicit. Only an explicitly selected closed room can be deleted, after a summary
of its contents is shown to the user. Deletion cascades in one transaction.
**Builder default:** opening an existing open name returns that room, while opening a closed
name returns `room_closed` with its ID and the explicit reopen operation. First join starts
with the room's complete history unread. A member's role is fixed at join and rejoin preserves
it. Changing roles requires leaving and joining as a new member.

A member is a persistent room participant with a harness or custom label, role, unique name,
join/leave dates, last command time, read cursor and optional native session binding.
Names default to `<harness>-<n>`, with `claude-<n>` for Claude Code. Numbers and chosen names
are never reused within that room, including after a leave. Chosen names use
`[a-z0-9][a-z0-9_-]{0,31}` and cannot be `all`. Room names use the same characters, up to 64.

Join returns an opaque member handle and the assigned name. Member operations carry that
handle explicitly, rather than identifying a participant by TCP connection, MCP metadata,
model name or working directory. These are addressing handles, not a claim of strong
authorization among same-account agents. Administrative CLI actions may explicitly select a
member on the user's behalf; an agent binds only its own member handle.

Native identity is a separate value: `(harness, full native session ID)`. Join or bind may
receive it from a verified harness hook/environment field or an explicit user selection.
Never infer it from the newest file, one remaining candidate, a PID, an MCP connection, or a
name prefix. With no ID, join remains unbound and lists candidates. Binding must verify the
exact ID in the configured native root. A session already bound to another active member
in the room is refused. A new session ID requires an explicit rebind; no automatic following.
If multiple configured stores contain that full ID, require selection of a server-issued
source reference. A source reference identifies a configured store and cannot accept a
client-chosen filesystem path. Every bind/unbind increments a durable binding generation.

The server owns native-root configuration. Agent-supplied transcript paths or arbitrary roots
must not bypass the reduced-view/binding rules. Installing an adapter does not consent to
binding every session; automatic binding requires the selected room/role and that instance's
own stated ID. An unwritten session may join unbound and bind later.

**Builder default:** rejoining explicitly selects the earlier member, preserves its internal
ID, role, cursor, room batch and history, rotates its handle, and clears the old binding before
applying a new explicit binding atomically. It fences old in-flight requests at commit and
cancels old waits. A late request with the previous handle returns `stale_member`. This also
recovers an active member whose process crashed. Ordinary reconnect reuses the current handle.

`chat_unbind` revokes the member's binding, invalidates observation batches for that binding,
and cancels observation work. Leaving also revokes the handle and binding. Both are permitted
in a closed room and recorded as revocation events. Already delivered text cannot be recalled.
Closing a room preserves existing bindings, which members can explicitly revoke. New joins,
rejoins and binds require an open room. A crash alone changes neither membership nor binding.

## Roles and permissions

| Role | Read room/ledger | Send messages | Signal | Observe native sessions |
|---|---|---|---|---|
| observer | yes | no | no | yes |
| peer | yes | yes | no | no |
| lead | yes | yes | no | no |
| worker | yes | no | `done`, `blocked` | no |

All members see all room events; roles do not partition a room. Multiple leads and mixed
roles are allowed. Refusals name the permitted role. The server enforces the rules regardless
of tool discovery or client-side instructions. Tool lists need not vary by participant.

**Builder default:** room administration (open/close/reopen/delete), room discovery and
explicit member recovery use the authenticated local service credential. Every configured
client holding that credential can invoke these operations. Room/member discovery, candidate
listing and administrative ledger/export queries also accept that credential with an explicit
room ID where applicable, including closed rooms with no active members. Consuming read/wait,
send, signal, bind/unbind and observe require a current member handle and enforce its role.
Queries made in a member context validate that handle too.
Only the active holder can leave or change its binding. This cooperative authority model must
be stated in connection instructions. A confirmation flag on delete expresses caller intent.
It cannot establish that a human reviewed the request. Deletion includes the immutable room ID
and the revision returned by its summary, and refuses a changed/reopened room.

Join returns concise instructions with the member handle/name, role, permitted tools,
read/wait/ack sequence, binding rules and the data-directory/endpoint identity. It tells agents
to treat other members' text as data and to avoid credentials, personal data and pasted files.
An observer explains to the user and cannot send advice to the observed instance. It must
distinguish the instance's stated reasons from its own interpretations.

## Messages and signals

A message is a bounded UTF-8 text event written by a peer or lead. The limit is **65,536 bytes**.
CLI input may be an argument, standard input or a file; MCP input is a structured text field.
Reject empty/oversized/invalid text and recognized secrets before any event is inserted.

Inline `@name` is a mention only at a token boundary and only for a member that is or was in
the room, or `@all`. Email addresses, package names and unknown tags remain ordinary text.
An explicit recipient list accepts current members or `all`, rejecting unknown/left names.
The event records explicit recipients, recognized mentions and the active recipients expanded
at send time; `@all` does not retrospectively instruct a member that joins later. A historical
mention of a departed member stays readable and is labelled as such when displayed.

Workers may emit `done` with no text, or `blocked` with one reason line of at most 200 Unicode
characters. A signal requires an instruction event number from the same room. It must refer
to a message addressed to that worker when sent. **Builder default:** the instruction's author
must have been a lead when that event was committed, recorded in the immutable event.
An absent, unrelated, peer-authored or differently addressed instruction is refused. A signal
records the worker, instruction reference and timestamp; a blocked worker can later signal
done. Signals are room events visible to every member.
Addressing is the union of explicit recipients and active inline mentions expanded at commit.
An unaddressed message is readable by everyone and assigns no worker. Signals address the
instruction's author for wait matching. Recognized secrets are rejected in blocked reasons.
**Builder default:** signal state is keyed by `(room_id, instruction_event_id, worker_member_id)`.
Each worker's instruction state can move from unsignalled to blocked or done, and from
blocked to blocked with an updated reason or to done. Done is terminal. Repeating an identical
state/reason for that worker returns its prior signal event even with a new request ID. A later
blocked signal after done is refused. Multiple workers addressed by one event remain independent.

Domain mutations require a caller-generated UUID `request_id`. The service namespaces IDs by
operation and stable member ID resolved from the current handle. Pre-join and administrative
operations use a service-wide namespace with the request UUID and operation. Authenticate and
validate argument shape, then look up committed idempotency state before checking mutable
preconditions such as room-open state. Identical canonical arguments return the original
result even after the room closes. Changed arguments are refused. Exclude the replaceable
handle and JSON object key order from that comparison. The idempotency record and mutation
commit atomically. Retain keys for the resource lifetime. This handles a response lost after
commit without duplicating a message, signal or membership change.
The protocol JSON-RPC request ID is not a replacement for this application-level ID.
Keep minimal deletion/idempotency tombstones outside room cascades, without message content.
After a target room is deleted, an old create/join/send retry returns `resource_deleted` and
cannot recreate it or target a new room with the same name; repeating its delete succeeds.
Successful retries never reactivate a superseded membership handle. Such join/rejoin results
report `stale_member` and require explicit recovery. Reads/waits/observations use batch IDs
for replay, and acknowledgements use their batch IDs as idempotency keys.
Keep a one-way mapping of a revoked handle for exact replay of its successfully committed
`leave` with the same request ID and canonical arguments. This exception only returns the
recorded result and grants no authority for any new operation. Deletion tombstones take
precedence if the room has since been removed.

## Reading, waiting and delivery receipts

Every event has a database-assigned monotonically increasing number and immutable committed
content. IDs are never reused and gaps are allowed. Serialize 64-bit event numbers as decimal
strings across JSON interfaces to preserve exact values in JavaScript clients. Every member
has one committed room cursor. Peeking and ledger queries never advance it. Reads page from
oldest unread event forward. The default budget is 8,000 Unicode scalar values measured in
the canonical serialized structured page, including metadata. At least one complete event is
returned even if it exceeds that budget. `remaining` is the count after that page through its
captured high-water event number. Room messages are not truncated to fit a response.

`wait` returns an unread message or signal written by another member, optionally only one
addressed to the waiting member, using the recipient rules above. It returns all preceding
unread room events in order too.
Own events, joins, leaves and observation activity do not satisfy the trigger. Closing the
room or stopping the server terminates a wait with a distinct result. **Builder defaults:**
timeout 30 seconds, maximum 300; a timeout is a normal result with no cursor movement.
An existing backlog that matches returns immediately. With a budget too small to reach the
trigger, return its event number and the first page; later reads drain the preceding events.
Resolve any pending room batch first, even if a new budget or wait filter differs. Otherwise
check matching backlog, then room closure, then register the wait. A closed room without a
matching backlog returns `room_closed` and its unread count, which `read` can drain.

The service registers a waiter and rechecks committed state before sleeping, so a send between
the initial read and registration cannot be missed. Writers notify in-memory waiters only
after commit. No waiter port registry, per-command listening socket or auxiliary daemon is
needed. A bounded periodic recheck protects against an internal notification being missed.

**An HTTP write is not proof of client receipt.** Consuming reads, waits and observations
return a `batch_id`. For a room batch the service persists its event range and expected cursor
before returning it; the committed cursor does not move yet. `ack(batch_id)` advances the
cursor atomically and is idempotent. A later consuming request may include `ack_batch_id` to
acknowledge the previous batch and fetch the next in one operation. An acknowledgement can
refer only to the pending batch issued for that member and stream, never an arbitrary offset.
Keep an acknowledged-batch record so retrying an older ack succeeds without consuming a newer
batch. Unknown, foreign or invalidated IDs are refused. Commit an included ack before fetching
or waiting for the next batch. If that next step fails or is cancelled, the ack stays committed
and retry is safe. Empty responses have `batch_id: null` and require no receipt.

Without acknowledgement, a retry returns the same pending batch. This survives disconnects
and server restart. Acknowledging receipt does not attest that the model reasoned about or
acted on the events. CLI clients acknowledge only after successfully writing/flushing their
output. Adapters acknowledge after the host accepts the batch into its delivery mechanism;
if that cannot be verified, they keep it pending. Plain MCP tools expose the acknowledgement
explicitly and explain it in their instructions. No automatic acknowledgement on socket close.

Serialize consumption per stream. The room stream belongs to one member, and each observation
stream belongs to an observer, target and binding generation. One active read/wait/observe per
stream is allowed, while independent streams can progress concurrently. A pending batch has
no expiry and is durable cursor state. It holds no network request or database lock.
Serialize acknowledgements and revocation with staging. Release active guards on completion,
supported cancellation, deadline, panic or process exit. Handle disconnection according to the
protocol revision described below. A lost response can cause repeats identified by event and
batch IDs. Acknowledgements record receipt only.

## Reduced observation

Only an observer can read another active member's explicitly bound native session. The server
uses mukade's readers with read-only access. It never exposes raw dump/export through MCP.

| Native material | Permitted observation |
|---|---|
| Human prompt with known provenance | Masked text, capped at 1,200 characters |
| Assistant written reply | Masked text, capped at 1,200 characters |
| Harness-injected or unknown user entry | Kind only |
| A shell action recorded as a user message | Safe command metadata; never output |
| Known tool call | Allow-listed path, pattern, URL or safe command metadata, capped at 160 characters |
| Edit/write call | Path and change size when available; no body |
| Tool outcome | Success/error and content size only |
| Unknown/MCP tool | Name only |
| Compaction or invalidated cursor | Explicit notice |
| Reasoning, attachments, images, raw metadata | Absent |

This is a dedicated representation, not a serialization of the interchange model with a few
fields removed. A record contributes only positively identified allowed fields. Readers must
distinguish human and harness provenance, including mixed records; unknowns fail closed.
Command wrappers, aliases, inline scripts, patches and here-documents must not expose bodies.
Uncertain command syntax contributes only the executable or tool name.
The projection excludes tool bodies and outputs structurally. Allowed human/assistant text
can still quote code or file contents, and follows the masking and size rules above. This
boundary cannot guarantee semantic removal of every quoted payload from natural language.

Mask the full permitted text before truncation, cut on character boundaries, and report the
full permitted field length. Escape terminal control and direction-changing characters.
The same protection applies to every CLI textual field, including actor names and errors.
There is no flag to show additional native content.

Each item has a stable native identity plus a sub-item suffix and a projection fingerprint.
Fingerprint the complete permitted, masked field before truncation, together with projection
version and item metadata. A change beyond the displayed prefix must still produce an update.
Paginate oldest first with a default 8,000-character budget, report remaining items, and show
growing/changed projected items as updates. Track all consumed item fingerprints, because an
earlier entry can grow after later entries were observed. Rewinds, removed items and changed
ordering invalidate the relevant cursor with an explicit notice. Partial final records are
tolerated; other read failures get three attempts 200 ms apart, then a named error.

An explicit `latest` request may skip items and must report their count and selected range.
`last_turns` selects the last N positively identified human prompts and their following entries
on the active branch, and applies only to the first observation of a binding generation. If
turn boundaries are unknown, refuse that filter with a named error. Filtering and skipping
are committed only on acknowledgement. Reject incompatible filters, and replay any pending
batch before applying new options. Ordinary budget limits never silently skip. A newer session
in the same directory is only a hint, and never causes automatic rebinding.

Observation uses the receipt mechanism with a staged fingerprint delta per stream. A pending
reduced payload may be cached in memory, never in SQLite or an export. Persistent state holds
only source identities, hashes, lengths, counts, projection versions and receipt status.
While cached, replay the same payload even if the source grows. An ack commits exactly the
staged delta, and later changes appear in the next observation. Ack does not need to reread
the source, including after restart, provided the batch has not been invalidated or revoked.

After cache loss, a redelivery rebuilds the selected pending items using the staged projection
version and range. Unrelated appended entries do not invalidate it. Reuse the batch ID only
if those items and their relevant order match. Otherwise invalidate it with an explicit
notice and rebuild from the last acknowledged state. A late ack of an invalidated batch
cannot consume new content. Source unavailability returns an error and preserves pending and
committed state. Binding changes and revocation always invalidate pending observations.
Observation cannot recover historical content that the native harness itself deleted.

Observation receipts/counts stay outside the room event log and never notify the observed
instance. Revalidate role, membership and binding when staging and acknowledging a batch.

## Sensitive data and persistence

One library detector is used for messages, signal reasons and observation fields. It checks
private keys, known provider/service token patterns, bearer tokens, JWTs, secret assignments,
credential-bearing URLs/connection strings, environment-value lines and high-entropy strings.
Messages/reasons that match are refused with the category and line number, without repeating
the value. Observations replace matches with category markers before cutting.

The detector reduces risk and has explicit limits. Ordinary-word passwords, personal details
and unfamiliar credential formats can pass. Never claim that all sensitive data is blocked.
Application logs contain operation IDs, durations and safe error codes, not message bodies,
transcript excerpts, credentials or arbitrary incoming requests.

The durable ledger stores membership/binding changes, messages, signals and room changes.
Membership projections must be reconstructible from those events. Cursors, pending delivery
ranges, idempotency keys and observation counts are separate state. The ledger API lists
rooms and participants, including departed members, read position and last worker signal;
filters history by actor, kind, time, mention and text; and exports JSON Lines snapshots.
Exports omit credentials, pending-delivery internals, cursors and observed text.

There is no retention-by-age. A configurable size notice defaults to 200 MiB and names the
largest rooms. Content-adding operations check a free-space floor of 1 GiB and their bounded
payload allowance. Receipts, close, leave/unbind, delete and storage reclamation remain
permitted below that policy floor so recovery remains possible. Actual I/O failure still
rolls back each transaction. Disk-full can prevent any write, and the error then names the
need to free filesystem space. Where capacity cannot be queried, report that limitation.
Room removal uses incremental vacuum and WAL checkpointing; reclamation pinned by readers
is reported as pending, not promised as an immediate size reduction.

## MCP and CLI surfaces

MCP provides tools, not a guarantee that a model will invoke them. Expose one set of schemas,
with the same domain validation for MCP clients, CLI clients and any in-process host:

| Tool group | Required operations |
|---|---|
| Rooms | `chat_open`, `chat_close`, `chat_reopen`, `chat_rooms`, `chat_delete` |
| Membership | `chat_join`, `chat_rejoin`, `chat_leave`, `chat_members`, `chat_candidates`, `chat_bind`, `chat_unbind` |
| Exchange | `chat_send`, `chat_signal`, `chat_read`, `chat_peek`, `chat_wait`, `chat_ack` |
| Observation | `chat_observe` |
| Ledger | `chat_ledger`, `chat_export` |

Tools declare input/output schemas and return `structuredContent` with a textual rendering
of the same result for clients that use only text. IDs, message content and receipts must be
available in either representation. Annotations describe actual effects. Consuming reads,
waits and observations update delivery state and must not be advertised as read-only.

The implementation plan must define the complete input/output schemas before coding. Use
these shared fields and validation rules across CLI and MCP.

| Contract | Required content |
|---|---|
| Member operation | Current `member_handle`, immutable `room_id`, operation arguments, and `request_id` for domain mutations |
| Event | `event_id`, `room_id`, UTC timestamp, kind, member actor ID/name/role or explicit local-administration actor, typed payload and recipient IDs |
| Delivery | `service_id`, `room_id`, member ID, `stream_id`, nullable `batch_id`, typed items, high-water mark where applicable, `remaining`, notices and skipped count |
| Observation item | Target/binding generation, native item ID, projection version, fingerprint, new/updated kind and permitted fields |
| Error | Stable code, safe message, retryability, operation/request ID when known, and any committed ack ID |
| Query page | Captured snapshot/high-water mark, opaque next-page token, applied filters and bounded items |

Validate unknown fields and limits consistently. Lists, candidates, ledger and export all
paginate. Tokens bind to service, room, filters and snapshot, and expire explicitly where
applicable. Deleting a room revokes its query/export handles and active streams. Avoid holding
database read transactions while a client downloads a snapshot. The plan pins request limits,
page limits, wait concurrency and cache limits, with overload reported as a retryable error.
Errors distinguish invalid arguments, role refusal, stale member, deleted resource, pending
batch invalidation and storage failure. A normal wait timeout is a successful typed result.
Business failures use MCP tool errors with `isError`. Malformed protocol messages and unknown
methods use the selected revision's protocol errors. Long waits leave unrelated requests free.

### Protocol compatibility

Pin SDK and supported protocol revisions in the plan, then test each client/version against
that combination. Current reference revisions differ in connection and cancellation behavior.

| Revision | Required transport behavior for that revision |
|---|---|
| `2025-11-25` | Initialization handshake, optional transport session, explicit cancellation notification and optional resumable SSE. A disconnect alone does not cancel the request. |
| `2026-07-28` | Per-request version metadata, `server/discover`, request-scoped HTTP streams and cancellation when the SSE response stream closes. |

Use only the behaviors supported by the pinned SDK and advertised revision. Supporting both
requires independent conformance tests. Transport session IDs, where present, never identify
a chat member. Application batch receipts survive transport reconnection in either revision.
For a legacy disconnected wait, retain its active guard until completion, explicit cancellation
or its bounded deadline. For a cancelled request, release the guard and retain any staged batch.
Cancellation cannot undo a committed send or ack. Retry its application ID to resolve uncertainty.
The configured client tool timeout must exceed the requested wait timeout plus response margin.

The human/script CLI uses `mukade chat <operation>` for those operations and only talks to
the running service. Lifecycle commands are local process-control operations. `--json` emits
one object per line without extra prose. Retain exit codes 0 success/normal timeout, 1 invalid
CLI usage, 2 domain refusal, 4 operational failure. `status` reports a stopped instance as a
normal result. An unavailable service for a data operation is exit 4 and names `chat start`.
Include batch IDs in human-readable and JSON output. If output succeeds but ack fails, report
the batch ID and receipt failure on stderr with exit 4 so a caller can retry the ack directly.

CLI exports stream through the client, which creates the explicitly requested destination
without overwriting an existing file. An MCP export returns a paginated snapshot or resource
handle; it does not let an agent ask the service to write an arbitrary host path. Export
snapshot handles have an explicit bounded lifetime, and expiration is reported without
pretending that a later page belongs to the same snapshot. Administrative deletion requires
an explicit confirmation field/flag after inspecting the room summary.
Pin export to a committed event high-water mark and derive membership from that same point.
A failed export leaves an unmistakably incomplete temporary destination. Publish the final
filename only after the complete stream is written and flushed.

Optional resource subscriptions announce that unread state changed; they do not carry a raw
transcript, advance a cursor, or count as delivery. The reliable path remains read/wait plus
acknowledgement. A notification reaching a client does not prove that the host inserted text
into the model's context or started an inference turn.

## Harness integration

Support Claude Code, Codex, OpenCode and Pi as clients where their installed versions support
the required MCP transport, or through a small supported bridge. Other local agents can join
with a custom label and no native binding. Their sessions cannot be observed without a reader.

`mukade chat connect <harness>` is the explicit configuration operation. It offers a preview,
selects user/project scope, validates the endpoint and writes only the relevant MCP/plugin
configuration atomically. It preserves unrelated settings, detects conflicts and is idempotent.
Project-scoped configuration contains only an endpoint and a supported credential reference;
the bearer value remains in owner-only user configuration or the protected service directory.
If a harness cannot resolve credentials that way, reject project-scope setup and explain the
supported user-scope setup. Never place the credential in a project file that could be committed.
`disconnect` removes only entries owned by mukade and refuses to overwrite intervening user
changes. Neither command writes transcripts or delivers messages by editing a native store.
Keep any reversible configuration backup in mukade's own protected data directory, with
bounded rotation. The only exception to native-store write protection is this explicit,
scoped configuration operation.

Thin integrations provide role instructions, the instance's own verified session ID when
available, and safe turn-boundary reads/receipts where the harness supports them. They do not
duplicate domain logic or make their own database. Unsupported features are stated per harness.
Manual selection and explicit tools remain usable without automatic integration.
Each member has one designated room consumer, either explicit MCP/CLI calls or an automatic
adapter. The adapter must deliver the full batch including IDs and act on an attributable host
acceptance signal before acknowledging it. A host accepting data into a queue still leaves
model processing unverified. Do not run an invisible background consumer alongside the agent's
own reads. When both call accidentally, the common pending batch and stream guard prevent
skips, although repeated delivery remains possible. User-visible status identifies the mode.

Before claiming automatic integration, probe each supported harness/version for: endpoint
access in its sandbox, own-session-ID provenance, message delivery during an active turn,
idle-prompt behaviour, interruption/cancellation, receipt placement and reconnect. Record
observed results. No universal claim of waking idle agents is part of this design.

## Use cases and failure behaviour

1. **Explainer:** a peer explicitly binds its native session; an observer reads only the reduced
   view and explains progress/reasons to the user. It cannot send remarks to the peer.
2. **Dialogue:** several peers exchange room messages and tags, wait without model polling,
   acknowledge received batches, and decide together. Sealed rounds/voting are not required.
3. **Pair programming:** a driver sends paths and progress; a navigator reads the project and
   replies. Only one writes files, or they use separate worktrees. Chat does not prevent edits
   by other tools from conflicting.
4. **Lead/workers:** a lead sends addressed instructions; workers read the entire room but act
   on their own instructions, signal done/blocked against those event numbers, and wait again.
5. **Mixed room:** peers work while an observer explains their sessions and room exchange.
6. **Same harness twice:** each instance joins separately with its own member handle and exact
   native ID. Same cwd/model never merges or identifies them.
7. **Restart:** the user stops and starts chat days later, reconnects/rejoins named members,
   and reads pending events from persisted cursors. Nothing expires or silently follows a
   new native session.

An absent service, inaccessible data folder, unsupported schema or unavailable source produces
an actionable error, never a fallback implementation. Failed sends store no partial content.
A disconnected reader does not consume its pending batch. A crashed writer recovers atomically.
An unrecognized inline tag is text; an invalid explicit recipient is refused. Leaving preserves
history. Closed rooms permit reads/receipts/ledger/export and bound observation. Their only
new domain events are explicit reopen, leave or unbind. Delete removes the room. Reading may
update delivery state in a closed room.

## Existing foundations and implementation boundary

The repository contains library foundations and their tests; it does not yet contain this
MCP service or its public `chat` commands. Runtime documentation must keep that distinction.

| Existing area | Disposition for this design |
|---|---|
| `src/reader/origin.rs`, `src/bus/view.rs`, `src/bus/secrets.rs` | Reuse classification, projection and masking; extend mixed-origin coverage and write-size metadata. |
| `src/bus/room.rs` | Reuse role/name/room rules; require full native IDs, add durable member handles and crash-safe rejoin. |
| `src/bus/store.rs`, `src/portable.rs` | Reuse WAL, transaction, disk/path guards and data-root policy; add service ownership, migrations, receipt/idempotency state and configurable limits. |
| `src/bus/observe.rs` | Reuse projection traversal, pagination and update/rewind logic; replace immediate consumption with stage/ack/invalidate for network delivery. |
| `src/bus/ledger.rs` | Reuse read-only queries and export content boundaries; add message/signal fields, member read/signal state and bounded snapshot export. |
| `tests/bus_*.rs` | Keep domain/projection/store tests; adapt consumption tests when receipt APIs are implemented. They do not demonstrate an MCP server. |

The library's current in-process delivery callback confirms local consumption only and must
not be connected directly to HTTP flush as a receipt implementation. Network delivery is a
new service contract. Public chat commands must arrive together with their working service;
there is no parallel direct-database CLI or compatibility alias to maintain.

## Acceptance and implementation order

1. Select the SDK/transport versions using real client probes; specify service configuration,
   startup ownership, protected credentials and protocol compatibility. Verify one listener
   can serve concurrent clients without an external runtime.
2. Implement durable message/signal events, exact binding and member identity, idempotent
   mutations and stage/ack cursor APIs in the library. Test permission matrices, Unicode and
   secret boundaries, multiple leads and restart recovery.
3. Implement lifecycle and MCP tools using those APIs, then the chat CLI as a client. Test
   simultaneous starts, readiness failure, stale/PID-reused metadata, slow clients, cancellation,
   dropped responses before/after commit and graceful/crash shutdown. Check loopback/firewall
   behaviour on Windows without changing global firewall settings.
4. Adapt observation to receipts and add bounded ledger/export support. Test mutable native
   transcripts between staging and acknowledgement, source deletion, rewind, expired export
   handles and late acknowledgements.
5. Implement explicit connection setup and thin harness integrations against verified APIs.
   Test preservation/rollback of unrelated configuration and idempotent connect/disconnect.
6. Run original tests unchanged, targeted security/concurrency/crash tests, release build,
   dependency review and live interoperability. Live acceptance needs two real simultaneous
   harness instances: explicit identities, observation, a tagged message each way, an addressed
   worker signal, disconnect/reconnect and server restart with pending delivery.

No success claim is based solely on fixture tests, a notification arriving, or two archived
transcripts being readable. The client matrix states which behaviours were actually observed.

The following failure cases are mandatory acceptance scenarios for the contracts above.

| Scenario | Required result |
|---|---|
| Send commits, response is lost, room closes, same request is retried | Return the existing event without another insertion |
| Ack A succeeds, batch B is staged, ack A is retried | B stays pending and the cursor does not advance past A |
| A batch is pending and wait uses a different recipient filter | Replay the pending batch immediately |
| A lead waits for addressed traffic and a worker signals done | The signal wakes the lead and references the exact instruction |
| One instruction addresses two workers and the first completes | The second can still signal blocked or done independently |
| All members have left a closed room | Administrative history/export remains available without reopening |
| Leave commits but its response is lost | The same request with its revoked handle returns the committed leave result |
| Rejoin races with the old process sending or acknowledging | The transaction order determines the outcome, and old-handle operations after rejoin are refused |
| Unbind or leave races with observation, including in a closed room | Revoke staged delivery and reject later receipts for that binding |
| Native text changes beyond its displayed prefix | Emit an update because the full permitted fingerprint changed |
| A source appends while a batch is pending or before its ack | The staged delta can be acknowledged and later content remains unread |
| Restart loses an observation cache and selected items changed | Invalidate that batch explicitly and reject its late ack |
| Service restarts between status and stop | A stale instance ID cannot stop the replacement |
| Space falls below the policy floor | Refuse content growth while allowing transactional cleanup and receipts |
| Export spans concurrent joins, close or deletion | Preserve its captured snapshot or explicitly revoke it, with no mixed pages |
| Two supported protocol revisions disconnect during wait | Apply each revision's cancellation rules without consuming an unacknowledged batch |
| Existing schema is upgraded and the migration fails midway | Preserve the original schema and data atomically |

## Protocol references

- [MCP transports, 2025-11-25](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports)
- [Streamable HTTP, 2026-07-28](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/streamable-http)
- [Versioning and compatibility, 2026-07-28](https://modelcontextprotocol.io/specification/2026-07-28/basic/versioning)
- [Tools, 2026-07-28](https://modelcontextprotocol.io/specification/2026-07-28/server/tools)
- [Subscriptions, 2026-07-28](https://modelcontextprotocol.io/specification/2026-07-28/basic/patterns/subscriptions)

References checked on 2026-11-11. The implementation plan records the installed-client probes
and supported SDK revisions separately.
