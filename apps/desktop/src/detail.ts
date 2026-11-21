// F4: the selected session's facts, its lineage, and what can be done with it. The transcript
// reader takes this column over when it opens (reader.ts).

import './detail.css';
import { api, errorText } from './api.ts';
import { button, fill, h, icon, caps } from './dom.ts';
import { canStartIn } from './handoff.ts';
import { currentInstallation, selectedRow, state } from './state.ts';
import { ancestors, childrenText, kindText, mark } from './tree.ts';
import type { RoomMember } from './chatindex.ts';
import type { SessionRow, Work } from './types.ts';
import { isHosted, renderWork, workCache } from './work.ts';

export interface DetailActions {
  open(): void;
  hop(): void;
  exportTo(saveAs: boolean): void;
  resume(): void;
  startIn(): void;
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
    current ? h('span', { class: 'num im-num' }, 'this session') : null,
  ], { class: `lineage-row${current ? ' current' : ''}`, title: row.id });
  node.style.paddingLeft = `${10 + depth * 18}px`;
  if (!current) node.addEventListener('click', () => actions.pick(row.id));
  return node;
}

type WorkRead = { token: number; key: string; id: string } & ({ kind: 'loading' } | { kind: 'ready'; work: Work } | { kind: 'error'; message: string });
/** The work of the selected row: read once per selection, kept while the panel is drawn again. */
let workRead: WorkRead | null = null;
/** Where the section is on the page now: an answer draws into it only while it is still there. */
let workHost: HTMLElement | null = null;
let workToken = 0;
const WORK_DELAY = 200;

function drawWork(host: HTMLElement): void {
  const read = workRead;
  if (!read) return;
  if (read.kind === 'loading') fill(host, h('p', { class: 'work-wait muted' }, 'Reading…'));
  else if (read.kind === 'error') fill(host, h('p', { class: 'note' }, read.message));
  else fill(host, renderWork(read.work));
}

function loadWork(key: string, row: SessionRow): void {
  if (workRead && workRead.key === key && workRead.id === row.id) return;
  const token = ++workToken;
  const folder = row.cwd ?? '';
  // A folder read a moment ago (by a session that shares it) is drawn at once, with no call.
  const cached = workCache.get(folder, Date.now());
  if (cached) {
    workRead = { token, key, id: row.id, kind: 'ready', work: cached };
    return;
  }
  workRead = { token, key, id: row.id, kind: 'loading' };
  const settle = (next: { kind: 'ready'; work: Work } | { kind: 'error'; message: string }): void => {
    // The selection moved on: this answer is for a row no longer shown.
    if (workRead?.token !== token) return;
    if (next.kind === 'ready') workCache.set(folder, next.work, Date.now());
    workRead = { token, key, id: row.id, ...next };
    if (workHost?.isConnected) drawWork(workHost);
  };
  // Moving through the list quickly asks for no repository it does not stop on.
  setTimeout(() => {
    if (workRead?.token !== token) return;
    api.work(key, row.id).then((work) => settle({ kind: 'ready', work }), (error) => settle({ kind: 'error', message: errorText(error) }));
  }, WORK_DELAY);
}

export function renderDetail(root: HTMLElement, actions: DetailActions): void {
  const installation = currentInstallation();
  const row = selectedRow();
  const head = h('div', { class: 'column-head im-panel-header' }, h('h2', {}, 'Session'), row ? h('small', { title: row.id }, row.id.slice(0, 8)) : null, h('span', { class: 'spring' }),
    row?.live ? h('span', { class: 'tag im-chip live' }, 'live') : null);
  if (!installation || !row) {
    const empty = h('div', { class: 'empty im-empty' });
    if (installation?.error) empty.append(h('h2', {}, installation.label), h('p', { class: 'note err' }, installation.error));
    else empty.append(h('p', {}, 'Pick a session to see its details.'));
    if (installation?.scanned) empty.append(kv([['store', installation.store], ['engine', installation.engine]]));
    fill(root, head, h('div', { class: 'column-body im-panel-body' }, empty));
    return;
  }
  const parentListed = row.parent !== undefined && state.rows.some((other) => other.id === row.parent);
  const unreadable = Boolean(row.unreadable);
  const actionsRow = h('div', { class: 'actions' },
    // One primary action: for a session that cannot be read it is Start in….
    button(['Open transcript ', caps('Enter')], { class: unreadable ? '' : 'primary', disabled: unreadable, onclick: () => actions.open() }),
    button([icon('hop'), 'Hop ', caps('H')], { disabled: unreadable, onclick: () => actions.hop() }),
    button([icon('export'), 'Export ', caps('E')], { disabled: unreadable, onclick: () => actions.exportTo(false) }),
    button('Save as…', { disabled: unreadable, title: 'Export to a file you choose', onclick: () => actions.exportTo(true) }),
    button([icon('play'), 'Resume ', caps('Shift+R')], { disabled: unreadable, onclick: () => actions.resume() }),
    canStartIn(row) ? button([icon('play'), 'Start in… ', caps('S')], { class: 'primary', title: 'Start another harness in this folder, with a note on what was done', onclick: () => actions.startIn() }) : null,
  );

  // The git work in the session's folder: read when the row is selected, drawn from what is held
  // when the panel is drawn again.
  let work: HTMLElement | null = null;
  if (row.cwd) {
    loadWork(installation.key, row);
    workHost = h('div', { class: 'work-host' });
    work = h('div', { class: 'box work' }, h('p', { class: 'label' }, 'Work on disk'), workHost);
    drawWork(workHost);
  } else {
    workRead = null;
    workHost = null;
  }

  const chain = ancestors(state.rows, row.id);
  const children = state.rows.filter((other) => other.parent === row.id && other.id !== row.id);
  const lineage = chain.length || children.length
    ? h('div', { class: 'box lineage' }, h('p', { class: 'label' }, 'Lineage'),
      ...chain.map((id, depth) => lineageRow(state.rows.find((other) => other.id === id)!, depth, false, actions)),
      lineageRow(row, chain.length, true, actions),
      ...children.map((child) => lineageRow(child, chain.length + 1, false, actions)))
    : null;

  fill(root, head, h('div', { class: 'column-body im-panel-body' }, h('div', { class: 'pad' },
    h('h3', {}, row.title ?? row.id),
    actionsRow,
    row.unreadable ? (isHosted(row)
      ? h('p', { class: 'note' }, 'Hosted by ChatGPT: the conversation is not on this machine; the work it left is in its folder')
      : h('p', { class: 'note err' }, `Cannot be read: ${row.unreadable}`)) : null,
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
    work,
    lineage,
    row.live ? h('p', { class: 'note' }, `This session is open in ${installation.label}. A hop or export copies what is on disk now.`) : null,
  )));
}

/** The rooms where a member is bound to the session: each opens the room at that member. */
export function inRoomsBox(found: RoomMember[], open: (roomId: string, memberId: string) => void): HTMLElement {
  return h('div', { class: 'box lineage in-rooms' }, h('p', { class: 'label' }, `In rooms · ${found.length}`),
    ...found.map(({ room, member }) => button([
      icon('chat'),
      h('span', { class: 'grow' }, room.name),
      h('span', { class: 'num im-num' }, `${member.name} · ${member.role}${member.left_at ? ' · left' : ''}${room.closed_at ? ' · closed' : ''}`),
    ], { class: 'lineage-row', title: 'Show the room at this member', onclick: () => open(room.room_id, member.member_id) })));
}
