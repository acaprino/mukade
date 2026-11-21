// Production IPC fixture for delayed reads, failures and stores disappearing between scans.
import assert from 'node:assert/strict';
import { answer } from '../src/mock.ts';

export async function checkInventory(browser, url) {
  const page = await browser.newPage({ viewport: { width: 520, height: 700 } });
  try {
    await page.exposeFunction('mockCommand', answer);
    await page.addInitScript(() => {
      const state = window.inventoryTest = {
        empty: false, sessions: 'normal', reject: null, resolve: null,
      };
      const stores = ['Alpha', 'Beta'].map(label => ({
        key: label, label, harness: label, count: 2, destination: true, scanned: false,
      }));
      let callback = 0;
      window.__TAURI_INTERNALS__ = {
        metadata: { currentWindow: { label: 'main' }, currentWebview: { label: 'main', windowLabel: 'main' } },
        transformCallback: () => ++callback,
        async invoke(command, args) {
          if (command === 'installations') return { installations: state.empty ? [] : stores, scans: [] };
          if (command === 'sessions') {
            if (state.sessions === 'defer') return new Promise((resolve, reject) => Object.assign(state, { resolve, reject }));
            if (state.sessions === 'fail') throw new Error('Temporary session read failure');
            if (state.sessions === 'empty') return [];
            return ['first', 'second'].map(id => ({ id, title: args.installation + ' ' + id, cwd: '/work' }));
          }
          if (command === 'plugin:window|is_maximized') return false;
          if (command.startsWith('plugin:') || ['window_caption_maximize', 'set_window_theme'].includes(command)) return;
          return window.mockCommand(command, args);
        },
      };
    });
    await page.goto(url);
    const selectedSession = page.locator('#sessions [aria-selected="true"]');
    await selectedSession.filter({ hasText: 'Alpha first' }).waitFor();
    const toggle = page.locator('.im-sidebar-toggle');
    const installations = page.getByRole('listbox', { name: 'Installations', exact: true });
    await toggle.click();
    assert.ok(await installations.evaluate(list => list === document.activeElement));
    await page.keyboard.press('ArrowDown');
    await selectedSession.filter({ hasText: 'Beta first' }).waitFor();
    assert.equal(await installations.getByRole('option', { selected: true }).getAttribute('data-id'), 'Beta');
    assert.ok(await installations.evaluate(list => list === document.activeElement));
    await page.keyboard.press('Tab');
    const folders = page.getByRole('listbox', { name: 'Folders' });
    assert.ok(await folders.evaluate(list => list === document.activeElement));
    await page.keyboard.press('ArrowDown');
    assert.equal(await folders.getByRole('option', { selected: true }).getAttribute('data-id'), '/work');
    assert.ok(await folders.evaluate(list => list === document.activeElement));

    await page.keyboard.press('c');
    await page.locator('#view-chat').waitFor();
    assert.equal(await toggle.getAttribute('aria-expanded'), 'false');
    assert.equal(await page.locator('.im-tabs [tabindex="0"]').getAttribute('id'), 'tab-chat');
    await page.keyboard.press('c');
    await page.locator('#sessions').waitFor();
    assert.equal(await toggle.getAttribute('aria-expanded'), 'false');
    assert.ok(await page.locator('#sessions').evaluate(el => el.contains(document.activeElement)));
    await page.setViewportSize({ width: 1200, height: 800 });
    await page.keyboard.press('Control+k');
    await page.getByRole('combobox', { name: 'Type a command' }).fill('Harnesses');
    await page.keyboard.press('Enter');
    await page.locator('#view-harnesses').waitFor();
    assert.equal(await page.locator('.im-tabs [tabindex="0"]').getAttribute('id'), 'tab-harnesses');
    await page.keyboard.press('m');

    const mode = value => page.evaluate(value => { window.inventoryTest.sessions = value; }, value);
    await mode('defer');
    await page.keyboard.press('r');
    await page.waitForFunction(() => typeof window.inventoryTest.reject === 'function');
    await mode('fail');
    await page.keyboard.press('r');
    const failure = page.locator('#sessions').getByRole('alert');
    await failure.filter({ hasText: 'Temporary session read failure' }).waitFor();
    await mode('empty');
    await failure.getByRole('button', { name: 'Retry' }).click();
    await page.locator('#sessions [data-im-state-layer="empty"]').waitFor();
    assert.equal(await page.locator('#sessions .row').count(), 0);
    assert.equal(await failure.count(), 0);
    await mode('normal');
    await page.keyboard.press('r');
    await selectedSession.filter({ hasText: 'Beta first' }).waitFor();
    await page.evaluate(() => window.inventoryTest.reject(new Error('Stale failure')));
    assert.equal(await failure.count(), 0);
    assert.ok(!(await page.locator('#status').innerText()).includes('Stale failure'));

    await mode('fail');
    await page.keyboard.press('r');
    await failure.waitFor();
    await mode('normal');
    await failure.getByRole('button', { name: 'Retry' }).click();
    await selectedSession.filter({ hasText: 'Beta first' }).waitFor();
    await page.locator('#detail').getByRole('button', { name: /Open transcript/ }).click();
    await page.locator('#reader .entry').first().waitFor();
    await page.evaluate(() => Object.assign(window.inventoryTest, { sessions: 'defer', resolve: null }));
    await page.keyboard.press('r');
    await page.waitForFunction(() => typeof window.inventoryTest.resolve === 'function');
    await page.evaluate(() => { window.inventoryTest.empty = true; });
    await page.keyboard.press('r');
    await page.locator('#sessions').getByText('No installations found.').waitFor();
    await page.evaluate(() => window.inventoryTest.resolve([{ id: 'stale', title: 'Old inventory' }]));
    assert.equal(await page.locator('#sessions .row, #reader .entry, #detail h3').count(), 0);
    assert.equal(await page.locator('#installation-count').innerText(), '0');
    assert.equal(await page.locator('#status-counts').innerText(), '0 installations · 0 sessions');
    await page.keyboard.press('Control+k');
    const commands = page.getByRole('combobox', { name: 'Type a command' });
    await commands.waitFor();
    for (const name of ['Hop to another installation', 'Open transcript', 'Export session']) {
      await commands.fill(name);
      assert.equal(await page.getByRole('option', { name: new RegExp(name) }).count(), 0);
    }
  } finally {
    await page.close();
  }
}
