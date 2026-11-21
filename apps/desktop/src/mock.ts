// What the commands answer outside Tauri: `npm run dev` in a browser and the Playwright smoke
// test. Made-up sessions shaped like real listings; nothing here touches a store.

import type { ChatEvent, HandedOff, HandoffOptions, HopArgs, Installation, Member, Receipt, Room, RowView, SessionRow, TranscriptView, Work, WriteOperation } from './types.ts';

const now = Date.now();
const ago = (minutes: number) => new Date(now - minutes * 60_000).toISOString();

const installations: Installation[] = [
  { key: 'claude_code', harness: 'claude_code', label: 'Claude Code', scanned: false, destination: true, count: 0 },
  { key: 'codex', harness: 'codex', label: 'Codex', scanned: false, destination: true, count: 0 },
  { key: 'opencode', harness: 'opencode', label: 'OpenCode', scanned: false, destination: true, count: 0 },
  { key: 'pi', harness: 'pi', label: 'Pi', scanned: false, destination: true, count: 0 },
  { key: 'hermes', harness: 'hermes', label: 'Hermes', scanned: false, destination: true, count: 0, error: 'state.db: database is locked' },
  { key: 'deepseek_harness', harness: 'deepseek_harness', label: 'DeepSeek Harness', scanned: false, destination: true, count: 0 },
];
const JANURA: Installation = {
  key: 'opencode@D:\\apps\\JanuraCode\\data\\share\\opencode\\opencode.db', harness: 'opencode', label: 'OpenCode · JanuraCode',
  store: 'D:\\apps\\JanuraCode\\data\\share\\opencode\\opencode.db', engine: 'D:\\apps\\JanuraCode\\resources\\opencode-cli\\opencode.exe',
  scanned: true, destination: true, count: 0,
};

const project = 'C:\\work\\mukade';
const row = (id: string, title: string, minutes: number, extra: Partial<SessionRow> = {}): SessionRow => ({
  id, title, cwd: project, updated: ago(minutes), archived: false, path: `C:\\Users\\me\\.claude\\projects\\C--work-mukade\\${id}.jsonl`, ...extra,
});
const main = '5f2c9a1e-8b47-4d02-9c3e-71a0de4416f2';
// A chat of the ChatGPT desktop app's Codex: its conversation is on the servers, its work is in a folder.
const HOSTED = '6b00c623-2d08-83eb-a3cc-df6dadef8474';
const DAODAN_FOLDER = 'C:\\Users\\dev\\Documents\\ChatGPT\\Daodan';
const sessions: Record<string, SessionRow[]> = {
  claude_code: [
    row(`${main}.a1`, 'Map the writers', 6, { parent: main, parent_kind: { kind: 'spawn', agent: 'Explore' } }),
    row(main, 'Hop carries subagents to Codex', 2, { live: true }),
    row(`${main}.b2`, 'Order of the phases', 9, { parent: main, parent_kind: { kind: 'spawn', agent: 'Plan' } }),
    row('9a3d71c0-0001-4c55-8d10-2b1f6a0c7e11', 'Try portable compaction', 25, { parent: main, parent_kind: { kind: 'fork' } }),
    row('1b7e44d2-7c3a-4f1e-9a2b-5d6c8e9f0a12', 'Folder filter: counts per cwd', 102),
    row('2c8f55e3-8d4b-4a2f-8b3c-6e7d9f0a1b23', 'Chat ledger paging', 60 * 26),
    row('2c8f55e3-8d4b-4a2f-8b3c-6e7d9f0a1b23.c3', 'Read the bus tests', 60 * 26 + 4, { parent: '2c8f55e3-8d4b-4a2f-8b3c-6e7d9f0a1b23', parent_kind: { kind: 'spawn', agent: 'general-purpose' } }),
    row('3d9066f4-9e5c-4b30-9c4d-7f8e0a1b2c34', 'DeepSeek headless resume', 60 * 27),
    row('4ea177a5-af6d-4c41-ad5e-8091b2c3d445', 'Destination shell for Windows hops', 60 * 50, { cwd: 'C:\\work\\shinatama' }),
    row('5fb288b6-b07e-4d52-be6f-91a2c3d4e556', 'Terminal launch on Windows', 60 * 51, { archived: true, cwd: 'C:\\work\\shinatama' }),
    row('6ac399c7-c18f-4e63-8f70-a2b3d4e5f667', 'Codex code mode: tool tables', 60 * 75, { cwd: 'C:\\work\\site' }),
    row('7c1e04aa-d290-4f74-9081-b3c4e5f6a778', '', 60 * 76, { title: undefined, unreadable: 'the session file is gone', cwd: undefined }),
    row('8d2f15bb-e3a1-4085-a192-c4d5e6f7a889', 'Harness adapters: one answer per harness', 60 * 99),
  ],
  codex: [row(HOSTED, 'Capire lo stack di Daodan', 60 * 30, {
    cwd: DAODAN_FOLDER, path: 'state_5.sqlite#' + HOSTED,
    unreadable: 'hosted by ChatGPT: the conversation is not on this machine; the work it left is in its folder',
  })],
  opencode: [row('ses_3f1a9c', 'Release notes draft', 45, { cwd: 'C:\\work\\site', path: 'opencode.db#ses_3f1a9c' })],
  pi: [row('pi-2026-11-10', 'Pi compaction check', 60 * 48, { path: 'C:\\Users\\me\\.pi\\agent\\sessions\\pi-2026-11-10.jsonl' })],
  hermes: [],
  deepseek_harness: [row('dsh-41c2', 'Headless review', 60 * 30, { cwd: 'C:\\work\\site' })],
};

