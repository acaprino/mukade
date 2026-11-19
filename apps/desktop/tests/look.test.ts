import { test } from 'node:test';
import assert from 'node:assert/strict';
import { nextTheme, paints, storedLook, storedTheme } from '../src/look.ts';

test('Smooth is the look unless Oni was chosen', () => {
  assert.equal(storedLook(null), 'smooth');
  assert.equal(storedLook('stale'), 'smooth');
  assert.equal(storedLook('oni'), 'oni');
});

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

test('system paints what the system says', () => {
  assert.equal(paints('system', true), 'light');
  assert.equal(paints('system', false), 'dark');
  assert.equal(paints('dark', true), 'dark');
});
