import { test } from 'node:test';
import assert from 'node:assert/strict';
import { atRisk, canStartIn, isHosted, leaf, makeWorkCache, otherRows, repositoryError, repositoryLine } from '../src/work.ts';
import type { Repository, SessionRow, Work } from '../src/types.ts';

const repo = (path: string, branch: string | null, uncommitted: number, branches: Repository['branches'], head = '41d916d'): Repository => ({
  path, worktrees: [{ path, branch, head, uncommitted, error: null }], branches, error: null,
});
const daodan = repo('C:\\Users\\dev\\Documents\\ChatGPT\\Daodan\\daodan', 'develop', 1, [{ name: 'develop', upstream: 'origin/develop', ahead: 0, unpushed: 0 }]);
const imago = repo('C:\\Users\\dev\\Documents\\ChatGPT\\Daodan\\imago', 'develop', 0, [{ name: 'develop', upstream: 'origin/develop', ahead: 0, unpushed: 0 }]);
const stackAnalyser = repo('C:\\Users\\dev\\Documents\\ChatGPT\\Daodan\\stack-analyser', 'feature/stack-detection', 133,
  [{ name: 'feature/stack-detection', upstream: null, ahead: null, unpushed: 3 }]);

test('a repository with uncommitted paths or commits on no remote is at risk', () => {
  assert.equal(atRisk(daodan), true);
  assert.equal(atRisk(stackAnalyser), true);
  assert.equal(atRisk(imago), false);
  // Commits on a branch no worktree has checked out are at risk too.
  const stray = repo('/r', 'main', 0, [{ name: 'main', upstream: 'origin/main', ahead: 0, unpushed: 0 }, { name: 'wip', upstream: null, ahead: null, unpushed: 2 }]);
  assert.equal(atRisk(stray), true);
  // A repository that could not be read is not called at risk: it says why.
  assert.equal(atRisk({ ...imago, error: 'not a git repository' }), false);
});

test('the line names branch, upstream, unpushed commits and uncommitted paths', () => {
  assert.equal(repositoryLine(stackAnalyser), 'feature/stack-detection · no upstream · 3 commits on no remote · 133 uncommitted');
  assert.equal(repositoryLine(imago), 'develop · origin/develop · clean');
  assert.equal(repositoryLine(daodan), 'develop · origin/develop · 1 uncommitted');
  const one = repo('/r', 'topic', 0, [{ name: 'topic', upstream: null, ahead: null, unpushed: 1 }]);
  assert.equal(repositoryLine(one), 'topic · no upstream · 1 commit on no remote');
  const ahead = repo('/r', 'develop', 0, [{ name: 'develop', upstream: 'origin/develop', ahead: 2, unpushed: 0 }]);
  assert.equal(repositoryLine(ahead), 'develop · origin/develop · 2 ahead');
  assert.equal(repositoryLine({ ...imago, error: 'git: not a repository' }), 'git: not a repository');
});

test('a detached worktree is named by its commit', () => {
  assert.equal(repositoryLine(repo('/r', null, 0, [])), 'detached at 41d916d · clean');
  assert.equal(repositoryLine(repo('/r', null, 4, [])), 'detached at 41d916d · 4 uncommitted');
});

test('other worktrees and branches at risk are listed in the same words', () => {
  const several: Repository = {
    path: '/work/app',
    worktrees: [
      { path: '/work/app', branch: 'develop', head: 'aaaaaaa', uncommitted: 0, error: null },
      { path: '/work/app-fix', branch: 'fix/x', head: 'bbbbbbb', uncommitted: 2, error: null },
      { path: '/work/app-clean', branch: 'ok', head: 'ccccccc', uncommitted: 0, error: null },
    ],
    branches: [
      { name: 'develop', upstream: 'origin/develop', ahead: 0, unpushed: 0 },
      { name: 'fix/x', upstream: null, ahead: null, unpushed: 1 },
      { name: 'ok', upstream: 'origin/ok', ahead: 0, unpushed: 0 },
      { name: 'old', upstream: null, ahead: null, unpushed: 5 },
      { name: 'merged', upstream: 'origin/merged', ahead: 0, unpushed: 0 },
    ],
    error: null,
  };
  assert.equal(repositoryLine(several), 'develop · origin/develop · clean');
  assert.deepEqual(otherRows(several).map((row) => [row.kind, row.name, row.line]), [
    ['worktree', 'app-fix', 'fix/x · no upstream · 1 commit on no remote · 2 uncommitted'],
    ['branch', 'old', 'no upstream · 5 commits on no remote'],
  ]);
  assert.equal(atRisk(several), true);
  assert.deepEqual(otherRows(imago), []);
});