// The work in a session's folder: the three repositories of a hosted chat, one clean repository
// for the project's own sessions, and nothing elsewhere.
const worktree = (path: string, branch: string, uncommitted: number) => ({ path, branch, head: '41d916d', uncommitted, error: null });
const repository = (name: string, branch: string, upstream: string | null, unpushed: number, uncommitted: number) => ({
  path: `${DAODAN_FOLDER}\\${name}`, worktrees: [worktree(`${DAODAN_FOLDER}\\${name}`, branch, uncommitted)],
  branches: [{ name: branch, upstream, ahead: upstream ? 0 : null, unpushed }], error: null,
});
const hostedWork: Work = {
  folder: DAODAN_FOLDER, note: null,
  repositories: [
    repository('daodan', 'develop', 'origin/develop', 0, 1),
    repository('imago', 'develop', 'origin/develop', 0, 0),
    repository('stack-analyser', 'feature/stack-detection', null, 3, 133),
  ],
};
function workOf(cwd: string | undefined): Work {
  if (cwd === DAODAN_FOLDER) return hostedWork;
  if (cwd === project) {
    return { folder: project, note: null, repositories: [{ path: project, worktrees: [worktree(project, 'develop', 0)],
      branches: [{ name: 'develop', upstream: 'origin/develop', ahead: 0, unpushed: 0 }], error: null }] };
  }
  return { folder: cwd ?? '', repositories: [], note: `no git repository in ${cwd}` };
}
const handoffNote = (row: SessionRow): string => `This continues a session that can no longer be used: "${row.title}" (Codex, id ${row.id}, last active ${row.updated}).
Its conversation is not available here: ${row.unreadable}.

Work it left in ${row.cwd}:
${workOf(row.cwd).repositories.map((repo) => `- ${repo.path}: ${repo.worktrees[0].branch}, ${repo.worktrees[0].uncommitted} uncommitted`).join('\n')}

Start by reading the state of these repositories. Do not push, discard or overwrite anything before asking.`;

// The transcript as the backend sends it: one row per block, `index` the native entry.
const r = (index: number, entry_id: string, role: RowView['role'], minutes: number, text: string, extra: Partial<RowView> = {}): RowView =>
  ({ index, entry_id, role, time: ago(minutes), text, truncated: false, text_chars: text.length, error: false, ...extra });
const tool = (index: number, entry: string, name: string, input: string): RowView =>
  r(index, entry, 'tool', 14, name, { tool: { name, call_id: `${entry}-${name}`, input, input_truncated: false } });
