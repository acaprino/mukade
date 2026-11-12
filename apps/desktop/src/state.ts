// The window's state: one object, changed by main.ts, read by the views when they draw.

import type { Installation, SessionRow, Transcript } from './types.ts';

export type Column = 'installations' | 'folders' | 'sessions' | 'detail';

export interface ReaderState {
  key: string;
  id: string;
  transcript: Transcript | null;
  /** 0: every turn. */
  turns: number;
  tab: 'conversation' | 'raw';
  /** The entry the Raw tab shows. */
  rawEntry: string | null;
  error: string | null;
}

export interface State {
  view: 'sessions' | 'chat';
  focus: Column;
  installations: Installation[];
  installation: string | null;
  rows: SessionRow[];
  /** The folder filter: null All, '' Unknown, else the cwd. */
  folder: string | null;
  query: string;
  collapsed: Set<string>;
  selected: string | null;
  reader: ReaderState | null;
  defaultTurns: number;
  scan: string[];
  /** The job the status line names; null when idle. */
  job: string | null;
  status: string;
  statusError: boolean;
  counts: string;
  lastExport: string | null;
}

export const state: State = {
  view: 'sessions',
  focus: 'installations',
  installations: [],
  installation: null,
  rows: [],
  folder: null,
  query: '',
  collapsed: new Set(),
  selected: null,
  reader: null,
  defaultTurns: 50,
  scan: [],
  job: null,
  status: 'starting…',
  statusError: false,
  counts: '',
  lastExport: null,
};

export function currentInstallation(): Installation | undefined {
  return state.installations.find((item) => item.key === state.installation);
}

export function selectedRow(): SessionRow | undefined {
  return state.rows.find((row) => row.id === state.selected);
}
