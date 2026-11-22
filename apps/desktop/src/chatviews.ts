// The room's three views of the same events (spec 2026-11-16): the graph, the lanes and the feed,
// with one event selected in all three. Built with the DOM, never with innerHTML: a message's
// text is always text.

import { claimKey } from '@acaprino/imago/ui';
import { button, chip, h, icon, iconButton } from './dom.ts';
import { shortTime } from './tree.ts';
import {
  actorOf, groupOf, harnessKey, instructionOf, instructions, linesUpTo, passes, presentAt, recipientsOf, segments,
  type Filter, type Instruction, type Segment,
} from './chatmodel.ts';
import type { ChatEvent, Member } from './types.ts';

const SVG = 'http://www.w3.org/2000/svg';
function svg(tag: string, attrs: Record<string, string | number> = {}, ...children: (Node | string)[]): SVGElement {
  const node = document.createElementNS(SVG, tag);
  for (const [name, value] of Object.entries(attrs)) node.setAttribute(name, String(value));
  node.append(...children);
  return node;
}

/** What every view draws from. */
export interface RoomContext {
  /** Every event loaded, oldest first. */
  events: ChatEvent[];
  /** The room's members, in join order. */
  members: Member[];
  filter: Filter;
  selected: ChatEvent | undefined;
  /** The window's own member in the room. */
  mine: string | null;
  select(id: string): void;
}

const MARKS = new Set(['claude_code', 'codex', 'opencode', 'pi', 'desktop']);
const markId = (harness: string): string => { const key = harnessKey(harness); return `g-${MARKS.has(key) ? key : 'other'}`; };
/** A member's harness mark, a symbol of index.html. */
export function mark(harness: string): SVGElement {
  return svg('svg', { class: 'glyph', 'aria-hidden': 'true' }, svg('use', { href: `#${markId(harness)}` }));
}

const nameOf = (id: string | null, members: Member[]): string => members.find((member) => member.member_id === id)?.name ?? id ?? 'someone';
const actorName = (event: ChatEvent): string => (event.actor.type === 'member' ? event.actor.name : 'local admin');
const time = (event: ChatEvent): string => new Date(event.timestamp).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });

/** What a non-message event says, in one line. */
export function describe(event: ChatEvent, ctx: Pick<RoomContext, 'events' | 'members'>): string {
  const who = actorName(event);
  const payload = event.payload;
  const targets = event.recipient_ids.map((id) => nameOf(id, ctx.members)).join(', ');
  switch (event.kind) {
    case 'open': return `${who} opened the room`;
    case 'close': return `${who} closed the room`;
    case 'reopen': return `${who} reopened the room`;
    case 'join': return `${who} joined as ${payload.role ?? 'member'}`;
    case 'rejoin': return `${who} rejoined as ${payload.role ?? 'member'}`;
    case 'leave': return `${who} left the room`;
    case 'observe': return `${who} observed ${targets || 'a member'}`;
    case 'signal': return `${who} signalled ${payload.signal} on #${payload.instruction_event_id}${payload.reason ? `: ${payload.reason}` : ''}`;
    case 'bind': return `${who} bound a session`;
    case 'unbind': return `${who} unbound their session`;
    case 'set_role': case 'role': return `${who} set a role${payload.role ? `: ${payload.role}` : ''}${targets ? ` for ${targets}` : ''}`;
    case 'retire': return `${who} retired ${targets || 'a member'}`;
    default: return `${who}: ${event.kind}`;
  }
}

/** A message's text, its known mentions marked. */
function richText(text: string, members: Member[]): HTMLElement {
  const paragraph = h('p', {});
  let at = 0;
  for (const found of text.matchAll(/(^|[^A-Za-z0-9_@.-])@([a-z0-9][a-z0-9_-]{0,31})/g)) {
    const start = (found.index ?? 0) + found[1].length;
    const known = found[2] === 'all' || members.some((member) => member.name === found[2]);
    paragraph.append(text.slice(at, start), known ? h('span', { class: 'at' }, `@${found[2]}`) : `@${found[2]}`);
    at = start + found[2].length + 1;
  }
  paragraph.append(text.slice(at));
  return paragraph;
}

