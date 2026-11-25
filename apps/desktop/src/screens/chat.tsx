// The chat view (spec 2026-11-16, layer A): the room in two views of the same events (feed and graph) chosen in a `ToggleGroup`, one
// filter, the replay of the room as a slider, and under it a composer through which the window writes as the room's host. The rooms are
// the sidebar's and the members the inspector's (members.tsx). Without the service running the view says so, with the button that
// starts it.
import { Empty, Note, SearchField } from '@acaprino/sigrid/components';
import { Panel } from '@acaprino/sigrid/layout';
import { PromptInput } from '@acaprino/sigrid/assemblies';
import { Badge, Button, Cell, Checkbox, Cluster, Column, DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger, Icon, Input, Slider, Spacer, Text, ToggleGroup, ToggleGroupItem } from '@acaprino/sigrid/primitives';
import { FormField } from '@acaprino/sigrid/components';
import type { ReactNode } from 'react';
import { GROUPS, memberNameRefusal, mentionsIn, type Group } from '../chatmodel.ts';
import { CHAT_VIEWS, POLL_MS, type ChatController, type ChatViewId } from '../chatcontroller.ts';
import type { Controller } from '../controller.ts';
import type { Room } from '../types.ts';
import { Feed, Graph } from './chatviews.tsx';
import { useController, useStore } from './hooks.ts';
import { HarnessMark } from './mark.tsx';

const KIND_LABEL: Record<Group, string> = { message: 'Messages', signal: 'Signals', system: 'System' };
const VIEW_LABEL: Record<ChatViewId, string> = { feed: 'Feed', graph: 'Graph' };

/** The service is off or does not answer: what it says, and the button that starts it. */
function Off({ chat }: { chat: ChatController }) {
  const status = chat.status;
  const stopped = status?.state === 'stopped';
  return (
    <Empty
      fill
      title={status ? (stopped ? 'The chat service is not running' : 'The chat service does not answer') : 'Looking for the chat service…'}
      description={(
        <Column gap="sm" align="center">
          {status ? <Text as="p">Start it here, or in a terminal with <Text mono>mukade chat start</Text>.</Text> : null}
          {status?.message ? <Note tone="info" title={status.message} /> : null}
          {chat.error ? <Note tone="error" title={chat.error} /> : null}
        </Column>
      )}
      action={stopped ? <Button variant="default" loading={chat.busy !== null} loadingText="Starting…" onClick={() => void chat.startService()}><Icon name="play" />Start the service</Button> : undefined}
    />
  );
}

/** The kinds of event and the members the filter keeps, and the text it asks the service for. */
function Filters({ chat }: { chat: ChatController }) {
  return (
    <Cluster gap="sm">
      <Cell grow><SearchField aria-label="Filter the messages by text" placeholder="Filter messages" value={chat.textInput} onValueChange={(value) => chat.setText(value)} /></Cell>
      <Cluster role="group" aria-label="Kinds of events" gap="xs">
        {GROUPS.map((kind) => <Button key={kind} variant="outline" size="sm" aria-pressed={chat.kinds.includes(kind)} onClick={() => chat.toggleKind(kind)}>{KIND_LABEL[kind]}</Button>)}
      </Cluster>
      {chat.members.length > 1 ? (
        <Cluster role="group" aria-label="Only these members" gap="xs">
          {chat.members.map((member) => (
            <Button key={member.member_id} variant="outline" size="sm" aria-pressed={chat.only.includes(member.member_id)} onClick={() => chat.toggleOnly(member.member_id)}>
              <HarnessMark harness={member.harness} />{member.name}
            </Button>
          ))}
        </Cluster>
      ) : null}
      {chat.filtered ? <Button variant="ghost" size="sm" onClick={() => chat.clearFilter()}>Clear</Button> : null}
    </Cluster>
  );
}

/** The replay of the room: play from the selection, step, and a slider over the events the filter shows. */
function Replay({ chat }: { chat: ChatController }) {
  const shown = chat.shown();
  const selected = chat.selected();
  const at = Math.max(0, selected ? shown.indexOf(selected) : shown.length - 1);
  return (
    <Cluster gap="sm" role="group" aria-label="Replay the room" wrap={false}>
      <Button variant="outline" size="icon-sm" aria-label={chat.playing ? 'Pause' : 'Play from here'} title={chat.playing ? 'Pause' : 'Play from here'} onClick={() => chat.togglePlay()}><Icon name={chat.playing ? 'pause' : 'play'} /></Button>
      <Button variant="ghost" size="icon-sm" aria-label="Previous event" disabled={at <= 0} onClick={() => { chat.choose(shown[at - 1]!.event_id); }}><Icon name="moveLeft" /></Button>
      <Button variant="ghost" size="icon-sm" aria-label="Next event" disabled={at >= shown.length - 1} onClick={() => { chat.choose(shown[at + 1]!.event_id); }}><Icon name="moveRight" /></Button>
      <Cell grow>
        <Slider aria-label="Replay position" min={0} max={Math.max(0, shown.length - 1)} step={1} value={at} disabled={shown.length < 2} onValueChange={(value) => { const event = shown[value]; if (event) chat.choose(event.event_id); }} />
      </Cell>
      <Text variant="muted">{shown.length ? `${at + 1} / ${shown.length}` : '0 / 0'}</Text>
    </Cluster>
  );
}

