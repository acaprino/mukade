// F3, F12: the session tree of the selected installation and folder, children and forks under
// their parent, each parent folding with space or its caret.

import './sessions.css';
import { setPanelState } from '@acaprino/imago/ui';
import { chip, fill, h, icon } from './dom.ts';
import { listbox, reveal, type ListRow } from './lists.ts';
import { currentInstallation, state } from './state.ts';
import { mark, shortTime, visible, type Place } from './tree.ts';
import { isHosted } from './work.ts';

export interface SessionsActions {
  pick(id: string): void;
  open(id: string): void;
  fold(id: string): void;
  reload(): void;
  retry(key: string): void;
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
  if (children && !open) out.push(h('span', { class: 'num im-num' }, `${children} inside`));
  if (row.live) out.push(chip('live', 'ok'));
  if (row.archived) out.push(chip('archived'));
  if (isHosted(row)) out.push(chip('hosted', '', row.unreadable));
  else if (row.unreadable) out.push(chip('unreadable', 'err', row.unreadable));
  out.push(h('span', { class: 'time', title: row.updated ?? '' }, shortTime(row.updated)));
  if (depth) { const indent = out[0] as HTMLElement; indent.classList.add('mk-indent'); indent.style.setProperty('--mk-depth', String(Math.min(depth, 6))); }
  return out;
}

export function renderSessions(root: HTMLElement, actions: SessionsActions): void {
  const installation = currentInstallation();
  const places = visiblePlaces();
  const folder = state.folder === null ? 'All folders' : state.folder || 'Unknown folder';
  // The header, the tools (the filter) and the body are the frame's and live as long as the window: what is typed in the
  // filter keeps its caret across a repaint.
  const head = root.querySelector<HTMLElement>(':scope > .im-panel-header')!;
  const body = root.querySelector<HTMLElement>(':scope > .im-panel-body')!;
  fill(head, h('h4', { class: 'im-panel-title' }, 'Sessions'), h('small', { title: folder }, folder), h('span', { class: 'im-spacer' }),
    h('small', {}, state.query.trim() ? `${places.length} match` : String(places.length)));
  fill(body);
  if (!installation) {
    if (state.inventoryRead.kind === 'error') setPanelState(root, { kind: 'error', text: state.inventoryRead.message, retry: { label: 'Reload stores', run: actions.reload } });
    else if (state.inventoryRead.kind === 'loading') setPanelState(root, { kind: 'loading', rows: 4 });
    else setPanelState(root, { kind: 'empty', text: 'No installations found.' });
  } else if (installation.error) {
    setPanelState(root, { kind: 'error', text: installation.error, retry: { label: 'Read the stores again', run: actions.reload } });
  } else if (state.sessionsRead.kind === 'error') {
    setPanelState(root, { kind: 'error', text: state.sessionsRead.message, retry: { label: 'Retry', run: () => actions.retry(installation.key) } });
  } else if (state.sessionsRead.kind === 'loading') {
    setPanelState(root, { kind: 'loading', rows: 4 });
  } else if (!places.length) {
    const text = state.query.trim() ? `No session matches “${state.query.trim()}”.`
      : `${installation.label} has no sessions${state.folder !== null ? ' in this folder' : ''}.`;
    setPanelState(root, { kind: 'empty', text });
  } else {
    setPanelState(root, { kind: 'ready' });
    const rows: ListRow[] = places.map((place) => ({
      id: place.row.id,
      class: [place.row.title ? '' : 'untitled', place.depth ? 'child' : ''].join(' ').trim() || undefined,
      title: place.row.cwd ? `${place.row.title ?? place.row.id}\n${place.row.cwd}` : undefined,
      cells: cells(place, actions),
    }));
    body.append(listbox('Sessions', rows, state.selected, actions.pick, { onOpen: actions.open, onFold: actions.fold }));
  }
  reveal(root.querySelector('.list'));
}
