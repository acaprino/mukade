// The commands of the window that are Mukade's own, declared with Sigrid's `useCommand` from the table of src/keys.ts: the palette
// (Ctrl+K) lists them with the shell's, their keys run from anywhere in the window but a field, and the foot names the ones that work
// now. The views, the look and the search of the view are the shell's. A key that types belongs to a field, and an open dialog owns the
// keyboard: Sigrid keeps both (`useCommand`), and `when` keeps the rest.
import { useCommand } from '@acaprino/sigrid/assemblies';
import type { Controller } from '../controller.ts';
import { COMMANDS, type Place } from '../keys.ts';
import { selectedRow } from '../state.ts';

/** Escape, when no dialog, field or list answered it: leave the field, then Raw to Conversation, then close the reader. */
function back(c: Controller): void {
  const active = document.activeElement;
  if (active instanceof HTMLInputElement || active instanceof HTMLTextAreaElement) { active.blur(); return; }
  const s = c.state;
  // An inspector that is a drawer closes with Escape from anywhere, as the shell's own does with the focus in it.
  if (!c.docked && s.view === 'chat' && c.chat.membersOpen === true) { c.chat.setMembersOpen(null); return; }
  if (!c.docked && s.view === 'sessions' && s.detailOpen === true) { c.setDetailOpen(null); return; }
  const reader = s.reader;
  if (s.view === 'sessions' && reader) {
    if (reader.tab === 'raw') c.setTab('conversation'); else c.closeReader(true);
  }
}

function runners(c: Controller): Record<string, () => void> {
  const s = c.state;
  const chat = c.chat;
  return {
    'search-everything': () => c.open({ kind: 'search' }),
    chat: () => c.chooseView(s.view === 'chat' ? 'sessions' : 'chat'),
    harnesses: () => c.chooseView(s.view === 'harnesses' ? 'sessions' : 'harnesses'),
    settings: () => c.open({ kind: 'settings' }),
    scan: () => { void c.scanFolder(); },
    import: () => c.importDocument(),
    reload: () => { void c.loadInventory(); },
    hop: () => c.startHop(),
    export: () => { void c.exportSelected(false); },
    'export-as': () => { void c.exportSelected(true); },
    resume: () => { const row = selectedRow(s); if (row && s.installation !== null) void c.resume(s.installation, row.id); },
    'start-in': () => c.startIn(),
    'open-reader': () => { void c.openReader(); },
    details: () => c.setDetailOpen(!(s.detailOpen ?? c.docked)),
    turns: () => { void c.toggleTurns(); },
    'reader-down': () => c.readerKeys?.by(60),
    'reader-up': () => c.readerKeys?.by(-60),
    'reader-top': () => c.readerKeys?.top(),
    'reader-bottom': () => c.readerKeys?.end(),
    'chat-feed': () => chat.setView('feed'),
    'chat-graph': () => chat.setView('graph'),
    'chat-next': () => { chat.choose(chat.shown()[Math.min(chat.shown().length - 1, Math.max(0, chat.shown().indexOf(chat.selected()!)) + 1)]?.event_id ?? ''); },
    'chat-prev': () => { chat.choose(chat.shown()[Math.max(0, chat.shown().indexOf(chat.selected()!) - 1)]?.event_id ?? ''); },
    'new-room': () => c.open({ kind: 'new-room' }),
    launch: () => c.open({ kind: 'launch' }),
    play: () => chat.togglePlay(),
    members: () => chat.setMembersOpen(!(chat.membersOpen ?? c.docked)),
    'start-service': () => { void chat.startService(); },
    'stop-service': () => chat.stopService(),
    back: () => back(c),
    keys: () => c.open({ kind: 'keys' }),
  };
}

/** Declares them all. Called once by the window. */
export function useMukadeCommands(c: Controller): void {
  const s = c.state;
  const chat = c.chat;
  const places: Record<Place, boolean> = {
    always: true,
    session: s.view === 'sessions' && selectedRow(s) !== undefined,
    reader: s.view === 'sessions' && s.reader !== null && s.reader.tab === 'conversation',
    chat: s.view === 'chat',
    room: s.view === 'chat' && chat.running && chat.hasRoom,
  };
  const run = runners(c);
  // Some commands need more than a place: a service that runs, a session that can be read, a room that is open.
  const extra: Record<string, boolean> = {
    'new-room': chat.running,
    launch: chat.running && chat.current()?.closed_at === null,
    'start-service': chat.status?.state === 'stopped',
    'stop-service': chat.running,
    'chat-next': chat.running && chat.hasRoom,
    'chat-prev': chat.running && chat.hasRoom,
    'chat-feed': chat.running,
    'chat-graph': chat.running,
    hop: !selectedRow(s)?.unreadable,
    export: !selectedRow(s)?.unreadable,
    'export-as': !selectedRow(s)?.unreadable,
    resume: !selectedRow(s)?.unreadable,
    'open-reader': !selectedRow(s)?.unreadable,
    'start-in': Boolean(selectedRow(s) && selectedRow(s)!.unreadable && selectedRow(s)!.cwd),
  };
  for (const spec of COMMANDS) {
    // eslint-disable-next-line react-hooks/rules-of-hooks -- the table is a constant: the hooks are always called in the same order
    useCommand({
      id: spec.id,
      label: spec.title,
      group: spec.view === 'always' ? 'Mukade' : spec.view === 'session' || spec.view === 'reader' ? 'Session' : 'Chat',
      keys: spec.keys,
      when: places[spec.view] && (extra[spec.id] ?? true) && c.dialog === null,
      run: run[spec.id] ?? (() => {}),
    });
  }
}
