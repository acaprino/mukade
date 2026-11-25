// Production IPC fixture for delayed reads, failures and stores disappearing between scans.
import assert from 'node:assert/strict';
import { answer } from '../src/mock.ts';
import { fakeWindow } from './fake-window.mjs';

export async function checkInventory(browser, url) {
  const page = await browser.newPage({ viewport: { width: 520, height: 700 } });
  const problems = [];
  try {
    await page.exposeFunction('mockCommand', answer);
    page.on('pageerror', (error) => problems.push(String(error)));
    await page.addInitScript(() => {
      const state = window.inventoryTest = { empty: false, sessions: 'normal', reject: null, resolve: null };
      const stores = ['Alpha', 'Beta'].map((label) => ({ key: label, label, harness: label, count: 2, destination: true, scanned: false }));
      const mock = window.mockCommand;
      window.mockCommand = async (command, args) => {
        if (command === 'installations') return { installations: state.empty ? [] : stores, scans: [] };
        if (command === 'sessions') {
          if (state.sessions === 'defer') return new Promise((resolve, reject) => Object.assign(state, { resolve, reject }));
          if (state.sessions === 'fail') throw new Error('Temporary session read failure');
          if (state.sessions === 'empty') return [];
          return ['first', 'second'].map((id) => ({ id, title: `${args.installation} ${id}`, cwd: '/work' }));
        }
        return mock(command, args);
      };
    });
    await page.addInitScript(fakeWindow);
    await page.goto(url);
    const tree = page.getByRole('tree', { name: 'Sessions' });
    const selected = (text) => tree.getByRole('treeitem', { selected: true, name: new RegExp(text) });
    await selected('Alpha first').waitFor();
    // The installations are in the drawer of the narrow window: a press on one reads its sessions.
    await page.getByRole('button', { name: /menu/i }).first().click();
    await page.getByRole('button', { name: /^Beta/ }).click();
    await selected('Beta first').waitFor();
    if (await page.getByRole('dialog').count()) await page.keyboard.press('Escape');
    await page.waitForFunction(() => document.querySelectorAll('[role="dialog"]').length === 0);

    // The views are the keys of the window, and the palette's.
    await page.keyboard.press('c');
    await page.waitForFunction(() => [...document.querySelectorAll('button')].find((button) => button.textContent.trim() === 'Chat')?.getAttribute('aria-current') === 'page');
    await page.keyboard.press('c');
    await tree.waitFor();
    await page.setViewportSize({ width: 1200, height: 800 });
    await page.keyboard.press('Control+k');
    await page.getByRole('combobox', { name: /command/i }).fill('Harnesses');
    await page.keyboard.press('Enter');
    await page.getByRole('region', { name: 'Claude Code' }).or(page.getByText('Looking for the harnesses')).first().waitFor();
    await page.waitForFunction(() => document.querySelectorAll('[role="dialog"]').length === 0);
    await page.keyboard.press('m');
    await tree.waitFor();

    const mode = (value) => page.evaluate((value) => { window.inventoryTest.sessions = value; }, value);
    await mode('defer');
    await page.keyboard.press('r');
    await page.waitForFunction(() => typeof window.inventoryTest.reject === 'function');
    await mode('fail');
    await page.keyboard.press('r');
    const failure = page.getByRole('alert').filter({ hasText: 'Temporary session read failure' });
    await failure.first().waitFor();
    // The failure is also a notice that stays until it is dismissed (and the panel keeps its own error with Retry).
    const notice = page.getByRole('status', { name: 'Notices' }).getByText('Temporary session read failure').first();
    await notice.waitFor();
    await page.waitForTimeout(6500);
    assert.equal(await notice.isVisible(), true, 'an error notice stays until it is dismissed');
    await mode('empty');
    await page.getByRole('button', { name: 'Retry' }).click();
    await page.getByText('has no sessions').waitFor();
    assert.equal(await tree.count(), 0);
    assert.equal(await failure.count(), 0);
    await mode('normal');
    await page.keyboard.press('r');
    await selected('Beta first').waitFor();
    await page.evaluate(() => window.inventoryTest.reject(new Error('Stale failure')));
    assert.equal(await failure.count(), 0);
    assert.equal(await page.getByText('Stale failure').count(), 0, 'a stale failure shows nowhere');

    await mode('fail');
    await page.keyboard.press('r');
    await failure.first().waitFor();
    await mode('normal');
    await page.getByRole('button', { name: 'Retry' }).click();
    await selected('Beta first').waitFor();
    await page.getByRole('button', { name: 'Open transcript' }).click();
    await page.getByRole('log', { name: 'Transcript' }).waitFor();
    await page.evaluate(() => Object.assign(window.inventoryTest, { sessions: 'defer', resolve: null }));
    await page.keyboard.press('r');
    await page.waitForFunction(() => typeof window.inventoryTest.resolve === 'function');
    await page.evaluate(() => { window.inventoryTest.empty = true; });
    await page.keyboard.press('r');
    await page.getByText('No installations found').waitFor();
    await page.evaluate(() => window.inventoryTest.resolve([{ id: 'stale', title: 'Old inventory' }]));
    assert.equal(await page.getByRole('treeitem').count(), 0);
    assert.equal(await page.getByRole('log', { name: 'Transcript' }).count(), 0);
    await page.getByText('0 installations · 0 sessions').waitFor();
    // With nothing to act on, the commands of a session are in the palette but cannot be done: the list does not jump, the keys do nothing.
    await page.keyboard.press('Control+k');
    const commands = page.getByRole('combobox', { name: /command/i });
    await commands.waitFor();
    for (const name of ['Hop to another installation', 'Open transcript', 'Export session']) {
      await commands.fill(name);
      const option = page.getByRole('option', { name: new RegExp(name) });
      assert.equal(await option.count() === 0 || await option.first().getAttribute('aria-disabled') === 'true', true, `${name} can be done with no session`);
    }
    assert.deepEqual(problems, [], 'the inventory page threw');
  } catch (error) {
    await page.screenshot({ path: new URL('out/inventory-failed.png', import.meta.url).pathname }).catch(() => {});
    throw error;
  } finally {
    await page.close();
  }
}
