// The session tree, folders, children counts and search: pure functions over the listing, so
// `tests/tree.test.ts` runs them without a window. Ported from the TUI's tree.rs and directory.rs.

import type { SessionRow } from './types.ts';

export interface Place { row: SessionRow; depth: number; children: number }

/** `rows` (in listed order) with each one whose parent is listed moved under it, children in
 *  listed order. A row whose parent is not listed stays a root; a cycle is broken at the row
 *  met first. */
export function arrange(rows: SessionRow[]): Place[] {
  const byId = new Map<string, number>();
  rows.forEach((row, index) => { if (!byId.has(row.id)) byId.set(row.id, index); });
  const kids: number[][] = rows.map(() => []);
  const roots: number[] = [];
  rows.forEach((row, index) => {
    const parent = row.parent !== undefined && row.parent !== row.id ? byId.get(row.parent) : undefined;
    if (parent === undefined) roots.push(index); else kids[parent].push(index);
  });
  const out: Place[] = [];
  const seen = new Set<number>();
  const walk = (index: number, depth: number) => {
    if (seen.has(index)) return;
    seen.add(index);
    out.push({ row: rows[index], depth, children: kids[index].length });
    for (const kid of kids[index]) walk(kid, depth + 1);
  };
  for (const root of roots) walk(root, 0);
  // Rows only reachable through a cycle: listed as roots, so none is lost.
  rows.forEach((_, index) => walk(index, 0));
  return out;
}

/** `places` without the rows under a collapsed parent. */
export function flatten(places: Place[], collapsed: ReadonlySet<string>): Place[] {
  const out: Place[] = [];
  let hideDeeperThan = Infinity;
  for (const place of places) {
    if (place.depth > hideDeeperThan) continue;
    hideDeeperThan = Infinity;
    out.push(place);
    if (place.children > 0 && collapsed.has(place.row.id)) hideDeeperThan = place.depth;
  }
  return out;
}

/** Whether `query` (any case) is in the row's title, id or folder. */
export function matches(row: SessionRow, query: string): boolean {
  const q = query.trim().toLowerCase();
  if (!q) return true;
  return [row.title, row.id, row.cwd].some((text) => text?.toLowerCase().includes(q));
}

/** F12: the places that match `query`, each with its ancestors (so it keeps its place in the tree)
 *  and, under a matching parent, every descendant. */
export function search(places: Place[], query: string): Place[] {
  if (!query.trim()) return places;
  const keep = new Array<boolean>(places.length).fill(false);
  const stack: number[] = [];
  let underMatch = -1;
  places.forEach((place, index) => {
    while (stack.length && places[stack[stack.length - 1]].depth >= place.depth) stack.pop();
    if (underMatch >= 0 && place.depth <= places[underMatch].depth) underMatch = -1;
    const hit = matches(place.row, query);
    if (hit || underMatch >= 0) {
      keep[index] = true;
      for (const ancestor of stack) keep[ancestor] = true;
    }
    if (hit && underMatch < 0) underMatch = index;
    stack.push(index);
  });
  return places.filter((_, index) => keep[index]);
}

export interface Folder { cwd: string | null; label: string; count: number }

/** F2: All, then every folder by path, then Unknown when some rows name none. A folder whose last
 *  name another folder shares is shown with enough of its path to tell them apart. */
export function folders(rows: SessionRow[]): Folder[] {
  const counts = new Map<string, number>();
  let unknown = 0;
  for (const row of rows) {
    const cwd = row.cwd?.trim();
    if (cwd) counts.set(cwd, (counts.get(cwd) ?? 0) + 1); else unknown += 1;
  }
  const paths = [...counts.keys()].sort();
  const out: Folder[] = [{ cwd: null, label: 'All', count: rows.length }];
  for (const cwd of paths) out.push({ cwd, label: distinguishing(cwd, paths), count: counts.get(cwd)! });
  if (unknown) out.push({ cwd: '', label: 'Unknown', count: unknown });
  return out;
}

const leaf = (path: string) => path.split(/[\\/]/).filter(Boolean).pop() ?? path;

function distinguishing(cwd: string, paths: string[]): string {
  const peers = paths.filter((other) => other !== cwd && leaf(other) === leaf(cwd));
  if (!peers.length) return cwd;
  const starts = [0];
  for (let i = 0; i < cwd.length; i += 1) if (cwd[i] === '/' || cwd[i] === '\\') starts.push(i + 1);
  for (const start of starts.filter((s) => s < cwd.length).reverse()) {
    const suffix = cwd.slice(start);
    if (peers.every((other) => !other.endsWith(suffix))) return suffix;
  }
  return cwd;
}

