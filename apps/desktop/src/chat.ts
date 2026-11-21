// The chat rooms (spec 2026-11-16, layer A): rooms on the left; in the middle the room in three
// views of the same events (graph, lanes, feed) with one selected event and one filter; members
// and the instructions leads gave on the right; under the room a composer, through which the
// window writes as the room's host. Through the running service only, polled every 3 s while the
// view is shown. The window starts and stops the service and launches agents; it joins a room
// only when asked to, and never as worker or observer.

import './chat.css';
import { api, errorKind, errorText } from './api.ts';
import { claimKey, keyClaimed } from '@acaprino/imago/ui';
import { button, caps, commandLine, fill, h, icon } from './dom.ts';
import { listbox, reveal, stepIn } from './lists.ts';
import { confirmModal, openModal } from './modal.ts';
import { shortTime } from './tree.ts';
import {
  GROUPS, harnessKey, instructions, memberNameRefusal, mentionsIn, mergeEvents, passes, requestId, roomNameRefusal, roomNamed, sendRefusal,
  type Filter, type Group,
} from './chatmodel.ts';
import { animateNow, feedCard, feedView, graphView, lanesView, mark, stripView, type RoomContext } from './chatviews.ts';
import type { ChatEvent, ChatStatus, Joined, Member, Room } from './types.ts';

const POLL_MS = 3000;
const MAX_PAGES = 20;
const VIEWS = ['graph', 'lanes', 'feed'] as const;
export type View = (typeof VIEWS)[number];
const KIND_LABEL: Record<Group, string> = { message: 'Messages', signal: 'Signals', system: 'System' };
const HARNESSES = [['claude_code', 'Claude Code'], ['codex', 'Codex'], ['opencode', 'OpenCode'], ['pi', 'Pi']] as const;
const ROLES = [['peer', 'peer'], ['lead', 'lead'], ['worker', 'worker'], ['observer', 'observer']] as const;

/** The window's member in a room: the handle it writes with, kept across restarts. */
interface Mine { member_id: string; handle: string; name: string; role: string; /** When it joined (ms). */ at?: number }
/** What is being written in a room, kept across rooms and restarts. */
interface Draft { text: string; to: string[] }

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

/** One choice among a few, as pressed buttons. */
function pickOne<T extends string>(label: string, options: readonly (readonly [T, string])[], current: T, pick: (value: T) => void): HTMLElement {
  return h('div', { class: 'seg im-seg', role: 'group', 'aria-label': label },
    ...options.map(([value, text]) => button(text, { 'aria-pressed': String(value === current), onclick: () => pick(value) })));
}

export interface ChatHooks {
  /** Show a bound session in the sessions view; false when it is not listed. */
  showSession(harness: string, id: string): boolean;
  status(text: string, error?: boolean): void;
}

export class ChatView {
  private status: ChatStatus | null = null;
  private rooms: Room[] = [];
  private room: string | null = null;
  private members: Member[] = [];
  /** When the members shown were asked for (ms): an answer asked before a join does not know it. */
  private membersAsked = 0;
  private events: ChatEvent[] = [];
  /** The text filter, asked of the service. */
  private text = '';
  private kinds: Group[] = [...GROUPS];
  /** The members the filter keeps; none: all of them. */
  private only: string[] = [];
  private view: View = ((stored: string) => (VIEWS as readonly string[]).includes(stored) ? stored as View : 'graph')(load('mukade.chat.view', 'graph'));
  /** The event picked in each room; none: the newest, followed as events arrive. */
  private picked = new Map<string, string>();
  private openMember: string | null = null;
  private mine: Record<string, Mine> = load('mukade.chat.members', {});
  private drafts: Record<string, Draft> = load('mukade.chat.drafts', {});
  /** A send whose answer was lost keeps its request id, so sending the same again writes once. */
  private pending = new Map<string, { id: string; key: string }>();
  private busy: string | null = null;
  private writeError: string | null = null;
  private joinName = 'desk';
  private joinRole: 'peer' | 'lead' = 'peer';
  private error: string | null = null;
  private timer = 0;
  private polling = false;
  private again = false;
  private signature = '';
  private playing = 0;
  private animated: string | null = null;
  private launched: { room: string; at: number; count: number; command: string[] } | null = null;
  private graphHost: HTMLElement | null = null;
  /** The composer takes the focus once it is drawn (after a join). */
  private focusComposer = false;
  private painted = '';
  private readonly resize = new ResizeObserver(() => this.paintGraph(false));
  focus: 'rooms' | 'messages' | 'members' = 'rooms';

  /** Fields that live as long as the view: a redraw moves them, never rebuilds them, so what is
   *  being typed (and the caret) survives the poll. */
  private readonly textInput: HTMLInputElement;
  private readonly roomInput: HTMLInputElement;
  private readonly composer: HTMLTextAreaElement;
  private readonly nameInput: HTMLInputElement;
  private readonly composerNote = h('div', { class: 'composer-note', 'aria-live': 'polite' });
  private readonly joinNote = h('div', { class: 'composer-note', 'aria-live': 'polite' });

