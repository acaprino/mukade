// Search everything (Ctrl+Shift+F): rooms and members by name, messages by text, sessions by title, id or folder, in one list over the
// window. A result opens its place: the room, a member's details, the room at that event in the feed, or the session in the tree.
// The focus stays in the field; the arrows move the result chosen and Enter opens it.
import { useEffect, useRef, useState } from 'react';
import { Dialog, ItemList, ItemRow, SearchField } from '@acaprino/sigrid/components';
import { Button, Column, Heading, Progress, Spacer, Text, ToggleGroup, ToggleGroupItem } from '@acaprino/sigrid/primitives';
import { api, errorText } from '../api.ts';
import { chatIndex } from '../chatindex.ts';
import { matches } from '../chatmodel.ts';
import type { Controller } from '../controller.ts';
import { shortTime } from '../tree.ts';
import type { SessionRow } from '../types.ts';
import { HarnessMark } from './mark.tsx';

const SCOPES = [['all', 'All'], ['rooms', 'Rooms'], ['members', 'Members'], ['messages', 'Messages'], ['sessions', 'Sessions']] as const;
type Scope = (typeof SCOPES)[number][0];
/** Results per group when every group is shown; a scope of one group shows more. */
const FEW = 5;
const MANY = 40;
/** Rooms whose messages are searched, open ones first. */
const MESSAGE_ROOMS = 20;

interface Result { id: string; group: string; title: string; mark?: string; detail: string; open(): void }

