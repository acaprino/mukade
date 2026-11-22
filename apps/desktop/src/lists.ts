// A keyboard-and-mouse list for one column: rows are options of a listbox, the list holds the
// focus (one tab stop) and names the active row, so the arrows and the screen reader agree. The
// list answers its own keys: ↑ ↓ j k, Home, End, Page Up, Page Down move the selection, Enter opens
// the row and Space folds it. They are claimed, so no command of the registry runs on them.

import './lists.css';
import { claimKey } from '@acaprino/imago/ui';
import { h } from './dom.ts';

export interface ListRow { id: string; cells: (Node | string)[]; class?: string; title?: string }

export interface ListKeys {
  /** Enter, or a double click. */
  onOpen?: (id: string) => void;
  /** Space. */
  onFold?: (id: string) => void;
}

export function listbox(label: string, rows: ListRow[], selected: string | null, onPick: (id: string) => void, { onOpen, onFold }: ListKeys = {}): HTMLElement {
  const list = h('ul', { class: 'list im-list', role: 'listbox', tabindex: '0', 'aria-label': label });
  for (const { id, cells, class: extra, title } of rows) {
    const item = h('li', {
      class: `row im-row im-row--1 ${extra ?? ''}`.trim(), role: 'option', id: `opt-${cssId(label)}-${cssId(id)}`,
      'aria-selected': String(id === selected), 'data-id': id, title,
    }, ...cells);
    item.addEventListener('mousedown', (event) => { event.preventDefault(); list.focus({ preventScroll: true }); onPick(id); });
    if (onOpen) item.addEventListener('dblclick', () => onOpen(id));
    list.append(item);
  }
  let current = selected;
  list.addEventListener('keydown', (event) => {
    if (event.defaultPrevented || event.isComposing || event.altKey || event.ctrlKey || event.metaKey) return;
    const ids = rows.map((row) => row.id);
    const steps: Record<string, number> = { ArrowDown: 1, j: 1, ArrowUp: -1, k: -1, PageDown: 10, PageUp: -10 };
    let next: string | null | undefined;
    if (event.key in steps) next = stepIn(ids, current, steps[event.key]);
    else if (event.key === 'Home') next = ids[0];
    else if (event.key === 'End') next = ids[ids.length - 1];
    else if (event.key === 'Enter' && onOpen && current !== null) onOpen(current);
    else if (event.key === ' ' && onFold && current !== null) onFold(current);
    else return;
    event.preventDefault();
    claimKey(event);
    if (next !== undefined && next !== null && next !== current) { current = next; onPick(next); }
  });
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
