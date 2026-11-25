import { test } from 'node:test';
import assert from 'node:assert/strict';
import { commandLine, eventLine, mergeEvents } from '../src/chatmodel.ts';
import type { ChatEvent } from '../src/types.ts';

const event = (id: string, kind: string, payload: Record<string, unknown> = {}, recipients: string[] = []): ChatEvent => ({
  room_id: 'r', event_id: id, timestamp: '2026-11-12T12:00:00Z', actor: { type: 'member', member_id: 'm1', name: 'claude-a', role: 'lead' },
  kind, payload, recipient_ids: recipients,
});

test('a message keeps its text, recipients and mentions; the rest are muted lines', () => {
  const names = new Map([['m2', 'codex-b']]);
  assert.deepEqual(eventLine(event('1', 'message', { text: 'hi @codex-b', recipients: ['codex-b'], mentions: ['codex-b'] }), names),
    { kind: 'message', text: 'hi @codex-b', to: ['codex-b'], mentions: ['codex-b'] });
  assert.equal(eventLine(event('2', 'observe', {}, ['m2']), names).text, 'claude-a observed codex-b');
  assert.equal(eventLine(event('3', 'signal', { signal: 'blocked', reason: 'no key' }), names).text, 'claude-a signalled blocked: no key');
  assert.equal(eventLine(event('4', 'join', { role: 'peer' }), names).kind, 'muted');
});

test('polled pages merge by event id, oldest first', () => {
  const merged = mergeEvents([event('2', 'message'), event('10', 'message')], [event('10', 'message', { text: 'edited' }), event('3', 'message')]);
  assert.deepEqual(merged.map((e) => e.event_id), ['2', '3', '10']);
  assert.equal(merged[2].payload.text, 'edited');
});

test('a receipt command reads as one line, quoting what needs it', () => {
  assert.equal(commandLine(['codex', 'exec', 'resume', 'abc', 'continue']), 'codex exec resume abc continue');
  assert.equal(commandLine(['claude', '--resume', 'id', 'C:\\Program Files\\x']), 'claude --resume id "C:\\Program Files\\x"');
});

import {
  harnessKey, instructions, linesUpTo, memberNameRefusal, mentionsIn, passes, presentAt, requestId, roomNameRefusal, roomNamed, sendRefusal,
} from '../src/chatmodel.ts';
import type { Member, Room } from '../src/types.ts';

const member = (id: string, name: string, role: string, joined = '2026-11-12T11:00:00Z', left: string | null = null): Member => ({
  room_id: 'r', member_id: id, name, harness: 'codex', role, joined_at: joined, left_at: left, last_command_at: null, session: null,
});
const lead = member('m1', 'claude-a', 'lead');
const worker = member('m2', 'codex-b', 'worker');
const peer = member('m3', 'opencode-c', 'peer', '2026-11-12T12:30:00Z');
const room = [lead, worker, peer];
const by = (who: Member, id: string, kind: string, payload: Record<string, unknown> = {}, recipients: string[] = []) =>
  ({ ...event(id, kind, payload, recipients), actor: { type: 'member' as const, member_id: who.member_id, name: who.name, role: who.role } });
const ledger = [
  by(lead, '1', 'message', { text: 'do it @codex-b', recipients: ['codex-b'] }),
  by(worker, '2', 'signal', { instruction_event_id: '1', signal: 'blocked', reason: 'no key' }, ['m1']),
  by(lead, '3', 'message', { text: 'hello everyone' }),
  by(worker, '4', 'signal', { instruction_event_id: '1', signal: 'done', reason: null }, ['m1']),
  by(lead, '5', 'message', { text: 'and this', recipients: ['codex-b'] }),
];

test('an instruction is a lead message to a worker; its last signal gives its state', () => {
  const owed = instructions(ledger, room);
  assert.deepEqual(owed.map((one) => [one.event.event_id, one.worker.name, one.state]), [['1', 'codex-b', 'done'], ['5', 'codex-b', 'waiting']]);
  assert.equal(instructions(ledger.slice(0, 2), room)[0].state, 'blocked');
});

test('the lines count each pair once per kind, a message to everyone goes to the hub, the filter hides events', () => {
  const all = { kinds: ['message', 'signal', 'system'] as const, members: [] };
  const lines = linesUpTo(ledger, ledger.length - 1, room, all);
  const line = (from: string, to: string, type: string) => lines.find((one) => [one.from, one.to].sort().join() === [from, to].sort().join() && one.type === type);
  assert.equal(line('m1', 'm2', 'msg')?.count, 2);
  assert.equal(line('m1', 'hub', 'msg')?.count, 1);
  assert.equal(line('m2', 'm1', 'blocked')?.count, 1);
  assert.equal(line('m2', 'm1', 'done')?.count, 1);
  assert.equal(linesUpTo(ledger, 0, room, all).length, 1);
  assert.equal(linesUpTo(ledger, ledger.length - 1, room, { kinds: ['signal'], members: [] }).every((one) => one.type !== 'msg'), true);
  assert.equal(passes(ledger[2], { kinds: ['message'], members: ['m3'] }, room), false);
  assert.equal(passes(ledger[0], { kinds: ['message'], members: ['m2'] }, room), true);
});

test('a member is drawn from its join on, and marked once it has left', () => {
  const gone = member('m4', 'pi-d', 'observer', '2026-11-12T11:00:00Z', '2026-11-12T11:30:00Z');
  const at = presentAt([...room, gone], '2026-11-12T12:00:00Z');
  assert.deepEqual(at.map((one) => [one.member.name, one.left]), [['claude-a', false], ['codex-b', false], ['pi-d', true]]);
});

test('the checks before a write: text, names, mentions, ids', () => {
  assert.ok(sendRefusal('   '));
  assert.ok(sendRefusal('x'.repeat(65_537)));
  assert.equal(sendRefusal('ok'), null);
  assert.equal(roomNameRefusal('design-review'), null);
  assert.ok(roomNameRefusal('Design'));
  assert.ok(roomNameRefusal('x'.repeat(65)));
  assert.ok(memberNameRefusal('all', room));
  assert.ok(memberNameRefusal('codex-b', room));
  assert.equal(memberNameRefusal('codex-b', [member('m2', 'codex-b', 'worker', undefined, '2026-11-12T12:00:00Z')]), null);
  assert.equal(memberNameRefusal('desk', room), null);
  assert.deepEqual(mentionsIn('ask @codex-b and @nobody, not a@b.c', room), ['codex-b']);
  assert.match(requestId(), /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/);
  assert.equal(harnessKey('claude'), 'claude_code');
  assert.equal(harnessKey('claude-code'), 'claude_code');
  const rooms = [{ room_id: 'a', name: 'x', closed_at: '2026-11-12T11:00:00Z' }, { room_id: 'b', name: 'x', closed_at: null }] as Room[];
  assert.equal(roomNamed(rooms, 'x')?.room_id, 'b');
  assert.equal(roomNamed(rooms.slice(0, 1), 'x')?.room_id, 'a');
});
