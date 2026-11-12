// F5, §4.3: the transcript reader. Only the entries in sight are in the page (a session can hold
// 10,000 of them): each entry's height is guessed until it has been drawn once, then measured.
// Tabs: Conversation, and Raw (the native records of one entry, pretty JSON).

import './reader.css';
import { button, fill, h, kbd } from './dom.ts';
import type { Block, EntryView, Transcript } from './types.ts';

/** Tool input shown before "Show all" (spec F5). */
export const INPUT_CUT = 240;

const ROLE_LABEL: Record<Block['role'], string> = {
  user: 'user', assistant: 'assistant', reasoning: 'reasoning', tool: 'tool', result: 'result',
  error: 'error', attachment: 'file', compaction: 'compaction',
};

export interface ReaderActions {
  back(): void;
  setTab(tab: 'conversation' | 'raw'): void;
  toggleTurns(): void;
  showRaw(entryId: string): void;
}

function timeOf(entry: EntryView): string {
  if (!entry.time) return '';
  const time = new Date(entry.time);
  return Number.isNaN(time.getTime()) ? entry.time : time.toLocaleTimeString();
}

/** The entry's first role, for its label and colour. */
function leadRole(entry: EntryView): Block['role'] {
  if (entry.kind === 'user') return 'user';
  if (entry.kind === 'compaction') return 'compaction';
  if (entry.kind === 'results') return entry.blocks.some((b) => b.role === 'error') ? 'error' : 'result';
  return entry.blocks.find((b) => b.role === 'assistant')?.role ?? entry.blocks[0]?.role ?? 'assistant';
}

export class Reader {
  readonly root: HTMLElement;
  private readonly scroller: HTMLElement;
  private readonly spacer: HTMLElement;
  private readonly content: HTMLElement;
  private entries: EntryView[] = [];
  private heights: number[] = [];
  private offsets: number[] = [0];
  private expanded = new Set<string>();
  private first = 0;
  private frame = 0;
  private observer: ResizeObserver;
  private actions: ReaderActions;

  constructor(actions: ReaderActions) {
    this.actions = actions;
    this.content = h('div', { class: 'reader-content' });
    this.spacer = h('div', { class: 'reader-spacer' }, this.content);
    this.scroller = h('div', { class: 'reader-scroll', tabindex: '-1' }, this.spacer);
    this.root = h('div', { class: 'reader' });
    this.scroller.addEventListener('scroll', () => this.schedule());
    this.observer = new ResizeObserver(() => this.measure());
    this.observer.observe(this.content);
    this.observer.observe(this.scroller);
  }

  /** Draw the head and either tab. */
  render(transcript: Transcript | null, opts: { tab: 'conversation' | 'raw'; turns: number; defaultTurns: number; title: string; error: string | null; raw: unknown[] | null; rawEntry: string | null }): void {
    const turnsLabel = opts.defaultTurns > 0 ? `Last ${opts.defaultTurns} turns` : 'Last turns';
    const head = h('div', { class: 'column-head' },
      button(['‹ Detail ', kbd('esc')], { class: 'ghost', 'aria-label': 'Back to the session detail', onclick: () => this.actions.back() }),
      h('h2', { class: 'reader-title', title: opts.title }, opts.title),
      h('span', { class: 'spring' }),
      h('div', { class: 'seg', role: 'group', 'aria-label': 'Reader tab' },
        button('Conversation', { 'aria-pressed': String(opts.tab === 'conversation'), onclick: () => this.actions.setTab('conversation') }),
        button('Raw', { 'aria-pressed': String(opts.tab === 'raw'), onclick: () => this.actions.setTab('raw') })),
      h('div', { class: 'seg', role: 'group', 'aria-label': 'How much to show' },
        button(turnsLabel, { 'aria-pressed': String(opts.turns !== 0), onclick: () => opts.turns === 0 && this.actions.toggleTurns() }),
        button(['All ', kbd('t')], { 'aria-pressed': String(opts.turns === 0), onclick: () => opts.turns !== 0 && this.actions.toggleTurns() })));
    let body: Node;
    if (opts.error) body = h('div', { class: 'empty' }, h('h2', {}, 'This transcript could not be read'), h('p', {}, opts.error));
    else if (!transcript) body = h('div', { class: 'empty' }, h('p', {}, 'Reading the transcript…'));
    else if (opts.tab === 'raw') body = this.rawView(transcript, opts.raw, opts.rawEntry);
    else {
      this.setEntries(transcript);
      body = this.scroller;
    }
    fill(this.root, head, transcript && transcript.hidden_turns > 0 && opts.tab === 'conversation'
      ? h('div', { class: 'reader-hidden' }, `${transcript.hidden_turns} earlier turn${transcript.hidden_turns === 1 ? '' : 's'} not shown · `,
        button(['Show all ', kbd('t')], { class: 'link', onclick: () => this.actions.toggleTurns() }))
      : null, body);
    if (body === this.scroller) this.schedule();
  }

