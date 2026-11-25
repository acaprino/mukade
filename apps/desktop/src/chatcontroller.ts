// The chat rooms (spec 2026-11-16, layer A): the state of the room the window shows, read through the running service only, polled
// every 3 s while the chat is shown, and the writes the window makes as the room's host. Rooms on the left (the sidebar), the room in
// three views of the same events (feed, graph) with one selected event and one filter, members and the instructions leads gave
// on the right (the inspector), and under the room a composer. The window starts and stops the service and launches agents; it joins a
// room only when asked to, and never as worker or observer. Plain TypeScript with no screen in it: screens/chat*.tsx draw it.

import { api, errorKind, errorText } from './api.ts';
import {
  GROUPS, commandLine, memberNameRefusal, mergeEvents, passes, requestId, roomNameRefusal, roomNamed, sendRefusal,
  type Filter, type Group,
} from './chatmodel.ts';
import type { Controller } from './controller.ts';
import { Store } from './store.ts';
import type { ChatEvent, ChatStatus, Joined, Member, Room } from './types.ts';

export const POLL_MS = 3000;
const MAX_PAGES = 20;
export const CHAT_VIEWS = ['feed', 'graph'] as const;
export type ChatViewId = (typeof CHAT_VIEWS)[number];
/** What the replay steps through: one event a second. */
const PLAY_MS = 1000;

export const HARNESSES = [['claude_code', 'Claude Code'], ['codex', 'Codex'], ['opencode', 'OpenCode'], ['pi', 'Pi']] as const;
export const ROLES = [['peer', 'peer'], ['lead', 'lead'], ['worker', 'worker'], ['observer', 'observer']] as const;

/** The window's member in a room: the handle it writes with, kept across restarts. */
export interface Mine { member_id: string; handle: string; name: string; role: string; /** When it joined (ms). */ at?: number }
/** What is being written in a room, kept across rooms and restarts. */
export interface Draft { text: string; to: string[] }

function load<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key);
    return raw ? (JSON.parse(raw) as T) : fallback;
  } catch {
    return fallback;
  }
}
function save(key: string, value: unknown): void {
  try { localStorage.setItem(key, JSON.stringify(value)); } catch { /* private window: kept until it closes */ }
}

export class ChatController extends Store {
  status: ChatStatus | null = null;
  rooms: Room[] = [];
  room: string | null = null;
  members: Member[] = [];
  events: ChatEvent[] = [];
  /** The text filter as it is typed; `text` is the part of it the service was asked for. */
  textInput = '';
  text = '';
  kinds: Group[] = [...GROUPS];
  /** The members the filter keeps; none: all of them. */
  only: string[] = [];
  view: ChatViewId = ((stored: string) => (CHAT_VIEWS as readonly string[]).includes(stored) ? stored as ChatViewId : 'feed')(load('mukade.chat.view', 'feed'));
  /** The event picked in each room; none: the newest, followed as events arrive. */
  picked = new Map<string, string>();
  openMember: string | null = null;
  /** The members of the inspector: the person's choice, or null for what the width says (docked: shown; a drawer: closed). */
  membersOpen: boolean | null = null;
  mine: Record<string, Mine> = load('mukade.chat.members', {});
  drafts: Record<string, Draft> = load('mukade.chat.drafts', {});
  busy: string | null = null;
  writeError: string | null = null;
  joinName = 'desk';
  joinRole: 'peer' | 'lead' = 'peer';
  error: string | null = null;
  playing = false;
  /** The event to scroll the feed to, once (a search result). */
  scrollTo: string | null = null;
  private membersAsked = 0;
  /** A send whose answer was lost keeps its request id, so sending the same again writes once. */
  private pending = new Map<string, { id: string; key: string }>();
  private launched: { room: string; at: number; count: number; command: string[] } | null = null;
  private timer = 0;
  private player = 0;
  private debounce = 0;
  private polling = false;
  private again = false;
  private signature = '';

  constructor(private readonly app: Controller) { super(); }

  // --- the view is shown or not -------------------------------------------------------------------

  show(): void {
    if (this.timer) return;
    void this.poll(true);
    this.timer = window.setInterval(() => void this.poll(), POLL_MS);
  }

  hide(): void {
    window.clearInterval(this.timer);
    this.timer = 0;
    this.stopPlay();
  }

  get running(): boolean { return this.status?.state === 'running'; }

  // --- reading ------------------------------------------------------------------------------------

