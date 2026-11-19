// §5: the chat rooms, read only, through the running chat service. Polled every 3 s while the
// view is shown; the service is never started, stopped or written to from here.

import './chat.css';
import { api, errorText } from './api.ts';
import { button, fill, h, icon } from './dom.ts';
import { listbox, reveal, stepIn } from './lists.ts';
import { shortTime } from './tree.ts';
import { eventLine, mergeEvents } from './chatmodel.ts';
import type { ChatEvent, ChatStatus, Member, Room } from './types.ts';

const POLL_MS = 3000;
const MAX_PAGES = 20;

export class ChatView {
  private status: ChatStatus | null = null;
  private rooms: Room[] = [];
  private room: string | null = null;
  private members: Member[] = [];
  private events: ChatEvent[] = [];
  private filter = '';
  private error: string | null = null;
  private timer = 0;
  private polling = false;
  private follow = true;
  focus: 'rooms' | 'messages' | 'members' = 'rooms';

  /** One filter field for the view's life: a redraw every poll moves it, never rebuilds it, so
   *  what is being typed (and the caret) survives the poll. */
  private readonly filterInput: HTMLInputElement;

  constructor(private readonly root: HTMLElement, private readonly showSession: (harness: string, id: string) => boolean) {
    this.filterInput = h('input', { id: 'chat-filter', type: 'search', placeholder: 'Filter messages', 'aria-label': 'Filter messages' }) as HTMLInputElement;
    let debounce = 0;
    this.filterInput.addEventListener('input', () => {
      window.clearTimeout(debounce);
      debounce = window.setTimeout(() => {
        const next = this.filterInput.value.trim();
        if (next === this.filter) return;
        this.filter = next;
        this.events = [];
        void this.poll();
      }, 300);
    });
  }

  show(): void {
    this.draw();
    void this.poll();
    this.timer = window.setInterval(() => void this.poll(), POLL_MS);
  }

  hide(): void {
    window.clearInterval(this.timer);
    this.timer = 0;
  }

  private async poll(): Promise<void> {
    if (this.polling) return;
    this.polling = true;
    try {
      this.status = await api.chatStatus();
      if (this.status.state !== 'running') { this.draw(); return; }
      this.rooms = await this.allRooms();
      if (!this.room || !this.rooms.some((room) => room.room_id === this.room)) {
        this.room = this.rooms[0]?.room_id ?? null;
        this.events = [];
      }
      if (this.room) await this.loadRoom(this.room);
      this.error = null;
    } catch (error) {
      this.error = errorText(error);
    } finally {
      this.polling = false;
    }
    this.draw();
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
    this.members = (await api.chatMembers(roomId)).items;
    const since = this.events.length ? this.events[this.events.length - 1].timestamp : undefined;
    let cursor: string | undefined;
    const fresh: ChatEvent[] = [];
    for (let page = 0; page < MAX_PAGES; page += 1) {
      const reply = await api.chatLedger(roomId, { cursor, since, text: this.filter || undefined });
      fresh.push(...reply.items);
      if (!reply.next_cursor) break;
      cursor = reply.next_cursor;
    }
    if (roomId === this.room) this.events = mergeEvents(this.events, fresh);
  }

  private pickRoom(id: string): void {
    if (id === this.room) return;
    this.room = id;
    this.events = [];
    this.members = [];
    this.follow = true;
    this.draw();
    void this.poll();
  }

  /** The arrow keys, on the focused column. */
  move(step: number): void {
    if (this.focus === 'rooms') {
      const next = stepIn(this.rooms.map((room) => room.room_id), this.room, step);
      if (next) this.pickRoom(next);
    } else if (this.focus === 'messages') {
      this.root.querySelector('.messages-body')?.scrollBy({ top: step * 60 });
    }
  }

  column(step: number): void {
    const order: ChatView['focus'][] = ['rooms', 'messages', 'members'];
    this.focus = order[Math.max(0, Math.min(2, order.indexOf(this.focus) + step))];
    this.draw();
  }

  focusFilter(): void {
    this.filterInput.focus();
  }

