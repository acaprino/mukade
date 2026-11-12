// F1, F2: the installations (the six standard stores, then the scanned ones) and the folder
// filter of the selected one.

import './sidebar.css';
import { fill, h } from './dom.ts';
import { listbox, reveal, type ListRow } from './lists.ts';
import { state } from './state.ts';
import { folders } from './tree.ts';

export interface SidebarActions {
  pickInstallation(key: string): void;
  pickFolder(cwd: string | null): void;
  focus(column: 'installations' | 'folders'): void;
}

export function folderKey(cwd: string | null): string {
  return cwd === null ? '*all' : cwd === '' ? '*unknown' : cwd;
}

export function renderSidebar(root: HTMLElement, actions: SidebarActions): void {
  const standard = state.installations.filter((item) => !item.scanned);
  const scanned = state.installations.filter((item) => item.scanned);
  const row = (item: (typeof state.installations)[number]): ListRow => ({
    id: item.key,
    title: item.error ? `${item.label}: ${item.error}` : item.store,
    cells: [
      h('span', { class: 'grow' }, item.label),
      item.scanned ? h('span', { class: 'tag' }, 'scanned') : null,
      item.error ? h('span', { class: 'tag bad' }, 'error') : h('span', { class: 'num' }, String(item.count)),
    ].filter(Boolean) as Node[],
  });
  const selected = state.installation;
  const pick = (id: string) => { actions.focus('installations'); actions.pickInstallation(id); };
  const installationsBox = h('div', { class: `pane${state.focus === 'installations' ? ' focused' : ''}`, 'data-pane': 'installations' },
    h('div', { class: 'column-head' }, h('h2', {}, 'Installations'), h('span', { class: 'spring' }), h('small', {}, String(state.installations.length))),
    h('div', { class: 'pane-body' },
      listbox('Installations', standard.map(row), selected, pick),
      scanned.length ? h('p', { class: 'label' }, 'Found by scan') : null,
      scanned.length ? listbox('Scanned installations', scanned.map(row), selected, pick) : null,
    ));

  const current = state.installations.find((item) => item.key === state.installation);
  const list = folders(state.rows);
  const folderRows: ListRow[] = list.map((folder) => ({
    id: folderKey(folder.cwd),
    title: folder.cwd || undefined,
    class: folder.cwd === '' ? 'untitled' : undefined,
    cells: [h('span', { class: 'grow' }, folder.label), h('span', { class: 'num' }, String(folder.count))],
  }));
  const foldersBox = h('div', { class: `pane${state.focus === 'folders' ? ' focused' : ''}`, 'data-pane': 'folders' },
    h('div', { class: 'column-head' }, h('h2', {}, 'Folders'), h('span', { class: 'spring' }), h('small', {}, current?.label ?? '')),
    h('div', { class: 'pane-body' },
      current?.error ? h('p', { class: 'note err' }, current.error) : null,
      listbox('Folders', folderRows, folderKey(state.folder), (id) => {
        actions.focus('folders');
        actions.pickFolder(list.find((folder) => folderKey(folder.cwd) === id)?.cwd ?? null);
      }),
    ));
  fill(root, installationsBox, foldersBox);
  root.querySelectorAll('.list').forEach(reveal);
}
