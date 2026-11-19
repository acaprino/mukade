// The backend's transcript rows (one per block) grouped into the reader's entries (one per native
// entry, in order). Pure, so tests/transcript.test.ts runs it in Node.

import type { Block, EntryView, RowView, Transcript, TranscriptView } from './types.ts';

function block(row: RowView): Block {
  switch (row.role) {
    case 'tool':
      return { role: 'tool', name: row.tool?.name ?? row.text, text: row.tool?.input ?? '', cut: row.tool?.input_truncated || undefined };
    case 'result':
      return { role: row.error ? 'error' : 'result', text: row.text, cut: row.truncated || undefined };
    case 'file':
      return { role: 'attachment', text: row.text };
    default:
      return { role: row.role, text: row.text, cut: row.truncated || undefined };
  }
}

function kindOf(rows: RowView[]): EntryView['kind'] {
  const roles = new Set(rows.map((row) => row.role));
  if (roles.has('user')) return 'user';
  if (roles.has('compaction')) return 'compaction';
  if (roles.has('result') && !roles.has('assistant') && !roles.has('tool') && !roles.has('reasoning')) return 'results';
  return 'step';
}

export function toTranscript(view: TranscriptView, fallbackTitle: string): Transcript {
  const entries: EntryView[] = [];
  let current: RowView[] = [];
  const flush = () => {
    if (!current.length) return;
    const first = current[0];
    entries.push({ id: first.entry_id, index: first.index, kind: kindOf(current), time: first.time, blocks: current.map(block) });
    current = [];
  };
  for (const row of view.rows) {
    if (current.length && current[0].index !== row.index) flush();
    current.push(row);
  }
  flush();
  return {
    title: view.meta.title?.trim() || fallbackTitle,
    meta: view.meta,
    entries,
    hidden_turns: view.hidden_turns,
    total_turns: view.total_turns,
  };
}