/** What the window may write: to everyone or to the members picked, as the room's host. */
function Composer({ chat, room }: { chat: ChatController; room: Room }) {
  const mine = chat.mine[room.room_id]!;
  const draft = chat.draft();
  const others = chat.members.filter((member) => member.left_at === null && member.member_id !== mine.member_id);
  const to = draft.to.filter((name) => others.some((member) => member.name === name));
  const named = [...new Set([...to, ...mentionsIn(draft.text, chat.members)])];
  const workers = named.filter((name) => chat.members.find((member) => member.name === name && member.left_at === null)?.role === 'worker');
  const stale = chat.writeError?.startsWith('stale_member');
  return (
    <Column gap="sm">
      <Cluster gap="md" role="group" aria-label="Recipients">
        <Text variant="muted">To</Text>
        <Checkbox label="everyone" checked={to.length === 0} onCheckedChange={() => chat.setRecipients([])} />
        {others.map((member) => (
          <Checkbox
            key={member.member_id}
            label={member.name}
            checked={to.includes(member.name)}
            onCheckedChange={(on) => chat.setRecipients(on === true ? [...to, member.name] : to.filter((name) => name !== member.name))}
          />
        ))}
      </Cluster>
      <PromptInput
        label="Message"
        placeholder="Write to the room. @name addresses a member."
        value={draft.text}
        onValueChange={(value) => chat.setDraft(value)}
        onSubmit={(text) => void chat.send(text)}
        busy={chat.busy === 'sending'}
      />
      <Cluster gap="sm">
        {chat.writeError
          ? <Text tone="danger" role="alert">{stale ? `The window’s handle in ${room.name} no longer writes.` : chat.writeError}</Text>
          : mine.role === 'lead' && workers.length
            ? <Text tone="warning">An instruction to {workers.join(', ')}: {workers.length === 1 ? 'it answers' : 'each answers'} with done or blocked.</Text>
            : <Text variant="muted">to {named.length ? named.join(', ') : 'everyone'}</Text>}
        {stale ? <Button variant="outline" size="sm" onClick={() => void chat.rejoin(room)}>Rejoin</Button> : null}
        <Spacer />
        <Text variant="muted">as {mine.name} · {mine.role}</Text>
        <Button variant="ghost" size="sm" disabled={chat.busy !== null} title="The window leaves the room" onClick={() => void chat.leave(room)}>Leave the room</Button>
      </Cluster>
    </Column>
  );
}

/** To write here the window joins the room as a member of its own, under a name and as a peer or a lead. */
function Join({ chat, room }: { chat: ChatController; room: Room }) {
  const refusal = chat.joinName ? memberNameRefusal(chat.joinName, chat.members) : null;
  return (
    <form onSubmit={(event) => { event.preventDefault(); void chat.join(room); }}>
      <Column gap="sm">
        <Text variant="muted">To write here, the window joins the room as a member of its own; what it sends reaches the agents as yours.</Text>
        <Cluster gap="md" align="end">
          <FormField label="Name" error={chat.writeError && !chat.mine[room.room_id] ? chat.writeError : refusal ?? undefined}>
            <Input value={chat.joinName} autoComplete="off" spellCheck={false} onChange={(event) => chat.setJoin(event.target.value)} />
          </FormField>
          <ToggleGroup aria-label="Role" value={chat.joinRole} onValueChange={(role) => chat.setJoin(chat.joinName, role as 'peer' | 'lead')}>
            <ToggleGroupItem value="peer">peer</ToggleGroupItem>
            <ToggleGroupItem value="lead">lead</ToggleGroupItem>
          </ToggleGroup>
          <Button type="submit" variant="default" loading={chat.busy === 'joining'} loadingText="Joining…">Join</Button>
        </Cluster>
        <Text variant="muted">{chat.joinRole === 'lead' ? 'A lead’s message to a worker is an instruction the worker answers.' : 'A peer reads and writes to everyone or to chosen members.'}</Text>
      </Column>
    </form>
  );
}

