// The command table (spec §4.1): every key of the window in one place, as data. main.ts registers
// each row with its `run` and its `when`; the registry (Imago's `createRegistry`) owns the binding,
// the key caps, the palette and the footer's key list. Pure, so tests/keys.test.ts checks it
// against a real registry without a window.
//
// Not here: Ctrl+K (the palette) and Ctrl+F (filter the current view), which `mountApp` registers;
// the arrows, Home, End, Page Up, Page Down, Enter and Space, which belong to the focused list or to
// the reader (lists.ts, reader.ts, chat.ts); F6, which is the regions'.

/** Where a command is available: always, with a session selected, with the reader open, in the chat, or in an open room. */
export type Place = 'always' | 'session' | 'reader' | 'chat' | 'room';

export interface CommandSpec { id: string; title: string; keys?: string; view: Place }

export const COMMANDS: CommandSpec[] = [
  { id: 'search-everything', title: 'Search rooms, members, messages and sessions', keys: 'Ctrl+Shift+F', view: 'always' },
  { id: 'sessions', title: 'Sessions', view: 'always' },
  { id: 'chat', title: 'Chat', keys: 'C', view: 'always' },
  { id: 'harnesses', title: 'Harnesses', keys: 'M', view: 'always' },
  { id: 'scan', title: 'Scan folder', keys: 'O', view: 'always' },
  { id: 'import', title: 'Import document', keys: 'I', view: 'always' },
  { id: 'reload', title: 'Reload stores', keys: 'R', view: 'always' },
  { id: 'hop', title: 'Hop to another installation', keys: 'H', view: 'session' },
  { id: 'export', title: 'Export session', keys: 'E', view: 'session' },
  { id: 'export-as', title: 'Export session as…', view: 'session' },
  { id: 'resume', title: 'Resume session in a terminal', keys: 'Shift+R', view: 'session' },
  { id: 'start-in', title: 'Start another harness in the session’s folder', keys: 'S', view: 'session' },
  { id: 'open-reader', title: 'Open transcript', view: 'session' },
  { id: 'turns', title: 'Last turns or all turns', keys: 'T', view: 'reader' },
  { id: 'reader-top', title: 'Reader: top', keys: 'G', view: 'reader' },
  { id: 'reader-bottom', title: 'Reader: bottom', keys: 'Shift+G', view: 'reader' },
  { id: 'chat-graph', title: 'Chat: graph', keys: '1', view: 'chat' },
  { id: 'chat-lanes', title: 'Chat: lanes', keys: '2', view: 'chat' },
  { id: 'chat-feed', title: 'Chat: feed', keys: '3', view: 'chat' },
  { id: 'new-room', title: 'Create chat room', keys: 'N', view: 'chat' },
  { id: 'launch', title: 'Launch an agent in the room', keys: 'L', view: 'room' },
  { id: 'play', title: 'Play the room back', view: 'room' },
  { id: 'start-service', title: 'Start the chat service', view: 'chat' },
  { id: 'stop-service', title: 'Stop the chat service', view: 'chat' },
  { id: 'back', title: 'Back: leave the field, close the reader, go back a column', keys: 'Escape', view: 'always' },
  { id: 'keys', title: 'Keyboard shortcuts', keys: 'Shift+?', view: 'always' },
];

export type FooterPlace = 'sessions' | 'reader' | 'chat' | 'harnesses';

/** The commands the footer lists, per place: ids of the registry (`imago.search` and `imago.palette` are `mountApp`'s). */
export const FOOTER: Record<FooterPlace, string[]> = {
  sessions: ['hop', 'export', 'resume', 'imago.search', 'imago.palette'],
  reader: ['turns', 'reader-top', 'reader-bottom', 'hop'],
  chat: ['chat-graph', 'chat-lanes', 'chat-feed', 'new-room', 'launch', 'imago.search'],
  harnesses: ['scan', 'reload', 'search-everything'],
};
