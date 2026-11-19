# Mukade chat: an overdue worker and a write turn

Date: 2026-11-16. Status: design; nothing here is implemented.
It extends [Mukade chat: a shared local MCP server](2026-11-11-mcp-chat-design.md) and uses its
terms: room, member, handle, role, instruction, signal, binding, batch, cursor, event.
Decisions labelled **Builder default** are implementation choices the user has not confirmed.
Facts marked **[U]** are not verified.

The idea comes from Meldwork's scheduling (one writer at a time in a conversation, a watchdog for a
silent agent, recovery after a crash), read on 2026-11-16 from its README and architecture document.
Meldwork starts the agents itself, so it can stop one. The chat service starts no agent, so
everything below reports and coordinates; it stops nothing.

## Problem Statement

A lead sends an instruction to a worker and waits. If the worker's process dies, or the worker ends
its turn without signalling, the instruction stays unsignalled for ever and the lead keeps waiting.
The service already records each member's last command time and shows it in the member listing,
but nothing compares it with the clock and nothing tells the lead.

Two members that share one checkout can edit the same files at the same moment. The chat design
says "only one writes files, or they use separate worktrees" and leaves it to the agents: the room
has no way to say whose turn it is, and no way to free a turn held by an agent that died.

A worker recovered with a rejoin keeps its cursor, so it does not see again an instruction it had
already acknowledged. It does not know what it still owes.

## Solution

**Overdue.** The service knows which instructions a worker has not signalled. When such a worker
shows no progress for longer than the silence threshold, its instruction is overdue. The member
listing shows it, and the lead's wait returns with an `overdue` event that names the worker, the
instruction, the reason and the last sign of progress. The lead decides what to do: send again,
give the work to another worker, or tell the user.

**Write turn.** A room has one write turn. A member takes it with `chat_claim` before it writes
files in a checkout it shares, and gives it back with `chat_release`. While one member holds it,
another member's claim is refused with the holder's name. The turn frees itself when its holder
leaves, when the room closes, or when the holder shows no progress for the silence threshold, so a
fallen agent does not keep it. A member that was refused is woken when the turn is free.

**Recovery.** A rejoin of a worker returns the instructions it has not signalled, and whether it
holds the write turn. A join always makes a new member, which owes nothing and holds nothing: a
worker that comes back with a join instead of a rejoin leaves its old member's instructions open,
and they turn overdue as any silent worker's do.

The turn is an agreement between agents. The service does not watch the filesystem: a member that
ignores the turn can still write.

## User Stories

Overdue:

1. As a lead, I want my wait to return when a worker I instructed has been silent past the
   threshold, so that I do not wait for ever on a dead worker.
2. As a lead, I want the overdue event to name the worker, the instruction and the last sign of
   progress, so that I can decide between sending again and reassigning.
3. As a lead, I want to know whether the last sign of progress came from the bus or from the
   worker's native session, so that I know how strong the evidence is.
4. As a lead, I want a worker that is in a long turn, with its bound session still growing, not to
   be reported, so that I am not woken by false alarms.
5. As a lead, I want a worker that ended its turn and listens without having signalled to be
   reported, so that a forgotten signal does not look like work in progress.
6. As a lead, I want to be told at once when an instructed worker leaves the room, so that I do
   not wait out the threshold for a member that is gone.
7. As a lead, I want one overdue event for one silence, so that my room is not filled with
   repeats.
8. As a lead, I want a second event when a worker that came back falls silent again, so that the
   first report does not hide the second failure.
9. As a worker, I want my `blocked` signal to stop the clock, so that waiting for the lead's
   answer is not reported as silence.
10. As any member or the user, I want the member listing to show each worker's open instructions
    and their state without changing anything, so that I can look without consuming.
11. As a user, I want to set the silence threshold or turn the feature off, so that long builds or
    slow models do not produce noise.
12. As a user, I want no room, member or history removed because of silence, so that the existing
    promise that nothing expires from history still holds.

Write turn:

13. As a peer, lead or worker, I want to take the room's write turn, so that the others know I am
    the one writing.
14. As a member whose claim is refused, I want the holder's name and the time of its last
    progress, so that I can wait or ask it in the room.
15. As a member whose claim was refused, I want my wait to return when the turn is free, so that I
    do not poll.
16. As the holder, I want my turn to last while I work, without calling again, so that a long turn
    does not lose it.
17. As the holder, I want to give the turn back when I finish, so that the next writer starts at
    once.
18. As a member, I want the turn of an agent that died to free itself, so that the room is not
    blocked until a person steps in.
19. As a former holder whose turn was freed for silence, I want my next claim or release to say
    so, and who holds the turn now, so that I know my last writes had no turn behind them.
20. As an observer, I want to see who holds the turn and its history in the ledger, so that I can
    explain to the user who wrote when.