  async poll(force = false): Promise<void> {
    if (this.polling) { this.again ||= force; return; }
    this.polling = true;
    try {
      this.status = await api.chatStatus();
      if (this.status.state === 'running') {
        this.rooms = await this.allRooms();
        if (!this.room || !this.rooms.some((room) => room.room_id === this.room)) this.enter(this.rooms[0]?.room_id ?? null);
        if (this.room) await this.loadRoom(this.room);
        this.forgetGone();
        this.watchLaunch();
      }
      this.error = null;
    } catch (error) {
      this.error = errorText(error);
    } finally {
      this.polling = false;
    }
    // A poll that changed nothing does not redraw: a click is never lost to a redraw under it.
    const signature = JSON.stringify([this.status, this.error, this.room,
      this.rooms.map((room) => [room.room_id, room.revision, room.closed_at, room.members]),
      this.members.map((member) => [member.member_id, member.left_at, member.role, member.last_command_at]), this.events.length, this.events.at(-1)?.event_id]);
    if (force || signature !== this.signature) { this.signature = signature; this.changed(); }
    if (this.again) { this.again = false; void this.poll(true); }
  }

  private async allRooms(): Promise<Room[]> {
    const out: Room[] = [];
    let cursor: string | undefined;
    for (let page = 0; page < MAX_PAGES; page += 1) {
      const reply = await api.chatRooms(cursor);
      out.push(...reply.items);
      if (!reply.next_cursor) break;
      cursor = reply.next_cursor;
    }
    return out;
  }

  private async loadRoom(roomId: string): Promise<void> {
    const asked = Date.now();
    const members = (await api.chatMembers(roomId)).items;
    const since = this.events.length ? this.events[this.events.length - 1].timestamp : undefined;
    let cursor: string | undefined;
    const fresh: ChatEvent[] = [];
    for (let page = 0; page < MAX_PAGES; page += 1) {
      const reply = await api.chatLedger(roomId, { cursor, since, text: this.text || undefined });
      fresh.push(...reply.items);
      if (!reply.next_cursor) break;
      cursor = reply.next_cursor;
    }
    // A slower answer for a room no longer shown does not replace the one shown.
    if (roomId !== this.room) return;
    this.members = members;
    this.membersAsked = asked;
    this.events = mergeEvents(this.events, fresh);
  }

  /** The window's member of the shown room, forgotten once it has left (or was retired). */
  private forgetGone(): void {
    const room = this.room;
    const mine = room ? this.mine[room] : undefined;
    if (!room || !mine || !this.members.length) return;
    const member = this.members.find((one) => one.member_id === mine.member_id);
    // Missing from a list asked for before the join is not gone: the list does not know it yet.
    const gone = member ? member.left_at !== null : this.membersAsked > (mine.at ?? 0);
    if (gone) { delete this.mine[room]; save('mukade.chat.members', this.mine); }
  }

  /** A launched agent joins at a poll; one that has not within a minute is said so. */
  private watchLaunch(): void {
    const launch = this.launched;
    if (!launch) return;
    const room = this.rooms.find((one) => one.room_id === launch.room);
    if (room && room.members > launch.count) {
      this.launched = null;
      this.app.report(`A launched agent joined ${room.name}`);
    } else if (Date.now() - launch.at > 60_000) {
      this.launched = null;
      this.app.report(`No member joined ${room?.name ?? 'the room'} within a minute; the command was: ${commandLine(launch.command)}`, true);
    }
  }

  current(): Room | undefined {
    return this.rooms.find((room) => room.room_id === this.room);
  }

  private enter(id: string | null): void {
    if (id === this.room) return;
    this.room = id;
    this.events = [];
    this.members = [];
    this.openMember = null;
    this.writeError = null;
    this.stopPlay();
  }

  pickRoom(id: string): void {
    if (id === this.room) return;
    this.enter(id);
    this.changed();
    void this.poll(true);
  }

  /** Show a room, with a member's details open or an event selected (search, a session's rooms). */
  openRoom(roomId: string, at: { member?: string; event?: string } = {}): void {
    this.enter(roomId);
    if (at.member) { this.openMember = at.member; if (!this.app.docked) this.membersOpen = true; }
    if (at.event) {
      this.picked.set(roomId, at.event);
      this.textInput = '';
      this.text = '';
      this.kinds = [...GROUPS];
      this.only = [];
      this.view = 'feed';
      this.scrollTo = at.event;
    }
    this.app.switchView('chat');
    this.changed();
    void this.poll(true);
  }

