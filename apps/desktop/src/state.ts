// The window's state: one object, changed by the controller (controller.ts), read by the screens when they draw. Plain data and
// the functions that read it, so node tests them without a window.

import type { Installation, SessionRow, Transcript } from './types.ts';
import { visible, type Place } from './tree.ts';

/** The three views of the window. */
export type ViewId = 'sessions' | 'chat' | 'harnesses';

export interface ReaderState {
  key: string;
  id: string;
  transcript: Transcript | null;
  /** 0: every turn. */
  turns: number;
  tab: 'conversation' | 'raw';
  /** The entry the Raw tab shows. */
  rawEntry: string | null;
  /** The native records of that entry, once read. */
  raw: unknown[] | null;
  error: string | null;
}

export type ReadState = { kind: 'loading' | 'ready' } | { kind: 'error'; message: string };

export interface State {
  view: ViewId;
  /** The details of the selected session: the person's choice, or null for what the width says (docked: open; a drawer: closed). */
  detailOpen: boolean | null;
  installations: Installation[];
  installation: string | null;
  inventoryRead: ReadState;
  sessionsRead: ReadState;
  rows: SessionRow[];
  /** The folder filter: null All, '' Unknown, else the cwd. */
  folder: string | null;
  query: string;
  /** The parents folded away (a parent is open unless it is here). */
  collapsed: Set<string>;
  selected: string | null;
  reader: ReaderState | null;
  defaultTurns: number;
  scan: string[];
  /** The job the status row names; null when idle. */
  job: string | null;
  status: string;
  statusError: boolean;
  counts: string;
  lastExport: string | null;
}

export function createState(): State {
  return {
    view: 'sessions',
    detailOpen: null,
    installations: [],
    installation: null,
    inventoryRead: { kind: 'loading' },
    sessionsRead: { kind: 'ready' },
    rows: [],
    folder: null,
    query: '',
    collapsed: new Set(),
    selected: null,
    reader: null,
    defaultTurns: 50,
    scan: [],
    job: null,
    status: 'Starting…',
    statusError: false,
    counts: '',
    lastExport: null,
  };
}

export function currentInstallation(state: State): Installation | undefined {
  return state.installations.find((item) => item.key === state.installation);
}

export function selectedRow(state: State): SessionRow | undefined {
  return state.rows.find((row) => row.id === state.selected);
}

/** What the session list shows: the folder's rows as a tree, filtered by the search, less the folded parents' children. */
export function visiblePlaces(state: State): Place[] {
  return visible(state.rows, state.folder, state.query, state.collapsed);
}