  private rawView(transcript: Transcript, raw: unknown[] | null, entryId: string | null): Node {
    const select = h('select', { 'aria-label': 'Entry' });
    for (const entry of transcript.entries) {
      const text = entry.blocks[0]?.text.replace(/\s+/g, ' ').slice(0, 70) ?? '';
      select.append(h('option', { value: entry.id, selected: entry.id === entryId }, `${leadRole(entry)} · ${text}`));
    }
    select.addEventListener('change', () => this.actions.showRaw(select.value));
    if (!entryId && transcript.entries[0]) queueMicrotask(() => this.actions.showRaw(transcript.entries[0].id));
    return h('div', { class: 'raw' },
      h('div', { class: 'raw-head' }, h('label', { class: 'lab' }, 'Native records of'), select),
      h('pre', { class: 'code raw-json' }, raw === null ? 'Reading…' : JSON.stringify(raw.length === 1 ? raw[0] : raw, null, 2)));
  }

  private setEntries(transcript: Transcript): void {
    if (transcript.entries === this.entries) return;
    const keep = this.entries.length && transcript.entries[0]?.id === this.entries[0]?.id;
    this.entries = transcript.entries;
    this.heights = this.entries.map((entry) => 44 + entry.blocks.reduce((sum, block) => sum + Math.min(400, 22 * Math.ceil(block.text.length / 90)), 0));
    this.offsets = [0];
    for (const height of this.heights) this.offsets.push(this.offsets[this.offsets.length - 1] + height);
    this.spacer.style.height = `${this.offsets[this.offsets.length - 1]}px`;
    if (!keep) {
      this.expanded.clear();
      // A transcript opens at its end, where the conversation is now.
      requestAnimationFrame(() => this.bottom());
    }
  }

  private schedule(): void {
    if (this.frame) return;
    this.frame = requestAnimationFrame(() => { this.frame = 0; this.draw(); });
  }

  /** Draw the entries in sight, plus a screen above and below. */
  private draw(): void {
    if (!this.scroller.isConnected) return;
    const top = this.scroller.scrollTop;
    const view = this.scroller.clientHeight || 600;
    let low = 0, high = this.entries.length;
    while (low < high) { const mid = (low + high) >> 1; if (this.offsets[mid + 1] < top - view) low = mid + 1; else high = mid; }
    const first = low;
    let last = first;
    while (last < this.entries.length && this.offsets[last] < top + view * 2) last += 1;
    this.first = first;
    this.content.style.transform = `translateY(${this.offsets[first]}px)`;
    fill(this.content, ...this.entries.slice(first, last).map((entry) => this.entryNode(entry)));
    if (!this.entries.length) fill(this.content, h('div', { class: 'empty' }, h('p', {}, 'This session has no conversation entries.')));
  }

  /** Put the drawn entries' real heights in place of the guesses. */
  private measure(): void {
    let changed = false;
    this.content.querySelectorAll<HTMLElement>('[data-index]').forEach((node) => {
      const index = Number(node.dataset.index);
      const height = node.offsetHeight;
      if (height && Math.abs(this.heights[index] - height) > 1) { this.heights[index] = height; changed = true; }
    });
    if (!changed) return;
    const anchor = this.scroller.scrollTop - this.offsets[this.first];
    this.offsets = [0];
    for (const height of this.heights) this.offsets.push(this.offsets[this.offsets.length - 1] + height);
    this.spacer.style.height = `${this.offsets[this.offsets.length - 1]}px`;
    this.content.style.transform = `translateY(${this.offsets[this.first]}px)`;
    if (this.pinned) this.scroller.scrollTop = this.scroller.scrollHeight;
    else this.scroller.scrollTop = this.offsets[this.first] + anchor;
  }

