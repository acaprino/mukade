import { test } from 'node:test';
import assert from 'node:assert/strict';
import { storedTheme } from '../src/look.ts';

test('ONI starts dark and Smooth follows the system; every look keeps any of the three themes', () => {
  assert.equal(storedTheme('oni', null), 'dark');
  assert.equal(storedTheme('oni', 'light'), 'light');
  assert.equal(storedTheme('oni', 'system'), 'system');
  assert.equal(storedTheme('smooth', null), 'system');
  assert.equal(storedTheme('smooth', 'dark'), 'dark');
  assert.equal(storedTheme('smooth', 'light'), 'light');
});

test("a stale or corrupt stored theme is the look's own", () => {
  assert.equal(storedTheme('oni', 'purple'), 'dark');
  assert.equal(storedTheme('smooth', ''), 'system');
});
