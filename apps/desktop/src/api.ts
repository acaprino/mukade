// The commands of src-tauri/src/main.rs, typed. Outside Tauri (the browser at `npm run dev`, the
// Playwright smoke test) the same calls answer from `mock.ts`, so every view can be seen and
// tested without stores.

import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type {
  AppInfo, ChatEvent, ChatStatus, CmdError, Exported, HopArgs, HopOptions, Inventory, Member,
  Page, Receipt, Resumed, Room, SessionRow, Transcript,
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
  appInfo: () => invoke<AppInfo>('app_info'),
  installations: (scan: string[]) => invoke<Inventory>('installations', { scan }),
  scan: (dir: string) => invoke<Inventory>('scan', { dir }),
  sessions: (installation: number) => invoke<SessionRow[]>('sessions', { installation }),
  transcript: (installation: number, id: string, turns: number) => invoke<Transcript>('transcript', { installation, id, turns }),
  raw: (installation: number, id: string, entryId: string) => invoke<unknown[]>('raw', { installation, id, entryId }),
  hopOptions: (source: number, id: string) => invoke<HopOptions>('hop_options', { source, id }),
  hop: (args: HopArgs) => invoke<Receipt>('hop', { args }),
  export: (installation: number, id: string, to?: string) => invoke<Exported>('export', { installation, id, to }),
  import: (document: string, into: number, prompt?: string) => invoke<Receipt>('import', { document, into, prompt }),
  resume: (installation: number, id: string) => invoke<Resumed>('resume', { installation, id }),
  pick: (kind: 'open' | 'save' | 'folder', name?: string) => invoke<string | null>('pick', { kind, name }),
  chatStatus: () => invoke<ChatStatus>('chat_status'),
  chatRooms: (cursor?: string) => invoke<Page<Room>>('chat_rooms', { cursor }),
  chatMembers: (roomId: string) => invoke<Page<Member>>('chat_members', { roomId }),
  chatLedger: (roomId: string, opts: { cursor?: string; since?: string; text?: string } = {}) =>
    invoke<Page<ChatEvent>>('chat_ledger', { roomId, ...opts }),
  quit: () => invoke<void>('quit'),
};

/** The window refused to close while a hop or import ran. */
export function onCloseBlocked(handler: () => void): void {
  if (inTauri) void listen('close-blocked', handler);
}
