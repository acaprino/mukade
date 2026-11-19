# Mukade desktop: the chat view redrawn, with controls and search

Date: 2026-11-16. Status: layer A implemented on 2026-11-19 (`apps/desktop/src/chat.ts`, `chatviews.ts`,
`chatmodel.ts`, `search.ts`, `chatindex.ts`; `src-tauri/src/chat.rs` for the writes, the service and
launches); layer B is not. Where the build differs from this text:

- The window's member is the host's (`join … host: true`, with the host credential): what it sends
  reaches the agents as the user's ([usage](../../usage.md#the-host)). The host came after this design.
- `←` `→` keep moving between columns, as in the sessions view; `↑` `↓` step the events of the room.
- The toolbar's field stays the sessions filter; Search everything is `ctrl+K` and a toolbar button.
- A plate's harness mark is the harness's logo where the window has one, else a plain mark.

The design as written:
It changes the chat view of [the desktop window](2026-11-12-desktop-design.md) and replaces two of
its statements: that the chat view is read only, and that chat writes are out of scope.
Prototype: [`docs/mockups/desktop-chat.html`](../../mockups/desktop-chat.html), one file, opened in
a browser, sample data. Where this text and the prototype differ, this text wins.
Decisions labelled **Builder default** are choices the user has not confirmed. Facts marked **[U]**
are not verified.

Two layers, kept apart all through:

- **Layer A** works on the chat service as it is today.
- **Layer B** needs [the overdue worker and the write turn](2026-11-16-chat-overdue-write-turn-design.md),
  which are not implemented either.

## Problem Statement

The chat view shows a room as a flat list. It does not show who wrote to whom: the recipient is a
grey line under the text, and a worker's signal is not tied to the instruction it answers. The
members are three tall cards of names and values, with nothing about who is working, who listens
and who went quiet. The only search is a text filter on the messages of one room. Half the window
stays empty.

The view is read only. To write into a room, open or close one, start an agent in one, or start the
service, the user leaves the window for a terminal. The window says how and stops there.

## Solution

The chat view keeps its three columns and changes what each one does.

- **Rooms**, on the left: filter, open and closed, a mark on a room that has an overdue worker, and
  New room.
- **The room**, in the middle, in three views of the same events:
  - **Graph**: each member is a plate with its harness mark, role and state; a line joins two
    members that wrote to each other, thicker with each message; a message to everyone goes to a
    hub in the middle. A strip under it has one mark per event: the user picks an event, steps
    through them, or plays the room back and watches each message travel along its line.
  - **Lanes**: one lane per member and time going down, as a sequence diagram. A message is a wire
    from its sender to its recipients with the text under it; a signal is a wire back to the lead.
  - **Feed**: the list of today, with the recipients as names and the state of each instruction.
- **Members and instructions**, on the right: one row per member with role and state, its details
  on demand, the write turn, and the instructions leads gave to workers with their state.
- **Controls**: write into the room, open, close, reopen and delete a room, launch an agent into a
  room, start and stop the service.
- **Search everything**: one field for rooms, members, messages and sessions.

The window writes by joining the room as a member of its own, which the agents see like any other.

## User Stories

Seeing a room:

1. As a user, I want to see at a glance who wrote to whom in a room, so that I understand a
   conversation between agents without reading all of it.
2. As a user, I want each member shown with its harness mark, so that two instances of the same
   harness and instances of different ones are told apart.
3. As a user, I want to play a room back event by event, so that I can follow how a discussion went.
4. As a user, I want to pick any event and see its text, so that the picture leads me to the words.
5. As a user, I want a worker's signal drawn back to the instruction it answers, so that I see what
   was done and what is still open.
6. As a user, I want a view with one lane per member, so that I can read order and direction
   exactly when the graph is too coarse.
7. As a user, I want the plain list kept, so that I can read a long exchange as text.
8. As a user, I want my place in the room kept when I change view, so that the three views are
   three readings of one selection.
9. As a user, I want members that joined later to appear when the replay reaches their join, so
   that the picture is true at each moment.

Filtering and searching:

10. As a user, I want to filter a room by member, by kind of event and by text, in all three views,
    so that I can isolate one thread.
