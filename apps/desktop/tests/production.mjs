// Exercise the packaged frontend with the desktop guard enabled. Only native IPC is substituted.
import assert from 'node:assert/strict';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { build, preview } from 'vite';
import { checkInventory } from './inventory.mjs';
import { answer } from '../src/mock.ts';
import { checkWriteRecovery, holdImports } from './write-recovery.mjs';

export async function checkProduction(browser) {
  const outDir = await mkdtemp(join(tmpdir(), 'mukade-desktop-'));
  const options = { configFile: fileURLToPath(new URL('../vite.config.ts', import.meta.url)), build: { outDir, emptyOutDir: true }, logLevel: 'error' };
  const page = await browser.newPage();
  let server;
  try {
    await build(options);
    server = await preview({ ...options, preview: { host: '127.0.0.1', port: 0, strictPort: false } });
    await checkInventory(browser, server.resolvedUrls.local[0]);
    const writes = holdImports(answer);
    await page.exposeFunction('mockCommand', writes.invoke);
    await page.addInitScript(() => {
      const state = window.testIpc = { imports: 0, hops: 0, reads: 0 };
      let callback = 0;
      window.__TAURI_INTERNALS__ = {
        metadata: { currentWindow: { label: 'main' }, currentWebview: { label: 'main', windowLabel: 'main' } },
        transformCallback: () => ++callback,
        async invoke(command, args) {
          if (command === 'plugin:window|is_maximized') return false;
          if (command.startsWith('plugin:') || ['window_caption_maximize', 'set_window_theme'].includes(command)) return;
          if (command === 'hop') state.hops++;
          if (command === 'installations') state.reads++;
          const result = await window.mockCommand(command, args);
          if (command === 'import') state.imports++;
          if (command === 'acknowledge_write') state.acknowledged = true;
          if (command === 'write_status') state.checked = true;
          return result;
        },
      };
    });
    await page.goto(server.resolvedUrls.local[0]);
    await page.locator('#sessions .row').first().waitFor();
    await page.keyboard.press('Control+f');
    assert.equal(await page.locator('#search').evaluate(el => el === document.activeElement), true);
    const reads = await page.evaluate(() => window.testIpc.reads);
    // Escape leaves the filter, then `r` reads the stores again; F5 and Ctrl+R are the browser's reload, which the guard refuses.
    await page.keyboard.press('Escape');
    await page.keyboard.press('r');
    await page.waitForFunction(expected => window.testIpc.reads === expected, reads + 1);
    for (const key of ['F5', 'Control+r']) await page.keyboard.press(key);
    await page.waitForTimeout(400);
    assert.equal(await page.evaluate(() => window.testIpc.reads), reads + 1, 'F5 and Ctrl+R read nothing');
    await page.locator('#detail').getByRole('button', { name: 'Hop H', exact: true }).click();
    const summary = page.locator('details.mk-more > summary');
    await summary.focus();
    await summary.press('Enter');
    assert.equal(await summary.evaluate(el => el.parentElement.open), true);
    await summary.press('Tab');
    assert.equal(await page.locator('#hop-model').evaluate(el => el === document.activeElement), true);
    assert.equal(await page.evaluate(() => window.testIpc.hops), 0);
    // Enter on a radio or a checkbox is the control's own: it starts no hop (a write).
    for (const target of [page.getByRole('radio').first(), page.locator('#hop-no-tools')]) {
      await target.focus();
      await target.press('Enter');
    }
    assert.equal(await page.evaluate(() => window.testIpc.hops), 0);
    await page.keyboard.press('Escape');
    // The reader beside the list: the list's separator (kept under mukade.width.sessions) exists only while it is open.
    await page.locator('#detail').getByRole('button', { name: /Open transcript/ }).click();
    await page.locator('#reader .entry').first().waitFor();
    assert.equal(await page.locator('[role="separator"]').count(), 2, 'one separator for the sidebar and one for the list');
    // The list's separator first: a resize of the sidebar changes the width of the split, and the split keeps its fraction, so its value
    // moves with it (the package's behaviour). Each value is read once the layout has settled.
    for (const name of ['sessions', 'installations']) {
      await page.waitForTimeout(300);
      const divider = page.locator('#resize-' + name);
      await page.waitForFunction(id => {
        const handle = document.getElementById(id);
        const pane = document.getElementById(handle.getAttribute('aria-controls'));
        return Number(handle.getAttribute('aria-valuenow')) === Math.round(pane.getBoundingClientRect().width);
      }, 'resize-' + name);
      const initial = Number(await divider.getAttribute('aria-valuenow'));
      await divider.press('ArrowRight');
      assert.equal(Number(await divider.getAttribute('aria-valuenow')), initial + 8);
      assert.equal(await page.evaluate(key => localStorage.getItem(key), 'mukade.width.' + name), (initial + 8) + 'px');
      await divider.dblclick();
      assert.equal(await page.evaluate(key => localStorage.getItem(key), 'mukade.width.' + name), null);
    }
    await page.locator('#import').click();
    await page.getByRole('button', { name: 'Choose…' }).click();
    await page.waitForFunction(() => document.getElementById('import-path').value.endsWith('.json'));
    await page.locator('#import-path').fill('session.json');
    await page.keyboard.press('Enter');
    await page.keyboard.press('Enter');
    await page.waitForFunction(() => window.testIpc.imports === 1);
    assert.equal(writes.requests, 1);
    writes.fail();
    await page.getByRole('button', { name: 'Try again' }).click();
    await page.keyboard.press('Enter');
    await page.waitForFunction(() => window.testIpc.imports === 2);
    await checkWriteRecovery(page, writes);
  } finally {
    await page.close();
    await server?.close();
    await rm(outDir, { recursive: true, force: true });
  }
}
