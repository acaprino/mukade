// The commands of src-tauri/src/main.rs, typed. Outside Tauri (the browser at `npm run dev`, the
// Playwright smoke test) the same calls answer from `mock.ts`, so every view can be seen and
// tested without stores.

import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type {
  ChatEvent, ChatStatus, CmdError, Exported, HopArgs, HopOptions, InstallationList, Member,
  Page, RawPage, Receipt, Resumed, Room, SessionRow, Startup, TranscriptView,
} from './types.ts';

export const inTauri = typeof (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ !== 'undefined';

type Mock = (command: string, args?: Record<string, unknown>) => Promise<unknown>;
let mock: Promise<Mock> | null = null;

export async function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (inTauri) return tauriInvoke<T>(command, args);
  // The mock is a development aid: a production build leaves it out.
  if (!import.meta.env.DEV) throw { kind: 'failed', message: 'Mukade runs inside its desktop window' } satisfies CmdError;
  mock ??= import('./mock.ts').then((module) => module.answer);
  return (await mock)(command, args) as Promise<T>;
}

/** The message of whatever a command threw. */
export function errorText(error: unknown): string {
  if (error && typeof error === 'object' && 'message' in error) return String((error as CmdError).message);
  return String(error);
}

export function errorKind(error: unknown): CmdError['kind'] | null {
  return error && typeof error === 'object' && 'kind' in error ? (error as CmdError).kind : null;
}

export const api = {
  startup: () => invoke<Startup>('startup'),
  /** Every standard store, then what each folder in `scan` holds: pass every folder scanned so far. */
  installations: (scan: string[]) => invoke<InstallationList>('installations', { scan }),
  sessions: (installation: string) => invoke<SessionRow[]>('sessions', { installation }),
  transcript: (installation: string, id: string, turns: number) => invoke<TranscriptView>('transcript', { installation, id, turns }),
  /** Native records of the session's entries `offset` to `offset + limit` (at most 200). */
  raw: (installation: string, id: string, offset: number, limit: number) => invoke<RawPage>('raw', { installation, id, offset, limit }),
  hopOptions: (source: string, id: string) => invoke<HopOptions>('hop_options', { source, id }),
  hop: (args: HopArgs) => invoke<Receipt>('hop', { args }),
  export: (installation: string, id: string, to?: string) => invoke<Exported>('export', { installation, id, to }),
  import: (document: string, into: string, prompt?: string) => invoke<Receipt>('import', { document, into, prompt }),
  resume: (installation: string, id: string) => invoke<Resumed>('resume', { installation, id }),
  pick: (kind: 'open' | 'save' | 'folder', name?: string) => invoke<string | null>('pick', { kind, name }),
  chatStatus: () => invoke<ChatStatus>('chat_status'),
  chatRooms: (cursor?: string) => invoke<Page<Room>>('chat_rooms', { cursor }),
  chatMembers: (roomId: string) => invoke<Page<Member>>('chat_members', { roomId }),
  chatLedger: (roomId: string, opts: { cursor?: string; since?: string; text?: string } = {}) =>
    invoke<Page<ChatEvent>>('chat_ledger', { roomId, ...opts }),
  quit: () => invoke<void>('quit'),
};

/** Files dropped on the window (paths), in Tauri only: the WebView keeps them from the page. */
export function onFilesDropped(handler: (paths: string[]) => void): void {
  if (!inTauri) return;
  void import('@tauri-apps/api/webview').then(({ getCurrentWebview }) =>
    getCurrentWebview().onDragDropEvent((event) => {
      if (event.payload.type === 'drop') handler(event.payload.paths);
    }));
}

/** The window refused to close while a hop or import ran. */
export function onCloseBlocked(handler: () => void): void {
  if (inTauri) void listen('close-blocked', handler);
}
