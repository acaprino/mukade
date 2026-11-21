// Search everything (ctrl+K): rooms and members by name, messages by text, sessions by title, id
// or folder, in one list over the window. A result opens its place: the room, a member's details,
// the room at that event in the feed, or the session in the tree.

import { loader } from '@acaprino/imago/ui';
import { api, errorText } from './api.ts';
import { chatIndex } from './chatindex.ts';
import { matches } from './chatmodel.ts';
import { mark } from './chatviews.ts';
import { button, caps, choice, fill, h } from './dom.ts';
import { openModal } from './modal.ts';
import { shortTime } from './tree.ts';
import type { Installation, SessionRow } from './types.ts';

const SCOPES = [['all', 'All'], ['rooms', 'Rooms'], ['members', 'Members'], ['messages', 'Messages'], ['sessions', 'Sessions']] as const;
type Scope = (typeof SCOPES)[number][0];
/** Results per group when every group is shown; a scope of one group shows more. */
const FEW = 5;
const MANY = 40;
/** Rooms whose messages are searched, open ones first. */
const MESSAGE_ROOMS = 20;

export interface SearchHooks {
  installations(): Installation[];
  openSession(key: string, id: string): void;
  openRoom(roomId: string, at?: { member?: string; event?: string }): void;
}

interface Result { group: string; label: Node[]; detail: string; open(): void }

let listings: Map<string, SessionRow[]> | null = null;

