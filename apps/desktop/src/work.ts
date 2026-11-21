// What a session left in its folder: the git repositories there, the commits that exist on no
// remote and the paths not committed. Pure lines, so tests/work.test.ts checks them; renderWork draws them.

import { h } from './dom.ts';
import type { Branch, Repository, SessionRow, Work, Worktree } from './types.ts';

/** Only a session whose conversation cannot be read, but whose folder is known, is started elsewhere:
 *  a readable one is hopped or resumed. */
export function canStartIn(row: SessionRow): boolean {
  return Boolean(row.unreadable && row.cwd);
}

/** A chat that ChatGPT keeps on its servers: only the folder it worked in is on this machine. */
export function isHosted(row: SessionRow): boolean {
  return Boolean(row.unreadable?.startsWith('hosted by ChatGPT'));
}

/** How long a folder's work is kept before it is read again, and how many folders are kept. */
export const WORK_TTL_MS = 60_000;
export const WORK_CACHE_FOLDERS = 20;

export interface WorkCache {
  get(folder: string, now: number): Work | undefined;
  set(folder: string, work: Work, now: number): void;
}

/** Work by folder, so sessions that share a folder read it once. The time is a parameter: the
 *  caller passes Date.now(), a test passes what it likes. The oldest folder goes first. */
export function makeWorkCache(ttl = WORK_TTL_MS, capacity = WORK_CACHE_FOLDERS): WorkCache {
  const kept = new Map<string, { work: Work; at: number }>();
  return {
    get(folder, now) {
      const entry = kept.get(folder);
      if (!entry) return undefined;
      if (now - entry.at > ttl) { kept.delete(folder); return undefined; }
      return entry.work;
    },
    set(folder, work, now) {
      kept.delete(folder);
      kept.set(folder, { work, at: now });
      while (kept.size > capacity) kept.delete(kept.keys().next().value as string);
    },
  };
}

export const workCache: WorkCache = makeWorkCache();

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
  if (main?.error) return main.error;
  return main ? worktreeLine(main, repo.branches) : 'no worktree';
}

/** Why the repository's own line is a note: git refused the repository, or its main worktree. */
export function repositoryError(repo: Repository): string | null {
  return repo.error ?? repo.worktrees[0]?.error ?? null;
}

export interface OtherRow { kind: 'worktree' | 'branch'; name: string; line: string; title: string; error?: boolean }

/** The other worktrees and the branches no worktree has checked out that hold work at risk. */
export function otherRows(repo: Repository): OtherRow[] {
  if (repo.error) return [];
  const rows: OtherRow[] = [];
  for (const worktree of repo.worktrees.slice(1)) {
    // A worktree git could not read says so: it may hold work that is not counted.
    if (worktree.error) { rows.push({ kind: 'worktree', name: leaf(worktree.path), line: worktree.error, title: worktree.path, error: true }); continue; }
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
      repositoryError(repo) ? h('p', { class: 'note' }, repositoryError(repo)) : h('p', { class: 'work-line' }, repositoryLine(repo)));
    for (const other of otherRows(repo)) {
      item.append(h('p', { class: `${other.error ? 'note' : 'work-line'} work-other`, title: other.title }, h('span', { class: 'work-kind' }, other.kind), ' ', h('b', {}, other.name), ' · ', other.line));
    }
    root.append(item);
  }
  return root;
}