const STATE_TEXT: Record<Instruction['state'], string> = { waiting: 'waiting', blocked: 'blocked', done: 'done' };

/** One event as the feed shows it; `plain` is the caption under the graph (not selectable). */
export function feedCard(event: ChatEvent, ctx: RoomContext, plain = false): HTMLElement {
  const current = !plain && event.event_id === ctx.selected?.event_id;
  const attrs = plain ? { class: '' } : { 'data-id': event.event_id, role: 'option', 'aria-selected': String(current), id: `ev-${event.event_id}` };
  if (event.kind !== 'message') {
    const kind = event.kind === 'signal' ? String(event.payload.signal) : event.kind;
    const instruction = event.kind === 'signal' ? instructionOf(event, ctx.events) : undefined;
    return h('div', { ...attrs, class: `msg sys k-${kind}` },
      h('span', {}, `${time(event)} · #${event.event_id} · ${describe(event, ctx)}`),
      instruction ? button(`show #${instruction.event_id}`, { class: 'im-btn--ghost im-btn--sm', onclick: (click: Event) => { click.stopPropagation(); ctx.select(instruction.event_id); } }) : null);
  }
  const member = ctx.members.find((one) => one.member_id === actorOf(event));
  const to = recipientsOf(event, ctx.members).map((id) => nameOf(id, ctx.members));
  const states = instructions(ctx.events, ctx.members).filter((one) => one.event.event_id === event.event_id);
  const host = event.actor.type === 'member' && (event.actor.host || member?.host);
  return h('article', { ...attrs, class: `msg${actorOf(event) === ctx.mine ? ' mine' : ''}` },
    h('div', { class: 'meta' },
      h('span', { class: 'who' }, mark(member?.harness ?? 'other'), actorName(event),
        actorOf(event) === ctx.mine ? chip('you') : host ? chip('user', '', 'The host’s member: what it sends is the user’s') : null),
      h('span', {}, `${event.actor.type === 'member' ? `${event.actor.role ?? ''} · ` : ''}${time(event)} · #${event.event_id}`)),
    h('div', { class: 'body' },
      richText(String(event.payload.text ?? ''), ctx.members),
      h('div', { class: 'to' }, 'to ', ...(to.length ? to.map((name) => h('b', {}, name)) : ['everyone']),
        ...states.map((state) => h('span', { class: `state st-${state.state}` }, `${state.worker.name}: ${STATE_TEXT[state.state]}`)))));
}

/** The feed: every event the filter shows, as a list with the selected one. */
export function feedView(ctx: RoomContext, shown: ChatEvent[]): HTMLElement {
  const list = h('div', { class: 'feed', role: 'listbox', 'aria-label': 'Events' });
  for (const event of shown) list.append(feedCard(event, ctx));
  if (ctx.selected && shown.includes(ctx.selected)) list.setAttribute('aria-activedescendant', `ev-${ctx.selected.event_id}`);
  list.addEventListener('click', (click) => {
    const card = (click.target as HTMLElement).closest<HTMLElement>('[data-id]');
    if (card && !(click.target as HTMLElement).closest('button')) ctx.select(card.dataset.id!);
  });
  return list;
}

