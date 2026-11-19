import { test } from 'node:test';
import assert from 'node:assert/strict';
import { actionFor } from '../src/keys.ts';

test('the TUI keys map to their actions', () => {
  const cases: [string, string][] = [['j', 'down'], ['ArrowDown', 'down'], ['k', 'up'], ['h', 'hop'], ['e', 'export'], ['i', 'import'],
    ['R', 'resume'], ['r', 'reload'], ['t', 'turns'], ['g', 'top'], ['G', 'bottom'], ['o', 'scan'], ['/', 'search'], ['c', 'chat'], [' ', 'fold'],
    ['Enter', 'enter'], ['Escape', 'back'], ['PageDown', 'pageDown']];
  for (const [key, action] of cases) assert.equal(actionFor({ key }, false), action, key);
  assert.equal(actionFor({ key: 'x' }, false), null);
});

test('in a text field only Escape and Enter act; ctrl+f always searches', () => {
  assert.equal(actionFor({ key: 'h' }, true), null);
  assert.equal(actionFor({ key: 'Escape' }, true), 'back');
  assert.equal(actionFor({ key: 'Enter' }, true), 'enter');
  assert.equal(actionFor({ key: 'f', ctrlKey: true }, true), 'search');
  assert.equal(actionFor({ key: 'r', ctrlKey: true }, false), null);
  assert.equal(actionFor({ key: 'h', altKey: true }, false), null);
});
