// F3, F12: the session tree of the selected installation and folder, children and forks under
// their parent, each parent folding with space or its caret.

import './sessions.css';
import { fill, h, icon } from './dom.ts';
import { listbox, reveal, type ListRow } from './lists.ts';
import { currentInstallation, state } from './state.ts';
import { mark, shortTime, visible, type Place } from './tree.ts';

export interface SessionsActions {
  pick(id: string): void;
  open(id: string): void;
  fold(id: string): void;
  focus(): void;
}

export function visiblePlaces(): Place[] {
  return visible(state.rows, state.folder, state.query, state.collapsed);
}

function cells(place: Place, actions: SessionsActions): Node[] {
  const { row, depth, children } = place;
  const out: Node[] = [];
  const searching = Boolean(state.query.trim());
  const open = searching || !state.collapsed.has(row.id);
  const caret = h('span', { class: 'caret', 'aria-hidden': 'true' }, children ? (open ? '▾' : '▸') : '');
  if (children && !searching) caret.addEventListener('mousedown', (event) => { event.stopPropagation(); event.preventDefault(); actions.fold(row.id); });
  out.push(caret);
  const marker = mark(row);
  if (marker?.kind === 'fork') out.push(h('span', { class: 'fork' }, icon('fork'), 'fork'));
  else if (marker) out.push(h('span', { class: marker.kind }, marker.text));
  out.push(h('span', { class: 'grow' }, row.title ?? `${row.id.slice(0, 8)} (no title)`));
  if (children && !open) out.push(h('span', { class: 'num' }, `${children} inside`));
  if (row.live) out.push(h('span', { class: 'tag live' }, 'live'));
  if (row.archived) out.push(h('span', { class: 'tag' }, 'archived'));
  if (row.unreadable) out.push(h('span', { class: 'tag bad', title: row.unreadable }, 'unreadable'));
  out.push(h('span', { class: 'time', title: row.updated ?? '' }, shortTime(row.updated)));
  if (depth) (out[0] as HTMLElement).style.marginLeft = `${Math.min(depth, 6) * 18}px`;
  return out;
}

export function renderSessions(root: HTMLElement, actions: SessionsActions): void {
  const installation = currentInstallation();
  const places = visiblePlaces();
  const folder = state.folder === null ? 'All folders' : state.folder || 'Unknown folder';
  const head = h('div', { class: 'column-head' },
    h('h2', {}, 'Sessions'), h('small', { title: folder }, folder), h('span', { class: 'spring' }),
    h('small', {}, state.query.trim() ? `${places.length} match` : String(places.length)));
  let body: Node;
  if (!installation) body = h('div', { class: 'empty' }, h('p', {}, 'Reading the stores…'));
  else if (installation.error) body = h('div', { class: 'empty' }, h('h2', {}, 'This store could not be read'), h('p', {}, installation.error));
  else if (!places.length) {
    body = h('div', { class: 'empty' }, state.query.trim()
      ? h('p', {}, `No session matches “${state.query.trim()}”.`)
      : h('p', {}, `${installation.label} has no sessions${state.folder !== null ? ' in this folder' : ''}.`));
  } else {
    const rows: ListRow[] = places.map((place) => ({
      id: place.row.id,
      class: [place.row.title ? '' : 'untitled', place.depth ? 'child' : ''].join(' ').trim() || undefined,
      title: place.row.cwd ? `${place.row.title ?? place.row.id}\n${place.row.cwd}` : undefined,
      cells: cells(place, actions),
    }));
    body = listbox('Sessions', rows, state.selected, (id) => { actions.focus(); actions.pick(id); }, actions.open);
  }
  fill(root, head, h('div', { class: 'column-body' }, body));
  reveal(root.querySelector('.list'));
}