/** The lanes: a column per member in join order, a row per event, a wire from sender to receivers. */
export function lanesView(ctx: RoomContext, shown: ChatEvent[]): HTMLElement {
  const members = ctx.members;
  const n = Math.max(members.length, 1);
  const center = (i: number) => ((i + 0.5) / n) * 100;
  const ix = (id: string | null) => members.findIndex((member) => member.member_id === id);
  const lives = () => members.map((_, i) => h('i', { class: 'life', style: `left:${center(i)}%` }));
  const head = h('div', { class: 'lhead' }, h('time', {}), h('div', { class: 'larea' },
    ...members.map((member, i) => h('div', { class: `lname${member.left_at ? ' left' : ''}`, style: `left:${center(i)}%` },
      h('span', {}, mark(member.harness), member.name), h('small', {}, member.role)))));
  const lanes = h('div', { class: 'lanes', role: 'listbox', 'aria-label': 'Events by member' }, head);
  for (const event of shown) {
    const from = ix(actorOf(event));
    const kind = event.kind === 'signal' ? String(event.payload.signal) : event.kind;
    let to: number[] = [];
    if (event.kind === 'message') {
      const addressed = recipientsOf(event, members).map(ix);
      to = addressed.length ? addressed : members.map((_, i) => i).filter((i) => i !== from);
    } else if (event.kind === 'signal') {
      const instruction = instructionOf(event, ctx.events);
      to = [ix(instruction ? actorOf(instruction) : null)];
    } else if (event.kind === 'observe') {
      to = event.recipient_ids.map(ix);
    }
    to = to.filter((i) => i >= 0);
    const wired = from >= 0 && to.length > 0;
    const tone = event.kind === 'message' ? '' : ` c-${kind}`;
    const area = h('div', { class: 'larea' }, ...lives());
    if (wired) {
      const lo = Math.min(from, ...to);
      const hi = Math.max(from, ...to);
      area.append(
        h('span', { class: `wire ${event.kind === 'message' ? '' : kind}`.trim(), style: `left:${center(lo)}%;width:${center(hi) - center(lo)}%` }),
        h('span', { class: `wdot${tone}`, style: `left:${center(from)}%` }),
        ...to.map((i) => h('span', { class: `warw ${i > from ? 'r' : 'l'}${tone}`, style: `left:${center(i)}%` })));
    }
    if (event.kind === 'message') {
      const lo = wired ? Math.min(from, ...to) : Math.max(from, 0);
      const hi = wired ? Math.max(from, ...to) : Math.max(from, 0);
      const width = Math.max(center(hi) - center(lo), 46);
      const left = Math.max(0, Math.min(center(lo) - 3, 100 - width));
      const names = recipientsOf(event, members).map((id) => nameOf(id, members));
      area.append(h('div', { class: 'lmsg', style: `margin-left:${left}%;width:${width}%` },
        richText(String(event.payload.text ?? ''), members),
        h('div', { class: 'to' }, `#${event.event_id} · ${actorName(event)} to ${names.length ? names.join(', ') : 'everyone'}`)));
    } else {
      const label = from >= 0 ? describe(event, ctx).replace(`${actorName(event)} `, '') : describe(event, ctx);
      area.append(h('div', { class: 'lchipwrap' },
        h('span', { class: `lchip k-${kind}`, style: `left:clamp(66px, ${from >= 0 ? center(from) : 50}%, calc(100% - 66px))` }, label)));
    }
    const current = event.event_id === ctx.selected?.event_id;
    lanes.append(h('div', {
      class: `lrow${wired && event.kind !== 'message' ? ' wired' : ''}`, role: 'option', id: `ln-${event.event_id}`, 'data-id': event.event_id,
      'aria-selected': String(current), 'aria-label': `${time(event)}, ${event.kind === 'message' ? `${actorName(event)}: ${String(event.payload.text ?? '')}` : describe(event, ctx)}`,
    }, h('time', {}, shortTime(event.timestamp)), area));
  }
  if (ctx.selected && shown.includes(ctx.selected)) lanes.setAttribute('aria-activedescendant', `ln-${ctx.selected.event_id}`);
  lanes.addEventListener('click', (click) => {
    const row = (click.target as HTMLElement).closest<HTMLElement>('[data-id]');
    if (row) ctx.select(row.dataset.id!);
  });
  return lanes;
}

// --- the graph -------------------------------------------------------------------------------------