  // --- the selection and the filter ---------------------------------------------------------------

  filter(): Filter {
    return { kinds: this.kinds, members: this.only };
  }

  shown(): ChatEvent[] {
    const filter = this.filter();
    return this.events.filter((event) => passes(event, filter, this.members));
  }

  selected(): ChatEvent | undefined {
    const id = this.room ? this.picked.get(this.room) : undefined;
    return (id === undefined ? undefined : this.events.find((event) => event.event_id === id)) ?? this.events.at(-1);
  }

  select(id: string): void {
    const event = this.events.find((one) => one.event_id === id);
    if (!this.room || !event) return;
    if (event === this.events.at(-1)) this.picked.delete(this.room); else this.picked.set(this.room, id);
    // An event the filter hides clears the filter, so the selection is always in sight.
    if (!passes(event, this.filter(), this.members)) { this.kinds = [...GROUPS]; this.only = []; }
    this.changed();
  }

  /** The person picked an event (the replay's slider, a member's instruction): the replay stops. */
  choose(id: string): void {
    this.stopPlay();
    this.select(id);
  }

  /** The next or previous event the filter shows; false at either end. */
  step(by: number): boolean {
    const shown = this.shown();
    const current = this.selected();
    const at = current ? shown.indexOf(current) : -1;
    const next = shown[at < 0 ? (by > 0 ? 0 : shown.length - 1) : at + by];
    if (!next) return false;
    this.select(next.event_id);
    return true;
  }

  toggleKind(group: Group): void {
    const kinds = this.kinds.includes(group) ? this.kinds.filter((one) => one !== group) : [...this.kinds, group];
    if (!kinds.length) return;
    this.kinds = GROUPS.filter((one) => kinds.includes(one));
    this.changed();
  }

  toggleOnly(id: string): void {
    this.only = this.only.includes(id) ? this.only.filter((one) => one !== id) : [...this.only, id];
    this.changed();
  }

  /** The text filter: the service is asked for it once the person pauses. */
  setText(value: string): void {
    this.textInput = value;
    this.changed();
    window.clearTimeout(this.debounce);
    this.debounce = window.setTimeout(() => {
      const next = value.trim();
      if (next === this.text) return;
      this.text = next;
      this.events = [];
      void this.poll(true);
    }, 300);
  }

  clearFilter(): void {
    this.kinds = [...GROUPS];
    this.only = [];
    window.clearTimeout(this.debounce);
    this.textInput = '';
    if (this.text) { this.text = ''; this.events = []; void this.poll(true); }
    this.changed();
  }

  get filtered(): boolean { return this.text !== '' || this.textInput.trim() !== '' || this.kinds.length < GROUPS.length || this.only.length > 0; }

  setView(view: ChatViewId): void {
    if (this.view === view) return;
    this.view = view;
    save('mukade.chat.view', view);
    if (view === 'feed') this.stopPlay();
    this.changed();
  }

  /** Play the room back from the selection, one event a second; from the start at the newest. */
  togglePlay(): void {
    if (this.playing) { this.stopPlay(); this.changed(); return; }
    if (this.view === 'feed') this.setView('graph');
    const shown = this.shown();
    if (!shown.length) return;
    if (this.selected() === shown.at(-1)) this.select(shown[0].event_id);
    this.playing = true;
    this.player = window.setInterval(() => { if (!this.step(1)) { this.stopPlay(); this.changed(); } }, PLAY_MS);
    this.changed();
  }

  private stopPlay(): void {
    window.clearInterval(this.player);
    this.player = 0;
    this.playing = false;
  }

  /** A room is open: the commands of a room (launch, play) are available. */
  get hasRoom(): boolean { return Boolean(this.current()); }

  setMembersOpen(open: boolean | null): void {
    this.membersOpen = open;
    this.changed();
  }

  toggleMember(id: string): void {
    this.openMember = this.openMember === id ? null : id;
    // Docked, the members are open already; as a drawer a member asks for them.
    if (this.openMember !== null && !this.app.docked) this.membersOpen = true;
    this.changed();
  }

  // --- the composer -------------------------------------------------------------------------------

  draft(): Draft {
    const room = this.room ?? '';
    this.drafts[room] ??= { text: '', to: [] };
    return this.drafts[room];
  }

  private saveDrafts(): void {
    // An empty draft is not kept.
    for (const [room, draft] of Object.entries(this.drafts)) if (!draft.text && !draft.to.length) delete this.drafts[room];
    save('mukade.chat.drafts', this.drafts);
  }

