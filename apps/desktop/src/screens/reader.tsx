// The transcript reader (spec F5, §4.3): Sigrid's `VirtualConversation`, of which only the entries in sight are in the page (a session can
// hold 10,000 of them), one `Message` per native entry with its reasoning, tool calls and results behind rows of their own. Tabs:
// Conversation, and Raw (the native records of one entry, pretty JSON). A transcript opens at its end, where the conversation is now;
// a live session read again keeps the place.
import { useEffect, useMemo, useRef } from 'react';
import { VirtualConversation, type VirtualConversationHandle } from '@acaprino/sigrid/virtual';
import { Empty, Message, Note, Reasoning, ToolCall } from '@acaprino/sigrid/components';
import { Panel } from '@acaprino/sigrid/layout';
import { Button, Cluster, Column, Icon, Progress, ScrollArea, Select, Text, ToggleGroup, ToggleGroupItem } from '@acaprino/sigrid/primitives';
import type { Block, EntryView } from '../types.ts';
import { useAfter, useController } from './hooks.ts';

/** The entry's first role, for its label. */
function leadRole(entry: EntryView): Block['role'] {
  if (entry.kind === 'user') return 'user';
  if (entry.kind === 'compaction') return 'compaction';
  if (entry.kind === 'results') return entry.blocks.some((b) => b.role === 'error') ? 'error' : 'result';
  return entry.blocks.find((b) => b.role === 'assistant')?.role ?? entry.blocks[0]?.role ?? 'assistant';
}

const ROLE_NAME: Record<Block['role'], string> = {
  user: 'You', assistant: 'Assistant', reasoning: 'Reasoning', tool: 'Tool', result: 'Tool result', error: 'Error', attachment: 'File', compaction: 'Compaction',
};

/** Tool input as indented JSON when it is JSON. */
function pretty(text: string): string {
  try { return JSON.stringify(JSON.parse(text), null, 2); } catch { return text; }
}

function timeOf(entry: EntryView): string | undefined {
  if (!entry.time) return undefined;
  const time = new Date(entry.time);
  return Number.isNaN(time.getTime()) ? entry.time : time.toLocaleTimeString();
}

const CUT = <Text variant="muted">Cut here: the Raw tab has the whole record.</Text>;

function BlockView({ block }: { block: Block }) {
  switch (block.role) {
    case 'user': case 'assistant':
      return <Column gap="xs"><Text as="p" variant="soft" pre>{block.text}</Text>{block.cut ? CUT : null}</Column>;
    case 'reasoning':
      return <Reasoning>{block.text}</Reasoning>;
    case 'attachment':
      return <Text mono variant="muted">file {block.text}</Text>;
    case 'compaction':
      return <Note tone="info" title="Compaction">{block.text}{block.cut ? CUT : null}</Note>;
    case 'tool':
      return <ToolCall name={block.name ?? 'tool'} input={pretty(block.text)} summary={block.cut ? 'input cut' : undefined} />;
    case 'result': case 'error': {
      const lines = block.text.split('\n').length;
      return (
        <ToolCall
          name={block.role === 'error' ? 'Error' : 'Result'}
          status={block.role === 'error' ? 'error' : 'success'}
          summary={`${lines} line${lines === 1 ? '' : 's'}${block.cut ? ' · cut' : ''}`}
          output={block.text}
        />
      );
    }
  }
}

function Entry({ entry, onRaw }: { entry: EntryView; onRaw: () => void }) {
  const role = leadRole(entry);
  const text = entry.blocks.filter((b) => b.role === 'user' || b.role === 'assistant').map((b) => b.text).join('\n\n');
  return (
    <Message
      from={entry.kind === 'user' ? 'user' : 'assistant'}
      name={ROLE_NAME[role]}
      time={timeOf(entry)}
      copyText={text || undefined}
      actions={<Button variant="ghost" size="icon-sm" aria-label="Show the native records of this entry" title="Show the native records of this entry" onClick={onRaw}><Icon name="code" /></Button>}
    >
      <Column gap="sm">{entry.blocks.map((block, at) => <BlockView key={at} block={block} />)}</Column>
    </Message>
  );
}

