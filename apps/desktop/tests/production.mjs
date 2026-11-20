// Exercise the packaged frontend with the desktop guard enabled. Only native IPC is substituted.
import assert from 'node:assert/strict';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { build, preview } from 'vite';
import { answer } from '../src/mock.ts';

export async function checkProduction(browser) {
  const outDir = await mkdtemp(join(tmpdir(), 'mukade-desktop-'));
  const options = { configFile: fileURLToPath(new URL('../vite.config.ts', import.meta.url)), build: { outDir, emptyOutDir: true }, logLevel: 'error' };
  const page = await browser.newPage();
  let server;
  try {
    await build(options);
    server = await preview({ ...options, preview: { host: '127.0.0.1', port: 0, strictPort: false } });
    await page.exposeFunction('mockCommand', answer);
    await page.addInitScript(() => {
      const state = window.testIpc = { imports: 0, reads: 0 };
      let callback = 0;
      window.__TAURI_INTERNALS__ = {
        metadata: { currentWindow: { label: 'main' }, currentWebview: { label: 'main', windowLabel: 'main' } },
        transformCallback: () => ++callback,
        async invoke(command, args) {
          if (command === 'plugin:window|is_maximized') return false;
          if (command.startsWith('plugin:') || ['window_caption_maximize', 'set_window_theme'].includes(command)) return;
          if (command === 'installations') state.reads++;
          if (command === 'import') {
            state.imports++;
            await new Promise((resolve, reject) => { state.finish = resolve; state.fail = reject; });
          }
          return window.mockCommand(command, args);
        },
      };
    });
    await page.goto(server.resolvedUrls.local[0]);
    await page.locator('#sessions .row').first().waitFor();
    await page.keyboard.press('Control+f');
    assert.equal(await page.locator('#search').evaluate(el => el === document.activeElement), true);
    const reads = await page.evaluate(() => window.testIpc.reads);
    for (const [index, key] of ['F5', 'Control+r'].entries()) {
      await page.keyboard.press(key);
      await page.waitForFunction(expected => window.testIpc.reads === expected, reads + index + 1);
    }
    await page.locator('#import').click();
    await page.locator('#import-path').fill('session.json');
    await page.keyboard.press('Enter');
    await page.keyboard.press('Enter');
    assert.equal(await page.evaluate(() => window.testIpc.imports), 1);
    await page.evaluate(() => window.testIpc.fail('temporary failure'));
    await page.getByRole('button', { name: 'Try again' }).click();
    await page.keyboard.press('Enter');
    assert.equal(await page.evaluate(() => window.testIpc.imports), 2);
    await page.evaluate(() => window.testIpc.finish());
    await page.getByRole('dialog', { name: 'Done', exact: true }).waitFor();
  } finally {
    await page.close();
    await server?.close();
    await rm(outDir, { recursive: true, force: true });
  }
}
