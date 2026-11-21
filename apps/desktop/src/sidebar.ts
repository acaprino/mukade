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
  /** Enter on a row: on to the sessions. */
  focusSessions(): void;
}

export function folderKey(cwd: string | null): string {
  return cwd === null ? '*all' : cwd === '' ? '*unknown' : cwd;
}

export function renderSidebar(root: HTMLElement, actions: SidebarActions): void {
  document.getElementById('installation-count')!.textContent = String(state.installations.length);
  const standard = state.installations.filter((item) => !item.scanned);
  const scanned = state.installations.filter((item) => item.scanned);
  const row = (item: (typeof state.installations)[number]): ListRow => ({
    id: item.key,
    title: item.error ? `${item.label}: ${item.error}` : item.store,
    cells: [
      h('span', { class: 'im-avatar im-rail-item', 'aria-hidden': 'true' }, item.label.slice(0, 2)),
      h('span', { class: 'grow im-row-text' }, item.label),
      item.scanned ? h('span', { class: 'tag im-chip' }, 'scanned') : null,
      item.error ? h('span', { class: 'tag im-chip bad' }, 'error') : h('span', { class: 'num im-num' }, String(item.count)),
    ].filter(Boolean) as Node[],
  });
  const selected = state.installation;
  const installationsBox = h('div', { class: 'pane', 'data-pane': 'installations' },
    h('div', { class: 'pane-body im-panel-body' },
      listbox('Installations', standard.map(row), selected, actions.pickInstallation, { onOpen: actions.focusSessions }),
      scanned.length ? h('p', { class: 'label' }, 'Found by scan') : null,
      scanned.length ? listbox('Scanned installations', scanned.map(row), selected, actions.pickInstallation, { onOpen: actions.focusSessions }) : null,
    ));

  const current = state.installations.find((item) => item.key === state.installation);
  const list = folders(state.rows);
  const folderRows: ListRow[] = list.map((folder) => ({
    id: folderKey(folder.cwd),
    title: folder.cwd || undefined,
    class: folder.cwd === '' ? 'untitled' : undefined,
    cells: [h('span', { class: 'im-avatar im-rail-item', 'aria-hidden': 'true' }, folder.label.slice(0, 2)), h('span', { class: 'grow im-row-text' }, folder.label), h('span', { class: 'num im-num' }, String(folder.count))],
  }));
  const foldersBox = h('div', { class: 'pane', 'data-pane': 'folders' },
    h('div', { class: 'column-head im-panel-header' }, h('h2', {}, 'Folders'), h('span', { class: 'spring' }), h('small', {}, current?.label ?? '')),
    h('div', { class: 'pane-body im-panel-body' },
      current?.error ? h('p', { class: 'note err' }, current.error) : null,
      listbox('Folders', folderRows, folderKey(state.folder), (id) => {
        actions.pickFolder(list.find((folder) => folderKey(folder.cwd) === id)?.cwd ?? null);
      }, { onOpen: actions.focusSessions }),
    ));
  fill(root, installationsBox, foldersBox);
  root.querySelectorAll('.list').forEach(reveal);
}
