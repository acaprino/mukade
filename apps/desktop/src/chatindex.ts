// What every room holds, for the search and a session's rooms: the rooms and their members,
// read through the running service and kept for a few seconds. Nothing when the service is off.

import { api } from './api.ts';
import { harnessKey } from './chatmodel.ts';
import type { Member, Room } from './types.ts';

export interface RoomMember { room: Room; member: Member }
export interface ChatIndex { rooms: Room[]; members: RoomMember[] }

const MAX_AGE_MS = 10_000;
let kept: { at: number; index: ChatIndex } | null = null;
let reading: Promise<ChatIndex | null> | null = null;

async function read(): Promise<ChatIndex | null> {
  const status = await api.chatStatus();
  if (status.state !== 'running') return null;
  const rooms: Room[] = [];
  let cursor: string | undefined;
  for (let page = 0; page < 20; page += 1) {
    const reply = await api.chatRooms(cursor);
    rooms.push(...reply.items);
    if (!reply.next_cursor) break;
    cursor = reply.next_cursor;
  }
  const members = (await Promise.all(rooms.map(async (room) =>
    (await api.chatMembers(room.room_id)).items.map((member) => ({ room, member }))))).flat();
  return { rooms, members };
}

/** The rooms and members of every room; `fresh` reads them again whatever their age. */
export async function chatIndex(fresh = false): Promise<ChatIndex | null> {
  if (!fresh && kept && Date.now() - kept.at < MAX_AGE_MS) return kept.index;
  reading ??= read().finally(() => { reading = null; });
  const index = await reading;
  kept = index ? { at: Date.now(), index } : null;
  return index;
}

/** The rooms where a member is bound to this session. */
export function boundTo(index: ChatIndex, harness: string, id: string): RoomMember[] {
  return index.members.filter(({ member }) => member.session?.id === id && harnessKey(member.session.harness) === harness);
}
