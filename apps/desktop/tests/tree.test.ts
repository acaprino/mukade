import { test } from 'node:test';
import assert from 'node:assert/strict';
import { ancestors, arrange, byUpdated, childrenText, flatten, folders, inFolder, kindText, mark, search, shortTime, visible } from '../src/tree.ts';
import type { SessionRow } from '../src/types.ts';

const row = (id: string, extra: Partial<SessionRow> = {}): SessionRow => ({ id, title: id, archived: false, path: `${id}.jsonl`, cwd: '/projects/a', ...extra });
const spawn = (parent: string, agent?: string): Partial<SessionRow> => ({ parent, parent_kind: { kind: 'spawn', agent } });
const fork = (parent: string): Partial<SessionRow> => ({ parent, parent_kind: { kind: 'fork' } });

// The TUI's listing: a child before its parent, a fork, a subagent of a session not listed.
const listing = [
  row('helper', spawn('main', 'explore')),
  row('main'),
  row('branch', fork('main')),
  row('stray', { ...spawn('gone'), cwd: '/projects/b' }),
  row('far-fork', { ...fork('main'), cwd: '/projects/b' }),
];

test('children and forks nest under their parent; an orphan stays a root', () => {
  const placed = arrange(listing);
  assert.deepEqual(placed.map((p) => [p.row.id, p.depth]), [['main', 0], ['helper', 1], ['branch', 1], ['far-fork', 1], ['stray', 0]]);
  assert.equal(placed[0].children, 3);
});

test('a cycle loses no row', () => {
  const placed = arrange([row('a', fork('b')), row('b', fork('a'))]);
  assert.deepEqual(placed.map((p) => p.row.id).sort(), ['a', 'b']);
});

test('a collapsed parent hides everything under it, and only that', () => {
  const placed = arrange([row('p'), row('c', spawn('p')), row('g', spawn('c')), row('next')]);
  assert.deepEqual(flatten(placed, new Set(['p'])).map((p) => p.row.id), ['p', 'next']);
  assert.deepEqual(flatten(placed, new Set(['c'])).map((p) => p.row.id), ['p', 'c', 'next']);
  assert.equal(flatten(placed, new Set()).length, 4);
});

test('search keeps a match with its ancestors, and every child of a matching parent', () => {
  const placed = arrange(listing);
  assert.deepEqual(search(placed, 'MAIN').map((p) => p.row.id), ['main', 'helper', 'branch', 'far-fork']);
  assert.deepEqual(search(placed, 'branch').map((p) => p.row.id), ['main', 'branch']);
  assert.deepEqual(search(placed, '/projects/b').map((p) => p.row.id), ['main', 'far-fork', 'stray']);
  assert.equal(search(placed, '  ').length, placed.length);
});

test('search opens collapsed parents: a match is never hidden', () => {
  assert.deepEqual(visible(listing, null, 'branch', new Set(['main'])).map((p) => p.row.id), ['main', 'branch']);
  assert.deepEqual(visible(listing, null, '', new Set(['main'])).map((p) => p.row.id), ['main', 'stray']);
});

test('folders: All, each path, Unknown; a shared leaf name keeps enough of its path', () => {
  const rows = [row('1', { cwd: '/w/one/app' }), row('2', { cwd: '/w/two/app' }), row('3', { cwd: '/w/two/app' }), row('4', { cwd: undefined })];
  assert.deepEqual(folders(rows), [
    { cwd: null, label: 'All', count: 4 },
    { cwd: '/w/one/app', label: 'one/app', count: 1 },
    { cwd: '/w/two/app', label: 'two/app', count: 2 },
    { cwd: '', label: 'Unknown', count: 1 },
  ]);
  assert.deepEqual(inFolder(rows, '').map((r) => r.id), ['4']);
  assert.deepEqual(inFolder(rows, '/w/two/app').map((r) => r.id), ['2', '3']);
});

test('marks, kind and children read as the TUI wrote them', () => {
  assert.deepEqual(mark(listing[0]), { text: '↳ agent explore', kind: 'spawn' });
  assert.equal(mark(listing[2])?.kind, 'fork');
  assert.equal(mark(listing[1]), null);
  assert.equal(kindText(listing[0]), 'subagent (explore)');
  assert.equal(kindText(listing[1]), 'session');
  assert.equal(childrenText(listing, 'main'), '3 (1 subagent, 2 forks)');
  assert.equal(childrenText(listing, 'stray'), null);
  assert.deepEqual(ancestors([row('a'), row('b', fork('a')), row('c', spawn('b'))], 'c'), ['a', 'b']);
});

test('newest first, timeless rows last', () => {
  const rows = [row('old', { updated: '2026-01-01T00:00:00Z' }), row('none'), row('new', { updated: '2026-11-12T00:00:00Z' })];
  assert.deepEqual(byUpdated(rows).map((r) => r.id), ['new', 'old', 'none']);
});

test('short times: today, yesterday, a date', () => {
  const now = new Date(2026, 10, 12, 15, 0);
  assert.equal(shortTime(new Date(2026, 10, 12, 9, 5).toISOString(), now), '09:05');
  assert.equal(shortTime(new Date(2026, 10, 11, 9, 5).toISOString(), now), 'yesterday');
  assert.equal(shortTime(new Date(2026, 10, 3).toISOString(), now), 'Nov 3');
  assert.equal(shortTime(new Date(2025, 0, 3).toISOString(), now), 'Jan 3 2025');
  assert.equal(shortTime(undefined, now), '');
});
