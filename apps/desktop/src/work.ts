// What a session left in its folder: the git repositories there, the commits that exist on no
// remote and the paths not committed. Pure lines, so tests/work.test.ts checks them; renderWork draws them.

import { h } from './dom.ts';
import type { Branch, Repository, SessionRow, Work, Worktree } from './types.ts';

/** Only a session whose conversation cannot be read, but whose folder is known, is started elsewhere:
 *  a readable one is hopped or resumed. */
export function canStartIn(row: SessionRow): boolean {
  return Boolean(row.unreadable && row.cwd);
}

/** The last part of a path, whichever separator it uses. */
export function leaf(path: string): string {
  return path.split(/[\\/]+/).filter(Boolean).pop() ?? path;
}

const plural = (n: number, one: string, many: string): string => `${n} ${n === 1 ? one : many}`;

/** The state of a branch checked out with `uncommitted` paths changed: what would be lost. */
function state(branch: Branch | undefined, uncommitted: number): string {
  const parts: string[] = [];
  if (branch && branch.unpushed > 0) parts.push(`${plural(branch.unpushed, 'commit', 'commits')} on no remote`);
  else if (branch && (branch.ahead ?? 0) > 0) parts.push(`${branch.ahead} ahead`);
  if (uncommitted > 0) parts.push(`${uncommitted} uncommitted`);
  return parts.length ? parts.join(' · ') : 'clean';
}

function worktreeLine(worktree: Worktree, branches: Branch[]): string {
  if (worktree.branch === null) return `detached at ${worktree.head} · ${state(undefined, worktree.uncommitted)}`;
  const branch = branches.find((item) => item.name === worktree.branch);
  return `${worktree.branch} · ${branch?.upstream ?? 'no upstream'} · ${state(branch, worktree.uncommitted)}`;
}

/** Uncommitted paths in a worktree, or commits on no remote on a branch: work that exists nowhere else. */
export function atRisk(repo: Repository): boolean {
  return repo.worktrees.some((worktree) => worktree.uncommitted > 0) || repo.branches.some((branch) => branch.unpushed > 0);
}

/** The repository's main worktree: `<branch> · <upstream> · <state>`. */
export function repositoryLine(repo: Repository): string {
  if (repo.error) return repo.error;
  const main = repo.worktrees[0];
  return main ? worktreeLine(main, repo.branches) : 'no worktree';
}

export interface OtherRow { kind: 'worktree' | 'branch'; name: string; line: string; title: string }

/** The other worktrees and the branches no worktree has checked out that hold work at risk. */
export function otherRows(repo: Repository): OtherRow[] {
  if (repo.error) return [];
  const rows: OtherRow[] = [];
  for (const worktree of repo.worktrees.slice(1)) {
    const branch = repo.branches.find((item) => item.name === worktree.branch);
    if (worktree.uncommitted > 0 || (branch?.unpushed ?? 0) > 0) rows.push({ kind: 'worktree', name: leaf(worktree.path), line: worktreeLine(worktree, repo.branches), title: worktree.path });
  }
  const checkedOut = new Set(repo.worktrees.map((worktree) => worktree.branch));
  for (const branch of repo.branches) {
    if (checkedOut.has(branch.name) || branch.unpushed === 0) continue;
    rows.push({ kind: 'branch', name: branch.name, line: `${branch.upstream ?? 'no upstream'} · ${state(branch, 0)}`, title: branch.name });
  }
  return rows;
}

const atRiskChip = (): HTMLElement => h('span', { class: 'tag im-chip warn', title: 'Work that exists only in this folder' }, 'at risk');

export function renderWork(work: Work): HTMLElement {
  const root = h('div', { class: 'work-body' }, h('p', { class: 'work-folder', title: work.folder }, work.folder));
  if (work.note) root.append(h('p', { class: 'note' }, work.note));
  for (const repo of work.repositories) {
    const item = h('div', { class: 'work-repo' },
      h('div', { class: 'work-head' }, h('span', { class: 'work-name', title: repo.path }, leaf(repo.path)), atRisk(repo) ? atRiskChip() : null),
      repo.error ? h('p', { class: 'note' }, repo.error) : h('p', { class: 'work-line' }, repositoryLine(repo)));
    for (const other of otherRows(repo)) {
      item.append(h('p', { class: 'work-line work-other', title: other.title }, h('span', { class: 'work-kind' }, other.kind), ' ', h('b', {}, other.name), ' · ', other.line));
    }
    root.append(item);
  }
  return root;
}