export function SearchDialog({ c }: { c: Controller }) {
  const [query, setQuery] = useState('');
  const [scope, setScope] = useState<Scope>('all');
  const [results, setResults] = useState<Result[]>([]);
  const [active, setActive] = useState(0);
  const [note, setNote] = useState('Type to search. ↑ ↓ to move, Enter to open.');
  const [busy, setBusy] = useState(false);
  // The sessions of every installation are read once per opening.
  const listings = useRef<Map<string, SessionRow[]> | null>(null);
  const ticket = useRef(0);

  useEffect(() => {
    const mine = ++ticket.current;
    const text = query.trim();
    if (!text) { setResults([]); setActive(0); setBusy(false); setNote('Type to search. ↑ ↓ to move, Enter to open.'); return; }
    setBusy(true);
    const timer = window.setTimeout(async () => {
      const limit = scope === 'all' ? FEW : MANY;
      const found: Result[] = [];
      const errors: string[] = [];
      const wants = (group: Scope) => scope === 'all' || scope === group;
      const chat = c.chat;
      try {
        const index = wants('rooms') || wants('members') || wants('messages') ? await chatIndex() : null;
        if (mine !== ticket.current) return;
        if (index && wants('rooms')) {
          for (const room of index.rooms.filter((one) => matches(one.name, text)).slice(0, limit)) {
            found.push({ id: `room:${room.room_id}`, group: 'Rooms', title: room.name, detail: `${room.closed_at ? 'closed' : 'open'} · ${room.members} members`, open: () => chat.openRoom(room.room_id) });
          }
        }
        if (index && wants('members')) {
          for (const { room, member } of index.members.filter(({ member: one }) => matches(one.name, text)).slice(0, limit)) {
            found.push({ id: `member:${room.room_id}:${member.member_id}`, group: 'Members', title: member.name, mark: member.harness, detail: `${member.role} in ${room.name}${member.left_at ? ' · left' : ''}`, open: () => chat.openRoom(room.room_id, { member: member.member_id }) });
          }
        }
        if (index && wants('messages')) {
          const rooms = [...index.rooms].sort((a, b) => Number(a.closed_at !== null) - Number(b.closed_at !== null)).slice(0, MESSAGE_ROOMS);
          const pages = await Promise.all(rooms.map(async (room) => ({ room, page: await api.chatLedger(room.room_id, { text }).catch(() => null) })));
          if (mine !== ticket.current) return;
          let count = 0;
          for (const { room, page } of pages) {
            for (const event of (page?.items ?? []).filter((one) => one.kind === 'message').reverse()) {
              if (count++ >= limit) break;
              const who = event.actor.type === 'member' ? event.actor.name : 'local admin';
              found.push({ id: `event:${room.room_id}:${event.event_id}`, group: 'Messages', title: String(event.payload.text ?? '').slice(0, 160), detail: `${who} in ${room.name} · ${shortTime(event.timestamp)}`, open: () => chat.openRoom(room.room_id, { event: event.event_id }) });
            }
          }
        }
      } catch (error) {
        errors.push(`chat: ${errorText(error)}`);
      }
      if (wants('sessions')) {
        if (!listings.current) {
          const read = new Map<string, SessionRow[]>();
          listings.current = read;
          await Promise.all(c.state.installations.filter((item) => item.count > 0).map(async (item) => {
            read.set(item.key, await api.sessions(item.key).catch(() => []));
          }));
        }
        if (mine !== ticket.current) return;
        let count = 0;
        for (const item of c.state.installations) {
          for (const row of listings.current.get(item.key) ?? []) {
            if (count >= limit) break;
            if (!matches(row.title, text) && !matches(row.id, text) && !matches(row.cwd, text)) continue;
            count += 1;
            found.push({ id: `session:${item.key}:${row.id}`, group: 'Sessions', title: row.title ?? row.id, mark: item.harness, detail: `${item.label} · ${row.cwd ?? ''} · ${shortTime(row.updated)}`, open: () => { void c.revealSession(item.key, row.id, false); } });
          }
        }
      }
      if (mine !== ticket.current) return;
      setResults(found);
      setActive(0);
      setBusy(false);
      setNote(`${found.length ? `${found.length} result${found.length === 1 ? '' : 's'}${scope === 'all' ? `, ${FEW} at most per group: pick a scope for more` : ''}` : 'Nothing found.'}${errors.length ? ` · ${errors.join(' · ')}` : ''}`);
    }, 250);
    return () => window.clearTimeout(timer);
  }, [c, query, scope]);

  const open = (result: Result | undefined): void => {
    if (!result) return;
    c.close();
    result.open();
  };
  const groups = [...new Set(results.map((result) => result.group))];
  return (
    <Dialog open onOpenChange={(next) => { if (!next) c.close(); }} title="Search everything" description="Rooms, members, messages and sessions" width="xl" footer={<><Text variant="muted">↑ ↓ move · Enter open</Text><Spacer /><Button variant="outline" onClick={() => c.close()}>Close</Button></>}>
      <Column gap="md">
        <SearchField
          autoFocus
          aria-label="Search everything"
          placeholder="rooms, members, messages, sessions"
          value={query}
          onValueChange={setQuery}
          onKeyDown={(event) => {
            if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
              event.preventDefault();
              if (results.length) setActive((at) => (at + (event.key === 'ArrowDown' ? 1 : -1) + results.length) % results.length);
            } else if (event.key === 'Enter') {
              event.preventDefault();
              open(results[active]);
            }
          }}
        />
        <ToggleGroup aria-label="Search in" value={scope} onValueChange={(value) => setScope(value as Scope)}>
          {SCOPES.map(([value, label]) => <ToggleGroupItem key={value} value={value}>{label}</ToggleGroupItem>)}
        </ToggleGroup>
        {busy ? <Progress inline aria-label="Searching" value={null} /> : null}
        <Text variant="muted" role="status">{note}</Text>
        {groups.map((group) => (
          <Column key={group} gap="xs">
            <Heading size="label" level={3}>{group}</Heading>
            <ItemList aria-label={group}>
              {results.filter((result) => result.group === group).map((result) => (
                <ItemRow
                  key={result.id}
                  title={result.title}
                  description={result.detail}
                  leading={result.mark ? <HarnessMark harness={result.mark} /> : undefined}
                  current={results[active]?.id === result.id}
                  onClick={() => open(result)}
                />
              ))}
            </ItemList>
          </Column>
        ))}
      </Column>
    </Dialog>
  );
}
