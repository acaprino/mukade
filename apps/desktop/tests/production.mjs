// Exercise the packaged frontend (Vite's build, served as the window serves it: with the page's content security policy of
// tauri.conf.json) with the desktop guard enabled and Tauri's IPC substituted (fake-window.mjs). The page runs as in a window: the keys of
// the browser are refused, the splitter keeps its size, a hop that is only started by its own button, an import that a reload does not lose.
import assert from 'node:assert/strict';
import { mkdtemp, readFile, rm, stat } from 'node:fs/promises';
import { createServer } from 'node:http';
import { tmpdir } from 'node:os';
import { extname, join, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { build } from 'vite';
import { answer } from '../src/mock.ts';
import { fakeWindow } from './fake-window.mjs';
import { checkInventory } from './inventory.mjs';
import { checkWriteRecovery, holdImports } from './write-recovery.mjs';

const MIME = { '.html': 'text/html; charset=utf-8', '.js': 'text/javascript; charset=utf-8', '.css': 'text/css; charset=utf-8', '.svg': 'image/svg+xml', '.png': 'image/png', '.woff2': 'font/woff2' };

/** Serves `dist` under the policy the window gives its page. */
async function serve(dist) {
  const policy = JSON.parse(await readFile(fileURLToPath(new URL('../src-tauri/tauri.conf.json', import.meta.url)), 'utf8')).app.security.csp;
  const server = createServer(async (request, response) => {
    const path = decodeURIComponent(new URL(request.url, 'http://localhost').pathname);
    const file = resolve(dist, `.${path.endsWith('/') ? `${path}index.html` : path}`);
    if (!file.startsWith(dist + sep) || !(await stat(file).catch(() => null))?.isFile()) { response.writeHead(404).end(); return; }
    response.writeHead(200, { 'content-type': MIME[extname(file)] ?? 'application/octet-stream', 'content-security-policy': policy });
    response.end(await readFile(file));
  });
  await new Promise((done) => server.listen(0, '127.0.0.1', done));
  return { url: `http://127.0.0.1:${server.address().port}/`, close: () => new Promise((done) => server.close(done)) };
}

export async function checkProduction(browser) {
  const outDir = await mkdtemp(join(tmpdir(), 'mukade-desktop-'));
  const options = { configFile: fileURLToPath(new URL('../vite.config.ts', import.meta.url)), build: { outDir, emptyOutDir: true }, logLevel: 'error' };
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
  const problems = [];
  let server;
  try {
    await build(options);
    server = await serve(outDir);
    await checkInventory(browser, server.url);
    const writes = holdImports(answer);
    await page.exposeFunction('mockCommand', writes.invoke);
    await page.addInitScript(fakeWindow);
    page.on('pageerror', (error) => problems.push(String(error)));
    page.on('console', (message) => { if (message.type() === 'error') problems.push(message.text()); });
    page.on('requestfailed', (request) => problems.push(`${request.url()} failed`));
    await page.goto(server.url);
    const tree = page.getByRole('tree', { name: 'Sessions' });
    await tree.getByRole('treeitem').first().waitFor();
    // The images the page draws (the logos of the harnesses) load under the policy: none failed, none was refused.
    assert.equal(await page.locator('img').evaluateAll((images) => images.filter((image) => !image.complete || image.naturalWidth === 0).length), 0, 'a logo did not load');
    // Ctrl+F is the search of the view: it lands in the filter, not in the browser's.
    await page.keyboard.press('Control+f');
    assert.equal(await page.getByRole('searchbox', { name: 'Filter sessions' }).evaluate((el) => el === document.activeElement), true);
    const reads = await page.evaluate(() => window.testIpc.reads);
    // Escape leaves the filter, then `r` reads the stores again; F5 and Ctrl+R are the browser's reload, which the guard refuses.
    await page.keyboard.press('Escape');
    await page.keyboard.press('r');
    await page.waitForFunction((expected) => window.testIpc.reads === expected, reads + 1);
    for (const key of ['F5', 'Control+r']) await page.keyboard.press(key);
    await page.waitForTimeout(400);
    assert.equal(await page.evaluate(() => window.testIpc.reads), reads + 1, 'F5 and Ctrl+R read nothing');
    await page.getByRole('button', { name: 'Hop', exact: true }).click();
    const hop = page.getByRole('dialog', { name: 'Hop' });
    const more = hop.getByRole('button', { name: 'More options' });
    await more.focus();
    await more.press('Enter');
    assert.equal(await more.getAttribute('aria-expanded'), 'true');
    await hop.getByRole('textbox', { name: 'Destination model' }).waitFor();
    assert.equal(await page.evaluate(() => window.testIpc.hops), 0);
    // Enter on a radio or a checkbox is the control's own: it starts no hop (a write).
    for (const [what, target] of [['a radio', hop.getByRole('radio').first()], ['a checkbox', hop.getByRole('checkbox', { name: 'Without tool calls and results' })]]) {
      await target.focus();
      await target.press('Enter');
      await page.waitForTimeout(300);
      assert.equal(await page.evaluate(() => window.testIpc.hops), 0, `Enter on ${what} started a hop`);
    }
    await page.keyboard.press('Escape');
    await hop.waitFor({ state: 'detached' });
    // The reader beside the list: the splitter keeps its size under mukade.split.sessions, and a double press forgets it.
    await page.getByRole('button', { name: 'Open transcript' }).click();
    await page.getByRole('log', { name: 'Transcript' }).waitFor();
    const divider = page.getByRole('separator', { name: 'Resize the sessions list' });
    const initial = Number(await divider.getAttribute('aria-valuenow'));
    await divider.press('ArrowRight');
    assert.equal(Number(await divider.getAttribute('aria-valuenow')), initial + 16);
    assert.equal(await page.evaluate((key) => localStorage.getItem(key), 'mukade.split.sessions'), String(initial + 16));
    await divider.dblclick();
    assert.equal(await page.evaluate((key) => localStorage.getItem(key), 'mukade.split.sessions'), null);
    // An import: Choose… takes the path from the native dialog, Enter in the field starts it, a failure is retried from its dialog.
    await page.keyboard.press('Escape');
    await page.getByRole('button', { name: 'Import', exact: true }).click();
    const importing = page.getByRole('dialog', { name: 'Import' });
    await importing.getByRole('button', { name: 'Choose…' }).click();
    await page.waitForFunction(() => document.querySelector('input[placeholder*="mukade-data"]')?.value.endsWith('.json'));
    const path = importing.getByRole('textbox', { name: 'Document' });
    await path.fill('session.json');
    await path.press('Enter');
    await page.waitForFunction(() => window.testIpc.imports === 1);
    assert.equal(writes.requests, 1);
    // While it runs the dialog has no form, and a second Enter writes nothing.
    await page.keyboard.press('Enter');
    assert.equal(writes.requests, 1);
    writes.fail();
    await page.getByRole('button', { name: 'Try again' }).click();
    await path.press('Enter');
    await page.waitForFunction(() => window.testIpc.imports === 2);
    await checkWriteRecovery(page, writes);
    assert.deepEqual(problems, [], 'the production page said something to the console');
  } catch (error) {
    await page.screenshot({ path: fileURLToPath(new URL('out/production-failed.png', import.meta.url)) }).catch(() => {});
    throw error;
  } finally {
    await page.close();
    await server?.close();
    await rm(outDir, { recursive: true, force: true });
  }
}
