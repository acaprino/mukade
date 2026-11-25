// The sidebar of each view, in Sigrid's `Sidebar` that the shell gives its state. Sessions: the installations (the standard stores,
// then the scanned ones) and the folders of the one chosen. Chat: the rooms, with a filter. Harnesses has none (the view is a page
// of cards), and a chat whose service does not run has none either.
import type { SidebarState } from '@acaprino/imago';
import { Badge, Button, Count, Icon } from '@acaprino/sigrid/primitives';
import { NavItem, Sidebar, SidebarGroup, SidebarRow } from '@acaprino/sigrid/layout';
import { currentInstallation } from '../state.ts';
import { folders } from '../tree.ts';
import { shortTime } from '../tree.ts';
import type { Installation } from '../types.ts';
import { HarnessMark } from './mark.tsx';
import { useController, useStore } from './hooks.ts';

export function folderKey(cwd: string | null): string {
  return cwd === null ? '*all' : cwd === '' ? '*unknown' : cwd;
}

function InstallationItem({ item, current, onPick }: { item: Installation; current: boolean; onPick: () => void }) {
  return (
    <NavItem
      icon={<HarnessMark harness={item.harness} />}
      current={current}
      title={item.error ? `${item.label}: ${item.error}` : item.store}
      end={item.error ? <Badge tone="danger">error</Badge> : <Count value={item.count} label="sessions" />}
      description={item.scanned ? 'Found by scan' : undefined}
      onClick={onPick}
    >{item.label}</NavItem>
  );
}

export function SessionsSidebar({ state }: { state: SidebarState }) {
  const c = useController();
  const s = c.state;
  const current = currentInstallation(s);
  const list = folders(s.rows);
  const standard = s.installations.filter((item) => !item.scanned);
  const scanned = s.installations.filter((item) => item.scanned);
  return (
    <Sidebar aria-label="Installations and folders" header={<SidebarRow>Installations <Count value={s.installations.length} label="installations" /></SidebarRow>} {...state}>
      <SidebarGroup>
        {standard.map((item) => <InstallationItem key={item.key} item={item} current={item.key === s.installation} onPick={() => void c.pickInstallation(item.key)} />)}
      </SidebarGroup>
      {scanned.length ? (
        <SidebarGroup label="Found by scan" icon={<Icon name="search" />}>
          {scanned.map((item) => <InstallationItem key={item.key} item={item} current={item.key === s.installation} onPick={() => void c.pickInstallation(item.key)} />)}
        </SidebarGroup>
      ) : null}
      <SidebarGroup label={current ? `Folders of ${current.label}` : 'Folders'} icon={<Icon name="folder" />}>
        {list.map((folder) => (
          <NavItem
            key={folderKey(folder.cwd)}
            icon={<Icon name="folder" />}
            current={folderKey(folder.cwd) === folderKey(s.folder)}
            title={folder.cwd || undefined}
            end={<Count value={folder.count} label="sessions" />}
            onClick={() => c.pickFolder(folder.cwd)}
          >{folder.label}</NavItem>
        ))}
      </SidebarGroup>
    </Sidebar>
  );
}

export function RoomsSidebar({ state }: { state: SidebarState }) {
  const c = useController();
  const chat = c.chat;
  useStore(chat);
  return (
    <Sidebar
      aria-label="Rooms"
      search
      header={(
        <SidebarRow
          end={<Button variant="ghost" size="icon-sm" aria-label="New room" title="New room" onClick={() => c.open({ kind: 'new-room' })}><Icon name="plus" /></Button>}
        >Rooms <Count value={chat.rooms.length} label="rooms" /></SidebarRow>
      )}
      {...state}
    >
      <SidebarGroup>
        {chat.rooms.map((room) => (
          <NavItem
            key={room.room_id}
            icon={<Icon name="threads" />}
            current={room.room_id === chat.room}
            description={`${room.members} members · ${room.events} events · ${shortTime(room.last_activity ?? room.opened_at)}`}
            end={room.closed_at ? <Badge>closed</Badge> : chat.mine[room.room_id] ? <Badge tone="info">joined</Badge> : undefined}
            onClick={() => chat.pickRoom(room.room_id)}
          >{room.name}</NavItem>
        ))}
      </SidebarGroup>
    </Sidebar>
  );
}