/** The foot of the panel holds one part that takes its whole width. */
function Footer({ chat, room }: { chat: ChatController; room: Room }) {
  if (room.closed_at) {
    return (
      <Cell grow>
        <Cluster gap="md" justify="between">
          <Text variant="muted">The room is closed; its history stays readable.</Text>
          <Button variant="outline" disabled={chat.busy !== null} onClick={() => void chat.reopen(room)}>Reopen</Button>
        </Cluster>
      </Cell>
    );
  }
  return <Cell grow>{chat.mine[room.room_id] ? <Composer chat={chat} room={room} /> : <Join chat={chat} room={room} />}</Cell>;
}

/** The menu of the room: close it, or reopen and delete it, and stop the service. */
function RoomMenu({ c, chat, room }: { c: Controller; chat: ChatController; room: Room }) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="outline" size="icon-sm" aria-label="More about the room" title="More about the room"><Icon name="more" /></Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        {room.closed_at
          ? <DropdownMenuItem disabled={chat.busy !== null} onSelect={() => void chat.reopen(room)}>Reopen the room</DropdownMenuItem>
          : <DropdownMenuItem disabled={chat.busy !== null} onSelect={() => void chat.closeRoom(room)}>Close the room</DropdownMenuItem>}
        {room.closed_at ? <DropdownMenuItem disabled={chat.busy !== null} onSelect={() => void chat.deleteRoom(room)}>Delete the room…</DropdownMenuItem> : null}
        <DropdownMenuSeparator />
        <DropdownMenuItem disabled={chat.busy !== null} onSelect={() => chat.stopService()}>Stop the service…</DropdownMenuItem>
        <DropdownMenuItem onSelect={() => c.open({ kind: 'new-room' })}>New room…</DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

export function ChatScreen() {
  const c = useController();
  const chat = c.chat;
  useStore(chat);
  if (!chat.running) return <Off chat={chat} />;
  const room = chat.current();
  if (!room) {
    return <Empty fill title="No room yet" description="Open a room, and the agents you launch into it meet there." action={<Button variant="outline" onClick={() => c.open({ kind: 'new-room' })}><Icon name="plus" />New room</Button>} />;
  }
  const shown = chat.shown();
  let body: ReactNode;
  if (!chat.events.length) {
    body = <Empty fill title={chat.text ? 'No message matches the filter' : 'No events yet'} action={chat.text ? <Button variant="outline" onClick={() => chat.clearFilter()}>Clear the filter</Button> : undefined} />;
  } else if (chat.view !== 'graph' && !shown.length) {
    body = <Empty fill title="Nothing in this room matches the filter" action={<Button variant="outline" onClick={() => chat.clearFilter()}>Clear the filter</Button>} />;
  } else {
    body = chat.view === 'feed' ? <Feed chat={chat} /> : <Graph chat={chat} />;
  }
  return (
    <Panel
      title={room.name}
      subtitle={<>{chat.events.length} events{chat.filtered ? ` · ${shown.length} shown` : ''}{room.closed_at ? ' · closed' : ''}</>}
      actions={(
        <Cluster gap="sm">
          {room.closed_at ? <Badge>closed</Badge> : null}
          <ToggleGroup aria-label="View of the room" value={chat.view} onValueChange={(value) => chat.setView(value as ChatViewId)}>
            {CHAT_VIEWS.map((view) => <ToggleGroupItem key={view} value={view}>{VIEW_LABEL[view]}</ToggleGroupItem>)}
          </ToggleGroup>
          <Button variant="outline" size="sm" onClick={() => chat.setMembersOpen(true)}><Icon name="contacts" />Members</Button>
          <Button variant="outline" size="sm" disabled={!!room.closed_at} title={`Run an agent in ${room.name}, in a new terminal`} onClick={() => c.open({ kind: 'launch' })}><Icon name="code" />Launch agent</Button>
          <RoomMenu c={c} chat={chat} room={room} />
        </Cluster>
      )}
      tools={(
        <Column gap="sm">
          <Filters chat={chat} />
          {chat.view !== 'feed' ? <Replay chat={chat} /> : null}
          {chat.error ? <Note tone="error" title={chat.error} /> : null}
          <Text variant="muted">{chat.busy ? `${chat.busy}…` : `Checked every ${POLL_MS / 1000} s`}</Text>
        </Column>
      )}
      footer={<Footer chat={chat} room={room} />}
    >
      {body}
    </Panel>
  );
}
