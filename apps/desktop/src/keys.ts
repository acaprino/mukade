// The command table (spec §4.1): every key of the window in one place, as data. screens/commands.tsx
// declares each row with Sigrid's `useCommand`, with its `run` and its `when`; Sigrid's registry owns
// the binding, the key caps, the palette and the foot's key list. Pure, so tests/keys.test.ts checks
// the table without a window.
//
// Not here: the views of the title bar (Sessions, Chat, Harnesses: the shell's «Go to» commands),
// Ctrl+K (the palette), Ctrl+F (the search of the view) and F6 (the next region), which are the
// shell's; light, dark and the system's look (the palette's); the arrows, Home, End, Page Up, Page
// Down, Enter and Space, which belong to the focused list, tree or reader.

/** Where a command is available: always, with a session selected, with the reader open, in the chat, or in an open room. */
export type Place = 'always' | 'session' | 'reader' | 'chat' | 'room';

export interface CommandSpec { id: string; title: string; keys?: string; view: Place }

export const COMMANDS: CommandSpec[] = [
  { id: 'search-everything', title: 'Search rooms, members, messages and sessions', keys: 'Ctrl+Shift+F', view: 'always' },
  { id: 'chat', title: 'Chat, or back to the sessions', keys: 'C', view: 'always' },
  { id: 'harnesses', title: 'Harnesses, or back to the sessions', keys: 'M', view: 'always' },
  { id: 'settings', title: 'Settings', keys: 'Ctrl+,', view: 'always' },
  { id: 'scan', title: 'Scan folder', keys: 'O', view: 'always' },
  { id: 'import', title: 'Import document', keys: 'I', view: 'always' },
  { id: 'reload', title: 'Reload stores', keys: 'R', view: 'always' },
  { id: 'hop', title: 'Hop to another installation', keys: 'H', view: 'session' },
  { id: 'export', title: 'Export session', keys: 'E', view: 'session' },
  { id: 'export-as', title: 'Export session as…', view: 'session' },
  { id: 'resume', title: 'Resume session in a terminal', keys: 'Shift+R', view: 'session' },
  { id: 'start-in', title: 'Start another harness in the session’s folder', keys: 'S', view: 'session' },
  { id: 'open-reader', title: 'Open transcript', view: 'session' },
  { id: 'details', title: 'Show or hide the details of the session', keys: 'D', view: 'session' },
  { id: 'turns', title: 'Last turns or all turns', keys: 'T', view: 'reader' },
  { id: 'reader-down', title: 'Reader: scroll down', keys: 'J', view: 'reader' },
  { id: 'reader-up', title: 'Reader: scroll up', keys: 'K', view: 'reader' },
  { id: 'reader-top', title: 'Reader: top', keys: 'G', view: 'reader' },
  { id: 'reader-bottom', title: 'Reader: bottom', keys: 'Shift+G', view: 'reader' },
  { id: 'chat-feed', title: 'Chat: feed', keys: '1', view: 'chat' },
  { id: 'chat-graph', title: 'Chat: graph', keys: '2', view: 'chat' },
  { id: 'chat-next', title: 'Chat: next event', keys: 'J', view: 'chat' },
  { id: 'chat-prev', title: 'Chat: previous event', keys: 'K', view: 'chat' },
  { id: 'new-room', title: 'Create chat room', keys: 'N', view: 'chat' },
  { id: 'launch', title: 'Launch an agent in the room', keys: 'L', view: 'room' },
  { id: 'play', title: 'Play the room back, or pause it', view: 'room' },
  { id: 'members', title: 'Show or hide the members of the room', view: 'room' },
  { id: 'start-service', title: 'Start the chat service', view: 'chat' },
  { id: 'stop-service', title: 'Stop the chat service', view: 'chat' },
  { id: 'back', title: 'Back: leave the field, close the reader, go back a column', keys: 'Escape', view: 'always' },
  { id: 'keys', title: 'Keyboard shortcuts', keys: 'Shift+?', view: 'always' },
];

export type FooterPlace = 'sessions' | 'reader' | 'chat' | 'harnesses';

/** The commands the foot lists, per place, before the shell's own keys (Ctrl+F, Ctrl+K, F6): ids of the registry. */
export const FOOTER: Record<FooterPlace, string[]> = {
  sessions: ['hop', 'export', 'resume', 'search-everything'],
  reader: ['turns', 'reader-top', 'reader-bottom', 'hop'],
  chat: ['chat-feed', 'chat-graph', 'new-room', 'launch'],
  harnesses: ['scan', 'reload', 'search-everything'],
};
