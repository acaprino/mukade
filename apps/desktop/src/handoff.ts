// Start another harness in a session's folder, with a note on what the session was doing. The
// note is composed by the backend and edited here; nothing is written until Start.

import { api, errorText } from './api.ts';
import { button, caps, field, fill, h } from './dom.ts';
import { openModal } from './modal.ts';
import { state } from './state.ts';
import type { HandedOff, HandoffOptions, SessionRow } from './types.ts';
import { canStartIn } from './work.ts';

// Where the rule lives: work.ts holds it so the tests can read it without a window.
export { canStartIn };

/** `done` hears the result and the label of the harness chosen. */
export function openHandoff(row: SessionRow, done: (result: HandedOff, label: string) => void): void {
  const key = state.installation;
  if (key === null || !canStartIn(row)) return;
  void run(key, row, done);
}

async function run(key: string, row: SessionRow, done: (result: HandedOff, label: string) => void): Promise<void> {
  const modal = openModal({ title: 'Start in…', subtitle: `${row.title ?? row.id.slice(0, 8)} · reading the session…`, icon: 'play' });
  fill(modal.body, h('p', { class: 'muted' }, 'Composing the note…'));
  const close = button(['Close ', caps('Escape')], { onclick: () => modal.close() });
  let options: HandoffOptions;
  try {
    options = await api.handoffOptions(key, row.id);
  } catch (error) {
    fill(modal.body, h('p', { class: 'note err' }, errorText(error)));
    fill(modal.foot, h('span', { class: 'spring' }), close);
    return;
  }
  modal.setTitle('Start in…', row.title ?? row.id.slice(0, 8));
  if (options.refusal || !options.destinations.length) {
    fill(modal.body, h('p', { class: 'note warn' }, options.refusal ?? 'No installation mukade can start from a note.'));
    fill(modal.foot, h('span', { class: 'spring' }), close);
    return;
  }

  const dest = h('select', { id: 'handoff-dest' }) as HTMLSelectElement;
  for (const item of options.destinations) dest.append(h('option', { value: item.key }, item.label));
  const note = h('textarea', { id: 'handoff-note', rows: '12', spellcheck: 'false' }) as HTMLTextAreaElement;
  note.value = options.note;
  const problem = h('p', { class: 'note err', role: 'alert', hidden: true });
  const cwd = options.cwd ?? row.cwd;
  fill(modal.body, h('div', { class: 'mk-col' },
    field('Harness', 'handoff-dest', dest),
    field('Note · the first thing it reads', 'handoff-note', note),
    cwd ? h('p', { class: 'note' }, `It starts in ${cwd}. The note is saved in mukade’s data folder and the harness is told to read it.`) : null,
    problem));

  dest.focus({ preventScroll: true });
  let starting = false;
  const start = async (): Promise<void> => {
    if (starting) return;
    starting = true;
    const label = options.destinations.find((item) => item.key === dest.value)?.label ?? dest.value;
    problem.hidden = true;
    modal.lock(true);
    try {
      const result = await api.handoff(key, row.id, dest.value, note.value);
      modal.lock(false);
      modal.close();
      done(result, label);
    } catch (error) {
      modal.lock(false);
      starting = false;
      problem.textContent = errorText(error);
      problem.hidden = false;
      go.focus({ preventScroll: true });
    }
  };
  const go = button(['Start ', caps('Ctrl+Enter')], { class: 'primary', onclick: () => void start() });
  fill(modal.foot, h('span', { class: 'spring' }), button(['Cancel ', caps('Escape')], { onclick: () => modal.close() }), go);
  // Enter belongs to the note; ctrl+Enter starts from it.
  modal.root.onkeydown = (event) => {
    if (event.key === 'Enter' && (event.ctrlKey || event.metaKey) && !event.isComposing) { event.preventDefault(); void start(); }
  };
}