21. As a user, I want to free the turn from the command line, so that I can end a deadlock.
22. As a user whose agents use separate worktrees, I want the turn to be optional, so that it
    costs nothing when it is not needed.

Recovery:

23. As a worker recovered with a rejoin, I want the list of instructions I have not signalled, so
    that I can go on with them.
24. As a holder recovered with a rejoin, I want to keep the turn if it has not been freed, so that
    a reconnect does not hand my work to someone else.
25. As a user who stops the service for days and starts it again, I want silence measured on the
    wall clock, so that the state after the restart is true.

## Implementation Decisions

### Progress

- A member's **last progress** is the later of two times: its last command time, as the service
  records it today, and, for a member with a binding, the last update time of its bound native
  session as the session reader reports it.
- The last command time keeps its present meaning: it moves on join, rejoin, bind, send, signal, a
  staged batch, an acknowledgement and an observation. A wait that returns nothing does not move
  it. This is what makes story 5 hold, and it must not change.
- Each derived state carries its **evidence**: `bus` or `session`, whichever time was later.
- A member without a binding has only the bus as evidence. A launched instance speaks to the bus
  at its turn boundaries only, so a long turn of an unbound member looks like silence. The lead
  sees `bus` as the evidence and judges accordingly.
- The update time is read through the server's configured stores only, as the binding rules
  require; no path comes from a client. Reading it must not hold a database snapshot open, as the
  candidate listing already takes care of. The cost of one read per bound worker with an open
  instruction is not measured **[U]**.

### Silence threshold

- One service setting, in seconds. **Builder default:** 600. Minimum 60. Zero turns off both the
  overdue state and the freeing of a silent holder's turn; explicit release, leave, close and the
  user's command still free the turn.
- Silence is wall-clock time, so it includes the time the service was stopped.
- All time comparisons go through one clock owned by the bus (see Testing Decisions).

### Overdue

- An **open instruction** of a worker is a lead's message addressed to that worker, by the rules
  the signal already applies, for which the worker's signal state is unsignalled. `blocked` and
  `done` close it for this purpose. Only members whose role is worker have open instructions.
- An open instruction is **overdue** when both the instruction and the worker's last progress are
  older than the silence threshold. Reason `silent`.
- It is overdue at once, with no threshold, when the worker has left the room. Reason `left`.
- The member listing derives, for each worker: the number of open instructions and, for the oldest
  five, the instruction's event number, its lead, its time, its state (`waiting` or `overdue`),
  the reason, the last progress and the evidence. The listing stays read-only and writes nothing.
- The `overdue` event is a room event written by the service, addressed to the instruction's
  author, so it satisfies that lead's wait as a signal does. It is written when the author makes a
  consuming read or wait: at the call and at each of the wait's periodic rechecks. No background
  timer is added: when no lead is reading, there is nobody to tell.
- One `overdue` event per instruction and worker for one silence. A new one is written only after
  the worker showed progress later than the previous event and fell silent again. The service
  keeps, per instruction and worker, the last overdue event and the progress time it was judged
  against.
- An overdue instruction can still be signalled; the signal closes it.
- A closed room writes no overdue events, as it writes no other new domain event.

### Write turn

- One turn per room. It is held by a member, not by a handle, so a rejoin keeps it.
- `chat_claim` takes the request ID, the room and the member handle. Peers, leads and workers may
  claim; an observer is refused with `role_refused`. A closed room refuses with `room_closed`.
  - Free turn: the caller holds it, and a `claim` event is written.
  - Held by the caller: success, no event. The result is the current turn.
  - Held by another member: refused with `turn_held`, carrying the holder's name, the time of the
    claim, the holder's last progress and its evidence. The caller is recorded as waiting.
- `chat_release` takes the same arguments. The holder frees the turn and a `release` event with
  reason `released` is written. A member that does not hold the turn is refused with
  `turn_not_held`, which names the present holder, if any, and says when and why the caller's own
  turn ended, if it had one.
- The turn frees itself, with a `release` event written by the service and its reason:
  - `left`: the holder leaves the room;
  - `closed`: the room closes;
  - `silent`: the holder's last progress is older than the silence threshold;
  - `forced`: the user frees it from the command line.
- A silent turn is freed lazily, with no timer: at the next claim by anyone, and at a consuming
  read or wait of a member recorded as waiting. A claim that finds a silent holder writes the
  `release` and then its own `claim` in one transaction.
- Progress keeps the turn: no renewal call exists. A bound holder keeps it while its session
  grows; an unbound holder keeps it while it speaks to the bus.
- A `release` event is addressed to the members recorded as waiting since the turn was taken, so
  their addressed wait returns. The record is cleared with the release. There is no queue: the
  first claim after a release wins.
