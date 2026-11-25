// The window of Mukade: Imago's `AppWindow` with Mukade's three views, and in its regions what each view says. The controllers
// (src/controller.ts, chatcontroller.ts, harnesscontroller.ts) hold what the page remembers and ask Rust; this file puts them in the
// shell's regions and tells the window what the shell cannot know: the view in its title. Sessions has the installations and the
// folders in its sidebar and the details of the session in its inspector; Chat the rooms and the members, and only while the service
// runs; Harnesses is a page of cards and has neither. Settings are a dialog of the palette, not a view.
import { useEffect, useMemo, useRef } from 'react';
import { AppWindow, ScreenBoundary, useNotify, useWindow, type AppView } from '@acaprino/imago';
import { Button, Cluster, Icon } from '@acaprino/sigrid/primitives';
import { Controller } from '../controller.ts';
import { FOOTER } from '../keys.ts';
import type { ViewId } from '../state.ts';
import { useMukadeCommands } from './commands.tsx';
import { ChatScreen } from './chat.tsx';
import { Dialogs } from './dialogs.tsx';
import { SessionInspector } from './detail.tsx';
import { HarnessesScreen } from './harnesses.tsx';
import { ControllerContext, useController, useStore } from './hooks.ts';
import { MembersInspector } from './members.tsx';
import { RoomsSidebar, SessionsSidebar } from './sidebar.tsx';
import { SessionsScreen } from './sessions.tsx';
import { Status } from './status.tsx';

const VIEWS: AppView<ViewId>[] = [
  { id: 'sessions', label: 'Sessions', icon: 'history', command: 'Go to Sessions' },
  { id: 'chat', label: 'Chat', icon: 'threads', command: 'Go to Chat' },
  { id: 'harnesses', label: 'Harnesses', icon: 'package', command: 'Go to Harnesses' },
];

function Window() {
  const c = useController();
  const chat = c.chat;
  useStore(chat);
  useStore(c.harnesses);
  const notify = useNotify();
  const window_ = useWindow();
  const field = useRef<HTMLInputElement>(null);
  const s = c.state;
  c.notice = (text, say) => notify(text, {
    ...(say?.error ? { kind: 'error' as const } : {}),
    ...(say?.undo ? { undo: { onClick: () => { void say.undo!(); } } } : {}),
  });
  useMukadeCommands(c);
  useEffect(() => { void c.start(); }, [c]);

  // The chat is read while it is shown, and not otherwise.
  const view = s.view;
  useEffect(() => {
    if (view === 'chat') chat.show(); else chat.hide();
    return () => chat.hide();
  }, [chat, view]);
  // The title of the window says the view (Alt+Tab and screen readers tell Sessions from Chat).
  useEffect(() => { window_.setTitle(`${VIEWS.find((one) => one.id === view)!.label} - Mukade`); }, [window_, view]);

  const chatOn = view === 'chat' && chat.running;
  let screen;
  if (view === 'sessions') screen = <SessionsScreen field={field} />;
  else if (view === 'chat') screen = <ChatScreen />;
  else screen = <HarnessesScreen />;
  const place = view === 'sessions' && s.reader ? 'reader' : view;

  return (
    <AppWindow
      views={VIEWS}
      view={view}
      onView={(next) => c.chooseView(next)}
      search={view === 'sessions'
        ? () => field.current?.focus()
        : view === 'chat'
          ? () => document.querySelector<HTMLInputElement>('input[aria-label="Filter the messages by text"]')?.focus()
          : () => c.open({ kind: 'search' })}
      status={<Status state={s} />}
      sidebar={view === 'sessions' ? (state) => <SessionsSidebar state={state} /> : chatOn ? (state) => <RoomsSidebar state={state} /> : undefined}
      inspector={view === 'sessions'
        ? <ScreenBoundary of={s.selected}><SessionInspector /></ScreenBoundary>
        : chatOn ? <ScreenBoundary of={chat.room}><MembersInspector /></ScreenBoundary> : undefined}
      actions={(
        <Cluster gap="sm" justify="end">
          <Button variant="outline" onClick={() => void c.scanFolder()} title="Look for portable or custom installations in a folder"><Icon name="folder" />Scan folder</Button>
          <Button variant="outline" onClick={() => c.importDocument()} title="Write a dump document into an installation"><Icon name="upload" />Import</Button>
          <Button variant="outline" onClick={() => void c.loadInventory()} title="Read every store again"><Icon name="refresh" />Reload</Button>
          <Button variant="outline" onClick={() => c.open({ kind: 'search' })} title="Search rooms, members, messages and sessions"><Icon name="search" />Search everything</Button>
        </Cluster>
      )}
      hints={FOOTER[place]}
    >
      {screen}
      <Dialogs c={c} />
    </AppWindow>
  );
}

export function Mukade() {
  const controller = useMemo(() => new Controller(), []);
  return (
    <ControllerContext value={controller}>
      <Window />
    </ControllerContext>
  );
}
