// The room's two views of the same events (spec 2026-11-16): the feed (a conversation) and the graph (who wrote to whom, up to the
// selected event), with one event selected in the graph and one filter. Drawn with Sigrid's `VirtualConversation` and `GraphCanvas`; the
// lines of the graph are computed by chatmodel.ts and here, from the events. A message's text is always text.
// The lanes of the first design (a column per member, a row per event) were tried on `GraphCanvas` with places given by the app: at the
// size that fits a room the names are too small to read and the canvas has to be panned, so the view is not kept (Feed and Graph).
import { useEffect, useMemo, useRef } from 'react';
import { Message } from '@acaprino/sigrid/components';
import { GraphCanvas, GraphLegend, type GraphEdge, type GraphLegendEntry, type GraphNode } from '@acaprino/sigrid/graph';
import { VirtualConversation, type VirtualConversationHandle } from '@acaprino/sigrid/virtual';
import { Badge, Cluster, Column, Text } from '@acaprino/sigrid/primitives';
import type { ChatController } from '../chatcontroller.ts';
import {
  actorOf, eventLine, instructions, linesUpTo, presentAt, recipientsOf, segments, type Segment,
} from '../chatmodel.ts';
import type { ChatEvent, Member } from '../types.ts';
import { HarnessMark, logoOf } from './mark.tsx';

const nameOf = (id: string | null, members: Member[]): string => members.find((member) => member.member_id === id)?.name ?? id ?? 'someone';
const actorName = (event: ChatEvent): string => (event.actor.type === 'member' ? event.actor.name : 'local admin');
const clock = (event: ChatEvent): string => new Date(event.timestamp).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });
const namesOf = (members: Member[]): Map<string, string> => new Map(members.map((member) => [member.member_id, member.name]));

// --- the feed ----------------------------------------------------------------------------------------

/** A message's text, its known mentions in strong type. */
function Words({ text, members }: { text: string; members: Member[] }) {
  const parts: React.ReactNode[] = [];
  let at = 0;
  for (const found of text.matchAll(/(^|[^A-Za-z0-9_@.-])@([a-z0-9][a-z0-9_-]{0,31})/g)) {
    const start = (found.index ?? 0) + found[1].length;
    const known = found[2] === 'all' || members.some((member) => member.name === found[2]);
    parts.push(text.slice(at, start), known ? <Text key={start} variant="strong">@{found[2]}</Text> : `@${found[2]}`);
    at = start + found[2].length + 1;
  }
  parts.push(text.slice(at));
  return <Text as="p" variant="soft" pre>{parts}</Text>;
}

/** One event as the feed shows it: a message as a `Message`, any other event as a line of soft text. */
function FeedItem({ event, chat }: { event: ChatEvent; chat: ChatController }) {
  const members = chat.members;
  if (event.kind !== 'message') {
    return <Text variant="soft">{clock(event)} · #{event.event_id} · {eventLine(event, namesOf(members)).text}</Text>;
  }
  const mine = chat.room ? chat.mine[chat.room]?.member_id ?? null : null;
  const member = members.find((one) => one.member_id === actorOf(event));
  const to = recipientsOf(event, members).map((id) => nameOf(id, members));
  const states = instructions(chat.events, members).filter((one) => one.event.event_id === event.event_id);
  const host = event.actor.type === 'member' && (event.actor.host || member?.host);
  const own = actorOf(event) === mine || Boolean(host);
  return (
    <Message
      from={own ? 'user' : 'assistant'}
      name={actorName(event)}
      avatar={own ? undefined : <HarnessMark harness={member?.harness ?? 'other'} />}
      time={`${event.actor.type === 'member' ? `${event.actor.role ?? ''} · ` : ''}${clock(event)} · #${event.event_id}`}
      copyText={String(event.payload.text ?? '')}
    >
      <Column gap="xs">
        <Words text={String(event.payload.text ?? '')} members={members} />
        <Cluster gap="sm">
          <Text variant="muted">to {to.length ? to.join(', ') : 'everyone'}</Text>
          {states.map((state) => <Badge key={state.worker.member_id} tone={state.state === 'done' ? 'success' : state.state === 'blocked' ? 'danger' : 'info'}>{state.worker.name}: {state.state}</Badge>)}
        </Cluster>
      </Column>
    </Message>
  );
}