  constructor(private readonly root: HTMLElement, private readonly hooks: ChatHooks) {
    this.textInput = h('input', { id: 'chat-filter', type: 'search', placeholder: 'Filter messages', 'aria-label': 'Filter the messages by text', autocomplete: 'off', spellcheck: 'false' }) as HTMLInputElement;
    let debounce = 0;
    this.textInput.addEventListener('input', () => {
      window.clearTimeout(debounce);
      debounce = window.setTimeout(() => {
        const next = this.textInput.value.trim();
        if (next === this.text) return;
        this.text = next;
        this.events = [];
        void this.poll(true);
      }, 300);
    });
    this.roomInput = h('input', { id: 'chat-room-filter', type: 'search', placeholder: 'Filter rooms', 'aria-label': 'Filter the rooms by name', autocomplete: 'off', spellcheck: 'false' }) as HTMLInputElement;
    this.roomInput.addEventListener('input', () => this.draw());
    this.composer = h('textarea', { id: 'chat-text', rows: '3', placeholder: 'Write to the room. @name addresses a member.', 'aria-label': 'Message' }) as HTMLTextAreaElement;
    this.composer.addEventListener('input', () => {
      if (!this.room) return;
      this.draft().text = this.composer.value;
      this.saveDrafts();
      if (this.writeError) this.writeError = null;
      this.paintComposerNote();
    });
    this.composer.addEventListener('keydown', (key) => { if (key.key === 'Enter' && (key.ctrlKey || key.metaKey)) { key.preventDefault(); void this.send(); } });
    this.nameInput = h('input', { id: 'chat-join-name', type: 'text', value: this.joinName, 'aria-label': 'The window’s name in the room', autocomplete: 'off', spellcheck: 'false' }) as HTMLInputElement;
    this.nameInput.addEventListener('input', () => { this.joinName = this.nameInput.value.trim(); this.writeError = null; this.paintJoinNote(); });
    this.nameInput.addEventListener('keydown', (key) => {
      const room = this.current();
      if (key.key === 'Enter' && room) { key.preventDefault(); void this.join(room); }
    });
    this.root.addEventListener('keydown', (event) => this.key(event));
  }

  /** ↑ ↓ j k move the room, the event or the member of the focused column, and Space plays the room back. The rooms'
   *  own list answers its keys itself (lists.ts); a field keeps every key; a button keeps Space. */
  private key(event: KeyboardEvent): void {
    if (keyClaimed(event) || event.defaultPrevented || event.isComposing || event.altKey || event.ctrlKey || event.metaKey) return;
    const target = event.target as HTMLElement;
    if (target.closest('input, textarea, select, [role="listbox"]')) return;
    const step = ({ ArrowDown: 1, j: 1, ArrowUp: -1, k: -1 } as Record<string, number>)[event.key];
    if (step !== undefined) this.move(step);
    else if (event.key === ' ' && !target.closest('button, summary')) this.togglePlay();
    else return;
    event.preventDefault();
    claimKey(event);
  }

  show(): void {
    this.draw();
    void this.poll(true);
    this.timer = window.setInterval(() => void this.poll(), POLL_MS);
  }

  hide(): void {
    window.clearInterval(this.timer);
    this.timer = 0;
    this.stopPlay();
    this.resize.disconnect();
  }

  // --- reading ------------------------------------------------------------------------------------