const HALF = { width: 88, height: 31 };
const HUB = { width: 58, height: 30 };
/** Lines of different kinds between the same two members run side by side. */
const SHIFT: Record<Segment['type'], number> = { msg: 0, done: 7, blocked: -7, obs: 14 };

type Point = { x: number; y: number };
/** Fixed places in join order around the hub: a member does not move when another joins. */
function layout(members: Member[], width: number, height: number): Map<string, Point> {
  const rx = Math.max(120, width / 2 - 108);
  const ry = Math.max(70, height / 2 - 54);
  return new Map(members.map((member, i) => {
    const angle = -Math.PI / 2 + (i * 2 * Math.PI) / Math.max(members.length, 1);
    return [member.member_id, { x: width / 2 + rx * Math.cos(angle), y: height / 2 - 8 + ry * Math.sin(angle) }];
  }));
}

/** How far `p` is from the segment `a`–`b`. */
function distance(p: Point, a: Point, b: Point): number {
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  const t = Math.max(0, Math.min(1, ((p.x - a.x) * dx + (p.y - a.y) * dy) / (dx * dx + dy * dy || 1)));
  return Math.hypot(p.x - (a.x + t * dx), p.y - (a.y + t * dy));
}

/** Where the line from `from` toward `to` leaves a box of half sizes `hw`, `hh` around `to`. */
function edge(from: Point, to: Point, hw: number, hh: number): Point {
  const dx = to.x - from.x;
  const dy = to.y - from.y;
  const t = Math.min(hw / Math.abs(dx || 1e-6), hh / Math.abs(dy || 1e-6), 0.9);
  return { x: to.x - dx * t, y: to.y - dy * t };
}

export interface GraphOptions {
  width: number;
  height: number;
  /** The member whose details are open. */
  open: string | null;
  pickMember(id: string): void;
}

/** Who wrote to whom up to the selected event: plates around the hub (everyone), a line per pair
 *  and kind, wider with each event, and the selected event's line with an arrowhead. */