export function openSearch(hooks: SearchHooks): void {
  const modal = openModal({ title: 'Search everything', icon: 'search', wide: true });
  const input = h('input', { id: 'search-all', type: 'search', placeholder: 'rooms, members, messages, sessions', autocomplete: 'off', spellcheck: 'false', 'aria-label': 'Search everything' }) as HTMLInputElement;
  const list = h('div', { class: 'search-results', role: 'listbox', 'aria-label': 'Results' });
  const note = h('p', { class: 'muted search-note', 'aria-live': 'polite' });
  let scope: Scope = 'all';
  let results: Result[] = [];
  let active = 0;
  let ticket = 0;
  // The sessions of every installation are read once per opening.
  listings = null;

  const scopes = (): HTMLElement => choice('Search in', SCOPES.map(([value, text]) => ({ value, label: text })), scope,
    (value) => { scope = value; void run(); drawTop(); });
  const top = h('div', { class: 'search-top' });
  const drawTop = () => fill(top, input, scopes());

  const paint = (): void => {
    fill(list);
    let group = '';
    results.forEach((result, i) => {
      if (result.group !== group) { group = result.group; list.append(h('p', { class: 'label' }, group)); }
      const row = button([h('span', { class: 'grow' }, ...result.label), h('span', { class: 'num im-num' }, result.detail)], {
        class: 'search-row', role: 'option', 'aria-selected': String(i === active), id: `sr-${i}`,
        onclick: () => { modal.close(); result.open(); },
      });
      list.append(row);
    });
    if (results.length) list.setAttribute('aria-activedescendant', `sr-${active}`);
    list.querySelector('[aria-selected="true"]')?.scrollIntoView({ block: 'nearest' });
  };

  const run = async (): Promise<void> => {
    const mine = ++ticket;
    const query = input.value.trim();
    if (!query) { results = []; active = 0; paint(); fill(note, 'Type to search. ↑ ↓ to move, ⏎ to open.'); return; }
    fill(note, loader(document, 'Searching…'));
    const limit = scope === 'all' ? FEW : MANY;
    const found: Result[] = [];
    const errors: string[] = [];
    const wants = (group: Scope) => scope === 'all' || scope === group;
    try {
      const index = wants('rooms') || wants('members') || wants('messages') ? await chatIndex() : null;
      if (mine !== ticket) return;
      if (index && wants('rooms')) {
        for (const room of index.rooms.filter((one) => matches(one.name, query)).slice(0, limit)) {
          found.push({ group: 'Rooms', label: [document.createTextNode(room.name)], detail: `${room.closed_at ? 'closed' : 'open'} · ${room.members} members`, open: () => hooks.openRoom(room.room_id) });
        }
      }
      if (index && wants('members')) {
        for (const { room, member } of index.members.filter(({ member: one }) => matches(one.name, query)).slice(0, limit)) {
          found.push({ group: 'Members', label: [mark(member.harness), document.createTextNode(` ${member.name}`)], detail: `${member.role} in ${room.name}${member.left_at ? ' · left' : ''}`, open: () => hooks.openRoom(room.room_id, { member: member.member_id }) });
        }
      }
      if (index && wants('messages')) {
        const rooms = [...index.rooms].sort((a, b) => Number(a.closed_at !== null) - Number(b.closed_at !== null)).slice(0, MESSAGE_ROOMS);
        const pages = await Promise.all(rooms.map(async (room) => ({ room, page: await api.chatLedger(room.room_id, { text: query }).catch(() => null) })));
        if (mine !== ticket) return;
        let count = 0;
        for (const { room, page } of pages) {
          for (const event of (page?.items ?? []).filter((one) => one.kind === 'message').reverse()) {
            if (count++ >= limit) break;
            const who = event.actor.type === 'member' ? event.actor.name : 'local admin';
            found.push({ group: 'Messages', label: [document.createTextNode(String(event.payload.text ?? '').slice(0, 160))], detail: `${who} in ${room.name} · ${shortTime(event.timestamp)}`, open: () => hooks.openRoom(room.room_id, { event: event.event_id }) });
          }
        }
      }
    } catch (error) {
      errors.push(`chat: ${errorText(error)}`);
    }
    if (wants('sessions')) {
      if (!listings) {
        listings = new Map();
        await Promise.all(hooks.installations().filter((item) => item.count > 0).map(async (item) => {
          listings!.set(item.key, await api.sessions(item.key).catch(() => []));
        }));
      }
      if (mine !== ticket) return;
      let count = 0;
      for (const item of hooks.installations()) {
        for (const row of listings.get(item.key) ?? []) {
          if (count >= limit) break;
          if (!matches(row.title, query) && !matches(row.id, query) && !matches(row.cwd, query)) continue;
          count += 1;
          found.push({ group: 'Sessions', label: [mark(item.harness), document.createTextNode(` ${row.title ?? row.id}`)], detail: `${item.label} · ${row.cwd ?? ''} · ${shortTime(row.updated)}`, open: () => hooks.openSession(item.key, row.id) });
        }
      }
    }
    if (mine !== ticket) return;
    results = found;
    active = 0;
    paint();
    fill(note, found.length ? `${found.length} result${found.length === 1 ? '' : 's'}${scope === 'all' ? `, ${FEW} at most per group: pick a scope for more` : ''}` : 'Nothing found.', errors.length ? ` · ${errors.join(' · ')}` : '');
  };

  let debounce = 0;
  input.addEventListener('input', () => { window.clearTimeout(debounce); debounce = window.setTimeout(() => void run(), 250); });
  modal.root.onkeydown = (key) => {
    if (key.key === 'ArrowDown' || key.key === 'ArrowUp') {
      key.preventDefault();
      if (!results.length) return;
      active = (active + (key.key === 'ArrowDown' ? 1 : -1) + results.length) % results.length;
      paint();
    } else if (key.key === 'Enter' && results[active]) {
      key.preventDefault();
      const result = results[active];
      modal.close();
      result.open();
    }
  };
  drawTop();
  fill(modal.body, h('div', { class: 'col search' }, top, note, list));
  fill(modal.foot, h('span', { class: 'mono muted' }, '↑ ↓ move · ⏎ open'), h('span', { class: 'spring' }), button(['Close ', caps('Escape')], { onclick: () => modal.close() }));
  void run();
  queueMicrotask(() => input.focus());
}
