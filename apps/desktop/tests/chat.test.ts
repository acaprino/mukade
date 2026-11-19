import { test } from 'node:test';
import assert from 'node:assert/strict';
import { eventLine, mergeEvents } from '../src/chatmodel.ts';
import { commandLine } from '../src/dom.ts';
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