11. As a user, I want one search over rooms, members, messages and sessions, so that I can find a
    sentence without knowing which room it was said in.
12. As a user, I want a search result to open the room at that event, so that finding leads to
    reading.
13. As a user, I want a filter with no match to say so and offer to clear it, so that an empty pane
    is never unexplained.

Members and instructions:

14. As a user, I want each member's role and state in one row, so that I see who works, who listens
    and who went quiet.
15. As a user, I want a member's session id, folder and last progress on demand, and a jump to its
    session, so that the room leads to the transcript.
16. As a user, I want the list of instructions leads gave to workers with their state (waiting,
    blocked, done; overdue with layer B), so that I see what the room owes.
17. As a user, I want a room with an overdue worker marked in the rooms list, so that I notice it
    from another room. (Layer B.)
18. As a user, I want to see who holds the write turn and to free it after a confirmation, so that
    I can end a deadlock. (Layer B.)

Controls:

19. As a user, I want to write into a room from the window, to everyone or to chosen members, so
    that I can steer agents without a terminal.
20. As a user, I want to choose whether the window joins as a peer or as a lead, so that I can
    either talk or give instructions workers must signal.
21. As a user, I want a refusal by the service shown at the field it concerns, in the service's
    words, so that I know what to change.
22. As a user, I want my unsent text kept per room and across a restart of the window, so that a
    change of room does not lose it.
23. As a user, I want to open, close, reopen and delete rooms, with a question before closing and a
    summary before deleting, so that I manage rooms where I read them.
24. As a user, I want to launch an agent into the room in a new terminal, choosing harness, role,
    folder and how long it listens, and to see the command first, so that starting a participant
    is one step.
25. As a user, I want to start and stop the chat service from the window, with a question before
    stopping, so that the window is enough to use chat.
26. As a user, I want to leave a room I joined, so that the window does not stay a member by
    accident.

The rest of the window:

27. As a user, I want a session to show the rooms a member bound to it is in, and to jump there, so
    that sessions and chat are two ends of one thing.
28. As a keyboard user, I want every action reachable with keys and the focus always visible, so
    that the picture is not a mouse-only view.
29. As a user who turned motion off, I want the replay to step without animation, so that the
    system setting is respected.

## Implementation Decisions

### What does not change

- The frame, the skin and its tokens, the three-column layout, polling every 3 seconds while the
  view is visible, and the rule that the window reaches chat **through the running service only**
  and never opens the store.
- The reader, the hop dialog, the receipt, the import dialog, Scan folder and Import.
- The terminal UI.

### The three views

- One selection per room: the selected event. It defaults to the newest. All three views show it
  and change it. A new event selects itself only when the newest was selected, the same rule the
  list has today for following the end.
- **Graph.**
  - Members keep a fixed place, in join order around the hub, for as long as the room is shown: a
    member does not move when another joins. A member is drawn from its join on, and dimmed after
    it leaves.
  - A plate shows the harness mark, the name, the role and the state. Its top edge carries the
    state's colour, as the window's panes carry focus. It is a button: it selects the member.
  - Lines, drawn for the events up to the selected one: message (solid, in the accent, one line per
    pair of members, wider with each message), signal done (dashed, live colour), signal blocked
    and overdue (dashed, warning colour), observation (dotted, muted). Lines of different kinds
    between the same two members run side by side.
  - A message with no recipient goes from its sender to the hub.
  - The selected event's line is drawn over the others with an arrowhead, and a mark travels along
    it once. With reduced motion there is no travelling mark.
  - The strip has one mark per event, coloured by kind, with play, pause, previous and next. Play
    advances one event about every second and stops at the end.
  - Under the strip, the selected event as a feed card.
  - **Known limit.** The graph is legible to about ten members. Past that, the lanes and the feed
    are the views to use; the graph is not given a second layout.
- **Lanes.** One column per member in join order, a sticky head with mark, name and role, one row
  per event. A message row has a wire from the sender's lane to each recipient's lane, with the
  text in a block under the wire. A signal, an observation and an overdue event have a wire to the
  member they concern and a short label. The other events are a label in their member's lane.
