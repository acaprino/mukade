// A keyboard-and-mouse list for one column: rows are options of a listbox, the column holds the
// focus and names the active row, so the arrows (main.ts) and the screen reader agree.

import './lists.css';
import { h } from './dom.ts';

export interface ListRow { id: string; cells: (Node | string)[]; class?: string; title?: string }

export function listbox(label: string, rows: ListRow[], selected: string | null, onPick: (id: string) => void, onOpen?: (id: string) => void): HTMLElement {
  const list = h('ul', { class: 'list', role: 'listbox', 'aria-label': label });
  for (const { id, cells, class: extra, title } of rows) {
    const item = h('li', {
      class: `row ${extra ?? ''}`.trim(), role: 'option', id: `opt-${cssId(label)}-${cssId(id)}`,
      'aria-selected': String(id === selected), 'data-id': id, title,
    }, ...cells);
    item.addEventListener('mousedown', (event) => { event.preventDefault(); onPick(id); });
    if (onOpen) item.addEventListener('dblclick', () => onOpen(id));
    list.append(item);
  }
  const active = rows.find((row) => row.id === selected);
  if (active) list.setAttribute('aria-activedescendant', `opt-${cssId(label)}-${cssId(active.id)}`);
  return list;
}

function cssId(text: string): string {
  return text.replace(/[^A-Za-z0-9_-]/g, '_');
}

/** Keep the selected row of `list` in sight. */
export function reveal(list: Element | null): void {
  list?.querySelector('[aria-selected="true"]')?.scrollIntoView({ block: 'nearest' });
}

/** The id `step` rows from `current` among `ids`, clamped. */
export function stepIn(ids: string[], current: string | null, step: number): string | null {
  if (!ids.length) return null;
  const index = current === null ? -1 : ids.indexOf(current);
  if (index < 0) return ids[step < 0 ? ids.length - 1 : 0];
  return ids[Math.max(0, Math.min(ids.length - 1, index + step))];
}