  private async poll(force = false): Promise<void> {
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
      this.members.map((member) => [member.member_id, member.left_at, member.role]), this.events.length, this.events.at(-1)?.event_id]);
    if (force || signature !== this.signature) { this.signature = signature; this.draw(); }
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
      this.hooks.status(`a launched agent joined ${room.name}`);
    } else if (Date.now() - launch.at > 60_000) {
      this.launched = null;
      this.hooks.status(`no member joined ${room?.name ?? 'the room'} within a minute; the command was: ${commandLine(launch.command)}`, true);
    }
  }

  private current(): Room | undefined {
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
    this.composer.value = id ? this.drafts[id]?.text ?? '' : '';
  }

  private pickRoom(id: string): void {
    if (id === this.room) return;
    this.enter(id);
    this.draw();
    void this.poll(true);
  }

  /** Show a room, with a member's details open or an event selected (search, a session's rooms). */
  openRoom(roomId: string, at: { member?: string; event?: string } = {}): void {
    this.enter(roomId);
    if (at.member) { this.openMember = at.member; this.focus = 'members'; }
    if (at.event) {
      this.picked.set(roomId, at.event);
      this.textInput.value = '';
      this.text = '';
      this.kinds = [...GROUPS];
      this.only = [];
      this.view = 'feed';
      this.focus = 'messages';
    }
    if (this.timer) { this.draw(); void this.poll(true); }
  }

  // --- the selection and the filter ---------------------------------------------------------------

  private filter(): Filter {
    return { kinds: this.kinds, members: this.only };
  }

  private shown(): ChatEvent[] {
    const filter = this.filter();
    return this.events.filter((event) => passes(event, filter, this.members));
  }

  private selected(): ChatEvent | undefined {
    const id = this.room ? this.picked.get(this.room) : undefined;
    return (id === undefined ? undefined : this.events.find((event) => event.event_id === id)) ?? this.events.at(-1);
  }

  private select(id: string): void {
    const event = this.events.find((one) => one.event_id === id);
    if (!this.room || !event) return;
    if (event === this.events.at(-1)) this.picked.delete(this.room); else this.picked.set(this.room, id);
    // An event the filter hides clears the filter, so the selection is always in sight.
    if (!passes(event, this.filter(), this.members)) { this.kinds = [...GROUPS]; this.only = []; }
    this.focus = 'messages';
    this.draw();
  }

  /** The next or previous event the filter shows; false at either end. */
  private step(by: number): boolean {
    const shown = this.shown();
    const current = this.selected();
    const at = current ? shown.indexOf(current) : -1;
    const next = shown[at < 0 ? (by > 0 ? 0 : shown.length - 1) : at + by];
    if (!next) return false;
    this.select(next.event_id);
    return true;
  }

  private toggleKind(group: Group): void {
    const kinds = this.kinds.includes(group) ? this.kinds.filter((one) => one !== group) : [...this.kinds, group];
    if (!kinds.length) return;
    this.kinds = GROUPS.filter((one) => kinds.includes(one));
    this.draw();
  }

  private toggleOnly(id: string): void {
    this.only = this.only.includes(id) ? this.only.filter((one) => one !== id) : [...this.only, id];
    this.draw();
  }

  private clearFilter(): void {
    this.kinds = [...GROUPS];
    this.only = [];
    if (this.text) { this.textInput.value = ''; this.text = ''; this.events = []; void this.poll(true); }
    this.draw();
  }

  setView(view: View): void {
    if (this.view === view) return;
    this.view = view;
    save('mukade.chat.view', view);
    if (view !== 'graph') this.stopPlay();
    this.draw();
  }

  /** Play the room back from the selection, one event a second; from the start at the newest. */
  togglePlay(): void {
    if (this.playing) { this.stopPlay(); this.draw(); return; }
    if (this.view !== 'graph') { this.view = 'graph'; save('mukade.chat.view', 'graph'); }
    const shown = this.shown();
    if (!shown.length) return;
    if (this.selected() === shown.at(-1)) this.select(shown[0].event_id);
    this.playing = window.setInterval(() => { if (!this.step(1)) { this.stopPlay(); this.draw(); } }, 1000);
    this.draw();
  }

  private stopPlay(): void {
    window.clearInterval(this.playing);
    this.playing = 0;
  }

  // --- keys from the window -----------------------------------------------------------------------

  /** ↑ ↓ on the focused column: a room, an event, a member. */
  move(step: number): void {
    if (this.focus === 'rooms') {
      const next = stepIn(this.visibleRooms().map((room) => room.room_id), this.room, step);
      if (next) this.pickRoom(next);
    } else if (this.focus === 'messages') {
      this.stopPlay();
      this.step(step);
    } else {
      const next = stepIn(this.members.map((member) => member.member_id), this.openMember, step);
      if (next) { this.openMember = next; this.draw(); }
    }
  }

  column(step: number): void {
    const order: ChatView['focus'][] = ['rooms', 'messages', 'members'];
    this.focus = order[Math.max(0, Math.min(2, order.indexOf(this.focus) + step))];
    this.draw();
  }

  /** A room is open: the commands of a room (launch, play) are available. */
  hasRoom(): boolean { return Boolean(this.current()); }

  focusFilter(): void {
    (this.focus === 'rooms' ? this.roomInput : this.textInput).focus();
  }

  // --- drawing ------------------------------------------------------------------------------------

  private draw(): void {
    const running = this.status?.state === 'running';
    const current = this.current();
    const bar = this.statusBar(running, current);
    if (!running) { this.graphHost = null; this.drawOff(bar); return; }
    const kept = this.remember();
    fill(this.root, bar, h('div', { class: 'chat-columns' }, this.roomsColumn(), this.stageColumn(current), this.membersColumn(current)));
    this.paintComposerNote();
    this.paintJoinNote();
    this.restore(kept);
    if (this.focusComposer && this.composer.isConnected) { this.focusComposer = false; this.composer.focus(); }
    this.paintGraph(true);
  }

  /** What a redraw must give back: the focused field and its caret, a focused button, the scrolls. */
  private remember() {
    const active = document.activeElement as HTMLElement | null;
    const field = [this.textInput, this.roomInput, this.composer, this.nameInput].find((one) => one === active) ?? null;
    const focused = !field && active && this.root.contains(active) && active.matches('button') ? `${active.className}|${active.getAttribute('aria-label') ?? ''}|${active.textContent}` : null;
    const list = this.root.querySelector<HTMLElement>('.stage-body > .feed, .stage-body > .lanes');
    return {
      field, start: field?.selectionStart ?? null, end: field?.selectionEnd ?? null, focused,
      list: list?.scrollTop ?? null, members: this.root.querySelector<HTMLElement>('#chat-members .column-body')?.scrollTop ?? 0,
    };
  }

  private restore(kept: ReturnType<ChatView['remember']>): void {
    if (kept.field && kept.field.isConnected) {
      kept.field.focus({ preventScroll: true });
      if (kept.start !== null && kept.end !== null) kept.field.setSelectionRange(kept.start, kept.end);
    } else if (kept.focused) {
      const again = [...this.root.querySelectorAll<HTMLElement>('button')].find((one) => `${one.className}|${one.getAttribute('aria-label') ?? ''}|${one.textContent}` === kept.focused);
      again?.focus({ preventScroll: true });
    }
    const members = this.root.querySelector<HTMLElement>('#chat-members .column-body');
    if (members) members.scrollTop = kept.members;
    const list = this.root.querySelector<HTMLElement>('.stage-body > .feed, .stage-body > .lanes');
    if (list) {
      const following = this.room !== null && !this.picked.has(this.room);
      if (following) list.scrollTop = list.scrollHeight;
      else {
        if (kept.list !== null) list.scrollTop = kept.list;
        list.querySelector('[aria-selected="true"]')?.scrollIntoView({ block: 'nearest' });
      }
    }
    reveal(this.root.querySelector('#chat-rooms .list'));
  }

  private statusBar(running: boolean, current: Room | undefined): HTMLElement {
    const status = this.status;
    const state = status?.state ?? 'checking';
    const label = state === 'running' ? 'service running' : state === 'stopped' ? 'service stopped' : state === 'checking' ? 'checking…' : 'unreachable';
    return h('div', { class: 'chat-status' },
      h('span', { class: `tag im-chip ${state === 'running' ? 'live' : state === 'stopped' ? '' : 'bad'}` }, label),
      h('span', { class: 'mono muted' }, status?.endpoint ?? status?.data_dir ?? ''),
      h('span', { class: 'spring' }),
      this.error ? h('span', { class: 'err-text' }, this.error) : null,
      this.busy ? h('span', { class: 'muted' }, `${this.busy}…`) : null,
      running ? button([icon('term'), 'Launch agent ', caps('L')], {
        disabled: !current || current.closed_at !== null,
        title: current ? `Run an agent in ${current.name}, in a new terminal` : 'Pick a room first',
        onclick: () => this.launchDialog(),
      }) : null,
      running ? button('Stop service', { disabled: this.busy !== null, onclick: () => void this.stopService() }) : null,
      h('span', { class: 'mono muted' }, `checked every ${POLL_MS / 1000} s`));
  }

  private drawOff(bar: HTMLElement): void {
    const status = this.status;
    fill(this.root, bar, h('div', { class: 'column im-panel chat-off' }, h('div', { class: 'empty im-empty' },
      h('h2', {}, status ? (status.state === 'stopped' ? 'The chat service is not running' : 'The chat service does not answer') : 'Looking for the chat service…'),
      status ? h('p', {}, 'Start it here, or in a terminal with ', h('code', {}, 'mukade chat start'), '.') : null,
      status?.message ? h('p', { class: 'note' }, status.message) : null,
      status?.state === 'stopped'
        ? button([icon('play'), this.busy ? 'Starting…' : 'Start the service'], { class: 'primary', disabled: this.busy !== null, onclick: () => void this.startService() })
        : null)));
  }

  private visibleRooms(): Room[] {
    const query = this.roomInput.value.trim().toLowerCase();
    return query ? this.rooms.filter((room) => room.name.includes(query)) : this.rooms;
  }

  private roomsColumn(): HTMLElement {
    const rooms = this.visibleRooms();
    const rows = rooms.map((room) => ({
      id: room.room_id,
      class: 'room-row',
      cells: [
        h('span', { class: 'grow' }, h('span', { class: 'room-name' }, room.name),
          h('span', { class: 'num im-num room-meta' }, `${room.members} members · ${room.events} events · ${shortTime(room.last_activity ?? room.opened_at)}`)),
        ...(this.mine[room.room_id] ? [h('span', { class: 'tag im-chip', title: 'The window is a member of this room' }, 'joined')] : []),
        h('span', { class: `tag im-chip ${room.closed_at ? '' : 'live'}` }, room.closed_at ? 'closed' : 'open'),
      ] as Node[],
    }));
    const body = rooms.length
      ? listbox('Rooms', rows, this.room, (id) => { this.focus = 'rooms'; this.pickRoom(id); })
      : h('div', { class: 'empty im-empty' }, h('p', {}, this.rooms.length ? 'No room matches the filter.' : 'No rooms yet.'),
        this.rooms.length
          ? button('Clear the filter', { onclick: () => { this.roomInput.value = ''; this.draw(); } })
          : button([icon('plus'), 'New room'], { onclick: () => this.newRoomDialog() }));
    return h('section', { class: `column im-panel${this.focus === 'rooms' ? ' focused' : ''}`, id: 'chat-rooms', 'aria-label': 'Rooms' },
      h('div', { class: 'column-head im-panel-header' }, h('h2', {}, 'Rooms'), h('small', {}, String(this.rooms.length)), h('span', { class: 'spring' }),
        button([icon('plus'), 'New ', caps('N')], { class: 'ghost', title: 'Open a new room', onclick: () => this.newRoomDialog() })),
      h('div', { class: 'column-tools' }, this.roomInput),
      h('div', { class: 'column-body im-panel-body' }, body));
  }

  private context(): RoomContext {
    return {
      events: this.events, members: this.members, filter: this.filter(), selected: this.selected(),
      mine: this.room ? this.mine[this.room]?.member_id ?? null : null,
      select: (id) => { this.stopPlay(); this.select(id); },
    };
  }

  private stageColumn(current: Room | undefined): HTMLElement {
    const ctx = this.context();
    const shown = this.shown();
    const filtered = this.text !== '' || this.kinds.length < GROUPS.length || this.only.length > 0;
    const head = h('div', { class: 'column-head im-panel-header' },
      h('h2', {}, current?.name ?? 'Room'),
      current ? h('small', {}, `${this.events.length} events${filtered ? ` · ${shown.length} shown` : ''}`) : null,
      current?.closed_at ? h('span', { class: 'tag im-chip' }, 'closed') : null,
      h('span', { class: 'spring' }),
      h('div', { class: 'seg im-seg views', role: 'group', 'aria-label': 'View' },
        ...VIEWS.map((view, i) => button([icon(view), view[0].toUpperCase() + view.slice(1), caps(String(i + 1))], {
          'aria-pressed': String(this.view === view), onclick: () => this.setView(view),
        }))),
      ...(current ? this.roomActions(current) : []));
    const tools = h('div', { class: 'stage-tools' },
      this.textInput,
      h('div', { class: 'seg im-seg kinds', role: 'group', 'aria-label': 'Kinds of events' },
        ...GROUPS.map((group) => button(KIND_LABEL[group], { 'aria-pressed': String(this.kinds.includes(group)), onclick: () => this.toggleKind(group) }))),
      this.members.length > 1
        ? h('div', { class: 'who-filter', role: 'group', 'aria-label': 'Only these members' },
          ...this.members.map((member) => button([mark(member.harness), member.name], {
            class: 'chip', 'aria-pressed': String(this.only.includes(member.member_id)), onclick: () => this.toggleOnly(member.member_id),
          })))
        : null,
      filtered ? button('Clear', { class: 'ghost', onclick: () => this.clearFilter() }) : null);
    let body: HTMLElement;
    if (!current) body = h('div', { class: 'empty im-empty' }, h('p', {}, 'Pick a room, or open a new one.'));
    else if (!this.events.length) {
      body = h('div', { class: 'empty im-empty' }, h('p', {}, this.text ? 'No message matches the filter.' : 'No events yet.'),
        this.text ? button('Clear the filter', { onclick: () => this.clearFilter() }) : null);
    } else if (this.view === 'graph') body = this.graphBody(ctx);
    else if (!shown.length) body = h('div', { class: 'empty im-empty' }, h('p', {}, 'Nothing in this room matches the filter.'), button('Clear the filter', { onclick: () => this.clearFilter() }));
    else body = this.view === 'lanes' ? lanesView(ctx, shown) : feedView(ctx, shown);
    if (this.view !== 'graph' || !current || !this.events.length) this.graphHost = null;
    return h('section', { class: `column im-panel stage${this.focus === 'messages' ? ' focused' : ''}`, id: 'chat-messages', 'aria-label': current ? `Room ${current.name}` : 'Room' },
      head, current ? tools : null, h('div', { class: 'stage-body' }, body), current ? this.composerBlock(current) : null);
  }

  private roomActions(room: Room): HTMLElement[] {
    return room.closed_at
      ? [button('Reopen', { disabled: this.busy !== null, onclick: () => void this.reopen(room) }),
        button('Delete…', { disabled: this.busy !== null, onclick: () => void this.deleteRoom(room) })]
      : [button('Close…', { disabled: this.busy !== null, onclick: () => void this.closeRoom(room) })];
  }

  private graphBody(ctx: RoomContext): HTMLElement {
    const canvas = h('div', { class: 'graph-canvas' });
    this.graphHost = canvas;
    const legend = h('div', { class: 'legend', 'aria-hidden': 'true' },
      ...([['l-msg', 'message'], ['l-done', 'signal done'], ['l-over', 'blocked'], ['l-obs', 'observation']] as const)
        .map(([cls, text]) => h('span', {}, h('i', { class: cls }), text)));
    return h('div', { class: 'graph' },
      h('div', { class: 'graph-top' }, canvas, legend),
      stripView(ctx, this.playing !== 0, { play: () => this.togglePlay(), step: (by) => { this.stopPlay(); this.step(by); } }),
      h('div', { class: 'caption', 'aria-live': 'polite' }, ctx.selected ? feedCard(ctx.selected, ctx, true) : null));
  }

  /** The graph is drawn at the size of its place, so it is drawn after the place is laid out. */
  private paintGraph(animate: boolean): void {
    const host = this.graphHost;
    if (!host || !host.isConnected) { this.resize.disconnect(); this.painted = ''; return; }
    const width = Math.max(host.clientWidth, 420);
    const height = Math.max(host.clientHeight, 240);
    const ctx = this.context();
    const key = `${width}x${height}|${ctx.selected?.event_id}|${this.events.length}|${this.openMember}|${this.kinds}|${this.only}|${this.members.length}`;
    if (!animate && key === this.painted && host.firstElementChild) return;
    this.painted = key;
    fill(host, graphView(ctx, { width, height, open: this.openMember, pickMember: (id) => this.toggleMember(id) }));
    this.resize.disconnect();
    this.resize.observe(host);
    const now = ctx.selected?.event_id ?? null;
    if (animate && now !== this.animated && host.firstElementChild) {
      this.animated = now;
      animateNow(host.firstElementChild);
    }
  }

  private toggleMember(id: string): void {
    this.openMember = this.openMember === id ? null : id;
    this.focus = 'members';
    this.draw();
  }

  private membersColumn(current: Room | undefined): HTMLElement {
    const mine = current ? this.mine[current.room_id] : undefined;
    const owed = instructions(this.events, this.members).reverse();
    return h('aside', { class: `column im-panel${this.focus === 'members' ? ' focused' : ''}`, id: 'chat-members', 'aria-label': 'Members and instructions' },
      h('div', { class: 'column-head im-panel-header' }, h('h2', {}, 'Members'), h('small', {}, String(this.members.length))),
      h('div', { class: 'column-body im-panel-body' },
        this.members.length
          ? h('div', { class: 'members' }, ...this.members.map((member) => this.memberRow(member, mine?.member_id === member.member_id)))
          : h('p', { class: 'hint muted pad-x' }, current ? 'Nobody joined yet.' : ''),
        h('p', { class: 'label' }, `Instructions · ${owed.length}`),
        owed.length
          ? h('div', { class: 'instructions' }, ...owed.map((one) => button([
            h('span', { class: `state st-${one.state}` }, one.state),
            h('span', { class: 'grow' }, h('b', {}, one.worker.name), ' ', String(one.event.payload.text ?? '')),
            h('span', { class: 'num im-num' }, `#${one.event.event_id}`),
          ], { class: 'instruction', title: 'Show the instruction', onclick: () => { this.stopPlay(); this.select(one.event.event_id); } })))
          : h('p', { class: 'hint muted pad-x' }, 'A lead’s message to a worker shows here, with what the worker signalled.')));
  }

  private memberRow(member: Member, mine: boolean): HTMLElement {
    const open = this.openMember === member.member_id;
    const state = member.left_at ? 'left' : member.last_command_at ? `last command ${shortTime(member.last_command_at)}` : 'joined';
    const row = button([
      mark(member.harness),
      h('span', { class: 'grow' }, h('span', { class: 'member-name' }, member.name),
        mine ? h('span', { class: 'tag im-chip' }, 'you') : null, member.host && !mine ? h('span', { class: 'tag im-chip' }, 'user') : null),
      h('span', { class: 'role' }, member.role),
      h('span', { class: `state${member.left_at ? ' left' : ''}` }, state),
    ], { class: `member-row${open ? ' open' : ''}`, 'aria-expanded': String(open), onclick: () => this.toggleMember(member.member_id) });
    if (!open) return h('div', { class: `member${member.left_at ? ' left' : ''}` }, row);
    const session = member.session;
    return h('div', { class: `member open${member.left_at ? ' left' : ''}` }, row,
      h('dl', { class: 'kv' },
        h('dt', {}, 'harness'), h('dd', {}, member.harness),
        h('dt', {}, 'session'), h('dd', { class: session ? 'mono' : 'muted' }, session ? session.id : 'not bound'),
        session?.cwd ? h('dt', {}, 'folder') : null, session?.cwd ? h('dd', { class: 'mono' }, session.cwd) : null,
        h('dt', {}, 'joined'), h('dd', {}, new Date(member.joined_at).toLocaleString()),
        member.last_command_at ? h('dt', {}, 'last command') : null, member.last_command_at ? h('dd', {}, new Date(member.last_command_at).toLocaleString()) : null,
        member.left_at ? h('dt', {}, 'left') : null, member.left_at ? h('dd', {}, new Date(member.left_at).toLocaleString()) : null),
      h('div', { class: 'actions' },
        !mine && !member.left_at ? button('Mention', { title: 'Address it in the message', onclick: () => this.mention(member) }) : null,
        session ? button([icon('list'), 'Show session'], {
          onclick: (event: Event) => {
            if (!this.hooks.showSession(harnessKey(session.harness), session.id)) (event.currentTarget as HTMLButtonElement).textContent = 'Not listed here';
          },
        }) : null));
  }

  // --- the composer -------------------------------------------------------------------------------

  private draft(): Draft {
    const room = this.room ?? '';
    this.drafts[room] ??= { text: '', to: [] };
    return this.drafts[room];
  }

  private saveDrafts(): void {
    // An empty draft is not kept.
    for (const [room, draft] of Object.entries(this.drafts)) if (!draft.text && !draft.to.length) delete this.drafts[room];
    save('mukade.chat.drafts', this.drafts);
  }

  private composerBlock(room: Room): HTMLElement {
    if (room.closed_at) {
      return h('div', { class: 'composer closed' }, h('span', { class: 'muted' }, 'The room is closed; its history stays readable.'), h('span', { class: 'spring' }),
        button('Reopen', { disabled: this.busy !== null, onclick: () => void this.reopen(room) }));
    }
    const mine = this.mine[room.room_id];
    if (!mine) return this.joinBlock(room);
    const draft = this.draft();
    const others = this.members.filter((member) => member.left_at === null && member.member_id !== mine.member_id);
    draft.to = draft.to.filter((name) => others.some((member) => member.name === name));
    const sending = this.busy === 'sending';
    return h('div', { class: 'composer' },
      h('div', { class: 'recipients', role: 'group', 'aria-label': 'Recipients' },
        h('span', { class: 'lab' }, 'To'),
        button('everyone', { class: 'chip', 'aria-pressed': String(draft.to.length === 0), onclick: () => { draft.to = []; this.saveDrafts(); this.draw(); } }),
        ...others.map((member) => button([mark(member.harness), member.name], {
          class: 'chip', 'aria-pressed': String(draft.to.includes(member.name)),
          onclick: () => { draft.to = draft.to.includes(member.name) ? draft.to.filter((name) => name !== member.name) : [...draft.to, member.name]; this.saveDrafts(); this.draw(); },
        }))),
      this.composer,
      h('div', { class: 'composer-foot' },
        this.composerNote,
        h('span', { class: 'spring' }),
        h('span', { class: 'muted' }, `as ${mine.name} · ${mine.role}`),
        button('Leave', { class: 'ghost', disabled: this.busy !== null, title: 'The window leaves the room', onclick: () => void this.leave(room) }),
        button([icon('send'), sending ? 'Sending…' : 'Send ', sending ? null : caps('Ctrl+Enter')], { class: 'primary', disabled: this.busy !== null, onclick: () => void this.send() })));
  }

  private joinBlock(room: Room): HTMLElement {
    if (document.activeElement !== this.nameInput) this.nameInput.value = this.joinName;
    return h('div', { class: 'composer join' },
      h('p', { class: 'muted' }, 'To write here, the window joins the room as a member of its own; what it sends reaches the agents as yours.'),
      h('div', { class: 'join-line' },
        h('div', { class: 'field im-field' }, h('label', { for: 'chat-join-name' }, 'Name'), this.nameInput),
        h('div', { class: 'field im-field' }, h('span', { class: 'lab' }, 'Role'),
          pickOne('Role', [['peer', 'peer'], ['lead', 'lead']] as const, this.joinRole, (role) => { this.joinRole = role; this.draw(); })),
        button(this.busy === 'joining' ? 'Joining…' : 'Join', { class: 'primary', disabled: this.busy !== null, onclick: () => void this.join(room) })),
      this.joinNote);
  }

  private paintJoinNote(): void {
    const refusal = this.joinName ? memberNameRefusal(this.joinName, this.members) : null;
    fill(this.joinNote, this.writeError && this.room && !this.mine[this.room]
      ? h('span', { class: 'err-text', role: 'alert' }, this.writeError)
      : refusal ? h('span', { class: 'warn-text' }, refusal)
        : h('span', { class: 'muted' }, this.joinRole === 'lead' ? 'A lead’s message to a worker is an instruction the worker answers.' : 'A peer reads and writes to everyone or to chosen members.'));
  }

  private paintComposerNote(): void {
    const room = this.current();
    const mine = room ? this.mine[room.room_id] : undefined;
    if (!room || !mine) { fill(this.composerNote); return; }
    if (this.writeError) {
      const stale = this.writeError.startsWith('stale_member');
      fill(this.composerNote, h('span', { class: 'err-text', role: 'alert' }, stale ? `The window’s handle in ${room.name} no longer writes.` : this.writeError),
        stale ? button('Rejoin', { title: 'Recover the member with a new handle', onclick: () => void this.rejoin(room) }) : null);
      return;
    }
    const draft = this.draft();
    const named = [...new Set([...draft.to, ...mentionsIn(this.composer.value, this.members)])];
    const workers = named.filter((name) => this.members.find((member) => member.name === name && member.left_at === null)?.role === 'worker');
    fill(this.composerNote, mine.role === 'lead' && workers.length
      ? h('span', { class: 'warn-text' }, `An instruction to ${workers.join(', ')}: ${workers.length === 1 ? 'it answers' : 'each answers'} with done or blocked.`)
      : h('span', { class: 'muted' }, named.length ? `to ${named.join(', ')}` : 'to everyone'));
  }

  private mention(member: Member): void {
    const text = this.composer.value;
    this.composer.value = `${text}${text && !/\s$/.test(text) ? ' ' : ''}@${member.name} `;
    this.draft().text = this.composer.value;
    this.saveDrafts();
    this.draw();
    if (this.composer.isConnected) { this.composer.focus(); this.composer.setSelectionRange(this.composer.value.length, this.composer.value.length); }
  }

  // --- writing ------------------------------------------------------------------------------------

  /** One write with a fresh request id; null when it failed (`writeError` says why). */
  private async write<T>(what: string, operation: string, args: Record<string, unknown>): Promise<T | null> {
    this.busy = what;
    this.writeError = null;
    this.draw();
    try {
      return await api.chatWrite<T>(operation, { request_id: requestId(), ...args });
    } catch (error) {
      this.writeError = errorText(error);
      return null;
    } finally {
      this.busy = null;
    }
  }

  /** A room's administration: a failure goes to the status line, not to the composer. */
  private async administer(what: string, operation: string, room: Room, args: Record<string, unknown> = {}): Promise<boolean> {
    const done = await this.write(what, operation, { room_id: room.room_id, ...args });
    if (done === null) { this.hooks.status(`${what} ${room.name} failed: ${this.writeError}`, true); this.writeError = null; }
    else this.hooks.status(`${operation === 'close' ? 'closed' : operation === 'reopen' ? 'reopened' : 'deleted'} ${room.name}`);
    await this.poll(true);
    return done !== null;
  }

  private async join(room: Room): Promise<void> {
    if (this.busy) return;
    const refusal = memberNameRefusal(this.joinName, this.members);
    if (refusal) { this.writeError = refusal; this.paintJoinNote(); this.nameInput.focus(); return; }
    const joined = await this.write<Joined>('joining', 'join', { room_id: room.room_id, harness: 'desktop', role: this.joinRole, name: this.joinName, host: true });
    if (joined) {
      this.mine[room.room_id] = { member_id: joined.member_id, handle: joined.member_handle, name: joined.name ?? this.joinName, role: joined.role ?? this.joinRole, at: Date.now() };
      save('mukade.chat.members', this.mine);
      this.hooks.status(`the window joined ${room.name} as ${this.joinName}`);
      this.focusComposer = true;
    }
    await this.poll(true);
  }

  private async rejoin(room: Room): Promise<void> {
    const mine = this.mine[room.room_id];
    if (!mine || this.busy) return;
    const joined = await this.write<Joined>('rejoining', 'rejoin', { room_id: room.room_id, member_id: mine.member_id, host: true });
    if (joined) {
      this.mine[room.room_id] = { ...mine, handle: joined.member_handle, at: Date.now() };
      save('mukade.chat.members', this.mine);
      this.hooks.status(`the window rejoined ${room.name} as ${mine.name}, with a new handle`);
    }
    await this.poll(true);
  }

  private async leave(room: Room): Promise<void> {
    const mine = this.mine[room.room_id];
    if (!mine || this.busy) return;
    const left = await this.write('leaving', 'leave', { room_id: room.room_id, member_handle: mine.handle });
    // A handle the service no longer knows has nothing left to leave.
    if (left !== null || this.writeError?.startsWith('stale_member')) {
      delete this.mine[room.room_id];
      save('mukade.chat.members', this.mine);
      this.writeError = null;
      this.hooks.status(`the window left ${room.name}`);
    }
    await this.poll(true);
  }

  private async send(): Promise<void> {
    const room = this.current();
    const mine = room ? this.mine[room.room_id] : undefined;
    if (!room || !mine || this.busy) return;
    const text = this.composer.value;
    const refusal = sendRefusal(text);
    if (refusal) { this.writeError = refusal; this.paintComposerNote(); this.composer.focus(); return; }
    const draft = this.draft();
    const key = JSON.stringify([text, draft.to]);
    const lost = this.pending.get(room.room_id);
    const id = lost?.key === key ? lost.id : requestId();
    this.pending.set(room.room_id, { id, key });
    this.busy = 'sending';
    this.writeError = null;
    this.draw();
    try {
      await api.chatWrite('send', { request_id: id, room_id: room.room_id, member_handle: mine.handle, text, ...(draft.to.length ? { recipients: draft.to } : {}) });
      this.pending.delete(room.room_id);
      this.composer.value = '';
      draft.text = '';
      this.saveDrafts();
      this.picked.delete(room.room_id);
    } catch (error) {
      // A lost answer keeps the request id: sending the same again cannot write twice. A refusal does not.
      if (errorKind(error) !== 'unreachable') this.pending.delete(room.room_id);
      this.writeError = errorText(error);
    } finally {
      this.busy = null;
    }
    await this.poll(true);
    if (this.composer.isConnected) this.composer.focus();
  }

  // --- rooms --------------------------------------------------------------------------------------

  newRoomDialog(): void {
    if (this.status?.state !== 'running') return;
    const modal = openModal({ title: 'New room', icon: 'plus' });
    const input = h('input', { id: 'room-name', type: 'text', placeholder: 'design-review', autocomplete: 'off', spellcheck: 'false' }) as HTMLInputElement;
    const note = h('div', { class: 'field-note', 'aria-live': 'polite' });
    let running = false;
    const run = async (): Promise<void> => {
      const name = input.value.trim();
      if (running || roomNameRefusal(name) || roomNamed(this.rooms, name)) return;
      running = true;
      modal.lock(true);
      try {
        const opened = await api.chatWrite<{ room_id?: string }>('open', { request_id: requestId(), name });
        modal.lock(false);
        modal.close();
        this.hooks.status(`opened ${name}`);
        await this.poll(true);
        const id = opened.room_id ?? roomNamed(this.rooms, name)?.room_id;
        if (id) this.pickRoom(id);
      } catch (error) {
        running = false;
        modal.lock(false);
        fill(note, h('span', { class: 'err-text', role: 'alert' }, errorText(error)));
      }
    };
    const go = button(['Open the room ', caps('Enter')], { class: 'primary', onclick: () => void run() });
    const paint = (): void => {
      const name = input.value.trim();
      const refusal = name ? roomNameRefusal(name) : null;
      const existing = name ? roomNamed(this.rooms, name) : undefined;
      go.disabled = !name || refusal !== null || existing !== undefined;
      if (refusal) fill(note, h('span', { class: 'warn-text' }, refusal));
      else if (existing && existing.closed_at === null) {
        fill(note, h('span', {}, `${name} is open already. `), button('Go to it', { class: 'link', onclick: () => { modal.close(); this.pickRoom(existing.room_id); } }));
      } else if (existing) {
        fill(note, h('span', {}, `${name} is closed. `), button('Reopen it', { class: 'link', onclick: () => { modal.close(); this.pickRoom(existing.room_id); void this.reopen(existing); } }));
      } else fill(note, h('span', { class: 'muted' }, 'Lowercase letters, digits, - and _.'));
    };
    input.addEventListener('input', paint);
    fill(modal.body, h('div', { class: 'col' }, h('div', { class: 'field im-field' }, h('label', { for: 'room-name' }, 'Name'), input), note));
    fill(modal.foot, h('span', { class: 'spring' }), button(['Cancel ', caps('Escape')], { onclick: () => modal.close() }), go);
    modal.root.onkeydown = (key) => { if (key.key === 'Enter' && key.target === input) { key.preventDefault(); void run(); } };
    paint();
  }

  private async closeRoom(room: Room): Promise<void> {
    if (!(await confirmModal(`Close ${room.name}`, 'Its members stop sending and receiving in it. Its history stays readable, and it can be reopened.', 'Close the room'))) return;
    await this.administer('closing', 'close', room);
  }

  private async reopen(room: Room): Promise<void> {
    await this.administer('reopening', 'reopen', room);
  }

  /** Delete asks with the room's summary from the service, and deletes that revision only. */
  private async deleteRoom(room: Room): Promise<void> {
    let fresh: Room | undefined;
    try { fresh = (await this.allRooms()).find((one) => one.room_id === room.room_id); } catch (error) { this.hooks.status(errorText(error), true); return; }
    if (!fresh) { await this.poll(true); return; }
    const summary = `${fresh.members} member${fresh.members === 1 ? '' : 's'} and ${fresh.events} event${fresh.events === 1 ? '' : 's'} go with it, for good (revision ${fresh.revision}).`;
    if (!(await confirmModal(`Delete ${fresh.name}`, summary, 'Delete for good'))) return;
    await this.administer('deleting', 'delete', fresh, { expected_revision: fresh.revision, confirm: true });
  }

  // --- the service and launches -------------------------------------------------------------------

  async startService(): Promise<void> {
    if (this.busy) return;
    this.busy = 'starting the service';
    this.draw();
    try {
      await api.chatStart();
      this.hooks.status('chat service started');
    } catch (error) {
      this.hooks.status(`the chat service did not start: ${errorText(error)}`, true);
    } finally {
      this.busy = null;
    }
    await this.poll(true);
  }

  async stopService(): Promise<void> {
    const open = this.rooms.filter((room) => room.closed_at === null);
    const members = open.reduce((sum, room) => sum + room.members, 0);
    const text = `${members} member${members === 1 ? '' : 's'} in ${open.length} open room${open.length === 1 ? '' : 's'} stop receiving until it starts again. Rooms and messages stay.`;
    if (!(await confirmModal('Stop the chat service', text, 'Stop the service'))) return;
    this.busy = 'stopping the service';
    this.draw();
    try {
      await api.chatStop();
      this.hooks.status('chat service stopped');
    } catch (error) {
      this.hooks.status(`the chat service did not stop: ${errorText(error)}`, true);
    } finally {
      this.busy = null;
    }
    await this.poll(true);
  }

  /** Run an agent in the shown room, in a new terminal, with the command shown first. */
  launchDialog(): void {
    const room = this.current();
    if (!room || room.closed_at || this.status?.state !== 'running') return;
    const modal = openModal({ title: 'Launch an agent', subtitle: `into ${room.name}`, icon: 'term' });
    const choice: { harness: (typeof HARNESSES)[number][0]; role: (typeof ROLES)[number][0]; listen: number } = { harness: 'claude_code', role: 'peer', listen: 300 };
    const folder = h('input', { id: 'launch-folder', type: 'text', placeholder: 'the folder the terminal opens in', autocomplete: 'off', spellcheck: 'false' }) as HTMLInputElement;
    const listen = h('input', { id: 'launch-listen', type: 'number', min: '0', max: '3600', step: '30', value: String(choice.listen) }) as HTMLInputElement;
    const preview = h('pre', { class: 'code' });
    const note = h('div', { class: 'field-note', 'aria-live': 'polite' });
    const paintPreview = () => fill(preview, commandLine(['mukade', 'chat', 'launch', choice.harness, '--room', room.name, '--role', choice.role, '--listen', String(choice.listen)]));
    listen.addEventListener('input', () => {
      const seconds = Number(listen.value);
      if (Number.isInteger(seconds) && seconds >= 0 && seconds <= 3600) { choice.listen = seconds; fill(note); } else fill(note, h('span', { class: 'warn-text' }, 'From 0 to 3600 seconds.'));
      paintPreview();
    });
    const browse = button('Choose…', { onclick: async () => { const picked = await api.pick('folder').catch(() => null); if (picked) folder.value = picked; } });
    const draw = (): void => {
      paintPreview();
      fill(modal.body, h('div', { class: 'col' },
        h('div', { class: 'field im-field' }, h('span', { class: 'lab' }, 'Harness'), pickOne('Harness', HARNESSES, choice.harness, (value) => { choice.harness = value; draw(); })),
        h('div', { class: 'field im-field' }, h('span', { class: 'lab' }, 'Role'), pickOne('Role', ROLES, choice.role, (value) => { choice.role = value; draw(); })),
        h('div', { class: 'row-fields' },
          h('div', { class: 'field im-field' }, h('label', { for: 'launch-listen' }, 'Listens · seconds between turns'), listen),
          h('div', { class: 'field im-field' }, h('label', { for: 'launch-folder' }, 'Folder'), h('div', { class: 'cmd' }, folder, browse))),
        h('div', { class: 'field im-field' }, h('span', { class: 'lab' }, 'It runs, in a new terminal'), preview),
        note,
        h('p', { class: 'note' }, choice.harness === 'codex'
          ? 'Codex runs the launch hooks only once they are trusted: mukade chat trust codex, once.'
          : 'The window does not join for the agent: its member appears when it joins.')));
    };
    const run = async (): Promise<void> => {
      if (!Number.isInteger(Number(listen.value))) return;
      modal.lock(true);
      try {
        const command = await api.chatLaunch(choice.harness, room.name, choice.role, choice.listen, folder.value.trim() || undefined);
        this.launched = { room: room.room_id, at: Date.now(), count: room.members, command };
        modal.lock(false);
        modal.close();
        this.hooks.status(`launched ${HARNESSES.find(([key]) => key === choice.harness)?.[1]} into ${room.name}; it appears when it joins`);
      } catch (error) {
        modal.lock(false);
        fill(note, h('span', { class: 'err-text', role: 'alert' }, errorText(error)));
      }
    };
    draw();
    fill(modal.foot, h('span', { class: 'spring' }), button(['Cancel ', caps('Escape')], { onclick: () => modal.close() }),
      button([icon('term'), 'Launch'], { class: 'primary', onclick: () => void run() }));
  }
}