  private draw(): void {
    const status = this.status;
    const state = status?.state ?? 'checking';
    const bar = h('div', { class: 'chat-status' },
      h('span', { class: `tag ${state === 'running' ? 'live' : state === 'stopped' ? '' : 'bad'}` }, state === 'running' ? 'service running' : state === 'stopped' ? 'service stopped' : state === 'checking' ? 'checking…' : 'unreachable'),
      h('span', { class: 'mono muted' }, status?.endpoint ? `${status.endpoint} · read only` : status?.data_dir ?? ''),
      h('span', { class: 'spring' }),
      this.error ? h('span', { class: 'mono err-text' }, this.error) : null,
      h('span', { class: 'mono muted' }, `checked every ${POLL_MS / 1000} s`));
    if (!status || status.state !== 'running') {
      fill(this.root, bar, h('div', { class: 'column chat-off' }, h('div', { class: 'empty' },
        h('h2', {}, status ? (status.state === 'stopped' ? 'The chat service is not running' : 'The chat service does not answer') : 'Looking for the chat service…'),
        status ? h('p', {}, 'Start it in a terminal with ', h('code', {}, 'mukade chat start'), '. This window only reads the rooms; it never starts the service.') : null,
        status?.message ? h('p', { class: 'note' }, status.message) : null)));
      return;
    }
    const names = new Map(this.members.map((member) => [member.member_id, member.name] as const));
    const roomRows = this.rooms.map((room) => ({
      id: room.room_id,
      class: 'room-row',
      cells: [h('span', { class: 'grow' }, h('span', { class: 'room-name' }, room.name),
        h('span', { class: 'num room-meta' }, `${room.members} members · ${room.events} events · ${shortTime(room.last_activity ?? room.opened_at)}`)),
        h('span', { class: `tag ${room.closed_at ? '' : 'live'}` }, room.closed_at ? 'closed' : 'open')] as Node[],
    }));
    const rooms = h('section', { class: `column${this.focus === 'rooms' ? ' focused' : ''}`, id: 'chat-rooms', 'aria-label': 'Rooms' },
      h('div', { class: 'column-head' }, h('h2', {}, 'Rooms'), h('span', { class: 'spring' }), h('small', {}, String(this.rooms.length))),
      h('div', { class: 'column-body' }, this.rooms.length ? listbox('Rooms', roomRows, this.room, (id) => { this.focus = 'rooms'; this.pickRoom(id); }) : h('div', { class: 'empty' }, h('p', {}, 'No rooms yet.'))));

    const current = this.rooms.find((room) => room.room_id === this.room);
    const filter = this.filterInput;
    const list = h('div', { class: 'column-body messages-body' });
    for (const event of this.events) {
      const line = eventLine(event, names);
      const who = event.actor.type === 'member' ? `${event.actor.name}${event.actor.role ? ` · ${event.actor.role}` : ''}` : 'local admin';
      if (line.kind === 'muted') {
        list.append(h('div', { class: 'msg muted-line' }, `${new Date(event.timestamp).toLocaleTimeString()} · ${line.text}`));
        continue;
      }
      const body = h('p', {});
      for (const part of line.text.split(/(@[\w.-]+)/)) body.append(part.startsWith('@') ? h('span', { class: 'at' }, part) : part);
      list.append(h('article', { class: 'msg' },
        h('div', { class: 'meta' }, h('b', {}, who), new Date(event.timestamp).toLocaleTimeString()),
        h('div', { class: 'body' }, body, h('span', { class: 'to' }, line.to.length ? `to ${line.to.join(', ')}` : 'to everyone'))));
    }
    if (!this.events.length) list.append(h('div', { class: 'empty' }, h('p', {}, this.filter ? 'No message matches the filter.' : 'No messages yet.')));
    list.addEventListener('scroll', () => { this.follow = list.scrollTop + list.clientHeight >= list.scrollHeight - 24; });
    const messages = h('section', { class: `column${this.focus === 'messages' ? ' focused' : ''}`, id: 'chat-messages', 'aria-label': 'Messages' },
      h('div', { class: 'column-head' }, h('h2', {}, current?.name ?? 'Messages'), h('small', {}, `${this.events.length} events · newest last`), h('span', { class: 'spring' }), filter),
      list);

    const memberBoxes = this.members.map((member) => h('div', { class: `box member${member.left_at ? ' left' : ''}` },
      h('dl', { class: 'kv' },
        h('dt', {}, 'name'), h('dd', {}, member.name),
        h('dt', {}, 'harness'), h('dd', {}, member.harness),
        h('dt', {}, 'role'), h('dd', {}, member.role),
        h('dt', {}, 'session'), h('dd', { class: member.session ? '' : 'muted' }, member.session ? member.session.id : 'not bound'),
        member.left_at ? h('dt', {}, 'left') : null, member.left_at ? h('dd', {}, shortTime(member.left_at)) : null),
      member.session ? button([icon('list'), 'Show session'], { onclick: (event: Event) => {
        if (!this.showSession(member.session!.harness, member.session!.id)) (event.currentTarget as HTMLButtonElement).textContent = 'Not listed here';
      } }) : null));
    const members = h('aside', { class: `column${this.focus === 'members' ? ' focused' : ''}`, id: 'chat-members', 'aria-label': 'Members' },
      h('div', { class: 'column-head' }, h('h2', {}, 'Members'), h('span', { class: 'spring' }), h('small', {}, String(this.members.length))),
      h('div', { class: 'column-body' }, h('div', { class: 'pad' }, ...memberBoxes)));

    const hadFocus = document.activeElement === filter;
    const caret = [filter.selectionStart, filter.selectionEnd] as const;
    const scroll = this.root.querySelector('.messages-body')?.scrollTop ?? 0;
    fill(this.root, bar, h('div', { class: 'chat-columns' }, rooms, messages, members));
    // Moving a focused field into the new tree blurs it: give the focus and the caret back.
    if (hadFocus) { filter.focus({ preventScroll: true }); filter.setSelectionRange(caret[0], caret[1]); }
    list.scrollTop = this.follow ? list.scrollHeight : scroll;
    reveal(rooms.querySelector('.list'));
  }
}