export function graphView(ctx: RoomContext, options: GraphOptions): SVGElement {
  const { width, height } = options;
  const events = ctx.events;
  const at = ctx.selected ? events.indexOf(ctx.selected) : events.length - 1;
  const current = events[at];
  const places = layout(ctx.members, width, height);
  const hub: Point = { x: width / 2, y: height / 2 - 8 };
  const point = (id: string) => (id === 'hub' ? hub : places.get(id));
  const half = (id: string) => (id === 'hub' ? HUB : HALF);
  const present = current ? presentAt(ctx.members, current.timestamp) : [];
  const shown = new Set(present.map((one) => one.member.member_id));
  const line = (segment: Segment, cls: string, extra: Record<string, string | number> = {}): SVGElement | null => {
    const a = point(segment.from);
    const b = point(segment.to);
    if (!a || !b || (segment.from !== 'hub' && !shown.has(segment.from)) || (segment.to !== 'hub' && !shown.has(segment.to))) return null;
    const [u, v] = [segment.from, segment.to].sort();
    const pu = point(u)!;
    const pv = point(v)!;
    const length = Math.hypot(pv.x - pu.x, pv.y - pu.y) || 1;
    const k = SHIFT[segment.type] / length;
    const offset = { x: -(pv.y - pu.y) * k, y: (pv.x - pu.x) * k };
    const from = { x: a.x + offset.x, y: a.y + offset.y };
    const to = { x: b.x + offset.x, y: b.y + offset.y };
    // Two members opposite each other would draw their line through the hub: it bends around it.
    if (segment.from !== 'hub' && segment.to !== 'hub' && distance(hub, from, to) < HUB.height + 14) {
      const control = { x: (from.x + to.x) / 2 - ((pv.y - pu.y) / length) * 150, y: (from.y + to.y) / 2 + ((pv.x - pu.x) / length) * 150 };
      const start = edge(control, from, half(segment.from).width, half(segment.from).height);
      const end = edge(control, to, half(segment.to).width, half(segment.to).height);
      const d = `M${start.x.toFixed(1)} ${start.y.toFixed(1)} Q${control.x.toFixed(1)} ${control.y.toFixed(1)} ${end.x.toFixed(1)} ${end.y.toFixed(1)}`;
      return svg('path', { class: `${cls} ${segment.type}`, d, ...extra });
    }
    const start = edge(to, from, half(segment.from).width, half(segment.from).height);
    const end = edge(from, to, half(segment.to).width, half(segment.to).height);
    return svg('line', { class: `${cls} ${segment.type}`, x1: start.x.toFixed(1), y1: start.y.toFixed(1), x2: end.x.toFixed(1), y2: end.y.toFixed(1), ...extra });
  };
  const root = svg('svg', { viewBox: `0 0 ${width} ${height}`, width, height, role: 'group', 'aria-label': `Who wrote to whom, up to event ${at + 1} of ${events.length}` },
    svg('defs', {}, svg('marker', { id: 'arw', viewBox: '0 0 10 10', refX: 9, refY: 5, markerWidth: 7, markerHeight: 7, orient: 'auto' },
      svg('path', { d: 'M0 0 10 5 0 10z', fill: 'context-stroke' }))));
  for (const segment of current ? linesUpTo(events, at, ctx.members, ctx.filter) : []) {
    const drawn = line(segment, 'g-line', { 'stroke-width': Math.min(1 + segment.count * 0.9, 5).toFixed(1) });
    if (drawn) root.append(drawn);
  }
  for (const segment of current ? segments(current, events, ctx.members) : []) {
    const drawn = line(segment, 'g-now', { 'marker-end': 'url(#arw)', 'data-now': '1' });
    if (drawn) root.append(drawn);
  }
  root.append(svg('g', { class: 'hub', transform: `translate(${hub.x} ${hub.y})` },
    svg('polygon', { class: 'plate', points: '-58,0 0,-30 58,0 0,30' }),
    svg('text', { class: 'nm', y: 0 }, 'everyone'),
    svg('text', { class: 'rl', y: 13 }, events.length ? `${at + 1} of ${events.length}` : 'no events')));
  for (const { member, left } of present) {
    const place = places.get(member.member_id)!;
    const dim = ctx.filter.members.length > 0 && !ctx.filter.members.includes(member.member_id);
    const state = left ? 'left' : member.member_id === ctx.mine ? 'you' : '';
    const plate = svg('g', {
      class: ['node', left ? 'left' : '', dim ? 'dim' : '', member.member_id === ctx.mine ? 'mine' : ''].filter(Boolean).join(' '),
      transform: `translate(${place.x.toFixed(1)} ${place.y.toFixed(1)})`, tabindex: 0, role: 'button', 'aria-pressed': String(options.open === member.member_id),
      'aria-label': [member.name, member.role, member.harness, state].filter(Boolean).join(', '),
    },
    svg('rect', { class: 'plate', x: -HALF.width, y: -HALF.height, width: HALF.width * 2, height: HALF.height * 2 }),
    svg('line', { class: 'top', x1: -HALF.width + 1, y1: -HALF.height + 1, x2: HALF.width - 1, y2: -HALF.height + 1 }),
    svg('use', { href: `#${markId(member.harness)}`, x: -76, y: -13, width: 26, height: 26, class: 'plate-mark' }),
    svg('text', { class: 'nm', x: -40, y: -3 }, member.name),
    svg('text', { class: 'rl', x: -40, y: 15 }, member.role),
    svg('text', { class: 'st', x: 78, y: 15 }, state));
    plate.addEventListener('click', () => options.pickMember(member.member_id));
    plate.addEventListener('keydown', (key) => { if ((key as KeyboardEvent).key === 'Enter' || (key as KeyboardEvent).key === ' ') { key.preventDefault(); options.pickMember(member.member_id); } });
    root.append(plate);
  }
  // The nodes are one tab stop, with the arrows between them. `roving` serves HTML elements only (it ignores the key of an SVG
  // target), so the graph does it itself, on the same rules: the arrows and Home and End stop at the first and the last node.
  const nodes = [...root.querySelectorAll<SVGGElement>('.node')];
  const stop = nodes.find((node) => node.getAttribute('aria-pressed') === 'true') ?? nodes[0];
  for (const node of nodes) node.setAttribute('tabindex', node === stop ? '0' : '-1');
  root.addEventListener('focusin', (event) => {
    const node = (event.target as Element).closest('.node');
    if (node) for (const one of nodes) one.setAttribute('tabindex', one === node ? '0' : '-1');
  });
  root.addEventListener('keydown', (event) => {
    const key = (event as KeyboardEvent).key;
    const from = nodes.indexOf((event.target as Element).closest('.node') as SVGGElement);
    if (from < 0 || (event as KeyboardEvent).altKey || (event as KeyboardEvent).ctrlKey || (event as KeyboardEvent).metaKey) return;
    const next = key === 'ArrowRight' || key === 'ArrowDown' ? Math.min(nodes.length - 1, from + 1)
      : key === 'ArrowLeft' || key === 'ArrowUp' ? Math.max(0, from - 1)
      : key === 'Home' ? 0 : key === 'End' ? nodes.length - 1 : -1;
    if (next < 0) return;
    event.preventDefault();
    claimKey(event);
    nodes[next].focus();
  });
  return root;
}

