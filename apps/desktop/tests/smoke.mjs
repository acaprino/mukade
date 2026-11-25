// Smoke test (spec §7): the page on Vite with the mocked commands (src/mock.ts), driven in a browser by what a person and a screen reader
// see: roles, names and aria attributes, never a class. It draws the views at the widths where the frame changes, in both schemes, and
// checks that nothing runs off the page; then it follows the flows end to end (the sessions and their reader, the hop, the chat, the
// harnesses, the search, the palette). Screenshots go under tests/out/ (git-ignored). Playwright is a pinned devDependency.
//   node apps/desktop/tests/smoke.mjs
import assert from 'node:assert/strict';
import { mkdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';
import { createServer } from 'vite';
import { checkProduction } from './production.mjs';

const here = fileURLToPath(new URL('.', import.meta.url));
const out = `${here}out/`;
mkdirSync(out, { recursive: true });
const server = await createServer({ configFile: `${here}../vite.config.ts`, server: { port: 1431, strictPort: false }, logLevel: 'error' });
await server.listen();
const url = server.resolvedUrls.local[0];
const browser = await chromium.launch({ args: ['--disable-lcd-text'] });
const failures = [];
/** The pages that are open: when a flow throws, each is photographed where it stopped. */
const pages = new Set();
const check = (ok, message) => { if (!ok) failures.push(message); };

/** The widths where the frame changes (340 is the smallest; 620, 860 and 1000 are its steps) and a wide one. */
const SIZES = [[340, 700], [620, 700], [860, 700], [1000, 700], [1440, 900]];

/** What sticks out of the page or of a region of the frame, sideways. */
const overflow = (page) => page.evaluate(() => {
  const found = [];
  const wide = document.documentElement;
  if (wide.scrollWidth > wide.clientWidth) found.push(`the page is ${wide.scrollWidth} wide in ${wide.clientWidth}`);
  for (const slot of ['title-bar', 'app-shell-status', 'sidebar', 'app-shell-content', 'app-shell-actions', 'app-shell-footer', 'panel-header', 'panel-tools', 'panel-body', 'panel-footer', 'inspector-body']) {
    for (const region of document.querySelectorAll(`[data-slot="${slot}"]`)) {
      if (region.clientWidth > 1 && region.scrollWidth > region.clientWidth) found.push(`${slot} is ${region.scrollWidth} wide in ${region.clientWidth}`);
    }
  }
  // The action bar holds its buttons at its end: what does not fit would go off to the start, where scrolling does not see it.
  for (const button of document.querySelectorAll('[data-slot="app-shell-actions"] button')) {
    const box = button.getBoundingClientRect();
    if (box.left < -1 || box.right > wide.clientWidth + 1) found.push(`the action «${button.textContent.trim()}» is at ${Math.round(box.left)}..${Math.round(box.right)} of ${wide.clientWidth}`);
  }
  return found;
});

/** A page of its own, with the scheme kept and the errors of the page counted as failures. */
async function open(name, { width = 1440, height = 900, scheme = 'dark', system = scheme } = {}) {
  const page = await browser.newPage({ viewport: { width, height }, colorScheme: system, deviceScaleFactor: 1 });
  await page.addInitScript((value) => { try { localStorage.setItem('mukade.theme', value); } catch { /* no storage */ } }, scheme);
  page.on('pageerror', (error) => failures.push(`${name}: ${error}`));
  page.on('console', (message) => { if (message.type() === 'error') failures.push(`${name}: ${message.text()}`); });
  pages.add(page);
  page.on('close', () => pages.delete(page));
  await page.goto(url);
  await page.getByRole('tree', { name: 'Sessions' }).waitFor();
  return page;
}

const tree = (page) => page.getByRole('tree', { name: 'Sessions' });
const row = (page, name) => tree(page).getByRole('treeitem', { name });
/** Runs a command of the palette by its name; the palette is gone before the keys go on (a plain key is left to an open surface). */
const palette = async (page, command) => {
  await page.keyboard.press('Control+k');
  await page.getByRole('combobox', { name: /command/i }).fill(command);
  await page.keyboard.press('Enter');
  await page.getByRole('combobox', { name: /command/i }).waitFor({ state: 'detached' });
};
const notice = (page, text) => page.getByRole('status', { name: 'Notices' }).getByText(text).first();

try {
  await checkProduction(browser);

  // --- the frame at every width, in both schemes -------------------------------------------------------------------------
  for (const scheme of ['dark', 'light']) {
    for (const [width, height] of SIZES) {
      const page = await open(`${scheme} ${width}`, { width, height, scheme });
      for (const view of ['Sessions', 'Chat', 'Harnesses']) {
        // The views are in the title bar from 860px, in a row under it below; either way a view is a button that says where it is.
        await page.getByRole('button', { name: view, exact: true }).first().click();
        await page.waitForTimeout(250);
        const found = await overflow(page);
        check(found.length === 0, `${scheme} ${width} ${view}: ${found.join('; ')}`);
        if (width === 340 || width === 1440) await page.screenshot({ path: `${out}${view.toLowerCase()}-${scheme}-${width}.png` });
      }
      check(await page.getByRole('button', { name: 'Harnesses', exact: true }).first().getAttribute('aria-current') === 'page', `${scheme} ${width}: the view in use is not marked`);
      await page.close();
    }
  }

  // --- the sessions: the tree, its keys, the details ---------------------------------------------------------------------
  {
    const page = await open('sessions');
    check(await page.getByRole('searchbox', { name: 'Filter sessions' }).count() === 1, 'sessions: the filter is missing');
    // Each parent holds its children, and a fork is marked; an orphan stays a root.
    await row(page, /Hop carries subagents to Codex/).waitFor();
    check(await row(page, /Hop carries subagents to Codex/).getAttribute('aria-expanded') === 'true', 'sessions: the parent is not open');
    check(await row(page, /Map the writers/).count() === 1, 'sessions: a subagent is not under its parent');
    // The tree is one tab stop with the arrows: End and Home move the selection (the details follow), j and k too.
    await row(page, /Hop carries subagents to Codex/).focus();
    await page.keyboard.press('End');
    await page.getByRole('heading', { name: /Harness adapters/ }).waitFor();
    await page.keyboard.press('Home');
    await page.getByRole('heading', { name: /Hop carries subagents to Codex/ }).waitFor();
    await page.keyboard.press('j');
    await page.getByRole('heading', { name: /Map the writers/ }).waitFor();
    await page.keyboard.press('k');
    await page.getByRole('heading', { name: /Hop carries subagents to Codex/ }).waitFor();
    // Space folds a parent; its children leave the tree and come back.
    await page.keyboard.press('Space');
    await row(page, /Map the writers/).waitFor({ state: 'detached' });
    await page.keyboard.press('Space');
    await row(page, /Map the writers/).waitFor();
    // The filter keeps what matches, with its parents; Enter hands the focus to the tree.
    const filter = page.getByRole('searchbox', { name: 'Filter sessions' });
    await filter.fill('writers');
    await row(page, /Map the writers/).waitFor();
    check(await row(page, /Hop carries subagents to Codex/).count() === 1, 'sessions: a match lost its parent');
    check(await row(page, /Chat ledger paging/).count() === 0, 'sessions: the filter kept a row that does not match');
    await page.getByText(/2 match|1 match/).first().waitFor();
    await filter.fill('');
    await row(page, /Chat ledger paging/).waitFor();
    // The details: the work on disk (the three repositories of a hosted chat are below), the lineage, the facts.
    await row(page, /Hop carries subagents to Codex/).click();
    await page.getByRole('term').filter({ hasText: 'Folder' }).first().waitFor();
    await page.getByText('develop · origin/develop · clean').waitFor();
    await page.getByRole('tree', { name: 'Lineage' }).waitFor();
    // An unreadable session says why, and its actions are off.
    await row(page, /no title/).click();
    await page.getByRole('alert').filter({ hasText: 'Cannot be read' }).waitFor();
    check(await page.getByRole('button', { name: 'Open transcript' }).isDisabled(), 'sessions: an unreadable session can be opened');
    check(await tree(page).count() === 1, 'sessions: the tree is gone');
    await page.screenshot({ path: `${out}sessions-dark.png` });
    await page.close();
  }

  // --- the reader beside the list, its tabs, the live re-read --------------------------------------------------------------
  {
    const page = await open('reader');
    await row(page, /Hop carries subagents to Codex/).click();
    await page.keyboard.press('Enter');
    const log = page.getByRole('log', { name: 'Transcript' });
    await log.waitFor();
    await page.getByText('3 earlier turns not shown').waitFor();
    check(await tree(page).count() === 1, 'reader: the list is gone while reading');
    await page.screenshot({ path: `${out}reader-dark.png` });
    // Raw shows the native records of an entry; the whole is one press away.
    await page.getByRole('button', { name: 'Raw', exact: true }).click();
    await page.getByRole('combobox', { name: 'Native records of' }).waitFor();
    await page.getByRole('region', { name: 'Native records' }).waitFor();
    await page.getByRole('button', { name: 'Conversation', exact: true }).click();
    await log.waitFor();
    // All turns: the earlier ones are read, and the note goes.
    await page.getByRole('button', { name: 'All', exact: true }).click();
    await page.getByText('3 earlier turns not shown').waitFor({ state: 'detached' });
    // A live session is read again: the status row says so, and no notice does (a state, not a result).
    await page.getByRole('status').filter({ hasText: /Live: read again/ }).first().waitFor({ timeout: 25000 });
    check(await notice(page, 'Live: read again').count() === 0, 'reader: a live re-read showed a notice');
    // Escape closes the reader; the session stays selected.
    await page.keyboard.press('Escape');
    await log.waitFor({ state: 'detached' });
    await page.getByRole('heading', { name: /Hop carries subagents to Codex/ }).waitFor();
    // The divider between the list and the reader is a splitter the keys move.
    await page.keyboard.press('Enter');
    await log.waitFor();
    const divider = page.getByRole('separator', { name: 'Resize the sessions list' });
    const start = Number(await divider.getAttribute('aria-valuenow'));
    await divider.focus();
    await page.keyboard.press('ArrowRight');
    check(Number(await divider.getAttribute('aria-valuenow')) === start + 16, 'reader: the arrow did not move the divider');
    await divider.dblclick();
    check(Number(await divider.getAttribute('aria-valuenow')) === 420, 'reader: a double press did not put the divider back');
    await page.close();
  }

  // --- a narrow window: one pane at a time, the way back, the drawers ----------------------------------------------------
  {
    const page = await open('narrow', { width: 520, height: 700 });
    await row(page, /Hop carries subagents to Codex/).click();
    await row(page, /Hop carries subagents to Codex/).press('Enter');
    await page.getByRole('log', { name: 'Transcript' }).waitFor();
    check(!(await tree(page).isVisible()), 'narrow: the list stayed beside the reader');
    await page.getByRole('region', { name: 'Transcript' }).getByRole('button', { name: 'Sessions', exact: true }).click();
    await tree(page).waitFor();
    check(await page.getByRole('log', { name: 'Transcript' }).count() === 0, 'narrow: the reader stayed after the way back');
    await page.close();
  }

  // --- the hop: the plan, Cancel, the receipt ------------------------------------------------------------------------------
  {
    const page = await open('hop');
    await row(page, /Hop carries subagents to Codex/).click();
    await page.keyboard.press('h');
    const hop = page.getByRole('dialog', { name: 'Hop' });
    await hop.waitFor();
    await hop.getByText('Plan · nothing is written yet').waitFor();
    check(await hop.getByRole('radio').count() >= 2, 'hop: the destinations are not radios');
    // Enter on Cancel cancels: it never starts the hop (a write).
    await hop.getByRole('button', { name: 'Cancel', exact: true }).focus();
    await page.keyboard.press('Enter');
    await hop.waitFor({ state: 'detached' });
    await page.keyboard.press('h');
    await hop.waitFor();
    // The plan follows the choice: native compaction wants the window of the destination's model, and holds the button back.
    await hop.getByRole('button', { name: 'native', exact: true }).click();
    await hop.getByText('the context window is needed').waitFor();
    check(await hop.getByRole('button', { name: /^Hop to/ }).isDisabled(), 'hop: native without a context window can start');
    await hop.getByRole('textbox', { name: /Context window/ }).fill('200000');
    await hop.getByText('200000 tokens').waitFor();
    await hop.getByRole('button', { name: 'none', exact: true }).click();
    // A destination's shell is one select; More options opens in place.
    await hop.getByRole('button', { name: 'More options' }).click();
    await hop.getByRole('textbox', { name: 'Destination model' }).waitFor();
    await page.screenshot({ path: `${out}hop-dark.png` });
    await hop.getByRole('button', { name: /^Hop to/ }).click();
    const done = page.getByRole('dialog', { name: 'Done' });
    await done.getByText('Losses · 2').waitFor();
    await page.screenshot({ path: `${out}receipt-dark.png` });
    check(await notice(page, 'hop done').count() > 0, 'hop: the receipt was not told in a notice');
    await done.getByRole('button', { name: 'Close', exact: true }).first().click();
    await done.waitFor({ state: 'detached' });
    await page.close();
  }

  // --- a hosted chat: its work, and starting another harness in its folder -------------------------------------------------
  {
    const page = await open('hosted');
    await page.getByRole('button', { name: /^Codex/ }).first().click();
    await row(page, /Capire lo stack/).click();
    await page.getByRole('status').filter({ hasText: /Hosted by ChatGPT/ }).first().waitFor().catch(() => {});
    await page.getByText('Hosted by ChatGPT').first().waitFor();
    await page.getByText('feature/stack-detection · no upstream · 3 commits on no remote · 133 uncommitted').waitFor();
    check(await page.getByText('at risk').count() === 2, 'hosted: two repositories should be at risk');
    await page.screenshot({ path: `${out}hosted-dark.png` });
    await page.getByRole('button', { name: /^Start in/ }).click();
    const dialog = page.getByRole('dialog', { name: 'Start in…' });
    const note = dialog.getByRole('textbox', { name: /Note/ });
    await note.waitFor();
    check((await note.inputValue()).includes('Capire lo stack di Daodan'), 'hosted: the note does not name the session');
    await page.keyboard.press('Escape');
    await dialog.waitFor({ state: 'detached' });
    await page.getByRole('button', { name: /^Start in/ }).click();
    await dialog.getByRole('button', { name: 'Start', exact: true }).click();
    await page.getByText(/Started Claude Code in C:\\Users\\dev\\Documents\\ChatGPT\\Daodan; note saved to /).first().waitFor();
    await dialog.waitFor({ state: 'detached' });
    await page.close();
  }

  // --- the chat: the feed, the graph, the composer, the rooms, the members ----------------------------------------------
  {
    const page = await open('chat');
    await page.keyboard.press('c');
    await page.getByRole('log', { name: 'Events of the room' }).waitFor();
    await page.getByRole('button', { name: 'claude-a' }).first().waitFor();
    check(await page.getByRole('navigation', { name: 'Rooms' }).count() + await page.getByText('hop-review').count() > 0, 'chat: the rooms are not in the sidebar');
    await page.screenshot({ path: `${out}chat-feed-dark.png` });
    // The graph: who wrote to whom, with its key; the replay is a slider.
    await page.keyboard.press('2');
    await page.getByRole('application', { name: /Who wrote to whom/ }).or(page.getByLabel(/Who wrote to whom/)).first().waitFor();
    await page.getByText('signal done').first().waitFor();
    const replay = page.getByRole('slider', { name: 'Replay position' });
    await replay.focus();
    await page.keyboard.press('Home');
    await page.getByText(/^1 \/ 11$/).waitFor();
    await page.screenshot({ path: `${out}chat-graph-dark.png` });
    await page.keyboard.press('1');
    // The window joins and writes as the user; the message comes back from the service.
    await page.getByRole('textbox', { name: 'Name' }).waitFor();
    await page.getByRole('button', { name: 'Join', exact: true }).click();
    const message = page.getByRole('textbox', { name: 'Message' });
    await message.waitFor();
    await message.fill('Smoke check: @codex-b is the receipt done?');
    await page.keyboard.press('Enter');
    await page.getByText('Smoke check', { exact: false }).first().waitFor();
    check((await message.inputValue()) === '', 'chat: the message was not cleared once sent');
    await page.screenshot({ path: `${out}chat-composer-dark.png` });
    // A new room, from its key; closing it is at once, with Undo; deleting asks and names what goes.
    await page.keyboard.press('Escape');
    await page.keyboard.press('n');
    const room = page.getByRole('dialog', { name: 'New room' });
    await room.getByRole('textbox', { name: 'Name' }).fill('smoke-room');
    await page.keyboard.press('Enter');
    await page.getByRole('button', { name: /^smoke-room/ }).first().waitFor();
    await page.getByRole('button', { name: 'More about the room' }).click();
    await page.getByRole('menuitem', { name: 'Close the room' }).click();
    await notice(page, 'Closed smoke-room').waitFor();
    await page.getByRole('status', { name: 'Notices' }).getByRole('button', { name: 'Undo' }).first().click();
    await notice(page, 'Reopened smoke-room').waitFor();
    await page.getByRole('button', { name: 'More about the room' }).click();
    await page.getByRole('menuitem', { name: 'Close the room' }).click();
    await notice(page, 'Closed smoke-room').waitFor();
    await page.getByRole('button', { name: 'More about the room' }).click();
    await page.getByRole('menuitem', { name: 'Delete the room…' }).click();
    const confirm = page.getByRole('alertdialog').or(page.getByRole('dialog', { name: /Delete smoke-room/ }));
    await confirm.getByRole('button', { name: 'Delete smoke-room', exact: true }).waitFor();
    check(await confirm.getByRole('button', { name: 'Cancel' }).count() === 1, 'chat: no Cancel in the delete dialog');
    await confirm.getByRole('button', { name: 'Cancel' }).click();
    // What is being typed in the filter survives the 3 s poll.
    const filter = page.getByRole('searchbox', { name: 'Filter the messages by text' });
    await filter.click();
    await page.keyboard.type('sub');
    await page.waitForTimeout(3400);
    check((await filter.inputValue()) === 'sub', 'chat: the filter lost what was typed');
    await filter.fill('');
    // The service stopped: the view offers to start it, and the sidebar is off.
    await page.getByRole('button', { name: 'More about the room' }).click();
    await page.getByRole('menuitem', { name: 'Stop the service…' }).click();
    await page.getByRole('button', { name: 'Stop the service', exact: true }).click();
    await page.getByRole('button', { name: 'Start the service' }).waitFor();
    check(await page.getByRole('navigation', { name: 'Rooms' }).count() === 0, 'chat: the rooms stayed while the service is stopped');
    await page.getByRole('button', { name: 'Start the service' }).click();
    await page.getByRole('log', { name: 'Events of the room' }).waitFor({ timeout: 10000 });
    await page.close();
  }

  // --- the members: the inspector docked, a drawer at 900 --------------------------------------------------------------
  {
    const page = await open('members', { width: 1440 });
    await page.keyboard.press('c');
    const members = page.getByRole('list', { name: 'Members' });
    await members.waitFor();
    await members.getByRole('button', { name: /codex-b/ }).click();
    await page.getByRole('term').filter({ hasText: 'Harness' }).first().waitFor();
    await page.setViewportSize({ width: 900, height: 800 });
    await page.waitForTimeout(300);
    check(!(await members.isVisible()), 'members: open at 900 before they were asked for');
    await page.getByRole('button', { name: 'Members', exact: true }).click();
    await members.waitFor();
    await page.keyboard.press('Escape');
    await members.waitFor({ state: 'hidden' });
    await page.close();
  }

  // --- search everything, the harnesses, the settings, the keys ---------------------------------------------------------
  {
    const page = await open('search');
    await page.keyboard.press('Control+Shift+f');
    const search = page.getByRole('dialog', { name: 'Search everything' });
    await search.getByRole('searchbox', { name: 'Search everything' }).fill('codex');
    await search.getByRole('list', { name: 'Members' }).waitFor();
    check(await search.getByRole('button').count() > 3, 'search: no results');
    await page.screenshot({ path: `${out}search-dark.png` });
    // A result opens its place: a session in the tree.
    await search.getByRole('searchbox', { name: 'Search everything' }).fill('ledger');
    await search.getByRole('button', { name: /Chat ledger paging/ }).waitFor();
    await page.keyboard.press('Enter');
    await search.waitFor({ state: 'detached' });
    await page.getByRole('heading', { name: /Chat ledger paging/ }).waitFor();
    // The harnesses: every harness, and a scan of the computer that finds a portable one.
    await page.keyboard.press('m');
    await page.getByRole('region', { name: 'Claude Code' }).waitFor();
    check(await page.getByRole('navigation', { name: 'Installations and folders' }).count() === 0, 'harnesses: the page has a sidebar');
    await page.getByRole('button', { name: 'Scan this computer' }).click();
    await notice(page, 'System scan done').waitFor({ timeout: 20000 });
    await page.getByRole('list', { name: 'More installations' }).getByText('JanuraCode').first().waitFor();
    // A kept folder is forgotten at once, and the notice's Undo keeps it again.
    await page.getByRole('button', { name: 'Forget folder' }).first().click();
    await notice(page, 'Forgot the folder').waitFor();
    await page.getByRole('status', { name: 'Notices' }).getByRole('button', { name: 'Undo' }).first().click();
    await page.getByRole('button', { name: 'Forget folder' }).first().waitFor();
    await page.screenshot({ path: `${out}harnesses-dark.png` });
    // Install a sandbox, forget it at once, and Undo lists it again.
    await page.getByRole('region', { name: 'Claude Code' }).getByRole('button', { name: 'Install…' }).click();
    const install = page.getByRole('dialog', { name: 'Install Claude Code' });
    await install.getByRole('button', { name: 'Install in a sandbox' }).click();
    await notice(page, 'Claude Code installed in a sandbox').waitFor({ timeout: 15000 });
    await install.waitFor({ state: 'detached' });
    await page.getByRole('button', { name: /^Sandbox try$/ }).first().click();
    await page.getByRole('menuitem', { name: 'Forget sandbox' }).click();
    await notice(page, 'Sandbox try is no longer listed').waitFor();
    await page.getByRole('status', { name: 'Notices' }).getByRole('button', { name: 'Undo' }).first().click();
    await page.getByRole('button', { name: /^Sandbox try$/ }).first().waitFor();
    // Settings are a dialog of the palette, not a view: the theme is the palette's, and a browser has no glass.
    await palette(page, 'Settings');
    const settings = page.getByRole('dialog', { name: 'Settings' });
    await settings.getByText('Appearance').first().waitFor();
    check(await settings.getByRole('switch').count() === 0, 'settings: a browser shows the glass switch');
    check(await page.getByRole('button', { name: 'Settings', exact: true }).count() === 0, 'settings: it is a view of the bar');
    await page.keyboard.press('Escape');
    await settings.waitFor({ state: 'detached' });
    // The keys: Shift+? lists them.
    await page.keyboard.press('Shift+?');
    await page.getByRole('dialog', { name: 'Keys' }).waitFor();
    await page.keyboard.press('Escape');
    await page.close();
  }

  // --- the look is the suite's one: no look to choose, the palette has light, dark and the system's ----------------------
  {
    const page = await open('look', { scheme: 'dark' });
    await page.keyboard.press('Control+k');
    const commands = page.getByRole('combobox', { name: /command/i });
    await commands.fill('look');
    check(await page.getByRole('option', { name: /ONI|Smooth/i }).count() === 0, 'look: the palette offers a look of Mukade\'s own');
    await commands.fill('light');
    await page.keyboard.press('Enter');
    await page.waitForFunction(() => document.documentElement.dataset.theme === 'light' || document.documentElement.classList.contains('light') || localStorage.getItem('mukade.theme') === 'light');
    await page.screenshot({ path: `${out}sessions-light.png` });
    await page.close();
  }

} catch (error) {
  let at = 0;
  for (const page of pages) await page.screenshot({ path: `${out}failed-${at++}.png` }).catch(() => {});
  failures.push(String(error?.stack ?? error).split('\n').slice(0, 12).join('\n'));
} finally {
  await browser.close();
  await server.close();
}
if (failures.length) {
  console.error(failures.join('\n'));
  process.exit(1);
}
console.log(`smoke ok: screenshots in ${out}`);