- **Feed.** As today, plus: the sender's mark, recipients as names, the state of each instruction a
  message carries, and on a signal a link to its instruction.
- **Filter.** Text, members (any number), and three kinds: messages; signals and turn; system. It
  applies to the three views. In the graph a member outside the filter is dimmed and the lines are
  drawn from the events that pass. Selecting an event the filter hides clears the filter.

### Members, instructions, turn

- A member row: mark, name, role, state. A click opens its details: harness, session id or "not
  bound", folder, last progress, and the actions Mention and Show session.
- **State, layer A:** `left`, or nothing more than the last command time the service gives. The
  window does not invent working or listening from it.
- **State, layer B:** working, listening, overdue, from the progress and the open instructions of
  the chat design; the evidence (bus or session) is shown beside the last progress.
- **Instructions:** every lead message addressed to a worker, newest first, with the worker and the
  state. Layer A derives waiting, blocked and done from the signals in the ledger. Layer B adds
  overdue and the action Send again, which fills the composer with the instruction and its
  recipients and sends nothing by itself.
- **Write turn (layer B):** the holder, since when, its last progress; Free the turn asks first and
  says that it stops nobody's writes.

### Writing: the window is a member

- To write, the window joins the room as a member with the service's join: a name the user gives
  (**Builder default** `desk`), the role peer or lead, a label of its own as harness, no binding.
  It never joins as worker or observer.
- The window reads without joining, as today. Joining is asked for at the first write, in the
  composer's place.
- The window keeps its member handle per room in its own data, so that a restart of the window
  writes as the same member. A handle the service calls stale is recovered with a rejoin, after
  telling the user.
- The window's member does not consume the room: the window goes on reading the ledger, which
  moves no cursor. Whether a member that never reads has any other effect in the service is not
  verified **[U]**.
- **Composer.** Recipients as toggles (everyone, or members), a text field, Send, Leave. A mention
  typed in the text addresses that member, as the service already rules. Checked before the call:
  empty text, and the size limit of a message. Everything else is the service's answer, shown under
  the field in its words: a recognized secret, a closed room, a role refusal. While a send is on
  its way the button is disabled and says so. After it, the window polls at once.
- A lead's message to a worker is an instruction; the composer says so before it is sent.
- A closed room has no composer: a line says the room is closed and offers Reopen.

### Rooms

- New room: a name by the service's rule. An open room of that name is offered ("Go to it"); a
  closed one is named, with the advice to reopen it.
- Close asks first and says what stays readable. Reopen does not ask.
- Delete exists on a closed room only. It shows the room's summary from the service, then calls
  delete with the revision of that summary, as the service requires.
- Open, close, reopen and delete use the service's administrative credential, as the command line
  does; the window needs no member for them.

### Launch an agent

- A dialog: harness (Claude Code, Codex, OpenCode, Pi), role, seconds it listens (0 to 3600), the
  folder the terminal opens in, and the command it will run, shown as text.
- Launch opens a new terminal window in that folder with the chat launch command, by the same
  means Resume uses. The window does not join for the agent: the new member appears at a poll.
- If no member joins within a minute, the window says so and shows the command again. **Builder
  default.** Codex needs its launch hooks trusted once; how a launch that waits on that looks from
  the window is not verified **[U]**.

### The service

- Start and Stop call the same lifecycle the command line calls. Stop asks first and says how many
  members in how many open rooms stop receiving. The stopped screen keeps the terminal command
  beside the Start button.
- This replaces "never started from here". The window still starts no agent by itself and joins no
  room without being asked.

### Search everything

- One field in the toolbar, `ctrl+K` or `/`, opening a list over the window: rooms and members by
  name, messages by text, sessions by title, id or folder; a scope to narrow to one of the four.
- Messages are searched through the ledger's text query, one room at a time. **Builder default:**
  open rooms first, five results per group, more on narrowing the scope.
- A result opens its place: the room, the room with the member's details, the room at that event
  in the feed, or the session in the tree.
- Each list keeps its own filter: rooms in the rooms head, messages in the room, sessions in the
  sessions head. The toolbar's field is no longer the session filter.

### Sessions