/** The rows a folder filter keeps: `null` all, `''` the ones with no folder. */
export function inFolder(rows: SessionRow[], cwd: string | null): SessionRow[] {
  if (cwd === null) return rows;
  if (cwd === '') return rows.filter((row) => !row.cwd?.trim());
  return rows.filter((row) => row.cwd?.trim() === cwd);
}

/** `↳ agent Explore`, `⑂ fork`, `↳` — or nothing for a row without a parent. */
export function mark(row: SessionRow): { text: string; kind: 'spawn' | 'fork' | 'other' } | null {
  if (row.parent === undefined) return null;
  const kind = row.parent_kind;
  if (kind?.kind === 'spawn') return { text: kind.agent ? `↳ agent ${kind.agent}` : '↳ agent', kind: 'spawn' };
  if (kind?.kind === 'fork') return { text: 'fork', kind: 'fork' };
  return { text: '↳', kind: 'other' };
}

/** The detail pane's `kind` line. */
export function kindText(row: SessionRow): string {
  const kind = row.parent_kind;
  if (row.parent === undefined) return 'session';
  if (kind?.kind === 'spawn') {
    const agent = kind.agent ? ` (${kind.agent})` : '';
    return `subagent${agent}${kind.inherits_context ? ', started from the parent’s conversation' : ''}`;
  }
  if (kind?.kind === 'fork') return kind.at ? `fork at ${kind.at}` : 'fork';
  return 'child';
}

/** `3 (2 subagents, 1 fork)`, or null when none. */
export function childrenText(rows: SessionRow[], id: string): string | null {
  let subagents = 0, forks = 0, other = 0;
  for (const row of rows) {
    if (row.parent !== id || row.id === id) continue;
    if (row.parent_kind?.kind === 'spawn') subagents += 1;
    else if (row.parent_kind?.kind === 'fork') forks += 1;
    else other += 1;
  }
  const total = subagents + forks + other;
  if (!total) return null;
  const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;
  const parts = [
    subagents && plural(subagents, 'subagent', 'subagents'),
    forks && plural(forks, 'fork', 'forks'),
    other && plural(other, 'other', 'others'),
  ].filter(Boolean);
  return `${total} (${parts.join(', ')})`;
}

/** The ids on the path from a root down to `id` (its ancestors), nearest last. */
export function ancestors(rows: SessionRow[], id: string): string[] {
  const byId = new Map(rows.map((row) => [row.id, row] as const));
  const out: string[] = [];
  let parent = byId.get(id)?.parent;
  while (parent !== undefined && byId.has(parent) && !out.includes(parent) && parent !== id) {
    out.unshift(parent);
    parent = byId.get(parent)!.parent;
  }
  return out;
}

/** A short time: hh:mm today, the weekday this week, else the date. */
export function shortTime(iso: string | undefined, now = new Date()): string {
  if (!iso) return '';
  const time = new Date(iso);
  if (Number.isNaN(time.getTime())) return iso.slice(0, 16);
  const pad = (n: number) => String(n).padStart(2, '0');
  const sameDay = time.toDateString() === now.toDateString();
  if (sameDay) return `${pad(time.getHours())}:${pad(time.getMinutes())}`;
  const yesterday = new Date(now); yesterday.setDate(now.getDate() - 1);
  if (time.toDateString() === yesterday.toDateString()) return 'yesterday';
  const months = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];
  const sameYear = time.getFullYear() === now.getFullYear();
  return `${months[time.getMonth()]} ${time.getDate()}${sameYear ? '' : ` ${time.getFullYear()}`}`;
}

/** Newest first, rows without a time last, keeping the listed order between equals. */
export function byUpdated(rows: SessionRow[]): SessionRow[] {
  return rows
    .map((row, index) => ({ row, index, time: row.updated ? Date.parse(row.updated) : NaN }))
    .sort((a, b) => {
      const at = Number.isNaN(a.time) ? -Infinity : a.time;
      const bt = Number.isNaN(b.time) ? -Infinity : b.time;
      return bt - at || a.index - b.index;
    })
    .map(({ row }) => row);
}

/** What the session column lists: the folder's rows, newest first, as a tree, filtered by the
 *  search (which opens every collapsed parent: a match is never hidden), less collapsed parents'
 *  children. */
export function visible(rows: SessionRow[], folder: string | null, query: string, collapsed: ReadonlySet<string>): Place[] {
  const placed = arrange(byUpdated(inFolder(rows, folder)));
  return query.trim() ? search(placed, query) : flatten(placed, collapsed);
}
