// The members of the open room and the instructions leads gave, in Sigrid's `Inspector` (docked in the chat while the service runs; a
// drawer under 1000px, opened from the room's Members button or from a member of the graph). A member opens its details in place: its
// harness, its session and when it joined, with what can be done with it (address it, show its session).
import { DescriptionList, ItemList, ItemRow } from '@acaprino/sigrid/components';
import { Inspector, useAppShell } from '@acaprino/sigrid/layout';
import { Badge, Button, Cluster, Column, Section, Text } from '@acaprino/sigrid/primitives';
import { harnessKey, instructions } from '../chatmodel.ts';
import type { ChatController } from '../chatcontroller.ts';
import type { Controller } from '../controller.ts';
import { shortTime } from '../tree.ts';
import type { Member } from '../types.ts';
import { useController, useStore } from './hooks.ts';
import { HarnessMark } from './mark.tsx';

function when(iso: string): string {
  return new Date(iso).toLocaleString();
}

function MemberDetails({ c, chat, member, mine }: { c: Controller; chat: ChatController; member: Member; mine: boolean }) {
  const session = member.session;
  return (
    <Column gap="sm">
      <DescriptionList
        aria-label={`Details of ${member.name}`}
        items={[
          { name: 'Harness', value: member.harness },
          { name: 'Session', value: session ? session.id : 'not bound', code: Boolean(session) },
          session?.cwd ? { name: 'Folder', value: session.cwd, code: true } : null,
          { name: 'Joined', value: when(member.joined_at) },
          member.last_command_at ? { name: 'Last command', value: when(member.last_command_at) } : null,
          member.left_at ? { name: 'Left', value: when(member.left_at) } : null,
        ]}
      />
      <Cluster gap="sm">
        {!mine && !member.left_at ? <Button variant="outline" size="sm" title="Address it in the message" onClick={() => chat.mention(member)}>Mention</Button> : null}
        {session ? (
          <Button
            variant="outline"
            size="sm"
            onClick={() => {
              const installation = c.state.installations.find((item) => item.harness === harnessKey(session.harness) && !item.scanned) ?? c.state.installations.find((item) => item.harness === harnessKey(session.harness));
              if (!installation) { c.report(`${session.id} is not listed here`, true); return; }
              void c.revealSession(installation.key, session.id, false);
            }}
          >Show session</Button>
        ) : null}
      </Cluster>
    </Column>
  );
}

export function MembersInspector() {
  const c = useController();
  const chat = c.chat;
  useStore(chat);
  const layout = useAppShell();
  const room = chat.current();
  const open = chat.membersOpen ?? layout.inspector === 'docked';
  c.docked = layout.inspector === 'docked';
  const mine = room ? chat.mine[room.room_id] : undefined;
  const opened = chat.members.find((member) => member.member_id === chat.openMember);
  const owed = instructions(chat.events, chat.members).reverse();
  return (
    <Inspector title="Members" open={open} onOpenChange={(next) => chat.setMembersOpen(next)}>
      <Column gap="lg">
        {chat.members.length ? (
          <ItemList aria-label="Members">
            {chat.members.map((member) => {
              const isMine = mine?.member_id === member.member_id;
              const state = member.left_at ? 'left' : member.last_command_at ? `last command ${shortTime(member.last_command_at)}` : 'joined';
              return (
                <ItemRow
                  key={member.member_id}
                  title={member.name}
                  description={state}
                  leading={<HarnessMark harness={member.harness} />}
                  meta={member.role}
                  end={isMine ? <Badge tone="info">you</Badge> : member.host ? <Badge variant="secondary">user</Badge> : undefined}
                  current={chat.openMember === member.member_id}
                  onClick={() => chat.toggleMember(member.member_id)}
                />
              );
            })}
          </ItemList>
        ) : <Text variant="muted">{room ? 'Nobody joined yet.' : ''}</Text>}
        {opened ? (
          <Section title={opened.name} level={3} icon={<HarnessMark harness={opened.harness} />}>
            <MemberDetails c={c} chat={chat} member={opened} mine={mine?.member_id === opened.member_id} />
          </Section>
        ) : null}
        <Section title={`Instructions · ${owed.length}`} level={3}>
          {owed.length ? (
            <ItemList aria-label="Instructions">
              {owed.map((one) => (
                <ItemRow
                  key={one.event.event_id}
                  title={<><Text variant="strong">{one.worker.name}</Text> {String(one.event.payload.text ?? '')}</>}
                  meta={`#${one.event.event_id}`}
                  end={<Badge tone={one.state === 'done' ? 'success' : one.state === 'blocked' ? 'danger' : 'info'}>{one.state}</Badge>}
                  onClick={() => chat.choose(one.event.event_id)}
                />
              ))}
            </ItemList>
          ) : <Text variant="muted">A lead’s message to a worker shows here, with what the worker signalled.</Text>}
        </Section>
      </Column>
    </Inspector>
  );
}