- The installations list shows the harness marks.
- A session shows "In rooms": each room where a member is bound to it, with the member's name and
  role, and a jump to that room with the member's details open.

### Commands between the window and its backend

- New, each a pass-through to one service operation or one lifecycle call, typed as the existing
  chat reads are: open, close, reopen, delete, join, rejoin, leave, send, the room summary for
  delete, start, stop, and launch in a terminal. With layer B: the forced release.
- Each write carries a request id made by the window, so a repeated click or a lost answer cannot
  send twice.

### Keys

- `c` chat and sessions; `ctrl+K` and `/` search; `1` `2` `3` graph, lanes, feed; `←` `→` previous
  and next event; `space` play and pause in the graph; `n` new room; `l` launch; `ctrl+Enter` send;
  `Escape` closes a dialog and returns the focus to the control that opened it.
- No single-letter key acts while a field has the focus.

### Accessibility

- A plate is a focusable button with a spoken label: name, role, harness, state, and the write turn
  when it holds it. The lines are not focusable: the strip, the lanes and the feed give every event
  to the keyboard and to a screen reader.
- The selected event's card under the graph is announced when it changes.
- State is never colour alone: each state is also a word.

## Testing Decisions

- A good test drives the window as a user does (a click, a key, typed text) or calls a command with
  a service reply, and asserts on what is shown or returned. It does not look at private state.
- **Seams: the existing two, no new one.**
  - The window's command boundary, mocked, which the smoke test and the unit tests already use.
  - Each backend command's pure part on a canned service reply, as the chat reads are tested today.
- **Unit tests in the window:** the lines of a room up to an event (count per pair, kind, a message
  to everyone going to the hub); the members present at an event; the lane rows of each kind of
  event; the state of an instruction from the ledger; the three filters; the search and its
  grouping; the checks made before a send. Prior art: the chat ledger, tree and key tests.
- **Backend tests:** each new command's arguments from the window's input and its reply into the
  window's rows; the stale handle path; the delete carrying the summary's revision. Prior art: the
  chat reply tests with canned JSON and the stopped and unreachable states.
- **Smoke:** one screenshot per view of the room, dark and light, and the flows join, send, new
  room, launch dialog and search, on the mocked commands. Prior art: the existing smoke run.
- **The prototype's walk** is the list of cases to start from: 56 checks on the prototype, all
  passing, among them reserved and taken names, an empty send, a refused secret, loading states,
  a reload that keeps view, draft and an open dialog, focus returning after Escape, and no sideways
  scroll at 820 px.
- **Live check:** one run recorded with the real service: the window joins, sends to a launched
  Claude Code and gets its answer; a launch from the dialog joins the room. Not done **[U]**.

## Out of Scope

- The terminal UI.
- A redesign of the reader, the hop dialog, the receipt and the import dialog.
- The window observing a member's session, or joining as worker or observer.
- Editing or deleting a message. The service has neither.
- Enforcing the write turn, and stopping or restarting an agent from the window.
- Operating-system notifications for a new message or an overdue worker.
- Moving the plates by hand, and a graph layout for large rooms.
- Exporting a room from the window.

## Further Notes

- **Order of work.** Each step ships alone.
  1. The three views, the filter, the members and instructions column and "In rooms", on the
     reads the window already has. No new command, no change to the service.
  2. Search everything.
  3. Writing: join, send, leave, rejoin.
  4. Rooms: open, close, reopen, delete.
  5. Launch, then start and stop.
  6. Layer B, when the chat design it depends on is built.
- **What the prototype fakes.** Every wait is a timer and nothing reaches a service. The states
  working, listening and overdue, and the write turn, are sample data for layer B. The refusal of
  a secret is a pattern in the page, standing in for the service's own check.
- **What the prototype was not tested in:** the real window (WebView2 under Tauri), a screen
  reader, a full Tab pass over every control, reduced motion, and a machine without the skin's
  fonts.
- **The harness marks** are the ones the documentation already ships, each its owner's. Showing
  them in the window needs the same notice the documentation carries.
- **Documents to change when this is built:** the desktop design's chat section and its out of
  scope list, the desktop guide's view table and keys.