  setDraft(text: string): void {
    if (!this.room) return;
    this.draft().text = text;
    this.saveDrafts();
    if (this.writeError) this.writeError = null;
    this.changed();
  }

  /** Who a message is for: nobody picked is everyone. */
  setRecipients(to: string[]): void {
    this.draft().to = to;
    this.saveDrafts();
    this.changed();
  }

  /** Puts `@name` at the end of the draft. */
  mention(member: Member): void {
    const text = this.draft().text;
    this.setDraft(`${text}${text && !/\s$/.test(text) ? ' ' : ''}@${member.name} `);
  }

  setJoin(name: string, role: 'peer' | 'lead' = this.joinRole): void {
    this.joinName = name.trim();
    this.joinRole = role;
    this.writeError = null;
    this.changed();
  }

  // --- writing ------------------------------------------------------------------------------------

  /** One write with a fresh request id; null when it failed (`writeError` says why). */
  private async write<T>(what: string, operation: string, args: Record<string, unknown>): Promise<T | null> {
    this.busy = what;
    this.writeError = null;
    this.changed();
    try {
      return await api.chatWrite<T>(operation, { request_id: requestId(), ...args });
    } catch (error) {
      this.writeError = errorText(error);
      return null;
    } finally {
      this.busy = null;
    }
  }

  /** A room's administration: a failure goes to the status row, not to the composer. */
  private async administer(what: string, operation: string, room: Room, args: Record<string, unknown> = {}, undo?: () => Promise<void>): Promise<boolean> {
    const done = await this.write(what, operation, { room_id: room.room_id, ...args });
    if (done === null) { this.app.report(`${what} ${room.name} failed: ${this.writeError}`, true); this.writeError = null; }
    else this.app.report(`${operation === 'close' ? 'Closed' : operation === 'reopen' ? 'Reopened' : 'Deleted'} ${room.name}`, false, undo);
    await this.poll(true);
    return done !== null;
  }

  async join(room: Room): Promise<boolean> {
    if (this.busy) return false;
    const refusal = memberNameRefusal(this.joinName, this.members);
    if (refusal) { this.writeError = refusal; this.changed(); return false; }
    const joined = await this.write<Joined>('joining', 'join', { room_id: room.room_id, harness: 'desktop', role: this.joinRole, name: this.joinName, host: true });
    if (joined) {
      this.mine[room.room_id] = { member_id: joined.member_id, handle: joined.member_handle, name: joined.name ?? this.joinName, role: joined.role ?? this.joinRole, at: Date.now() };
      save('mukade.chat.members', this.mine);
      this.app.report(`The window joined ${room.name} as ${this.joinName}`);
    }
    await this.poll(true);
    return joined !== null;
  }

  async rejoin(room: Room): Promise<void> {
    const mine = this.mine[room.room_id];
    if (!mine || this.busy) return;
    const joined = await this.write<Joined>('rejoining', 'rejoin', { room_id: room.room_id, member_id: mine.member_id, host: true });
    if (joined) {
      this.mine[room.room_id] = { ...mine, handle: joined.member_handle, at: Date.now() };
      save('mukade.chat.members', this.mine);
      this.app.report(`The window rejoined ${room.name} as ${mine.name}, with a new handle`);
    }
    await this.poll(true);
  }

  async leave(room: Room): Promise<void> {
    const mine = this.mine[room.room_id];
    if (!mine || this.busy) return;
    const left = await this.write('leaving', 'leave', { room_id: room.room_id, member_handle: mine.handle });
    // A handle the service no longer knows has nothing left to leave.
    if (left !== null || this.writeError?.startsWith('stale_member')) {
      delete this.mine[room.room_id];
      save('mukade.chat.members', this.mine);
      this.writeError = null;
      this.app.report(`The window left ${room.name}`);
    }
    await this.poll(true);
  }

