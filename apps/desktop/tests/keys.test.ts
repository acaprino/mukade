// The table of the window's own keys (src/keys.ts). The commands themselves are Sigrid's `useCommand` (src/screens/commands.tsx);
// how Sigrid runs a key is its own test, and the browser proves the two together (smoke.mjs).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { COMMANDS, FOOTER } from '../src/keys.ts';

test('no id and no key twice', () => {
  assert.equal(new Set(COMMANDS.map((command) => command.id)).size, COMMANDS.length);
  // One key may serve two places (J and K scroll the reader and step the chat), never two commands of one place or of «always».
  const keys = COMMANDS.flatMap((command) => (command.keys ? [`${command.view === 'always' ? '' : command.view}|${command.keys.toLowerCase()}`] : []));
  assert.equal(new Set(keys).size, keys.length);
  const always = new Set(COMMANDS.filter((command) => command.view === 'always' && command.keys).map((command) => command.keys!.toLowerCase()));
  for (const command of COMMANDS) if (command.view !== 'always' && command.keys) assert.ok(!always.has(command.keys.toLowerCase()), command.id);
});

test('no command binds Enter, Space, F6, Ctrl+K or Ctrl+F: those are the lists, the regions and the shell', () => {
  for (const { keys } of COMMANDS) {
    if (!keys) continue;
    assert.doesNotMatch(keys, /(^|\+)(Enter|Space| )$/i, keys);
    assert.doesNotMatch(keys, /^(Shift\+)?F6$/i, keys);
    assert.notEqual(keys.toLowerCase(), 'ctrl+k');
    assert.notEqual(keys.toLowerCase(), 'ctrl+f');
  }
});

test('each key of the design is on its command', () => {
  const keyOf = (id: string) => COMMANDS.find((command) => command.id === id)?.keys;
  const wanted: [string, string][] = [
    ['chat', 'C'], ['harnesses', 'M'], ['scan', 'O'], ['import', 'I'], ['reload', 'R'], ['hop', 'H'], ['export', 'E'], ['resume', 'Shift+R'],
    ['start-in', 'S'], ['turns', 'T'], ['reader-top', 'G'], ['reader-bottom', 'Shift+G'], ['reader-down', 'J'], ['reader-up', 'K'], ['chat-next', 'J'], ['chat-prev', 'K'], ['details', 'D'], ['chat-feed', '1'], ['chat-graph', '2'],
    ['new-room', 'N'], ['launch', 'L'], ['keys', 'Shift+?'], ['search-everything', 'Ctrl+Shift+F'], ['back', 'Escape'],
  ];
  for (const [id, keys] of wanted) assert.equal(keyOf(id), keys, id);
});

test('R and Shift+R, G and Shift+G are different commands', () => {
  const keys = COMMANDS.map((command) => command.keys);
  for (const pair of [['R', 'Shift+R'], ['G', 'Shift+G']]) for (const one of pair) assert.ok(keys.includes(one), one);
});

test('every place of a command is one the window knows, and every id of the foot is in the table', () => {
  const places = new Set(['always', 'session', 'reader', 'chat', 'room']);
  for (const command of COMMANDS) assert.ok(places.has(command.view), command.id);
  const ids = new Set(COMMANDS.map((command) => command.id));
  for (const [place, list] of Object.entries(FOOTER)) for (const id of list) assert.ok(ids.has(id), `${place}: ${id}`);
});
