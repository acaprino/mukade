// The records the Rust commands send (src-tauri/src/view.rs, commands.rs, chat.rs), mirrored by
// hand: this file is their one home on this side.

export interface CmdError { kind: 'refused' | 'not_found' | 'failed' | 'unreachable'; message: string; exit_code?: number }

export interface Startup { turns: number; scan: string[]; version: string }

/** `key` is `<harness>` for a standard store, `<harness>@<store>` for a scanned one: stable. */
export interface Installation {
  key: string; harness: string; label: string; store?: string; engine?: string;
  scanned: boolean; destination: boolean; count: number; error?: string;
}

export interface ScanNote { dir: string; added: number; stopped_early: boolean }
export interface InstallationList { installations: Installation[]; scans: ScanNote[] }

export type ParentKind =
  | { kind: 'fork'; at?: string }
  | { kind: 'spawn'; call_id?: string; agent?: string; inherits_context?: boolean };

export interface SessionRow {
  id: string; title?: string; cwd?: string; updated?: string; archived: boolean; live?: boolean;
  unreadable?: string; path: string; parent?: string; parent_kind?: ParentKind;
}

// The transcript as the backend sends it (src-tauri/src/sessions.rs): one row per block.
export interface ToolView { name: string; call_id: string; input: string; input_truncated: boolean }
export interface RowView {
  index: number; entry_id: string; role: 'user' | 'file' | 'reasoning' | 'assistant' | 'tool' | 'result' | 'compaction';
  time?: string; text: string; truncated: boolean; text_chars: number; tool?: ToolView; error: boolean;
}
export interface MetaView {
  harness: string; version: string; id: string; title?: string; cwd?: string; created?: string; updated?: string;
  git_branch?: string; models: string[]; entries: number; branches: number;
}
export interface TranscriptView { meta: MetaView; rows: RowView[]; total_turns: number; hidden_turns: number }
export interface RawItem { index: number; entry_id: string; kind: string; raw: unknown[]; truncated: boolean }
export interface RawPage { total: number; items: RawItem[] }

// The transcript as the reader draws it (transcript.ts groups the rows): one entry per native entry.
export type BlockRole = 'user' | 'assistant' | 'reasoning' | 'tool' | 'result' | 'error' | 'attachment' | 'compaction';
export interface Block { role: BlockRole; text: string; name?: string; cut?: boolean }
export interface EntryView { id: string; index: number; kind: 'user' | 'step' | 'results' | 'compaction'; time?: string; blocks: Block[] }
export interface Transcript { title: string; meta: MetaView; entries: EntryView[]; hidden_turns: number; total_turns: number }

export interface Destination { key: string; label: string; default_shell?: 'posix' | 'powershell'; model?: string }
export interface HopOptions {
  source_label: string; title: string; destinations: Destination[]; compactions: string[];
  subagents: string[]; forks: string[]; all_branches_supported: boolean; intents: string[]; live?: boolean;
}
export interface HopArgs {
  source: string; id: string; dest: string; compaction: string; children: boolean; all_branches: boolean;
  reasoning: string; dest_shell: string; prompt?: string;
  /** Summary: the intent of its prompt, the model asked, the last turns kept whole after it. */
  intent?: string; summary_model?: string; keep_turns?: number;
  /** Native: the destination model's context window and output limit, in tokens. */
  context_window?: number; max_output_tokens?: number;
  /** The destination's model and provider, instead of its configuration's. */
  model?: string; model_provider?: string;
  without_tools?: boolean; interrupted?: 'keep' | 'drop';
}

export interface ChildLine { source_id: string; dest_id: string; linked: boolean }
export interface Receipt {
  dest_id: string; dest_label: string; dest_key: string; location: string; headless: string[];
  interactive: string[]; refusal?: string; manifest_path?: string; losses: string[]; children: ChildLine[];
  reply?: string; warning?: string;
}

export type WriteOperation = { id: number; kind: 'hop' | 'import' } & (
  | { status: 'running' }
  | { status: 'completed'; receipt: Receipt }
  | { status: 'failed'; error: CmdError }
);

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
  /** A member the host (this window, or Shinatama) created: what it sends is the user's. */
  host?: boolean;
}
export type Actor = { type: 'local_admin' } | { type: 'member'; member_id: string | null; name: string; role: string | null; host?: boolean };
/** What a join or rejoin answers: the member, and the handle it writes with. */
export interface Joined { room_id: string; member_id: string; member_handle: string; name: string; role: string }
export interface ChatEvent {
  room_id: string; event_id: string; timestamp: string; actor: Actor; kind: string;
  payload: Record<string, unknown>; recipient_ids: string[];
}
/** A harness as the harnesses page shows it. */
export interface HarnessView {
  id: string; display: string; status: 'installed' | 'configured' | 'missing'; binary?: string; source: string; version?: string;
  store?: string; store_exists: boolean; binary_env: string; store_env: string; destination: boolean;
  install_command?: string; sandbox_package?: string;
}
export interface Found { harness: string; store: string; engine?: string }
export interface FolderScan { dir: string; found: Found[]; visited: number; truncated: boolean }
export interface Sandbox { harness: string; name: string; root: string; store: string; exe: string; package: string; created: string }
export interface Page<T> { items: T[]; next_cursor: string | null; high_water?: string }

// The work a session left on disk, and handing it to another harness (src-tauri/src/actions.rs).
export interface Branch { name: string; upstream: string | null; ahead: number | null; unpushed: number }
export interface Worktree { path: string; branch: string | null; head: string; uncommitted: number; error: string | null }
export interface Repository { path: string; worktrees: Worktree[]; branches: Branch[]; error: string | null }
export interface Work { folder: string; repositories: Repository[]; note: string | null }
export interface HandoffOptions { refusal?: string; destinations: Destination[]; note: string; cwd?: string | null }
export interface HandedOff { launched: boolean; command: string[]; note_path: string; cwd: string }