const rows: RowView[] = [
  r(4, 'u1', 'user', 14, 'The hop to Codex leaves the subagents behind. Carry them across with their parent and say in the receipt what could not travel.'),
  r(5, 's1', 'reasoning', 14, 'A Codex rollout is flat, so each subagent has to become a thread of its own that names its parent. First find where the writer decides the thread id.'),
  r(5, 's1', 'assistant', 14, 'I will read how the Codex writer lays out a rollout, then run the children tests.'),
  tool(5, 's1', 'Read', '{"file_path":"src/writers/codex.rs"}'),
  tool(5, 's1', 'Bash', '{"command":"cargo test -p mukade children -- --nocapture","description":"Run the children tests","timeout":120000,"env":{"RUST_LOG":"info","MUKADE_DATA_DIR":"C:\\\\work\\\\mukade\\\\mukade-data"},"note":"a long input is cut at 240 characters behind Show all"}'),
  r(6, 'r1', 'result', 13, '//! Codex writer\nuse crate::model::Session;\n… 214 lines'),
  r(6, 'r1', 'result', 13, 'running 14 tests\ntest children::spawn_keeps_agent_type ... ok\ntest children::fork_stays_a_fork ... ok\ntest result: ok. 14 passed; 0 failed'),
  r(7, 'c1', 'compaction', 8, 'The conversation so far: subagent sessions are read with their parent; the Codex writer gives each a thread of its own; the fork cannot travel as a child.'),
  r(8, 's2', 'assistant', 3, 'Subagents now travel as threads that name their parent. The fork has no place in a Codex thread tree, so it is written as a separate session and listed under losses.'),
  r(9, 'u2', 'user', 2, 'Good. Show the child lines in the hop result too.'),
];
const transcript: TranscriptView = {
  meta: { harness: 'claude_code', version: '2.1.4', id: '5f2c9a1e', title: 'Hop carries subagents to Codex', cwd: 'C:\\work\\mukade', models: ['anthropic/claude'], entries: 12, branches: 0 },
  rows, hidden_turns: 3, total_turns: 5,
};

const receipt = (args: { dest: string }): Receipt => ({
  dest_id: '0199a7c2-4e1b-7d30-b6f4-2c8e5a90d113',
  dest_label: installations.find((item) => item.key === args.dest)?.label ?? 'Codex',
  dest_key: args.dest,
  location: 'C:\\Users\\me\\.codex\\sessions\\2026\\11\\12\\rollout-2026-11-12T12-06-41-0199a7c2.jsonl',
  headless: ['codex', 'exec', 'resume', '0199a7c2-4e1b-7d30-b6f4-2c8e5a90d113', 'continue'],
  interactive: ['codex', 'resume', '0199a7c2-4e1b-7d30-b6f4-2c8e5a90d113'],
  manifest_path: 'C:\\work\\mukade\\mukade-data\\manifests\\codex-0199a7c2.json',
  losses: ['reasoning dropped: made by another provider protocol (12)', 'tool calls written as text: no Codex counterpart (4)'],
  children: [
    { source_id: `${main}.a1`, dest_id: '0199a7c2-51aa-7e01-9c2d-11aa22bb33cc', linked: true },
    { source_id: `${main}.b2`, dest_id: '0199a7c2-5f07-7e02-8d3e-44dd55ee66ff', linked: true },
  ],
  reply: 'Picking up where the Claude Code session stopped: the child lines still have to appear in the hop result. I will start from the receipt view model.',
});

