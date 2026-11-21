import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createRegistry } from '@acaprino/imago/ui';
import { COMMANDS, FOOTER } from '../src/keys.ts';

const key = (name: string, flags: { ctrl?: boolean; shift?: boolean; alt?: boolean } = {}) =>
  ({ key: name, ctrlKey: Boolean(flags.ctrl), shiftKey: Boolean(flags.shift), altKey: Boolean(flags.alt), metaKey: false });

function loaded() {
  const registry = createRegistry();
  for (const command of COMMANDS) registry.register({ id: command.id, title: command.title, keys: command.keys, run: () => {} });
  return registry;
}

test('every command of the table registers into a real registry, with no id and no key twice', () => {
  const registry = loaded();
  assert.equal(registry.list().length, COMMANDS.length);
  assert.equal(new Set(COMMANDS.map((command) => command.id)).size, COMMANDS.length);
});

test('no command binds Enter, Space, F6, Ctrl+K or Ctrl+F: those are the lists, the regions and mountApp', () => {
  for (const { keys } of COMMANDS) {
    if (!keys) continue;
    assert.doesNotMatch(keys, /(^|\+)(Enter|Space| )$/i, keys);
    assert.doesNotMatch(keys, /^(Shift\+)?F6$/i, keys);
    assert.notEqual(keys.toLowerCase(), 'ctrl+k');
    assert.notEqual(keys.toLowerCase(), 'ctrl+f');
  }
});

test('each key of the design reaches its command', () => {
  const registry = loaded();
  const cases: [ReturnType<typeof key>, string][] = [
    [key('c'), 'chat'], [key('m'), 'harnesses'], [key('o'), 'scan'], [key('i'), 'import'], [key('r'), 'reload'],
    [key('h'), 'hop'], [key('e'), 'export'], [key('R', { shift: true }), 'resume'], [key('s'), 'start-in'],
    [key('t'), 'turns'], [key('g'), 'reader-top'], [key('G', { shift: true }), 'reader-bottom'],
    [key('1'), 'chat-graph'], [key('2'), 'chat-lanes'], [key('3'), 'chat-feed'], [key('n'), 'new-room'], [key('l'), 'launch'],
    [key('?', { shift: true }), 'keys'], [key('F', { ctrl: true, shift: true }), 'search-everything'], [key('Escape'), 'back'],
  ];
  for (const [event, id] of cases) assert.equal(registry.match(event)?.id, id, `${event.key} → ${id}`);
});

test('R and Shift+R, G and Shift+G are different commands', () => {
  const registry = loaded();
  assert.equal(registry.match(key('r'))?.id, 'reload');
  assert.equal(registry.match(key('R', { shift: true }))?.id, 'resume');
  assert.equal(registry.match(key('g'))?.id, 'reader-top');
  assert.equal(registry.match(key('G', { shift: true }))?.id, 'reader-bottom');
});

test('dropped keys run nothing: the slash, F5, Ctrl+R, the arrows, Ctrl+Shift+K', () => {
  const registry = loaded();
  for (const event of [key('/'), key('F5'), key('r', { ctrl: true }), key('ArrowLeft'), key('ArrowRight'), key('k', { ctrl: true, shift: true })]) {
    assert.equal(registry.match(event), undefined, event.key);
  }
});

test('in a text field a letter belongs to the field; Escape and Ctrl chords still run', () => {
  const registry = loaded();
  const field = { tagName: 'INPUT' };
  assert.equal(registry.match({ ...key('h'), target: field }), undefined);
  assert.equal(registry.match({ ...key('Escape'), target: field })?.id, 'back');
  assert.equal(registry.match({ ...key('F', { ctrl: true, shift: true }), target: field })?.id, 'search-everything');
});

test('every id of the footer is in the table or is one of mountApp', () => {
  const ids = new Set([...COMMANDS.map((command) => command.id), 'imago.palette', 'imago.search']);
  for (const [place, list] of Object.entries(FOOTER)) for (const id of list) assert.ok(ids.has(id), `${place}: ${id}`);
});