export function Feed({ chat }: { chat: ChatController }) {
  const shown = chat.shown();
  const handle = useRef<VirtualConversationHandle>(null);
  // A search result opens the feed at its event, once.
  const target = chat.scrollTo;
  useEffect(() => {
    if (target === null) return;
    const at = shown.findIndex((event) => event.event_id === target);
    if (at >= 0) handle.current?.scrollToIndex(at, 'center');
    chat.scrollTo = null;
  }, [chat, target, shown]);
  return (
    <Column fill>
      <VirtualConversation label="Events of the room" ref={handle} count={shown.length} rowKey={(index) => shown[index]!.event_id} estimateSize={88}>
        {(index) => <FeedItem event={shown[index]!} chat={chat} />}
      </VirtualConversation>
    </Column>
  );
}

// --- the graph ---------------------------------------------------------------------------------------

const LINE: Record<Segment['type'], { label: string; style: NonNullable<GraphEdge['style']>; tone: NonNullable<GraphEdge['tone']> }> = {
  msg: { label: 'message', style: 'solid', tone: 'default' },
  done: { label: 'signal done', style: 'solid', tone: 'accent' },
  blocked: { label: 'blocked', style: 'dashed', tone: 'destructive' },
  obs: { label: 'observation', style: 'dotted', tone: 'default' },
};

/** Who wrote to whom up to the selected event: the members present then around «everyone», a line per pair and kind (counted), and the lines of the selected event in the accent. */
export function Graph({ chat }: { chat: ChatController }) {
  const { events, members } = chat;
  const selected = chat.selected();
  const at = selected ? events.indexOf(selected) : events.length - 1;
  const current = events[at];
  const mine = chat.room ? chat.mine[chat.room]?.member_id ?? null : null;
  const filter = chat.filter();
  const { nodes, edges, legend } = useMemo(() => {
    const present = current ? presentAt(members, current.timestamp) : [];
    const here = new Set(present.map((one) => one.member.member_id));
    const lines = current ? linesUpTo(events, at, members, filter) : [];
    const now = current ? segments(current, events, members) : [];
    const key = (segment: Segment) => `${[segment.from, segment.to].sort().join('|')}|${segment.type}`;
    const nowKeys = new Set(now.map(key));
    const used = lines.filter((line) => (line.from === 'hub' || here.has(line.from)) && (line.to === 'hub' || here.has(line.to)));
    const graphNodes: GraphNode[] = present.map(({ member, left }) => ({
      id: member.member_id,
      label: member.name,
      detail: [member.role, left ? 'left' : member.member_id === mine ? 'you' : ''].filter(Boolean).join(' · '),
      image: logoOf(member.harness),
      icon: logoOf(member.harness) ? undefined : 'device',
    }));
    if (used.some((line) => line.from === 'hub' || line.to === 'hub')) graphNodes.push({ id: 'hub', label: 'everyone', icon: 'globe' });
    const graphEdges: GraphEdge[] = used.map((line) => ({
      id: key(line),
      from: line.from,
      to: line.to,
      kind: 'link',
      directed: true,
      label: line.count > 1 ? `×${line.count}` : undefined,
      style: LINE[line.type].style,
      tone: LINE[line.type].tone,
      path: nowKeys.has(key(line)) || undefined,
    }));
    const count = (type: Segment['type']) => used.filter((line) => line.type === type).reduce((sum, line) => sum + line.count, 0);
    const entries: GraphLegendEntry[] = (Object.keys(LINE) as Segment['type'][]).map((type) => ({ label: LINE[type].label, style: LINE[type].style, tone: LINE[type].tone, count: count(type) }));
    return { nodes: graphNodes, edges: graphEdges, legend: entries };
  }, [events, members, current, at, filter.kinds, filter.members, mine]);
  return (
    <Column fill>
      <Column fill>
        <GraphCanvas
          aria-label={current ? `Who wrote to whom, up to event ${at + 1} of ${events.length}` : 'Who wrote to whom'}
          model="graph"
          fill
          nodes={nodes}
          edges={edges}
          layoutKey={members.map((member) => member.member_id).join(',')}
          selected={chat.openMember}
          onSelect={(id) => { if (id !== null && id !== 'hub') chat.toggleMember(id); }}
        />
      </Column>
      <Column pad="sm"><GraphLegend aria-label="Kinds of line" entries={legend} /></Column>
    </Column>
  );
}
