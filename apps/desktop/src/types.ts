// The records the Rust commands send (src-tauri/src/view.rs, commands.rs, chat.rs), mirrored by
// hand: this file is their one home on this side.

export interface CmdError { kind: 'refused' | 'not_found' | 'failed' | 'unreachable'; message: string; exit_code?: number }

export interface AppInfo { name: string; version: string; turns: number; scan: string[]; platform: string }

export interface Installation {
  key: number; harness: string; label: string; store?: string; engine?: string;
  scanned: boolean; writable: boolean; count: number; error?: string;
}

export interface Inventory { installations: Installation[]; added: number[]; stopped_early: boolean; status: string }

export type ParentKind =
  | { kind: 'fork'; at?: string }
  | { kind: 'spawn'; call_id?: string; agent?: string; inherits_context?: boolean };

export interface SessionRow {
  id: string; title?: string; cwd?: string; updated?: string; archived: boolean; live?: boolean;
  unreadable?: string; path: string; parent?: string; parent_kind?: ParentKind;
}

export type BlockRole = 'user' | 'assistant' | 'reasoning' | 'tool' | 'result' | 'error' | 'attachment' | 'compaction';
export interface Block { role: BlockRole; text: string; name?: string; cut?: boolean }
export interface EntryView { id: string; kind: 'user' | 'step' | 'results' | 'compaction' | 'meta'; time?: string; blocks: Block[]; records: number }
export interface Transcript { title: string; entries: EntryView[]; hidden_turns: number; total_turns: number }

export interface Destination { key: number; label: string; default_shell?: 'posix' | 'powershell' }
export interface HopOptions {
  source_label: string; title: string; destinations: Destination[]; compactions: string[];
  subagents: string[]; forks: string[]; all_branches_supported: boolean; live?: boolean;
}
export interface HopArgs {
  source: number; id: string; dest: number; compaction: string; children: boolean; all_branches: boolean;
  reasoning: string; dest_shell: string; prompt?: string;
}

export interface ChildLine { source_id: string; dest_id: string; linked: boolean }
export interface Receipt {
  dest_id: string; dest_label: string; dest_key: number; location: string; headless: string[];
  interactive: string[]; refusal?: string; manifest_path?: string; losses: string[]; children: ChildLine[];
  reply?: string; warning?: string;
}

export interface Exported { path: string; messages: number }
export interface Resumed { launched: boolean; refusal?: string; command: string[]; cwd?: string }

export interface ChatStatus { state: 'running' | 'stopped' | 'unreachable' | string; endpoint?: string; data_dir?: string; message?: string }
export interface Room {
  room_id: string; name: string; opened_at: string; closed_at: string | null; opened_by: string | null;
  kept: boolean; members: number; events: number; last_activity: string | null; revision: string;
}
export interface Binding { harness: string; id: string; cwd: string | null; source_ref?: string | null }
export interface Member {
  room_id: string; member_id: string; name: string; harness: string; role: string; joined_at: string;
  left_at: string | null; last_command_at: string | null; session: Binding | null;
}
export type Actor = { type: 'local_admin' } | { type: 'member'; member_id: string | null; name: string; role: string | null };
export interface ChatEvent {
  room_id: string; event_id: string; timestamp: string; actor: Actor; kind: string;
  payload: Record<string, unknown>; recipient_ids: string[];
}
export interface Page<T> { items: T[]; next_cursor: string | null; high_water?: string }