test('a name is the last part of a path', () => {
  assert.equal(leaf('C:\\a\\b\\daodan'), 'daodan');
  assert.equal(leaf('/a/b/imago/'), 'imago');
});

test('only an unreadable row with a folder can be started elsewhere', () => {
  const row = (extra: Partial<SessionRow>): SessionRow => ({ id: 'x', archived: false, path: 'x', ...extra });
  assert.equal(canStartIn(row({ unreadable: 'hosted by ChatGPT', cwd: 'C:\\work' })), true);
  assert.equal(canStartIn(row({ cwd: 'C:\\work' })), false);
  assert.equal(canStartIn(row({ unreadable: 'the session file is gone' })), false);
});

test('a hosted row is told by the start of its reason', () => {
  const row = (extra: Partial<SessionRow>): SessionRow => ({ id: 'x', archived: false, path: 'x', ...extra });
  assert.equal(isHosted(row({ unreadable: 'hosted by ChatGPT: the conversation is not on this machine' })), true);
  assert.equal(isHosted(row({ unreadable: 'the session file is gone' })), false);
  assert.equal(isHosted(row({})), false);
});

test('a worktree git could not read is a note row, and its repository is not called at risk by it', () => {
  const broken: Repository = {
    path: '/work/app',
    worktrees: [
      { path: '/work/app', branch: 'develop', head: 'aaaaaaa', uncommitted: 0, error: null },
      { path: '/work/app-side', branch: 'side', head: 'bbbbbbb', uncommitted: 0, error: 'fatal: invalid gitfile format' },
    ],
    branches: [{ name: 'develop', upstream: 'origin/develop', ahead: 0, unpushed: 0 }, { name: 'side', upstream: null, ahead: null, unpushed: 0 }],
    error: null,
  };
  assert.deepEqual(otherRows(broken), [
    { kind: 'worktree', name: 'app-side', line: 'fatal: invalid gitfile format', title: '/work/app-side', error: true },
  ]);
  assert.equal(repositoryError(broken), null);
  assert.equal(atRisk(broken), false);
  // The main worktree's error is the repository's own line.
  const main: Repository = { ...broken, worktrees: [{ ...broken.worktrees[0], error: 'fatal: nope' }] };
  assert.equal(repositoryError(main), 'fatal: nope');
  assert.equal(repositoryLine(main), 'fatal: nope');
});

const work = (folder: string): Work => ({ folder, repositories: [], note: null });

test('the work of a folder is kept for 60 seconds', () => {
  const cache = makeWorkCache();
  const kept = work('/a');
  cache.set('/a', kept, 1000);
  assert.equal(cache.get('/a', 1000), kept);
  assert.equal(cache.get('/a', 1000 + 60_000), kept);
  assert.equal(cache.get('/a', 1000 + 60_001), undefined);
  // An old entry is gone for good, and an unknown folder was never there.
  assert.equal(cache.get('/a', 1000), undefined);
  assert.equal(cache.get('/b', 1000), undefined);
});

test('the 21st folder evicts the oldest', () => {
  const cache = makeWorkCache();
  for (let i = 0; i < 20; i++) cache.set(`/f${i}`, work(`/f${i}`), 1000 + i);
  assert.ok(cache.get('/f0', 2000));
  cache.set('/f20', work('/f20'), 1100);
  assert.equal(cache.get('/f0', 2000), undefined);
  assert.ok(cache.get('/f1', 2000));
  assert.ok(cache.get('/f20', 2000));
  // Setting a folder again renews it: it is no longer the oldest.
  cache.set('/f1', work('/f1'), 1200);
  cache.set('/f21', work('/f21'), 1300);
  assert.ok(cache.get('/f1', 2000));
  assert.equal(cache.get('/f2', 2000), undefined);
});