// The chat service, kept in memory: what the window writes (open, join, send…) changes it, as the
// running service would, so every flow can be driven on `npm run dev` and in the smoke test.
let chatRunning = true;
const chatRooms: Room[] = [
  { room_id: 'r-hop', name: 'hop-review', opened_at: ago(40), closed_at: null, opened_by: 'claude-a', kept: true, members: 0, events: 0, last_activity: null, revision: '0' },
  { room_id: 'r-spec', name: 'desktop-spec', opened_at: ago(300), closed_at: null, opened_by: 'claude-a', kept: true, members: 0, events: 0, last_activity: null, revision: '0' },
  { room_id: 'r-codex', name: 'codex-code-mode', opened_at: ago(3000), closed_at: ago(2900), opened_by: 'codex-b', kept: true, members: 0, events: 0, last_activity: null, revision: '0' },
];
const member = (room_id: string, member_id: string, name: string, harness: string, role: string, joined: number, session: Member['session'] = null): Member =>
  ({ room_id, member_id, name, harness, role, joined_at: ago(joined), left_at: null, last_command_at: ago(Math.max(joined - 30, 1)), session });
const chatMembers: Member[] = [
  member('r-hop', 'm1', 'claude-a', 'claude_code', 'lead', 40, { harness: 'claude_code', id: main, cwd: project }),
  member('r-hop', 'm2', 'codex-b', 'codex', 'worker', 38, { harness: 'codex', id: '0199a7c2-4e1b-7d30-b6f4-2c8e5a90d113', cwd: project }),
  member('r-hop', 'm3', 'opencode-c', 'opencode', 'peer', 30),
  member('r-hop', 'm4', 'pi-d', 'pi', 'observer', 20),
  member('r-spec', 'm5', 'claude-a', 'claude_code', 'peer', 300, { harness: 'claude_code', id: main, cwd: project }),
  member('r-spec', 'm6', 'codex-b', 'codex', 'peer', 290),
  member('r-codex', 'm7', 'codex-b', 'codex', 'peer', 3000),
];
const actor = (one: Member) => ({ type: 'member' as const, member_id: one.member_id, name: one.name, role: one.role });
const byId = (id: string) => chatMembers.find((one) => one.member_id === id)!;
let lastEvent = 0;
const event = (room_id: string, who: string | null, minutes: number, kind: string, payload: Record<string, unknown> = {}, recipient_ids: string[] = []): ChatEvent =>
  ({ room_id, event_id: String(++lastEvent), timestamp: ago(minutes), actor: who ? actor(byId(who)) : { type: 'local_admin' }, kind, payload, recipient_ids });
const say = (room_id: string, who: string, minutes: number, text: string, to: string[] = []): ChatEvent =>
  event(room_id, who, minutes, 'message', { text, recipients: to, mentions: to, historical_mentions: [] });
const chatEvents: ChatEvent[] = [
  event('r-hop', 'm1', 40, 'open', { name: 'hop-review' }),
  event('r-hop', 'm2', 38, 'join', { harness: 'codex', role: 'worker' }),
  event('r-hop', 'm3', 30, 'join', { harness: 'opencode', role: 'peer' }),
  say('r-hop', 'm1', 14, 'The hop wrote three Codex threads. @codex-b open 0199a7c2 and check that both subagent threads name it as their parent.', ['codex-b']),
  event('r-hop', 'm4', 12, 'join', { harness: 'pi', role: 'observer' }),
  event('r-hop', 'm4', 11, 'observe', {}, ['m2']),
  event('r-hop', 'm2', 9, 'signal', { instruction_event_id: '4', signal: 'done', reason: null }, ['m1']),
  say('r-hop', 'm3', 8, 'Both name it. The Explore thread keeps its agent type; the Plan thread lost its title and shows the first user line instead.'),
  say('r-hop', 'm1', 6, 'That is a loss the receipt should list. @opencode-c does the same happen on your side?', ['opencode-c']),
  say('r-hop', 'm1', 4, '@codex-b list the Plan thread’s lost title as a loss in the receipt, with the subagent id.', ['codex-b']),
  say('r-hop', 'm3', 1, 'No. OpenCode keeps the title of a child session, so nothing to list there.'),
  event('r-spec', 'm5', 300, 'open', { name: 'desktop-spec' }),
  event('r-spec', 'm6', 290, 'join', { harness: 'codex', role: 'peer' }),
  say('r-spec', 'm5', 70, 'The chat view gets three views of the same events: graph, lanes and feed.'),
  say('r-spec', 'm6', 60, 'Then the selection must be one per room, shared by the three.', ['claude-a']),
  event('r-codex', 'm7', 3000, 'open', { name: 'codex-code-mode' }),
  say('r-codex', 'm7', 2950, 'Code mode reads as tool calls; nothing to add.'),
  event('r-codex', null, 2900, 'close'),
];
/** A room as the service lists it: its counts and revision from what it holds now. */
function roomNow(room: Room): Room {
  const events = chatEvents.filter((one) => one.room_id === room.room_id);
  return { ...room, members: chatMembers.filter((one) => one.room_id === room.room_id && one.left_at === null).length, events: events.length,
    last_activity: events.at(-1)?.timestamp ?? room.opened_at, revision: events.at(-1)?.event_id ?? '0' };
}
const handles = new Map<string, string>();
const replies = new Map<string, unknown>();
const uuid = () => crypto.getRandomValues(new Uint32Array(4)).reduce((text, part) => text + part.toString(16).padStart(8, '0'), '').replace(/^(.{8})(.{4})(.{4})(.{4})(.{12})$/, '$1-$2-$3-$4-$5');
const stamp = () => new Date().toISOString();

