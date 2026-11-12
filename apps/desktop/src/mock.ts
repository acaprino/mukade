// What the commands answer outside Tauri: `npm run dev` in a browser and the Playwright smoke
// test. Made-up sessions shaped like real listings; nothing here touches a store.

import type { ChatEvent, HopArgs, Installation, Member, Receipt, Room, SessionRow, Transcript } from './types.ts';

const now = Date.now();
const ago = (minutes: number) => new Date(now - minutes * 60_000).toISOString();

const installations: Installation[] = [
  { key: 0, harness: 'claude_code', label: 'Claude Code', scanned: false, writable: true, count: 0 },
  { key: 1, harness: 'codex', label: 'Codex', scanned: false, writable: true, count: 0, error: 'state_5.sqlite: database is locked' },
  { key: 2, harness: 'opencode', label: 'OpenCode', scanned: false, writable: true, count: 0 },
  { key: 3, harness: 'pi', label: 'Pi', scanned: false, writable: true, count: 0 },
  { key: 4, harness: 'hermes', label: 'Hermes', scanned: false, writable: true, count: 0 },
  { key: 5, harness: 'deepseek_harness', label: 'DeepSeek Harness', scanned: false, writable: true, count: 0 },
];

const project = 'C:\\work\\mukade';
const row = (id: string, title: string, minutes: number, extra: Partial<SessionRow> = {}): SessionRow => ({
  id, title, cwd: project, updated: ago(minutes), archived: false, path: `C:\\Users\\me\\.claude\\projects\\C--work-mukade\\${id}.jsonl`, ...extra,
});
const main = '5f2c9a1e-8b47-4d02-9c3e-71a0de4416f2';
const sessions: Record<number, SessionRow[]> = {
  0: [
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
  2: [row('ses_3f1a9c', 'Release notes draft', 45, { cwd: 'C:\\work\\site', path: 'opencode.db#ses_3f1a9c' })],
  3: [row('pi-2026-11-10', 'Pi compaction check', 60 * 48, { path: 'C:\\Users\\me\\.pi\\agent\\sessions\\pi-2026-11-10.jsonl' })],
  4: [],
  5: [row('dsh-41c2', 'Headless review', 60 * 30, { cwd: 'C:\\work\\site' })],
};

const transcript: Transcript = {
  title: 'Hop carries subagents to Codex',
  hidden_turns: 3,
  total_turns: 5,
  entries: [
    { id: 'u1', kind: 'user', time: ago(14), records: 1, blocks: [{ role: 'user', text: 'The hop to Codex leaves the subagents behind. Carry them across with their parent and say in the receipt what could not travel.' }] },
    { id: 's1', kind: 'step', time: ago(14), records: 2, blocks: [
      { role: 'reasoning', text: 'A Codex rollout is flat, so each subagent has to become a thread of its own that names its parent. First find where the writer decides the thread id.' },
      { role: 'assistant', text: 'I will read how the Codex writer lays out a rollout, then run the children tests.' },
      { role: 'tool', name: 'Read', text: '{"file_path":"src/writers/codex.rs"}' },
      { role: 'tool', name: 'Bash', text: '{"command":"cargo test -p mukade children -- --nocapture","description":"Run the children tests","timeout":120000,"env":{"RUST_LOG":"info","MUKADE_DATA_DIR":"C:\\\\work\\\\mukade\\\\mukade-data"}}' },
    ] },
    { id: 'r1', kind: 'results', time: ago(13), records: 1, blocks: [
      { role: 'result', text: '//! Codex writer\nuse crate::model::Session;\n… 214 lines' },
      { role: 'result', text: 'running 14 tests\ntest children::spawn_keeps_agent_type ... ok\ntest children::fork_stays_a_fork ... ok\ntest result: ok. 14 passed; 0 failed' },
    ] },
    { id: 'c1', kind: 'compaction', time: ago(8), records: 1, blocks: [{ role: 'compaction', text: 'The conversation so far: subagent sessions are read with their parent; the Codex writer gives each a thread of its own; the fork cannot travel as a child.' }] },
    { id: 's2', kind: 'step', time: ago(3), records: 1, blocks: [{ role: 'assistant', text: 'Subagents now travel as threads that name their parent. The fork has no place in a Codex thread tree, so it is written as a separate session and listed under losses.' }] },
    { id: 'u2', kind: 'user', time: ago(2), records: 1, blocks: [{ role: 'user', text: 'Good. Show the child lines in the hop result too.' }] },
  ],
};

const receipt = (args: { dest: number }): Receipt => ({
  dest_id: '0199a7c2-4e1b-7d30-b6f4-2c8e5a90d113',
  dest_label: installations[args.dest]?.label ?? 'Codex',
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

const rooms: Room[] = [
  { room_id: 'r-hop', name: 'hop-review', opened_at: ago(40), closed_at: null, opened_by: 'claude-a', kept: true, members: 3, events: 9, last_activity: ago(1), revision: '9' },
  { room_id: 'r-spec', name: 'desktop-spec', opened_at: ago(300), closed_at: null, opened_by: 'claude-a', kept: true, members: 2, events: 41, last_activity: ago(60), revision: '41' },
  { room_id: 'r-codex', name: 'codex-code-mode', opened_at: ago(3000), closed_at: ago(2900), opened_by: 'codex-b', kept: true, members: 2, events: 9, last_activity: ago(2900), revision: '9' },
];

const members: Member[] = [
  { room_id: 'r-hop', member_id: 'm1', name: 'claude-a', harness: 'claude_code', role: 'lead', joined_at: ago(40), left_at: null, last_command_at: ago(2), session: { harness: 'claude_code', id: main, cwd: project } },
  { room_id: 'r-hop', member_id: 'm2', name: 'codex-b', harness: 'codex', role: 'peer', joined_at: ago(38), left_at: null, last_command_at: ago(3), session: { harness: 'codex', id: '0199a7c2-4e1b-7d30-b6f4-2c8e5a90d113', cwd: project } },
  { room_id: 'r-hop', member_id: 'm3', name: 'opencode-c', harness: 'opencode', role: 'observer', joined_at: ago(30), left_at: null, last_command_at: ago(1), session: null },
];

const actor = (member: Member) => ({ type: 'member' as const, member_id: member.member_id, name: member.name, role: member.role });
const message = (id: number, member: Member, minutes: number, text: string, to: string[] = [], mentions: string[] = []): ChatEvent => ({
  room_id: 'r-hop', event_id: String(id), timestamp: ago(minutes), actor: actor(member), kind: 'message',
  payload: { text, recipients: to, mentions, historical_mentions: [] }, recipient_ids: [],
});
const ledger: ChatEvent[] = [
  { room_id: 'r-hop', event_id: '1', timestamp: ago(40), actor: actor(members[0]), kind: 'open', payload: { name: 'hop-review' }, recipient_ids: [] },
  { room_id: 'r-hop', event_id: '2', timestamp: ago(38), actor: actor(members[1]), kind: 'join', payload: { harness: 'codex', role: 'peer' }, recipient_ids: [] },
  message(3, members[0], 12, 'The hop wrote three Codex threads. @codex-b open 0199a7c2 and check that both subagent threads name it as their parent.', ['codex-b'], ['codex-b']),
  { room_id: 'r-hop', event_id: '4', timestamp: ago(11), actor: actor(members[1]), kind: 'observe', payload: {}, recipient_ids: ['m1'] },
  message(5, members[1], 9, 'Both name it. The Explore thread keeps its agent type; the Plan thread lost its title and shows the first user line instead.'),
  message(6, members[0], 8, 'That is a loss the receipt should list. @opencode-c does the same happen on your side?', ['opencode-c'], ['opencode-c']),
  { room_id: 'r-hop', event_id: '7', timestamp: ago(7), actor: actor(members[1]), kind: 'signal', payload: { instruction_event_id: '3', signal: 'done', reason: null }, recipient_ids: ['m1'] },
  message(8, members[2], 1, 'No. OpenCode keeps the title of a child session, so nothing to list there.'),
];

const delay = <T>(value: T, ms = 120) => new Promise<T>((resolve) => setTimeout(() => resolve(structuredClone(value)), ms));

export async function answer(command: string, args: Record<string, unknown> = {}): Promise<unknown> {
  const key = Number(args.installation ?? args.source ?? 0);
  switch (command) {
    case 'app_info': return delay({ name: 'mukade', version: '0.1.0 · preview', turns: 50, scan: [], platform: 'browser' }, 0);
    case 'installations':
    case 'scan': {
      const list = installations.map((item) => ({ ...item, count: item.error ? 0 : (sessions[item.key] ?? []).length }));
      if (command === 'scan' && !list.some((item) => item.scanned)) {
        list.push({ key: 6, harness: 'opencode', label: 'OpenCode · JanuraCode', store: 'D:\\apps\\JanuraCode\\data\\share\\opencode\\opencode.db', engine: 'D:\\apps\\JanuraCode\\resources\\opencode-cli\\opencode.exe', scanned: true, writable: true, count: 0 });
        installations.push(list[6]);
        sessions[6] = [];
      }
      return delay({ installations: list, added: command === 'scan' ? [6] : [], stopped_early: false, status: command === 'scan' ? '1 installation(s) added from D:\\apps · 1 store(s) could not be read' : '1 store(s) could not be read' }, 300);
    }
    case 'sessions': return delay(sessions[key] ?? []);
    case 'transcript': return delay({ ...transcript, entries: Number(args.turns) === 0 ? transcript.entries : transcript.entries, hidden_turns: Number(args.turns) === 0 ? 0 : transcript.hidden_turns }, 200);
    case 'raw': return delay([{ type: 'assistant', uuid: String(args.entryId), timestamp: ago(14), message: { role: 'assistant', content: [{ type: 'text', text: 'A native record, verbatim.' }] } }]);
    case 'hop_options': {
      const source = sessions[key]?.find((row) => row.id === args.id);
      const kids = (sessions[key] ?? []).filter((row) => row.parent === args.id);
      return delay({
        source_label: installations[key].label, title: source?.title ?? String(args.id),
        destinations: installations.filter((item) => item.key !== key && item.writable).map((item) => ({ key: item.key, label: item.label, default_shell: item.harness === 'codex' ? 'powershell' : 'posix' })),
        compactions: ['none', 'portable', 'summary'],
        subagents: kids.filter((k) => k.parent_kind?.kind === 'spawn').map((k) => k.title ?? k.id),
        forks: kids.filter((k) => k.parent_kind?.kind === 'fork').map((k) => k.title ?? k.id),
        all_branches_supported: installations[key].harness === 'codex', live: source?.live,
      });
    }
    case 'hop': return delay(receipt(args.args as HopArgs), 900);
    case 'import': return delay(receipt({ dest: Number(args.into) }), 700);
    case 'export': return delay({ path: String(args.to ?? `C:\\work\\mukade\\mukade-data\\exports\\claude_code-${args.id}.json`), messages: 42 });
    case 'resume': {
      const harness = installations[key].harness;
      if (harness === 'deepseek_harness') return delay({ launched: false, refusal: `DeepSeek Harness has no terminal UI to open a session in; continue it with a task: dsh --profile headless --session-id ${args.id} "<task>"`, command: [] });
      return delay({ launched: true, command: ['claude', '--resume', String(args.id)], cwd: project });
    }
    case 'pick': return delay(args.kind === 'folder' ? 'D:\\apps' : args.kind === 'save' ? `C:\\Users\\me\\Desktop\\${args.name}` : 'C:\\work\\mukade\\mukade-data\\exports\\claude_code-5f2c9a1e.json', 50);
    case 'chat_status': return delay({ state: 'running', endpoint: 'http://127.0.0.1:47631/mcp', data_dir: 'C:\\work\\mukade\\mukade-data' }, 60);
    case 'chat_rooms': return delay({ items: rooms, next_cursor: null });
    case 'chat_members': return delay({ items: members.filter((m) => m.room_id === args.roomId || args.roomId !== 'r-hop' && m.member_id !== 'm3'), next_cursor: null, high_water: '8' });
    case 'chat_ledger': {
      const text = String(args.text ?? '').toLowerCase();
      const items = args.roomId === 'r-hop' ? ledger : ledger.slice(0, 3).map((e) => ({ ...e, room_id: String(args.roomId) }));
      return delay({ items: text ? items.filter((e) => String(e.payload.text ?? '').toLowerCase().includes(text)) : items, next_cursor: null, high_water: '8' });
    }
    case 'quit': return undefined;
    default: throw { kind: 'failed', message: `no mock for ${command}` };
  }
}
