// F4: the selected session's facts, its lineage, and what can be done with it. The transcript
// reader takes this column over when it opens (reader.ts).

import './detail.css';
import { button, fill, h, icon, kbd } from './dom.ts';
import { currentInstallation, selectedRow, state } from './state.ts';
import { ancestors, childrenText, kindText, mark } from './tree.ts';
import type { SessionRow } from './types.ts';

export interface DetailActions {
  open(): void;
  hop(): void;
  exportTo(saveAs: boolean): void;
  resume(): void;
  pick(id: string): void;
}

function kv(pairs: [string, Node | string | null | undefined][]): HTMLElement {
  const list = h('dl', { class: 'kv' });
  for (const [key, value] of pairs) {
    if (value === null || value === undefined || value === '') continue;
    list.append(h('dt', {}, key), h('dd', {}, value));
  }
  return list;
}

function lineageRow(row: SessionRow, depth: number, current: boolean, actions: DetailActions): HTMLElement {
  const marker = mark(row);
  const node = button([
    marker?.kind === 'fork' ? h('span', { class: 'fork' }, icon('fork'), 'fork') : marker ? h('span', { class: marker.kind }, marker.text) : null,
    h('span', { class: 'grow' }, row.title ?? row.id),
    current ? h('span', { class: 'num' }, 'this session') : null,
  ], { class: `lineage-row${current ? ' current' : ''}`, title: row.id });
  node.style.paddingLeft = `${10 + depth * 18}px`;
  if (!current) node.addEventListener('click', () => actions.pick(row.id));
  return node;
}

export function renderDetail(root: HTMLElement, actions: DetailActions): void {
  const installation = currentInstallation();
  const row = selectedRow();
  const head = h('div', { class: 'column-head' }, h('h2', {}, 'Session'), row ? h('small', { title: row.id }, row.id.slice(0, 8)) : null, h('span', { class: 'spring' }),
    row?.live ? h('span', { class: 'tag live' }, 'live') : null);
  if (!installation || !row) {
    const empty = h('div', { class: 'empty' });
    if (installation?.error) empty.append(h('h2', {}, installation.label), h('p', { class: 'note err' }, installation.error));
    else empty.append(h('p', {}, 'Pick a session to see its details.'));
    if (installation?.scanned) empty.append(kv([['store', installation.store], ['engine', installation.engine]]));
    fill(root, head, h('div', { class: 'column-body' }, empty));
    return;
  }
  const parentListed = row.parent !== undefined && state.rows.some((other) => other.id === row.parent);
  const unreadable = Boolean(row.unreadable);
  const actionsRow = h('div', { class: 'actions' },
    button(['Open transcript ', kbd('⏎')], { class: 'primary', disabled: unreadable, onclick: () => actions.open() }),
    button([icon('hop'), 'Hop ', kbd('h')], { disabled: unreadable, onclick: () => actions.hop() }),
    button([icon('export'), 'Export ', kbd('e')], { disabled: unreadable, onclick: () => actions.exportTo(false) }),
    button('Save as…', { disabled: unreadable, title: 'Export to a file you choose', onclick: () => actions.exportTo(true) }),
    button([icon('play'), 'Resume ', kbd('R')], { disabled: unreadable, onclick: () => actions.resume() }),
  );

  const chain = ancestors(state.rows, row.id);
  const children = state.rows.filter((other) => other.parent === row.id && other.id !== row.id);
  const lineage = chain.length || children.length
    ? h('div', { class: 'box lineage' }, h('p', { class: 'label' }, 'Lineage'),
      ...chain.map((id, depth) => lineageRow(state.rows.find((other) => other.id === id)!, depth, false, actions)),
      lineageRow(row, chain.length, true, actions),
      ...children.map((child) => lineageRow(child, chain.length + 1, false, actions)))
    : null;

  fill(root, head, h('div', { class: 'column-body' }, h('div', { class: 'pad' },
    h('h3', {}, row.title ?? row.id),
    actionsRow,
    row.unreadable ? h('p', { class: 'note err' }, `Cannot be read: ${row.unreadable}`) : null,
    kv([
      ['harness', installation.label],
      ['id', row.id],
      ['kind', kindText(row)],
      ['parent', row.parent === undefined ? null : parentListed ? row.parent : `${row.parent} (not listed)`],
      ['children', childrenText(state.rows, row.id)],
      ['cwd', row.cwd],
      ['updated', row.updated ? new Date(row.updated).toLocaleString() : null],
      ['archived', row.archived ? 'yes' : 'no'],
      ['live', row.live === undefined ? null : row.live ? 'yes' : 'no'],
      ['path', row.path],
      ['store', installation.store],
      ['engine', installation.engine],
    ]),
    lineage,
    row.live ? h('p', { class: 'note' }, `This session is open in ${installation.label}. A hop or export copies what is on disk now.`) : null,
  )));
}