function chatWrite(operation: string, a: Record<string, unknown>): unknown {
  const fail = (code: string, message: string): never => { throw { kind: 'failed', message: `${code}: ${message}` }; };
  const request = String(a.request_id ?? '');
  if (replies.has(request)) return replies.get(request);
  const room = chatRooms.find((one) => one.room_id === a.room_id);
  const needRoom = (): Room => room ?? fail('room_not_found', 'No room has that ID');
  const writer = (): Member => chatMembers.find((one) => one.member_id === handles.get(String(a.member_handle)) && one.left_at === null)
    ?? fail('stale_member', 'The member handle is not active; recover the member with rejoin');
  const push = (room_id: string, who: Member | null, kind: string, payload: Record<string, unknown> = {}, recipient_ids: string[] = []) => {
    const added: ChatEvent = { room_id, event_id: String(++lastEvent), timestamp: stamp(), actor: who ? { ...actor(who), host: who.host } : { type: 'local_admin' }, kind, payload, recipient_ids };
    chatEvents.push(added);
    return added;
  };
  let reply: unknown;
  switch (operation) {
    case 'open': {
      const name = String(a.name ?? '');
      if (chatRooms.some((one) => one.name === name && one.closed_at === null)) fail('room_exists', `An open room is named ${name}`);
      const room_id = `r-${uuid().slice(0, 8)}`;
      chatRooms.push({ room_id, name, opened_at: stamp(), closed_at: null, opened_by: null, kept: true, members: 0, events: 0, last_activity: null, revision: '0' });
      push(room_id, null, 'open', { name });
      reply = { room_id };
      break;
    }
    case 'close': case 'reopen': {
      const target = needRoom();
      if ((operation === 'close') === (target.closed_at !== null)) fail('room_state', operation === 'close' ? 'The room is closed already' : 'The room is open');
      target.closed_at = operation === 'close' ? stamp() : null;
      push(target.room_id, null, operation);
      reply = { status: 'ok' };
      break;
    }
    case 'delete': {
      const target = needRoom();
      if (!target.closed_at) fail('room_open', 'Close the room before deleting it');
      if (String(a.expected_revision) !== roomNow(target).revision) fail('revision_mismatch', 'The room changed since its summary');
      chatRooms.splice(chatRooms.indexOf(target), 1);
      for (const list of [chatMembers, chatEvents] as { room_id: string }[][]) for (let i = list.length - 1; i >= 0; i -= 1) if (list[i].room_id === target.room_id) list.splice(i, 1);
      reply = { status: 'ok' };
      break;
    }
    case 'join': {
      const target = needRoom();
      if (target.closed_at) fail('room_closed', 'The room is closed');
      const name = String(a.name ?? '');
      if (chatMembers.some((one) => one.room_id === target.room_id && one.name === name && one.left_at === null)) fail('name_taken', `${name} is taken in this room`);
      const joined: Member = { ...member(target.room_id, `m${uuid().slice(0, 6)}`, name, String(a.harness), String(a.role), 0), joined_at: stamp(), last_command_at: stamp(), host: a.host === true };
      chatMembers.push(joined);
      const handle = uuid();
      handles.set(handle, joined.member_id);
      push(target.room_id, joined, 'join', { harness: joined.harness, role: joined.role });
      reply = { status: 'ok', room_id: target.room_id, member_id: joined.member_id, member_handle: handle, name, harness: joined.harness, role: joined.role, host: joined.host };
      break;
    }
    case 'rejoin': {
      const target = needRoom();
      const again = chatMembers.find((one) => one.member_id === a.member_id && one.room_id === target.room_id) ?? fail('member_not_found', 'No member has that ID in this room');
      again.left_at = null;
      const handle = uuid();
      handles.set(handle, again.member_id);
      push(target.room_id, again, 'rejoin', { role: again.role });
      reply = { status: 'ok', room_id: target.room_id, member_id: again.member_id, member_handle: handle, name: again.name, role: again.role };
      break;
    }
    case 'leave': {
      const target = needRoom();
      const leaving = writer();
      leaving.left_at = stamp();
      push(target.room_id, leaving, 'leave');
      reply = { status: 'ok' };
      break;
    }
    case 'send': {
      const target = needRoom();
      if (target.closed_at) fail('room_closed', 'The room is closed');
      const sender = writer();
      const text = String(a.text ?? '');
      if (/sk-[A-Za-z0-9]{10,}/.test(text)) fail('secret_detected', 'The message looks like it holds a credential; nothing was sent');
      const named = [...text.matchAll(/@([a-z0-9][a-z0-9_-]{0,31})/g)].map((found) => found[1]);
      const to = (Array.isArray(a.recipients) ? a.recipients.map(String) : []).concat(named.filter((name) => !(a.recipients as string[] | undefined)?.includes(name)));
      const ids = to.flatMap((name) => chatMembers.find((one) => one.room_id === target.room_id && one.name === name && one.left_at === null)?.member_id ?? []);
      sender.last_command_at = stamp();
      reply = { status: 'ok', event_id: push(target.room_id, sender, 'message', { text, recipients: to, mentions: named, historical_mentions: [] }, ids).event_id };
      break;
    }
    default: fail('unknown_operation', operation);
  }
  replies.set(request, reply);
  return reply;
}

