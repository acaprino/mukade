import { test } from 'node:test';
import assert from 'node:assert/strict';
import { nextTheme, storedTheme } from '../src/look.ts';

test('Oni is dark unless light was chosen; Smooth follows the system unless told', () => {
  assert.equal(storedTheme('oni', null), 'dark');
  assert.equal(storedTheme('oni', 'system'), 'dark');
  assert.equal(storedTheme('oni', 'light'), 'light');
  assert.equal(storedTheme('smooth', null), 'system');
  assert.equal(storedTheme('smooth', 'dark'), 'dark');
  assert.equal(storedTheme('smooth', 'light'), 'light');
});

test('the Theme button walks the themes of the look', () => {
  assert.equal(nextTheme('oni', 'dark'), 'light');
  assert.equal(nextTheme('oni', 'light'), 'dark');
  assert.deepEqual(['system', 'light', 'dark'].map((theme) => nextTheme('smooth', theme as 'system')), ['light', 'dark', 'system']);
});