  /** Sends `text` as the window's member. A refusal or a failure keeps the draft, so nothing typed is lost. */
  async send(text: string): Promise<void> {
    const room = this.current();
    const mine = room ? this.mine[room.room_id] : undefined;
    if (!room || !mine || this.busy) return;
    const refusal = sendRefusal(text);
    if (refusal) { this.writeError = refusal; this.draft().text = text; this.saveDrafts(); this.changed(); return; }
    const draft = this.draft();
    const key = JSON.stringify([text, draft.to]);
    const lost = this.pending.get(room.room_id);
    const id = lost?.key === key ? lost.id : requestId();
    this.pending.set(room.room_id, { id, key });
    this.busy = 'sending';
    this.writeError = null;
    this.changed();
    try {
      await api.chatWrite('send', { request_id: id, room_id: room.room_id, member_handle: mine.handle, text, ...(draft.to.length ? { recipients: draft.to } : {}) });
      this.pending.delete(room.room_id);
      draft.text = '';
      this.saveDrafts();
      this.picked.delete(room.room_id);
    } catch (error) {
      // A lost answer keeps the request id: sending the same again cannot write twice. A refusal does not.
      if (errorKind(error) !== 'unreachable') this.pending.delete(room.room_id);
      this.writeError = errorText(error);
      draft.text = text;
      this.saveDrafts();
    } finally {
      this.busy = null;
    }
    await this.poll(true);
  }

  // --- rooms --------------------------------------------------------------------------------------

  /** Opens a room by name; the dialog has checked the name. The answer is the room's id, or null with the failure in `writeError`. */
  async openNewRoom(name: string): Promise<string | null> {
    if (roomNameRefusal(name) || roomNamed(this.rooms, name)) return null;
    try {
      const opened = await api.chatWrite<{ room_id?: string }>('open', { request_id: requestId(), name });
      this.app.report(`Opened ${name}`);
      await this.poll(true);
      const id = opened.room_id ?? roomNamed(this.rooms, name)?.room_id ?? null;
      if (id) this.pickRoom(id);
      return id;
    } catch (error) {
      this.writeError = errorText(error);
      return null;
    }
  }

  /** A room closes at once, and the notice has Undo: it opens the room again. */
  async closeRoom(room: Room): Promise<void> {
    await this.administer('Closing', 'close', room, {}, () => this.reopen(room));
  }

  async reopen(room: Room): Promise<void> {
    await this.administer('Reopening', 'reopen', room);
  }

  /** Delete asks with the room's summary from the service, and deletes that revision only. */
  async deleteRoom(room: Room): Promise<void> {
    let fresh: Room | undefined;
    try { fresh = (await this.allRooms()).find((one) => one.room_id === room.room_id); } catch (error) { this.app.report(errorText(error), true); return; }
    if (!fresh) { await this.poll(true); return; }
    const summary = `${fresh.members} member${fresh.members === 1 ? '' : 's'} and ${fresh.events} event${fresh.events === 1 ? '' : 's'} go with it, for good (revision ${fresh.revision}).`;
    const target = fresh;
    this.app.confirm(`Delete ${target.name}?`, summary, `Delete ${target.name}`, () => void this.administer('Deleting', 'delete', target, { expected_revision: target.revision, confirm: true }));
  }

  // --- the service and launches -------------------------------------------------------------------

  async startService(): Promise<void> {
    if (this.busy) return;
    this.busy = 'starting the service';
    this.changed();
    try {
      await api.chatStart();
      this.app.report('Chat service started');
    } catch (error) {
      this.app.report(`The chat service did not start: ${errorText(error)}`, true);
    } finally {
      this.busy = null;
    }
    await this.poll(true);
  }

  /** Stopping the service cuts what the members are doing in it: it asks first. */
  stopService(): void {
    const open = this.rooms.filter((room) => room.closed_at === null);
    const members = open.reduce((sum, room) => sum + room.members, 0);
    const text = `${members} member${members === 1 ? '' : 's'} in ${open.length} open room${open.length === 1 ? '' : 's'} stop receiving until it starts again. Rooms and messages stay.`;
    this.app.confirm('Stop the chat service?', text, 'Stop the service', () => void this.stop());
  }

  private async stop(): Promise<void> {
    this.busy = 'stopping the service';
    this.changed();
    try {
      await api.chatStop();
      this.app.report('Chat service stopped');
    } catch (error) {
      this.app.report(`The chat service did not stop: ${errorText(error)}`, true);
    } finally {
      this.busy = null;
    }
    await this.poll(true);
  }

  /** Runs an agent in the shown room, in a new terminal; answers the command it runs. */
  async launch(harness: string, role: string, listen: number, cwd?: string): Promise<string[]> {
    const room = this.current();
    if (!room) throw new Error('Pick a room first');
    const command = await api.chatLaunch(harness, room.name, role, listen, cwd);
    this.launched = { room: room.room_id, at: Date.now(), count: room.members, command };
    this.app.report(`Launched ${HARNESSES.find(([key]) => key === harness)?.[1]} into ${room.name}; it appears when it joins`);
    return command;
  }
}