- A `claim` event is addressed to nobody and wakes no wait.
- The member listing shows the turn: the holder, the time of the claim, the holder's last progress
  with its evidence, and whether it is past the threshold. A rejoin's result says whether the
  caller holds it.
- Claim and release are domain mutations: they follow the existing request ID rules. A refusal
  commits nothing but the waiting record.
- The command line gets the same two operations. Freeing the turn on a member's behalf (`forced`)
  exists on the command line only and is not in the MCP tool schema.

### Recovery

- The result of a worker's rejoin lists its open instructions, by the same derivation and the same
  limit as the member listing. A join's result does not: a joined member is new and nothing was
  addressed to it yet.

### Instructions and adapters

- The instructions a join returns gain one sentence for peers, leads and workers: take the write
  turn before writing files in a checkout another member also works in, and release it after. The
  lead's instructions gain one sentence on the overdue event.
- The launch adapters carry those sentences in their introduction and render the `overdue` and
  `release` events like other room events. They add no hook and do not block a tool.

### Storage and surfaces

- The schema change is additive: the turn of each room, the members waiting for it, and the
  overdue record per instruction and worker. Existing tables do not change.
- Two new tools; three new event kinds (`overdue`, `claim`, `release`), visible in the ledger and
  the export like any other event; new fields in the member listing and in the rejoin result. Existing fields keep their names and meaning.

## Testing Decisions

- A good test here drives the chat operations as a client does and asserts on results and on the
  events in the room. It does not read tables or private state.
- **Seam.** The existing one: the bus's chat call, which the chat domain tests already use for
  every operation. **One new seam, proposed:** the bus owns its clock and a test can set it. Every
  rule in this document depends on time, and the alternative is tests that sleep. The clock is set
  where the bus is opened, so no operation gains a time argument. This needs the user's
  confirmation.
- **Prior art.** The chat domain test of independent signal state for addressed workers; the test
  of a pending wait and an idempotent acknowledgement across restart and rejoin; the binding test
  that uses a scratch store; the tool definition test, which must list the two new tools; the
  adapter tests for the introduction text.
- **Cases to cover.** An unsignalled instruction becomes overdue after the threshold and not
  before; `blocked` and `done` close it; a bound worker whose session was updated is not overdue;
  an empty wait by the worker does not count as progress; a leave reports at once; one event per
  silence, a second after progress and a new silence; the lead's addressed wait returns on it; the
  listing writes no event. A claim on a free turn, a refused claim, a repeated claim by the
  holder, a release, a release by a non-holder; the turn freed by leave, close and silence; a
  refused member's wait returning on release; a rejoin keeping the turn; the threshold at zero.
- **Live check.** One run added to the chat live record, with the existing lead and worker script:
  a worker that is stopped mid-instruction, and a lead woken by the overdue event; two launched
  instances in one checkout taking the turn one after the other. Whether real models call
  `chat_claim` when told to is unknown until that run **[U]**.

## Out of Scope

- **Stopping or restarting an agent.** The service owns no agent process.
- **Enforcing the turn.** No filesystem watch and no hook that blocks an edit. A launch adapter
  could refuse a write tool when its member does not hold the turn; that is a later design, worth
  doing if the live check shows models ignoring the turn.
- **More than one turn in a room** (per path, per checkout). One room, one turn. Agents that work
  apart use separate worktrees and do not claim.
- **A queue for the turn.** First claim after a release wins.
- **A progress signal from workers.** An unbound worker in a long turn can be reported as silent.
  If that proves noisy, a third signal value is the fix.
- **Detecting a dead process before the threshold.** The Claude Code reader can say that a
  session's process is not running; using it as a reason of its own (`not_running`) is left out
  until its cost and reliability per call are measured **[U]**.
- **Automatic reassignment** of an overdue instruction. The lead decides.
- **Meldwork's other mechanisms**: the frozen task snapshot, the human gate, run budgets.

## Further Notes

- **Order of work.** Each step is usable alone: (1) progress, the derived state in the member
  listing, open instructions in the rejoin result; (2) the `overdue` event and the lead's wait;
  (3) the write turn. Step 1 needs no schema change.
- **The chat ADR** says that a stop or a crash expires nothing. That stays true of rooms, members
  and history. The write turn is the first state that ends with time; the ADR needs one sentence
  saying so when this is accepted.
- **Known limit.** A holder inside one tool call longer than the threshold (a long build) writes
  nothing to its session and loses the turn. It learns this at its next claim or release. If
  nobody claimed meanwhile, its next claim simply takes the turn again.
- **After a long stop**, every open instruction is overdue and a held turn is silent. The first
  read of each lead gets one event per open instruction and worker.
- **The user-facing guide and the tool list** in the documentation change with step 2 and step 3.
