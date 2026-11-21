import { test } from 'node:test';
import assert from 'node:assert/strict';
import { atRisk, canStartIn, leaf, otherRows, repositoryLine } from '../src/work.ts';
import type { Repository, SessionRow } from '../src/types.ts';

const repo = (path: string, branch: string | null, uncommitted: number, branches: Repository['branches'], head = '41d916d'): Repository => ({
  path, worktrees: [{ path, branch, head, uncommitted }], branches, error: null,
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
      { path: '/work/app', branch: 'develop', head: 'aaaaaaa', uncommitted: 0 },
      { path: '/work/app-fix', branch: 'fix/x', head: 'bbbbbbb', uncommitted: 2 },
      { path: '/work/app-clean', branch: 'ok', head: 'ccccccc', uncommitted: 0 },
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
