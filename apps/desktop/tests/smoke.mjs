// Smoke test (spec §7): the app on Vite with the mocked commands (src/mock.ts), one screenshot
// per view in each look (Smooth, Oni), dark and light, under tests/out/. Fails on a page error or a missing view.
//   node apps/desktop/tests/smoke.mjs
import assert from 'node:assert/strict';
import { checkProduction } from './production.mjs';
import { createServer } from 'vite';
import { chromium } from 'playwright';
import { mkdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

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
  await checkProduction(browser);
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
      // The look and the theme are the palette's: ONI from Ctrl+K, kept, and back to Smooth.
      await page.keyboard.press('Control+k');
      await page.getByRole('combobox', { name: 'Type a command' }).fill('ONI look');
      await page.keyboard.press('Enter');
      await page.waitForFunction(() => document.documentElement.dataset.imLook === 'oni' && localStorage.getItem('mukade.look') === 'oni');
      await page.keyboard.press('Control+k');
      await page.getByRole('combobox', { name: 'Type a command' }).fill('Smooth look');
      await page.keyboard.press('Enter');
      await page.waitForFunction(() => document.documentElement.dataset.imLook === 'smooth' && localStorage.getItem('mukade.look') === 'smooth');
      await page.setViewportSize({ width: 520, height: 650 });
      await page.getByRole('button', { name: 'Views' }).click();
      await page.getByRole('menuitemradio', { name: 'Chat' }).click();
      await page.locator('#view-chat').waitFor();
      await page.getByRole('button', { name: 'Views' }).click();
      await page.getByRole('menuitemradio', { name: 'Sessions' }).click();
      const sidebar = page.locator('.im-sidebar-toggle');
      const mainBounds = await page.locator('.im-work').boundingBox();
      await sidebar.click();
      await expect(page, '#session-sidebar', 'narrow sidebar drawer');
      assert.deepEqual(await page.locator('.im-work').boundingBox(), mainBounds, 'the drawer overlays the content');
      await page.keyboard.press('Escape');
      if (await sidebar.getAttribute('aria-expanded') !== 'false') failures.push('narrow sidebar did not close');
      await page.locator('#sessions .row').first().click();
      await expect(page, '#detail h3', 'narrow inspector');
      await page.getByRole('button', { name: 'Back one column' }).click();
      if (await page.locator('#detail').isVisible()) failures.push('narrow inspector did not close');
      await page.setViewportSize({ width: 1440, height: 900 });
    }
    await page.getByRole('listbox', { name: 'Sessions' }).focus();
    // The list answers its own keys: End and Home move the selection, with the focus kept in the list.
    await page.keyboard.press('End');
    await page.waitForFunction(() => document.querySelector('#sessions [role="option"]:last-child')?.getAttribute('aria-selected') === 'true');
    await page.keyboard.press('Home');
    await page.waitForFunction(() => document.querySelector('#sessions [role="option"]')?.getAttribute('aria-selected') === 'true');
    await page.screenshot({ path: `${out}sessions-${name}.png` });
    await page.keyboard.press('Enter');
    await page.locator('.entry').first().waitFor();
    await page.waitForTimeout(300);
    await page.screenshot({ path: `${out}reader-${name}.png` });
    await page.keyboard.press('Escape');
    // Enter on Cancel cancels: it never starts the hop (a write).
    await page.keyboard.press('h');
    await page.locator('.im-form-dialog .plan').waitFor();
    await page.getByRole('button', { name: /^Cancel/ }).focus();
    await page.keyboard.press('Enter');
    await page.waitForTimeout(400);
    if (await page.locator('.im-form-dialog').count()) failures.push(`${name}: Enter on Cancel did not cancel the hop`);
    await page.keyboard.press('h');
    await page.locator('.im-form-dialog .plan').waitFor();
    await page.screenshot({ path: `${out}hop-${name}.png` });
    await page.keyboard.press('Enter');
    await page.getByText('Losses · 2').waitFor();
    await page.screenshot({ path: `${out}receipt-${name}.png` });
    await page.keyboard.press('Escape');
    // A chat hosted by ChatGPT: its chip, the work it left in its folder, and starting another harness there.
    await page.locator('[data-id="codex"]').click();
    await page.locator('#sessions .row .tag', { hasText: 'hosted' }).waitFor();
    await page.locator('#detail .work-repo').nth(2).waitFor();
    const repositories = await page.locator('#detail .work-repo').allInnerTexts();
    if (repositories.length !== 3) failures.push(`${name}: ${repositories.length} repositories in the work, not 3`);
    if (!repositories.some((text) => text.includes('feature/stack-detection · no upstream · 3 commits on no remote · 133 uncommitted'))) failures.push(`${name}: the line of stack-analyser is missing`);
    if (await page.locator('#detail .note:not(.err)', { hasText: 'Hosted by ChatGPT: the conversation is not on this machine' }).count() !== 1) failures.push(`${name}: the hosted note is missing or red`);
    if (await page.locator('#detail .work .im-chip', { hasText: 'at risk' }).count() !== 2) failures.push(`${name}: two repositories should be at risk`);
    await page.screenshot({ path: `${out}hosted-${name}.png` });
    await page.getByRole('button', { name: /^Start in/ }).waitFor();
    await page.getByRole('listbox', { name: 'Sessions' }).focus();
    await page.keyboard.press('s');
    await page.locator('.im-form-dialog #handoff-note').waitFor();
    if (!(await page.locator('#handoff-note').inputValue()).includes('Capire lo stack di Daodan')) failures.push(`${name}: the note does not name the session`);
    await page.screenshot({ path: `${out}handoff-${name}.png` });
    await page.keyboard.press('Escape');
    await page.waitForTimeout(300);
    if (await page.locator('.im-form-dialog').count()) failures.push(`${name}: Escape did not close the handoff dialog`);
    await page.getByRole('button', { name: /^Start in/ }).click();
    await page.locator('.im-form-dialog #handoff-note').waitFor();
    await page.locator('.im-form-dialog').getByRole('button', { name: /^Start/ }).click();
    await page.locator('#status', { hasText: 'Started Claude Code in C:\\Users\\dev\\Documents\\ChatGPT\\Daodan; note saved to ' }).waitFor();
    if (await page.locator('.im-form-dialog').count()) failures.push(`${name}: the handoff dialog stayed open after Start`);
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
    await page.keyboard.press('Control+Shift+f');
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
    // The palette switches the look, and the choice is kept.
    const other = look === 'smooth' ? 'oni' : 'smooth';
    await page.keyboard.press('Escape');
    for (const [to, title] of [[other, other === 'oni' ? 'ONI look' : 'Smooth look'], [look, look === 'oni' ? 'ONI look' : 'Smooth look']]) {
      await page.keyboard.press('Control+k');
      await page.getByRole('combobox', { name: 'Type a command' }).fill(title);
      await page.keyboard.press('Enter');
      const kept = await page.evaluate(() => `${document.documentElement.dataset.imLook}/${localStorage.getItem('mukade.look')}`);
      if (kept !== `${to}/${to}`) failures.push(`${name}: the palette did not switch to ${to} (${kept})`);
    }
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
console.log(`smoke ok: screenshots in ${out}`);
