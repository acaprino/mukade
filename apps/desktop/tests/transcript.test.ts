import { test } from 'node:test';
import assert from 'node:assert/strict';
import { toTranscript } from '../src/transcript.ts';
import type { RowView, TranscriptView } from '../src/types.ts';

const row = (index: number, role: RowView['role'], text: string, extra: Partial<RowView> = {}): RowView =>
  ({ index, entry_id: `e${index}`, role, text, truncated: false, text_chars: text.length, error: false, ...extra });
const view = (rows: RowView[], title?: string): TranscriptView => ({
  meta: { harness: 'claude_code', version: '1', id: 's', title, models: [], entries: rows.length, branches: 0 },
  rows, hidden_turns: 1, total_turns: 3,
});

test('rows of one native entry become one entry, in order, with its kind', () => {
  const t = toTranscript(view([
    row(0, 'user', 'hi'), row(0, 'file', 'image a.png'),
    row(1, 'reasoning', 'think'), row(1, 'assistant', 'ok'),
    row(1, 'tool', 'Bash', { tool: { name: 'Bash', call_id: 'c', input: '{"command":"ls"}', input_truncated: true } }),
    row(2, 'result', 'boom', { error: true }), row(2, 'result', 'fine', { truncated: true }),
    row(3, 'compaction', 'so far'),
  ], 'Title'), 'fallback');
  assert.equal(t.title, 'Title');
  assert.deepEqual(t.entries.map((e) => [e.index, e.kind, e.blocks.length]), [[0, 'user', 2], [1, 'step', 3], [2, 'results', 2], [3, 'compaction', 1]]);
  assert.deepEqual(t.entries[0].blocks[1], { role: 'attachment', text: 'image a.png' });
  assert.deepEqual(t.entries[1].blocks[2], { role: 'tool', name: 'Bash', text: '{"command":"ls"}', cut: true });
  assert.deepEqual(t.entries[2].blocks.map((b) => [b.role, b.cut]), [['error', undefined], ['result', true]]);
  assert.equal(t.hidden_turns, 1);
});

test('a session without a title takes the listing’s', () => {
  assert.equal(toTranscript(view([], '  '), 'from the listing').title, 'from the listing');
  assert.equal(toTranscript(view([]), 'x').entries.length, 0);
});