function Raw() {
  const c = useController();
  const reader = c.state.reader!;
  const entries = reader.transcript?.entries ?? [];
  const first = entries[0]?.id;
  // Raw opened from the tab, not from an entry: it starts on the first.
  useEffect(() => { if (!reader.rawEntry && first) void c.showRaw(first); }, [c, reader.rawEntry, first]);
  const options = entries.map((entry) => ({ value: entry.id, label: `${leadRole(entry)} · ${entry.blocks[0]?.text.replace(/\s+/g, ' ').slice(0, 70) ?? ''}` }));
  const records = reader.raw;
  return (
    <Column gap="md" pad="md" fill>
      <Select aria-label="Native records of" options={options} value={reader.rawEntry ?? first ?? ''} onValueChange={(id) => void c.showRaw(id)} />
      {records === null
        ? <Progress aria-label="Reading the records" value={null} />
        : <ScrollArea aria-label="Native records"><Text as="div" pre mono>{JSON.stringify(records.length === 1 ? records[0] : records, null, 2)}</Text></ScrollArea>}
    </Column>
  );
}

export function Reader({ single }: { single: boolean }) {
  const c = useController();
  const s = c.state;
  const reader = s.reader;
  const handle = useRef<VirtualConversationHandle>(null);
  const box = useRef<HTMLDivElement>(null);
  const waiting = useAfter(reader !== null && reader.transcript === null && reader.error === null);
  const transcript = reader?.transcript ?? null;
  const entries = useMemo(() => transcript?.entries ?? [], [transcript]);
  const tab = reader?.tab ?? 'conversation';

  // What the keys do to the conversation, and where the focus goes when it is read.
  useEffect(() => {
    c.readerKeys = { top: () => handle.current?.scrollToStart(), end: () => handle.current?.scrollToEnd(), by: (pixels) => box.current?.scrollBy({ top: pixels }) };
    c.focusReader = () => box.current?.focus({ preventScroll: true });
    return () => { c.readerKeys = null; c.focusReader = () => {}; };
  }, [c, tab, reader === null]);

  if (reader === null) {
    return <Empty fill title="No transcript open" description="Select a session and press Enter to read it." />;
  }
  const row = s.rows.find((one) => one.id === reader.id);
  const title = transcript?.title ?? row?.title ?? reader.id;
  const hidden = transcript?.hidden_turns ?? 0;
  const turns = s.defaultTurns > 0 ? `Last ${s.defaultTurns} turns` : 'Last turns';

  let body;
  if (reader.error !== null) {
    body = <Empty fill title="The transcript could not be read" description={<Note tone="error" title={reader.error} actions={<Button variant="outline" onClick={() => void c.openReader(reader.turns)}>Read transcript again</Button>} />} />;
  } else if (transcript === null) {
    body = waiting ? <Empty fill title="Reading the transcript" action={<Progress inline aria-label="Reading the transcript" value={null} />} /> : null;
  } else if (tab === 'raw') {
    body = <Raw />;
  } else {
    body = (
      <Column fill>
        <VirtualConversation
          label="Transcript"
          ref={handle}
          containerRef={box}
          count={entries.length}
          rowKey={(index) => entries[index]!.id}
          empty={<Empty title="This session has no conversation entries" />}
        >
          {(index) => <Entry entry={entries[index]!} onRaw={() => void c.showRaw(entries[index]!.id)} />}
        </VirtualConversation>
      </Column>
    );
  }

  return (
    <Panel
      title={title}
      level={3}
      subtitle={row?.id.slice(0, 8)}
      actions={<Button variant="ghost" size="icon-sm" aria-label="Close the transcript" title="Close the transcript" onClick={() => c.closeReader(true)}><Icon name="close" /></Button>}
      tools={(
        <Cluster gap="sm">
          {single ? <Button variant="outline" size="sm" onClick={() => c.closeReader(true)}><Icon name="back" />Sessions</Button> : null}
          <ToggleGroup aria-label="Reader tab" value={tab} onValueChange={(value) => c.setTab(value as 'conversation' | 'raw')}>
            <ToggleGroupItem value="conversation">Conversation</ToggleGroupItem>
            <ToggleGroupItem value="raw">Raw</ToggleGroupItem>
          </ToggleGroup>
          <ToggleGroup aria-label="How much to show" value={reader.turns === 0 ? 'all' : 'last'} onValueChange={(value) => { if ((value === 'all') !== (reader.turns === 0)) void c.toggleTurns(); }}>
            <ToggleGroupItem value="last">{turns}</ToggleGroupItem>
            <ToggleGroupItem value="all">All</ToggleGroupItem>
          </ToggleGroup>
          {hidden > 0 && tab === 'conversation' ? <Text variant="muted">{hidden} earlier turn{hidden === 1 ? '' : 's'} not shown</Text> : null}
        </Cluster>
      )}
    >
      {body}
    </Panel>
  );
}
