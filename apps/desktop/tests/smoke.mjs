// Smoke test (spec §7): the app on Vite with the mocked commands (src/mock.ts), one screenshot
// per view in each look (Smooth, Oni), dark and light, under tests/out/. Fails on a page error or a missing view.
//   node apps/desktop/tests/smoke.mjs
import { createServer } from 'vite';
import { createRequire } from 'node:module';
import { mkdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

// Playwright is the machine's own (CI image or a global install), not a dependency of the app.
const { chromium } = createRequire(import.meta.url)('playwright');
const here = fileURLToPath(new URL('.', import.meta.url));
const out = `${here}out/`;
mkdirSync(out, { recursive: true });
const server = await createServer({ configFile: `${here}../vite.config.ts`, server: { port: 1431, strictPort: false }, logLevel: 'error' });
await server.listen();
const url = server.resolvedUrls.local[0];
const browser = await chromium.launch();
const failures = [];
const expect = async (page, selector, what) => {
  if (!(await page.locator(selector).first().isVisible())) failures.push(`${what}: ${selector} not shown`);
};
try {
  for (const look of ['smooth', 'oni']) for (const scheme of ['dark', 'light']) {
    const name = `${look}-${scheme}`;
    const page = await browser.newPage({ viewport: { width: 1440, height: 900 }, colorScheme: scheme, deviceScaleFactor: 2 });
    // Oni is told its theme; Smooth is told nothing and follows the system's.
    await page.addInitScript(([look, theme]) => { localStorage.setItem('mukade.look', look); if (look === 'oni') localStorage.setItem('mukade.theme', theme); }, [look, scheme]);
    page.on('pageerror', (error) => failures.push(`${name}: ${error}`));
    page.on('console', (message) => { if (message.type() === 'error') failures.push(`${name}: ${message.text()}`); });
    await page.goto(url);
    await page.locator('#sessions .row').first().waitFor();
    await expect(page, '#detail h3', 'sessions');
    const root = await page.evaluate(() => `${document.documentElement.dataset.imLook}-${document.documentElement.dataset.imTheme}`);
    if (root !== name) failures.push(`${name}: the root says ${root}`);
    if (look === 'smooth' && scheme === 'dark') {
      await page.keyboard.press('Control+Shift+k');
      await page.getByRole('combobox', { name: 'Type a command' }).fill('look and theme');
      await page.keyboard.press('Enter');
      await page.getByRole('dialog', { name: 'Settings' }).waitFor();
      await page.keyboard.press('Escape');
      await page.setViewportSize({ width: 520, height: 650 });
      await page.getByRole('button', { name: 'Views' }).click();
      await page.getByRole('menuitem', { name: 'Chat' }).click();
      await page.locator('#view-chat').waitFor();
      await page.getByRole('button', { name: 'Views' }).click();
      await page.getByRole('menuitem', { name: 'Sessions' }).click();
      const sidebar = page.locator('.im-sidebar-toggle');
      await sidebar.click();
      await expect(page, '#session-sidebar', 'narrow sidebar drawer');
      await page.keyboard.press('Escape');
      if (await sidebar.getAttribute('aria-expanded') !== 'false') failures.push('narrow sidebar did not close');
      await page.locator('#sessions .row').first().click();
      await expect(page, '#detail h3', 'narrow inspector');
      await page.getByRole('button', { name: 'Back one column' }).click();
      if (await page.locator('#detail').isVisible()) failures.push('narrow inspector did not close');
      await page.setViewportSize({ width: 1440, height: 900 });
      await page.locator('#sessions').focus();
    }
    await page.screenshot({ path: `${out}sessions-${name}.png` });
    await page.keyboard.press('Enter');
    await page.locator('.entry').first().waitFor();
    await page.waitForTimeout(300);
    await page.screenshot({ path: `${out}reader-${name}.png` });
    await page.keyboard.press('Escape');
    // Enter on Cancel cancels: it never starts the hop (a write).
    await page.keyboard.press('h');
    await page.locator('.dialog .plan').waitFor();
    await page.getByRole('button', { name: /^Cancel/ }).focus();
    await page.keyboard.press('Enter');
    await page.waitForTimeout(400);
    if (await page.locator('.dialog').count()) failures.push(`${name}: Enter on Cancel did not cancel the hop`);
    await page.keyboard.press('h');
    await page.locator('.dialog .plan').waitFor();
    await page.screenshot({ path: `${out}hop-${name}.png` });
    await page.keyboard.press('Enter');
    await page.getByText('Losses · 2').waitFor();
    await page.screenshot({ path: `${out}receipt-${name}.png` });
    await page.keyboard.press('Escape');
    await page.keyboard.press('c');
    // The room in its three views: the graph (the first), the lanes, the feed.
    await page.locator('.graph-canvas svg .node').first().waitFor();
    await expect(page, '#chat-members .member', 'chat');
    await page.waitForTimeout(700);
    await page.screenshot({ path: `${out}chat-${name}.png` });
    await page.keyboard.press('2');
    await page.locator('.lanes .lrow').first().waitFor();
    await page.screenshot({ path: `${out}chat-lanes-${name}.png` });
    await page.keyboard.press('3');
    await page.locator('.feed .msg').first().waitFor();
    // The window joins and writes as the user; the message comes back from the service.
    await page.getByRole('button', { name: 'Join', exact: true }).click();
    await page.locator('#chat-text').waitFor();
    await page.locator('#chat-text').fill('Smoke check: @codex-b is the receipt done?');
    await page.keyboard.press('Control+Enter');
    await page.locator('.feed .msg', { hasText: 'Smoke check' }).waitFor();
    await page.screenshot({ path: `${out}chat-feed-${name}.png` });
    // A new room, from its key.
    await page.keyboard.press('Escape');
    await page.keyboard.press('n');
    await page.locator('#room-name').fill('smoke-room');
    await page.keyboard.press('Enter');
    await page.locator('#chat-rooms .row', { hasText: 'smoke-room' }).waitFor();
    // Search everything finds a member of another room.
    await page.keyboard.press('Control+k');
    await page.locator('#search-all').fill('codex');
    await page.locator('.search-row').first().waitFor();
    await page.screenshot({ path: `${out}search-${name}.png` });
    await page.keyboard.press('Escape');
    // The harnesses page: every harness, and a scan of the computer that finds a portable one.
    await page.keyboard.press('m');
    await page.locator('.harness-card').first().waitFor();
    await page.getByRole('button', { name: 'Scan this computer' }).click();
    await page.locator('#status', { hasText: 'system scan done' }).waitFor({ timeout: 20000 });
    await page.locator('.install-row', { hasText: 'JanuraCode' }).waitFor();
    await page.screenshot({ path: `${out}harnesses-${name}.png` });
    await page.keyboard.press('c');
    // What is being typed in the filter survives the 3 s poll.
    await page.locator('#chat-filter').click();
    await page.keyboard.type('sub');
    await page.waitForTimeout(3400);
    const typed = await page.locator('#chat-filter').inputValue();
    if (typed !== 'sub') failures.push(`${name}: the chat filter lost what was typed (${JSON.stringify(typed)})`);
    // Settings switches the look, and the choice is kept.
    await page.locator('#settings').click();
    const other = look === 'smooth' ? 'oni' : 'smooth';
    await page.locator('.dialog').getByRole('button', { name: other, exact: true }).click();
    const kept = await page.evaluate(() => `${document.documentElement.dataset.imLook}/${localStorage.getItem('mukade.look')}`);
    if (kept !== `${other}/${other}`) failures.push(`${name}: Settings did not switch to ${other} (${kept})`);
    await page.locator('.dialog').getByRole('button', { name: look, exact: true }).click();
    await page.screenshot({ path: `${out}settings-${name}.png` });
    await page.close();
  }
} finally {
  await browser.close();
  await server.close();
}
if (failures.length) {
  console.error(failures.join('\n'));
  process.exit(1);
}
console.log(`smoke ok: 40 screenshots in ${out}`);