/** The selected event travels along its line once, unless motion is turned off. */
export function animateNow(root: Element): void {
  if (matchMedia('(prefers-reduced-motion: reduce)').matches) return;
  for (const line of root.querySelectorAll<SVGGeometryElement>('[data-now]')) {
    const length = line.getTotalLength();
    const frames = Array.from({ length: 13 }, (_, i) => {
      const point = line.getPointAtLength((length * i) / 12);
      return { transform: `translate(${point.x}px, ${point.y}px)` };
    });
    const dot = svg('rect', { class: 'g-dot', x: -4, y: -4, width: 8, height: 8 });
    root.append(dot);
    const motion = dot.animate(frames, { duration: 620, easing: 'cubic-bezier(.2,.8,.2,1)' });
    motion.onfinish = () => dot.remove();
  }
}

/** The strip under the graph: a mark per event, play, previous, next, where the replay is. */
export function stripView(ctx: RoomContext, playing: boolean, controls: { play(): void; step(by: number): void }): HTMLElement {
  const events = ctx.events;
  const at = ctx.selected ? events.indexOf(ctx.selected) : events.length - 1;
  const ticks = h('div', { class: 'ticks' });
  events.forEach((event, i) => {
    const group = groupOf(event);
    const warn = event.kind === 'signal' && event.payload.signal === 'blocked';
    ticks.append(button([], {
      class: `tick k-${warn ? 'warn' : group}${i > at ? ' after' : ''}${passes(event, ctx.filter, ctx.members) ? '' : ' hide'}`,
      'aria-current': String(i === at), title: `${shortTime(event.timestamp)} · #${event.event_id} · ${event.kind}`,
      'aria-label': `${shortTime(event.timestamp)}, ${event.kind === 'message' ? `${actorName(event)} wrote` : describe(event, ctx)}`,
      onclick: () => ctx.select(event.event_id),
    }));
  });
  return h('div', { class: 'scrub', role: 'group', 'aria-label': 'Replay the room' },
    iconButton(playing ? 'Pause' : 'Play from here', playing ? 'pause' : 'play', { title: `${playing ? 'Pause' : 'Play from here'} (Space)`, onclick: () => controls.play() }),
    iconButton('Previous event', 'prev', { disabled: at <= 0, onclick: () => controls.step(-1) }),
    iconButton('Next event', 'next', { disabled: at >= events.length - 1, onclick: () => controls.step(1) }),
    ticks,
    h('span', { class: 'pos' }, events.length ? `${at + 1} / ${events.length}` : '0 / 0'));
}