const mockSandboxes: { harness: string; name: string; root: string; store: string; exe: string; package: string; created: string }[] = [];

const delay = <T>(value: T, ms = 120) => new Promise<T>((resolve) => setTimeout(() => resolve(structuredClone(value)), ms));

let write: WriteOperation | null = null;
let writeSequence = 0;

function startWrite(kind: WriteOperation['kind'], args: Pick<HopArgs, 'dest'>, duration: number): WriteOperation {
  if (write) return write;
  const operation = { id: ++writeSequence, kind, status: 'running' } as const;
  write = operation;
  setTimeout(() => { write = { ...operation, status: 'completed', receipt: receipt(args) }; }, duration);
  return operation;
}

export async function answer(command: string, args: Record<string, unknown> = {}): Promise<unknown> {
  const key = String(args.installation ?? args.source ?? 'claude_code');
  const installation = installations.find((item) => item.key === key) ?? installations[0];
  switch (command) {
    case 'startup': return delay({ turns: 50, scan: [], version: '0.1.0 · preview' }, 0);
    case 'installations': {
      const scan = (args.scan as string[] | undefined) ?? [];
      if (scan.length && !installations.includes(JANURA)) { installations.push(JANURA); sessions[JANURA.key] = []; }
      const list = installations.map((item) => ({ ...item, count: item.error ? 0 : (sessions[item.key] ?? []).length }));
      return delay({ installations: list, scans: scan.map((dir) => ({ dir, added: 1, stopped_early: false })) }, 300);
    }
    case 'sessions': return delay(sessions[key] ?? []);
    case 'transcript': return delay({ ...transcript, hidden_turns: Number(args.turns) === 0 ? 0 : transcript.hidden_turns }, 200);
    // A live session changes every ten seconds.
    case 'session_stamp': return delay(String(Math.floor(Date.now() / 10_000)), 20);
    case 'raw': {
      const entry = rows.find((row) => row.index === Number(args.offset));
      return delay({ total: 12, items: [{ index: Number(args.offset), entry_id: entry?.entry_id ?? 'e', kind: 'step', truncated: false,
        raw: [{ type: 'assistant', uuid: entry?.entry_id, timestamp: entry?.time, message: { role: 'assistant', content: [{ type: 'text', text: entry?.text ?? '' }] } }] }] });
    }
    case 'hop_options': {
      const source = sessions[key]?.find((row) => row.id === args.id);
      const kids = (sessions[key] ?? []).filter((row) => row.parent === args.id);
      return delay({
        source_label: installation.label, title: source?.title ?? String(args.id),
        destinations: installations.filter((item) => item.key !== key && item.destination).map((item) => ({ key: item.key, label: item.label, default_shell: item.harness === 'codex' ? 'powershell' : 'posix', model: item.harness === 'codex' ? 'gpt-5.2-codex' : undefined })),
        compactions: ['none', 'portable', 'summary', 'native'], intents: ['generic', 'analyze', 'brainstorm', 'continue', 'execute', 'plan', 'review'],
        subagents: kids.filter((k) => k.parent_kind?.kind === 'spawn').map((k) => k.title ?? k.id),
        forks: kids.filter((k) => k.parent_kind?.kind === 'fork').map((k) => k.title ?? k.id),
        all_branches_supported: installation.harness === 'codex', live: source?.live,
      });
    }
    case 'write_status': return write;
    case 'acknowledge_write':
      if (write && write.id === args.id && write.status !== 'running') write = null;
      return;
    case 'hop': return startWrite('hop', args.args as HopArgs, 900);
    case 'import': return startWrite('import', { dest: String(args.into) }, 700);
    case 'export': return delay({ path: String(args.to ?? `C:\\work\\mukade\\mukade-data\\exports\\claude_code-${args.id}.json`), messages: 42 });
    case 'resume': {
      if (installation.harness === 'deepseek_harness') return delay({ launched: false, refusal: `DeepSeek Harness has no terminal UI to open a session in; continue it with a task: dsh --profile headless --session-id ${args.id} "<task>"`, command: ['dsh', '--profile', 'headless', '--session-id', String(args.id), '<task>'] });
      return delay({ launched: true, command: ['claude', '--resume', String(args.id)], cwd: project });
    }
    case 'work': return delay(workOf(sessions[key]?.find((row) => row.id === args.id)?.cwd), 350);
    case 'handoff_options': {
      const source = sessions[key]?.find((row) => row.id === args.id);
      if (!source?.cwd) return delay({ refusal: 'no folder: nothing to start a harness in', destinations: [], note: '', cwd: null } satisfies HandoffOptions);
      return delay({
        destinations: installations.filter((item) => item.harness === 'claude_code' || item.harness === 'codex').map((item) => ({ key: item.key, label: item.label })),
        note: handoffNote(source), cwd: source.cwd,
      } satisfies HandoffOptions, 250);
    }
    case 'handoff': {
      const source = sessions[key]?.find((row) => row.id === args.id);
      const dest = installations.find((item) => item.key === args.dest);
      return delay({
        launched: true, command: [dest?.harness === 'codex' ? 'codex' : 'claude', 'Read the note first.'],
        note_path: `C:\\Users\\dev\\mukade-data\\handoffs\\${String(args.id).slice(0, 8)}.md`, cwd: source?.cwd ?? project,
      } satisfies HandedOff, 300);
    }
    case 'pick': return delay(args.kind === 'folder' ? 'D:\\apps' : args.kind === 'save' ? `C:\\Users\\me\\Desktop\\${args.name}` : 'C:\\work\\mukade\\mukade-data\\exports\\claude_code-5f2c9a1e.json', 50);
    case 'chat_status': return delay(chatRunning ? { state: 'running', endpoint: 'http://127.0.0.1:47631/mcp', data_dir: 'C:\\work\\mukade\\mukade-data' } : { state: 'stopped', data_dir: 'C:\\work\\mukade\\mukade-data' }, 60);
    case 'chat_rooms': return delay({ items: chatRooms.map(roomNow), next_cursor: null });
    case 'chat_members': return delay({ items: chatMembers.filter((one) => one.room_id === args.roomId), next_cursor: null });
    case 'chat_ledger': {
      const text = String(args.text ?? '').toLowerCase();
      const items = chatEvents.filter((one) => one.room_id === args.roomId);
      return delay({ items: text ? items.filter((one) => String(one.payload.text ?? '').toLowerCase().includes(text)) : items, next_cursor: null });
    }
    case 'chat_write': return delay(chatWrite(String(args.operation), args.args as Record<string, unknown>), 160);
    case 'chat_start': chatRunning = true; return delay({ status: 'running' }, 400);
    case 'chat_stop': chatRunning = false; return delay({ status: 'stopped' }, 300);
    case 'chat_launch': {
      const room = chatRooms.find((one) => one.name === args.room);
      if (room) {
        window.setTimeout(() => {
          const joined: Member = { ...member(room.room_id, `m${uuid().slice(0, 6)}`, `${String(args.harness).replace('_code', '')}-${chatMembers.length}`, String(args.harness), String(args.role), 0), joined_at: stamp() };
          chatMembers.push(joined);
          chatEvents.push({ room_id: room.room_id, event_id: String(++lastEvent), timestamp: stamp(), actor: actor(joined), kind: 'join', payload: { harness: joined.harness, role: joined.role }, recipient_ids: [] });
        }, 1500);
      }
      return delay(['mukade.exe', 'chat', 'launch', String(args.harness), '--room', String(args.room), '--role', String(args.role), '--listen', String(args.listen)]);
    }
    case 'harness_inventory': return delay(installations.slice(0, 6).map((item, i) => ({
      id: item.harness, display: item.label, status: i === 4 ? 'missing' : 'installed', source: i === 1 ? 'the catalog\'s location' : 'PATH',
      binary: i === 4 ? undefined : `C:\\Users\\me\\AppData\\Roaming\\npm\\${item.harness}.cmd`, version: i === 4 ? undefined : `1.${i}.0`,
      store: `C:\\Users\\me\\.${item.harness}`, store_exists: i !== 4, binary_env: `MUKADE_${item.harness.toUpperCase()}_EXE`, store_env: `${item.harness.toUpperCase()}_HOME`,
      destination: true, install_command: item.harness === 'hermes' ? undefined : `npm install -g ${item.harness}`, sandbox_package: item.harness === 'hermes' ? undefined : `@example/${item.harness}`,
    })), 300);
    case 'system_roots': return delay(['C:\\Users\\me', 'C:\\Program Files', 'D:\\apps', 'D:\\Software']);
    case 'scan_folder': return delay({ dir: String(args.dir), visited: 120, truncated: false,
      found: String(args.dir) === 'D:\\apps' ? [{ harness: 'opencode', store: JANURA.store, engine: JANURA.engine }] : [] }, 400);
    case 'install_system': return delay(['powershell', '-NoExit', '-Command', `install ${args.harness}`]);
    case 'sandboxes': return delay(mockSandboxes);
    case 'sandbox_install': {
      const made = { harness: String(args.harness), name: String(args.name), root: `C:\\work\\mukade\\mukade-data\\sandboxes\\${args.harness}-${args.name}`,
        store: `C:\\work\\mukade\\mukade-data\\sandboxes\\${args.harness}-${args.name}\\home`, exe: `${args.harness}.cmd`, package: `@example/${args.harness}`, created: new Date().toISOString() };
      mockSandboxes.push(made);
      return delay(made, 1500);
    }
    case 'sandbox_forget': mockSandboxes.splice(mockSandboxes.findIndex((one) => one.root === args.root), 1); return delay(null);
    case 'sandbox_run': return delay(['run']);
    case 'quit': return undefined;
    default: throw { kind: 'failed', message: `no mock for ${command}` };
  }
}
