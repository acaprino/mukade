// The chat ledger as the room's views draw it, and the checks the window makes before it writes:
// pure, so tests/chat.test.ts runs it in Node.

import type { ChatEvent, Member, Room } from './types.ts';

/** A ledger event as one line of text: a message, or a muted line for the rest. */
export function eventLine(event: ChatEvent, names: Map<string, string>): { kind: 'message' | 'muted'; text: string; to: string[]; mentions: string[] } {
  const who = event.actor.type === 'member' ? event.actor.name : 'local admin';
  const payload = event.payload as Record<string, unknown>;
  const list = (key: string) => (Array.isArray(payload[key]) ? (payload[key] as unknown[]).map(String) : []);
  const targets = event.recipient_ids.map((id) => names.get(id) ?? id);
  switch (event.kind) {
    case 'message': return { kind: 'message', text: String(payload.text ?? ''), to: list('recipients'), mentions: list('mentions') };
    case 'observe': return { kind: 'muted', text: `${who} observed ${targets.join(', ') || 'a member'}`, to: [], mentions: [] };
    case 'signal': return { kind: 'muted', text: `${who} signalled ${payload.signal}${payload.reason ? `: ${payload.reason}` : ''}`, to: [], mentions: [] };
    case 'join': case 'rejoin': return { kind: 'muted', text: `${who} ${event.kind === 'join' ? 'joined' : 'rejoined'} as ${payload.role ?? 'member'}`, to: [], mentions: [] };
    case 'leave': return { kind: 'muted', text: `${who} left the room`, to: [], mentions: [] };
    case 'open': return { kind: 'muted', text: `${who} opened the room ${payload.name ?? ''}`.trim(), to: [], mentions: [] };
    case 'close': return { kind: 'muted', text: `${who} closed the room`, to: [], mentions: [] };
    case 'reopen': return { kind: 'muted', text: `${who} reopened the room`, to: [], mentions: [] };
    case 'bind': return { kind: 'muted', text: `${who} bound a session`, to: [], mentions: [] };
    case 'unbind': return { kind: 'muted', text: `${who} unbound their session`, to: [], mentions: [] };
    case 'set_role': case 'role': return { kind: 'muted', text: `${who} changed a role${payload.role ? ` to ${payload.role}` : ''}`, to: [], mentions: [] };
    case 'retire': return { kind: 'muted', text: `${who} retired ${targets.join(', ') || 'a member'}`, to: [], mentions: [] };
    default: return { kind: 'muted', text: `${who}: ${event.kind}`, to: [], mentions: [] };
  }
}

/** Events merged by id, oldest first. */
export function mergeEvents(known: ChatEvent[], fresh: ChatEvent[]): ChatEvent[] {
  const byId = new Map(known.map((event) => [event.event_id, event] as const));
  for (const event of fresh) byId.set(event.event_id, event);
  return [...byId.values()].sort((a, b) => Number(BigInt(a.event_id) - BigInt(b.event_id)));
}

/** The three kinds the room's filter knows: messages; signals; everything else. */
export type Group = 'message' | 'signal' | 'system';
export const GROUPS: readonly Group[] = ['message', 'signal', 'system'];
export const groupOf = (event: ChatEvent): Group => (event.kind === 'message' ? 'message' : event.kind === 'signal' ? 'signal' : 'system');

/** The member that did an event, when a member did it. */
export const actorOf = (event: ChatEvent): string | null => (event.actor.type === 'member' ? event.actor.member_id : null);

/** The members an event is addressed to: the service's ids, else a message's names looked up. */
export function recipientsOf(event: ChatEvent, members: Member[]): string[] {
  if (event.recipient_ids.length) return event.recipient_ids;
  const names = Array.isArray(event.payload.recipients) ? (event.payload.recipients as unknown[]).map(String) : [];
  return names.flatMap((name) => members.find((member) => member.name === name)?.member_id ?? []);
}

export interface Filter { kinds: readonly Group[]; members: readonly string[] }
/** Whether the filter shows an event: its kind, and with members chosen, one of them did it or receives it. */
export function passes(event: ChatEvent, filter: Filter, members: Member[]): boolean {
  if (!filter.kinds.includes(groupOf(event))) return false;
  if (!filter.members.length) return true;
  const actor = actorOf(event);
  return (actor !== null && filter.members.includes(actor)) || recipientsOf(event, members).some((id) => filter.members.includes(id));
}

export interface Instruction { event: ChatEvent; worker: Member; state: 'waiting' | 'blocked' | 'done' }
/** Every message a lead addressed to a worker, oldest first, with the state the worker's signals give it. */
export function instructions(events: ChatEvent[], members: Member[]): Instruction[] {
  const out: Instruction[] = [];
  for (const event of events) {
    if (event.kind !== 'message' || event.actor.type !== 'member' || event.actor.role !== 'lead') continue;
    for (const id of recipientsOf(event, members)) {
      const worker = members.find((member) => member.member_id === id);
      if (worker?.role !== 'worker') continue;
      let state: Instruction['state'] = 'waiting';
      for (const other of events) {
        if (other.kind !== 'signal' || String(other.payload.instruction_event_id) !== event.event_id || actorOf(other) !== id) continue;
        if (other.payload.signal === 'done' || other.payload.signal === 'blocked') state = other.payload.signal;
      }
      out.push({ event, worker, state });
    }
  }
  return out;
}

/** The instruction a signal answers. */
export const instructionOf = (signal: ChatEvent, events: ChatEvent[]): ChatEvent | undefined =>
  events.find((event) => event.event_id === String(signal.payload.instruction_event_id));