  private pinned = false;

  private entryNode(entry: EntryView): HTMLElement {
    const index = this.entries.indexOf(entry, this.first);
    const role = leadRole(entry);
    const blocks = entry.blocks.map((block, n) => this.blockNode(entry, block, n));
    const raw = button(`raw · ${entry.records}`, { class: 'ghost raw-link', title: 'Show the native records of this entry', onclick: () => this.actions.showRaw(entry.id) });
    return h('article', { class: `entry kind-${entry.kind} role-${role}`, 'data-index': String(index) },
      h('div', { class: 'who' }, h('span', { class: `r-${role}` }, ROLE_LABEL[role]), h('time', { datetime: entry.time ?? '' }, timeOf(entry)), raw),
      h('div', { class: 'what' }, ...blocks));
  }

  private blockNode(entry: EntryView, block: Block, n: number): Node {
    const key = `${entry.id}#${n}`;
    const open = this.expanded.has(key);
    const toggle = (label: string) => button(label, { class: 'ghost', onclick: () => { if (open) this.expanded.delete(key); else this.expanded.add(key); this.draw(); } });
    const cutNote = block.cut ? h('p', { class: 'muted cut' }, 'Cut here: the Raw tab has the whole record.') : null;
    switch (block.role) {
      case 'user': case 'assistant': return h('div', { class: `text ${block.role}` }, block.text, cutNote);
      case 'reasoning': return h('div', { class: 'text reasoning' }, block.text);
      case 'attachment': return h('div', { class: 'mono muted' }, `file ${block.text}`);
      case 'compaction': return h('div', { class: 'compaction' }, h('span', { class: 'lab' }, 'compaction'), h('p', {}, block.text), cutNote);
      case 'tool': {
        const long = block.text.length > INPUT_CUT;
        return h('div', { class: 'box tool' },
          h('div', { class: 'tool-head' }, h('span', { class: 'r-tool' }, block.name ?? 'tool'),
            h('span', { class: 'grow mono' }, long && !open ? `${block.text.slice(0, INPUT_CUT)}…` : ''),
            long ? toggle(open ? 'Show less' : 'Show all') : null),
          !long || open ? h('pre', { class: 'code' }, long ? pretty(block.text) : block.text) : null);
      }
      case 'result': case 'error': {
        const lines = block.text.split('\n');
        const long = lines.length > 6 || block.text.length > 600;
        const shown = long && !open ? `${lines.slice(0, 6).join('\n').slice(0, 600)}\n…` : block.text;
        return h('div', { class: `box result${block.role === 'error' ? ' failed' : ''}` },
          h('div', { class: 'tool-head' }, h('span', { class: block.role === 'error' ? 'r-error' : 'r-result' }, block.role === 'error' ? 'error' : 'result'),
            h('span', { class: 'grow muted mono' }, `${lines.length} line${lines.length === 1 ? '' : 's'}`), long ? toggle(open ? 'Show less' : 'Show all') : null),
          h('pre', { class: 'code' }, shown), cutNote);
      }
    }
  }

  // Keys (spec §4.1).
  scrollBy(lines: number): void { this.pinned = false; this.scroller.scrollTop += lines * 20; }
  page(direction: 1 | -1): void { this.pinned = false; this.scroller.scrollTop += direction * (this.scroller.clientHeight - 40); }
  top(): void { this.pinned = false; this.scroller.scrollTop = 0; }
  bottom(): void {
    this.pinned = true;
    this.scroller.scrollTop = this.scroller.scrollHeight;
    // Measured heights replace the guesses as the end comes into sight; follow them for a moment.
    setTimeout(() => { this.pinned = false; }, 400);
  }
  focus(): void { this.scroller.focus({ preventScroll: true }); }
}

/** Tool input as indented JSON when it is JSON. */
function pretty(text: string): string {
  try { return JSON.stringify(JSON.parse(text), null, 2); } catch { return text; }
}
