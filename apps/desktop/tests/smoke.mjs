// Smoke test (spec §7): the app on Vite with the mocked commands (src/mock.ts), one screenshot
// per view in dark and light, under tests/out/. Fails on a page error or a missing view.
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
  for (const scheme of ['dark', 'light']) {
    const page = await browser.newPage({ viewport: { width: 1440, height: 900 }, colorScheme: scheme, deviceScaleFactor: 2 });
    page.on('pageerror', (error) => failures.push(`${scheme}: ${error}`));
    page.on('console', (message) => { if (message.type() === 'error') failures.push(`${scheme}: ${message.text()}`); });
    await page.goto(url);
    await page.locator('#sessions .row').first().waitFor();
    await expect(page, '#detail h3', 'sessions');
    await page.screenshot({ path: `${out}sessions-${scheme}.png` });
    await page.keyboard.press('Enter');
    await page.locator('.entry').first().waitFor();
    await page.waitForTimeout(300);
    await page.screenshot({ path: `${out}reader-${scheme}.png` });
    await page.keyboard.press('Escape');
    // Enter on Cancel cancels: it never starts the hop (a write).
    await page.keyboard.press('h');
    await page.locator('.dialog .plan').waitFor();
    await page.getByRole('button', { name: /^Cancel/ }).focus();
    await page.keyboard.press('Enter');
    await page.waitForTimeout(400);
    if (await page.locator('.dialog').count()) failures.push(`${scheme}: Enter on Cancel did not cancel the hop`);
    await page.keyboard.press('h');
    await page.locator('.dialog .plan').waitFor();
    await page.screenshot({ path: `${out}hop-${scheme}.png` });
    await page.keyboard.press('Enter');
    await page.getByText('Losses · 2').waitFor();
    await page.screenshot({ path: `${out}receipt-${scheme}.png` });
    await page.keyboard.press('Escape');
    await page.keyboard.press('c');
    await page.locator('.msg').first().waitFor();
    await expect(page, '#chat-members .member', 'chat');
    await page.screenshot({ path: `${out}chat-${scheme}.png` });
    // What is being typed in the filter survives the 3 s poll.
    await page.locator('#chat-filter').click();
    await page.keyboard.type('sub');
    await page.waitForTimeout(3400);
    const typed = await page.locator('#chat-filter').inputValue();
    if (typed !== 'sub') failures.push(`${scheme}: the chat filter lost what was typed (${JSON.stringify(typed)})`);
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
console.log(`smoke ok: 10 screenshots in ${out}`);