/** A line one event draws between two members; `hub` stands for everyone. */
export interface Segment { from: string; to: string; type: 'msg' | 'done' | 'blocked' | 'obs' }
/** The lines one event draws: a message to each recipient (or to the hub), a signal back to its
 *  instruction's lead, an observation to the member observed. */
export function segments(event: ChatEvent, events: ChatEvent[], members: Member[]): Segment[] {
  const from = actorOf(event);
  if (!from) return [];
  if (event.kind === 'message') {
    const to = recipientsOf(event, members);
    return (to.length ? to : ['hub']).map((id) => ({ from, to: id, type: 'msg' as const }));
  }
  if (event.kind === 'signal') {
    const instruction = instructionOf(event, events);
    const to = instruction ? actorOf(instruction) : null;
    return to ? [{ from, to, type: event.payload.signal === 'blocked' ? 'blocked' : 'done' }] : [];
  }
  if (event.kind === 'observe') return event.recipient_ids.map((to) => ({ from, to, type: 'obs' as const }));
  return [];
}

/** The lines of a room up to and including the event at `upTo`: one per pair of members and
 *  kind, whichever way it ran, with how many events drew it. Events the filter hides draw none. */
export function linesUpTo(events: ChatEvent[], upTo: number, members: Member[], filter: Filter): (Segment & { count: number })[] {
  const lines = new Map<string, Segment & { count: number }>();
  for (const event of events.slice(0, upTo + 1)) {
    if (!passes(event, filter, members)) continue;
    for (const segment of segments(event, events, members)) {
      const key = `${[segment.from, segment.to].sort().join('|')}|${segment.type}`;
      lines.set(key, { ...segment, count: (lines.get(key)?.count ?? 0) + 1 });
    }
  }
  return [...lines.values()];
}

/** The members in the room at a moment: joined by then, and whether they had left. */
export function presentAt(members: Member[], timestamp: string): { member: Member; left: boolean }[] {
  const at = Date.parse(timestamp);
  return members.filter((member) => Date.parse(member.joined_at) <= at)
    .map((member) => ({ member, left: member.left_at !== null && Date.parse(member.left_at) <= at }));
}

/** The names a text addresses with `@name`, as the service reads them: active members only. */
export function mentionsIn(text: string, members: Member[]): string[] {
  const names = new Set<string>();
  for (const found of text.matchAll(/(^|[^A-Za-z0-9_@.-])@([a-z0-9][a-z0-9_-]{0,31})/g)) {
    if (members.some((member) => member.name === found[2] && member.left_at === null)) names.add(found[2]);
  }
  return [...names];
}

/** The longest message the service takes, in UTF-8 bytes. */
export const MAX_MESSAGE_BYTES = 65_536;
/** Why a text cannot be sent, checked before the call; null when it can. */
export function sendRefusal(text: string): string | null {
  if (!text.trim()) return 'Write something first.';
  if (text.includes('\0')) return 'The text holds a NUL character.';
  const bytes = new TextEncoder().encode(text).length;
  return bytes > MAX_MESSAGE_BYTES ? `The message is ${bytes} bytes; the limit is ${MAX_MESSAGE_BYTES}.` : null;
}

const NAME = /^[a-z0-9][a-z0-9_-]*$/;
/** Why a room name cannot be used, by the service's rule; null when it can. */
export function roomNameRefusal(name: string): string | null {
  if (!name) return 'Give the room a name.';
  if (name.length > 64 || !NAME.test(name)) return 'Lowercase letters, digits, - and _ (up to 64), starting with a letter or a digit.';
  return null;
}

/** Why the window's member cannot take a name in a room; null when it can. */
export function memberNameRefusal(name: string, members: Member[]): string | null {
  if (!name) return 'Give the window a name in the room.';
  if (name === 'all') return '“all” is reserved: it addresses everyone.';
  if (name.length > 32 || !NAME.test(name)) return 'Lowercase letters, digits, - and _ (up to 32), starting with a letter or a digit.';
  if (members.some((member) => member.name === name && member.left_at === null)) return `${name} is taken in this room.`;
  return null;
}

/** An open room of that name, else a closed one: what a new room's name would meet. */
export function roomNamed(rooms: Room[], name: string): Room | undefined {
  const same = rooms.filter((room) => room.name === name);
  return same.find((room) => room.closed_at === null) ?? same[0];
}

/** A request id for one write: a UUID v4, so a repeated click or a lost answer writes once. */
export function requestId(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(16));
  bytes[6] = (bytes[6] & 0x0f) | 0x40;
  bytes[8] = (bytes[8] & 0x3f) | 0x80;
  const hex = [...bytes].map((byte) => byte.toString(16).padStart(2, '0')).join('');
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

/** The binding's harness as the window names harnesses (`claude` and `claude-code` are `claude_code`). */
export const harnessKey = (harness: string): string => (harness === 'claude' || harness === 'claude-code' ? 'claude_code' : harness.replace(/-/g, '_'));

/** Case-insensitive containment, for the searches. */
export const matches = (text: string | null | undefined, query: string): boolean =>
  Boolean(text) && text!.toLowerCase().includes(query.trim().toLowerCase());

/** A command line as one shell-ish line, each part quoted when it holds a space or a quote. */
export function commandLine(argv: string[]): string {
  return argv.map((part) => (/[\s"']/.test(part) || part === '' ? `"${part.replace(/"/g, '\\"')}"` : part)).join(' ');
}
