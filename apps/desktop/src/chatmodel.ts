// The chat ledger as the message list draws it: pure, so tests/chat.test.ts runs it in Node.

import type { ChatEvent } from './types.ts';

/** A ledger event as the message list draws it: a message, or a muted line for the rest. */
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
    case 'open': return { kind: 'muted', text: `${who} opened the room ${payload.name ?? ''}`.trim(), to: [], mentions: [] };
    case 'bind': return { kind: 'muted', text: `${who} bound a session`, to: [], mentions: [] };
    case 'unbind': return { kind: 'muted', text: `${who} unbound their session`, to: [], mentions: [] };
    default: return { kind: 'muted', text: `${who}: ${event.kind}`, to: [], mentions: [] };
  }
}

/** Events merged by id, oldest first. */
export function mergeEvents(known: ChatEvent[], fresh: ChatEvent[]): ChatEvent[] {
  const byId = new Map(known.map((event) => [event.event_id, event] as const));
  for (const event of fresh) byId.set(event.event_id, event);
  return [...byId.values()].sort((a, b) => Number(BigInt(a.event_id) - BigInt(b.event_id)));
}
