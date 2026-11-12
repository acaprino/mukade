// The window: wires the views (sidebar, sessions, detail, reader, chat, dialogs) to the commands
// and the keyboard (spec §2, §4).

import './styles.css';
import { api, errorText, onCloseBlocked } from './api.ts';
import { ChatView } from './chat.ts';
import { renderDetail } from './detail.ts';
import { button, fill, h, kbd } from './dom.ts';
import { openHop, openImport, type JobHooks } from './hop.ts';
import { HINTS, actionFor, type Action } from './keys.ts';
import { stepIn } from './lists.ts';
import { closeModal, confirmModal, modalOpen, openModal } from './modal.ts';
import { Reader } from './reader.ts';
import { renderSessions, visiblePlaces } from './sessions.ts';
import { folderKey, renderSidebar } from './sidebar.ts';
import { currentInstallation, selectedRow, state, type Column } from './state.ts';
import { ancestors, folders } from './tree.ts';

const $ = (id: string) => document.getElementById(id)!;
const columns = { installations: $('installations'), sessions: $('sessions'), detail: $('detail') };
let rawRecords: unknown[] | null = null;

// --- status line --------------------------------------------------------------------------------

function setStatus(text: string, error = false): void {
  state.status = text;
  state.statusError = error;
  drawStatus();
}

function setJob(label: string | null): void {
  state.job = label;
  drawStatus();
}

function drawStatus(): void {
  const bar = $('statusbar');
  bar.classList.toggle('busy', state.job !== null);
  bar.classList.toggle('error', state.job === null && state.statusError);
  $('status').textContent = state.job ?? state.status;
  $('status-counts').textContent = state.counts;
  const hints = state.view === 'chat' ? HINTS.chat : state.reader ? HINTS.reader : HINTS.sessions;
  fill($('status-right'), ...hints.map(([key, what]) => h('span', {}, h('b', {}, key), ` ${what}`)));
}

const hooks: JobHooks = {
  job: setJob,
  status: setStatus,
  showSession: (key, id) => void revealSession(key, id, true),
  resumeIn: (key, id) => void resume(key, id),
};

// --- drawing ------------------------------------------------------------------------------------

const reader = new Reader({
  back: () => closeReader(),
  setTab: (tab) => { if (state.reader) { state.reader.tab = tab; drawDetail(); } },
  toggleTurns: () => void toggleTurns(),
  showRaw: (entryId) => void showRaw(entryId),
});

function drawSidebar(): void {
  renderSidebar(columns.installations, {
    pickInstallation: (key) => void pickInstallation(key),
    pickFolder: (cwd) => pickFolder(cwd),
    focus: (column) => setFocus(column),
  });
}

function drawSessions(): void {
  renderSessions(columns.sessions, {
    pick: (id) => pickSession(id),
    open: (id) => { pickSession(id); void openReader(); },
    fold: (id) => fold(id),
    focus: () => setFocus('sessions'),
  });
}

function drawDetail(): void {
  const view = $('view-sessions');
  view.classList.toggle('reading', state.reader !== null);
  view.classList.toggle('has-selection', state.selected !== null);
  if (state.reader) {
    const r = state.reader;
    reader.render(r.transcript, {
      tab: r.tab, turns: r.turns, defaultTurns: state.defaultTurns, title: r.transcript?.title ?? selectedRow()?.title ?? r.id,
      error: r.error, raw: rawRecords, rawEntry: r.rawEntry,
    });
    if (!columns.detail.contains(reader.root)) fill(columns.detail, reader.root);
    return;
  }
  renderDetail(columns.detail, {
    open: () => void openReader(),
    hop: () => startHop(),
    exportTo: (saveAs) => void exportSelected(saveAs),
    resume: () => { const row = selectedRow(); if (row && state.installation !== null) void resume(state.installation, row.id); },
    pick: (id) => pickSession(id),
  });
}

function drawFocus(): void {
  const focused: Record<Column, HTMLElement> = { installations: columns.installations, folders: columns.installations, sessions: columns.sessions, detail: columns.detail };
  for (const column of Object.values(columns)) column.classList.toggle('focused', column === focused[state.focus]);
  $('view-sessions').dataset.focus = state.focus === 'folders' ? 'installations' : state.focus;
}

function drawAll(): void {
  drawSidebar();
  drawSessions();
  drawDetail();
  drawFocus();
  drawStatus();
}

function setFocus(column: Column): void {
  if (state.focus === column) return;
  state.focus = column;
  drawSidebar();
  drawFocus();
}

// --- loading ------------------------------------------------------------------------------------

let listing = 0;

async function loadInventory(scan: string[], status?: string): Promise<void> {
  const ticket = ++listing;
  setJob(scan.length ? `scanning ${scan.join(', ')} for installations…` : 'reading every store…');
  try {
    const inventory = scan.length === 1 && status === 'scan' ? await api.scan(scan[0]) : await api.installations(scan);
    if (ticket !== listing) return; // a newer reload replaced this one
    state.installations = inventory.installations;
    const sessions = inventory.installations.reduce((sum, item) => sum + item.count, 0);
    state.counts = `${inventory.installations.length} installations · ${sessions} sessions`;
    setJob(null);
    setStatus(inventory.status);
    const keep = state.installation !== null && inventory.installations.some((item) => item.key === state.installation);
    const first = inventory.added[0] ?? (keep ? state.installation! : inventory.installations.find((item) => item.count > 0)?.key ?? 0);
    await pickInstallation(first, true);
  } catch (error) {
    if (ticket !== listing) return;
    setJob(null);
    setStatus(`reading the stores failed: ${errorText(error)}`, true);
  }
}

async function pickInstallation(key: number, keepSelection = false): Promise<void> {
  const changed = key !== state.installation;
  state.installation = key;
  if (changed) { state.folder = null; state.collapsed.clear(); }
  if (changed || !keepSelection) closeReader(false);
  try {
    state.rows = await api.sessions(key);
  } catch (error) {
    state.rows = [];
    setStatus(errorText(error), true);
  }
  if (state.installation !== key) return;
  if (!state.rows.some((row) => row.id === state.selected) || changed) state.selected = visiblePlaces()[0]?.row.id ?? null;
  drawAll();
}

function pickFolder(cwd: string | null): void {
  state.folder = cwd;
  const places = visiblePlaces();
  if (!places.some((place) => place.row.id === state.selected)) state.selected = places[0]?.row.id ?? null;
  closeReader(false);
  drawAll();
}

function pickSession(id: string): void {
  if (state.selected === id && !state.reader) return;
  state.selected = id;
  if (state.reader && state.reader.id !== id) closeReader(false);
  drawSessions();
  drawDetail();
}

function fold(id: string): void {
  if (state.collapsed.has(id)) state.collapsed.delete(id); else state.collapsed.add(id);
  drawSessions();
}

/** Show `id` of installation `key` in the tree (after a hop, or from a chat member). */
async function revealSession(key: number, id: string, reload: boolean): Promise<boolean> {
  if (state.view !== 'sessions') switchView('sessions');
  if (reload) await loadInventory([]);
  await pickInstallation(key);
  if (!state.rows.some((row) => row.id === id)) { setStatus(`${id} is not listed in ${currentInstallation()?.label}`, true); return false; }
  state.folder = null;
  state.query = '';
  ($('search') as HTMLInputElement).value = '';
  for (const parent of ancestors(state.rows, id)) state.collapsed.delete(parent);
  state.selected = id;
  state.focus = 'sessions';
  drawAll();
  return true;
}

// --- reader -------------------------------------------------------------------------------------

async function openReader(turns = state.defaultTurns): Promise<void> {
  const row = selectedRow();
  const key = state.installation;
  if (!row || key === null) return;
  if (row.unreadable) { setStatus(`${row.id} cannot be read: ${row.unreadable}`, true); return; }
  state.reader = { key, id: row.id, transcript: null, turns, tab: 'conversation', rawEntry: null, error: null };
  rawRecords = null;
  state.focus = 'detail';
  drawDetail();
  drawFocus();
  setJob(`reading ${row.title ?? row.id}…`);
  try {
    const transcript = await api.transcript(key, row.id, turns);
    if (state.reader?.id !== row.id) return;
    state.reader.transcript = transcript;
    const shown = transcript.total_turns - transcript.hidden_turns;
    setStatus(transcript.hidden_turns ? `showing the last ${shown} of ${transcript.total_turns} turns` : `${transcript.total_turns} turns`);
  } catch (error) {
    if (state.reader?.id !== row.id) return;
    state.reader.error = errorText(error);
    setStatus(`read failed: ${errorText(error)}`, true);
  } finally {
    setJob(null);
  }
  drawDetail();
  reader.focus();
}

function closeReader(draw = true): void {
  if (!state.reader) return;
  state.reader = null;
  rawRecords = null;
  if (draw) {
    state.focus = 'sessions';
    drawDetail();
    drawFocus();
    drawStatus();
  }
}

async function toggleTurns(): Promise<void> {
  if (!state.reader) return;
  await openReader(state.reader.turns === 0 ? Math.max(1, state.defaultTurns) : 0);
}

async function showRaw(entryId: string): Promise<void> {
  const r = state.reader;
  if (!r) return;
  r.tab = 'raw';
  r.rawEntry = entryId;
  rawRecords = null;
  drawDetail();
  try {
    rawRecords = await api.raw(r.key, r.id, entryId);
  } catch (error) {
    rawRecords = [{ error: errorText(error) }];
  }
  if (state.reader === r && r.rawEntry === entryId) drawDetail();
}

// --- actions ------------------------------------------------------------------------------------

function startHop(): void {
  const row = selectedRow();
  if (!row || state.installation === null) return void setStatus('no session selected', true);
  if (row.unreadable) return void setStatus(`${row.id} cannot be read: ${row.unreadable}`, true);
  void openHop(state.installation, row.id, hooks);
}

async function exportSelected(saveAs: boolean): Promise<void> {
  const row = selectedRow();
  const installation = currentInstallation();
  if (!row || !installation) return void setStatus('no session selected', true);
  let to: string | undefined;
  if (saveAs) {
    const picked = await api.pick('save', `${installation.harness}-${row.id.replace(/[^A-Za-z0-9._-]/g, '_')}.json`).catch((error) => { setStatus(errorText(error), true); return null; });
    if (!picked) return;
    to = picked;
  }
  setJob(`exporting ${row.title ?? row.id}…`);
  try {
    const exported = await api.export(installation.key, row.id, to);
    state.lastExport = exported.path;
    setJob(null);
    setStatus(`exported ${exported.messages} messages → ${exported.path}`);
    const modal = openModal({ title: 'Exported', subtitle: `${exported.messages} messages`, icon: 'export', tone: 'live' });
    fill(modal.body, h('div', { class: 'col' },
      h('dl', { class: 'kv' }, h('dt', {}, 'document'), h('dd', {}, exported.path), h('dt', {}, 'messages'), h('dd', {}, String(exported.messages))),
      h('p', { class: 'note' }, 'Import it into another installation with Import, or with ', h('code', {}, `mukade write <harness> "${exported.path}"`), '.')));
    fill(modal.foot, h('span', { class: 'spring' }),
      button(['Import it… ', kbd('i')], { onclick: () => { modal.close(); importDocument(); } }),
      button(['Close ', kbd('esc')], { class: 'primary', onclick: () => modal.close() }));
  } catch (error) {
    setJob(null);
    setStatus(`export failed: ${errorText(error)}`, true);
  }
}

function importDocument(): void {
  openImport(state.installations, state.installation, state.lastExport, hooks);
}

async function resume(key: number, id: string): Promise<void> {
  setJob('opening a terminal…');
  try {
    const result = await api.resume(key, id);
    setJob(null);
    if (result.refusal) {
      const modal = openModal({ title: 'Resume', subtitle: 'this harness has no terminal UI', icon: 'play' });
      const command = result.refusal.split(': ').slice(1).join(': ');
      fill(modal.body, h('div', { class: 'col' }, h('p', { class: 'note' }, result.refusal),
        command ? h('div', { class: 'cmd' }, h('pre', { class: 'code' }, command)) : null));
      const copy = button('Copy the command', { class: 'primary', onclick: async () => { await navigator.clipboard.writeText(command || result.refusal!).catch(() => {}); copy.textContent = 'Copied'; } });
      fill(modal.foot, h('span', { class: 'spring' }), button(['Close ', kbd('esc')], { onclick: () => modal.close() }), copy);
      setStatus('resume refused: no terminal UI');
    } else {
      setStatus(`resume → ${result.command.join(' ')}${result.cwd ? `  (in ${result.cwd})` : ''}`);
    }
  } catch (error) {
    setJob(null);
    setStatus(`resume failed: ${errorText(error)}`, true);
  }
}

async function scanFolder(): Promise<void> {
  const dir = await api.pick('folder').catch((error) => { setStatus(errorText(error), true); return null; });
  if (dir) await loadInventory([dir], 'scan');
}

// --- views and theme ----------------------------------------------------------------------------

const chat = new ChatView($('view-chat'), (harness, id) => {
  const installation = state.installations.find((item) => item.harness === harness && !item.scanned)
    ?? state.installations.find((item) => item.harness === harness);
  if (!installation) return false;
  void revealSession(installation.key, id, false);
  return true;
});

function switchView(view: 'sessions' | 'chat'): void {
  if (state.view === view) return;
  state.view = view;
  $('view-sessions').hidden = view !== 'sessions';
  $('view-chat').hidden = view !== 'chat';
  for (const tab of document.querySelectorAll<HTMLElement>('[data-view]')) tab.setAttribute('aria-selected', String(tab.dataset.view === view));
  // The chat view has its own filter, on its messages.
  $('search').hidden = view === 'chat';
  if (view === 'chat') chat.show(); else chat.hide();
  drawStatus();
}

const THEMES = ['auto', 'light', 'dark'] as const;
function applyTheme(theme: (typeof THEMES)[number]): void {
  if (theme === 'auto') delete document.documentElement.dataset.theme; else document.documentElement.dataset.theme = theme;
  $('theme-name').textContent = theme[0].toUpperCase() + theme.slice(1);
  $('theme').title = theme === 'auto' ? 'Theme: follows the system' : `Theme: ${theme}`;
  try { localStorage.setItem('mukade.theme', theme); } catch { /* private window: the theme is not kept */ }
}

// --- columns: widths kept, drawers when narrow --------------------------------------------------

function restoreWidths(): void {
  for (const name of ['installations', 'sessions']) {
    try {
      const width = localStorage.getItem(`mukade.width.${name}`);
      if (width) document.documentElement.style.setProperty(`--w-${name}`, width);
    } catch { /* not kept */ }
  }
}

function makeResizer(handle: HTMLElement, name: 'installations' | 'sessions'): void {
  handle.addEventListener('pointerdown', (event) => {
    const column = columns[name];
    const start = event.clientX;
    const width = column.getBoundingClientRect().width;
    handle.setPointerCapture(event.pointerId);
    const move = (e: PointerEvent) => {
      const next = Math.max(180, Math.min(900, width + e.clientX - start));
      document.documentElement.style.setProperty(`--w-${name}`, `${next}px`);
    };
    const up = () => {
      handle.removeEventListener('pointermove', move);
      try { localStorage.setItem(`mukade.width.${name}`, getComputedStyle(document.documentElement).getPropertyValue(`--w-${name}`)); } catch { /* not kept */ }
    };
    handle.addEventListener('pointermove', move);
    handle.addEventListener('pointerup', up, { once: true });
  });
  handle.addEventListener('dblclick', () => {
    document.documentElement.style.removeProperty(`--w-${name}`);
    try { localStorage.removeItem(`mukade.width.${name}`); } catch { /* not kept */ }
  });
}

// --- keys ---------------------------------------------------------------------------------------

function move(step: number): void {
  if (state.focus === 'installations') {
    const keys = state.installations.map((item) => String(item.key));
    const next = stepIn(keys, state.installation === null ? null : String(state.installation), step);
    if (next !== null) void pickInstallation(Number(next));
  } else if (state.focus === 'folders') {
    const list = folders(state.rows);
    const next = stepIn(list.map((f) => folderKey(f.cwd)), folderKey(state.folder), step);
    const folder = list.find((f) => folderKey(f.cwd) === next);
    if (folder) pickFolder(folder.cwd);
  } else if (state.focus === 'sessions') {
    const next = stepIn(visiblePlaces().map((place) => place.row.id), state.selected, step);
    if (next) pickSession(next);
  } else if (state.reader) {
    reader.scrollBy(step * 2);
  } else {
    columns.detail.querySelector('.column-body')?.scrollBy({ top: step * 40 });
  }
}

const ORDER: Column[] = ['installations', 'folders', 'sessions', 'detail'];

function column(step: number): void {
  const index = ORDER.indexOf(state.focus);
  const next = ORDER[Math.max(0, Math.min(ORDER.length - 1, index + step))];
  if (next === 'detail' && !state.selected) return;
  setFocus(next);
}

function act(action: Action, event: KeyboardEvent): boolean {
  if (action === 'search') {
    if (state.view === 'chat') chat.focusFilter(); else ($('search') as HTMLInputElement).focus();
    return true;
  }
  if (action === 'back') {
    if (closeModal()) return true;
    if (document.activeElement instanceof HTMLInputElement || document.activeElement instanceof HTMLTextAreaElement) { (document.activeElement as HTMLElement).blur(); return true; }
    if (state.view === 'chat') { chat.column(-1); return true; }
    if (state.reader) { if (state.reader.tab === 'raw') { state.reader.tab = 'conversation'; drawDetail(); } else closeReader(); return true; }
    column(-1);
    return true;
  }
  if (modalOpen()) return false;
  if (action === 'chat') { switchView(state.view === 'chat' ? 'sessions' : 'chat'); return true; }
  if (action === 'reload') { void loadInventory([]); return true; }
  if (action === 'scan') { void scanFolder(); return true; }
  if (action === 'import') { importDocument(); return true; }
  if (state.view === 'chat') {
    if (action === 'down' || action === 'up') { chat.move(action === 'down' ? 1 : -1); return true; }
    if (action === 'left' || action === 'right') { chat.column(action === 'right' ? 1 : -1); return true; }
    return false;
  }
  switch (action) {
    case 'down': case 'up': move(action === 'down' ? 1 : -1); return true;
    case 'left': if (!state.reader) column(-1); return !state.reader;
    case 'right': if (!state.reader) column(1); return !state.reader;
    case 'enter':
      if (event.target instanceof HTMLInputElement && event.target.id === 'search') { setFocus('sessions'); ($('search') as HTMLInputElement).blur(); return true; }
      if (state.focus === 'sessions' || state.focus === 'detail') { if (!state.reader) void openReader(); return true; }
      column(1);
      return true;
    case 'fold': if (state.focus === 'sessions' && state.selected) { fold(state.selected); return true; } return false;
    case 'hop': startHop(); return true;
    case 'export': void exportSelected(false); return true;
    case 'resume': { const row = selectedRow(); if (row && state.installation !== null) void resume(state.installation, row.id); return true; }
    case 'turns': if (state.reader) void toggleTurns(); return Boolean(state.reader);
    case 'top': if (state.reader) reader.top(); return Boolean(state.reader);
    case 'bottom': if (state.reader) reader.bottom(); return Boolean(state.reader);
    case 'pageUp': case 'pageDown':
      if (state.reader) reader.page(action === 'pageDown' ? 1 : -1); else move(action === 'pageDown' ? 10 : -10);
      return true;
    case 'help': showHelp(); return true;
    default: return false;
  }
}

function showHelp(): void {
  const modal = openModal({ title: 'Keys', subtitle: 'mouse works everywhere too' });
  const rows: [string, string][] = [
    ['↑ ↓ · j k', 'move in the focused list, scroll the reader'], ['← →', 'previous / next column'], ['⏎', 'next column, or open the transcript'],
    ['esc', 'close the reader or a dialog, or back one column'], ['h · e · i · R', 'hop · export · import · resume'], ['t', 'last turns ↔ all'],
    ['g G · PgUp PgDn', 'reader top · bottom · page'], ['o · r', 'scan a folder · reload'], ['/ · ctrl+f', 'filter'], ['c', 'chat ↔ sessions'], ['space', 'fold a parent'],
  ];
  fill(modal.body, h('dl', { class: 'kv keys' }, ...rows.flatMap(([key, what]) => [h('dt', {}, key), h('dd', {}, what)])));
}

document.addEventListener('keydown', (event) => {
  const target = event.target as HTMLElement;
  const inField = target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement || target instanceof HTMLSelectElement;
  // Inside a dialog only Escape is the window's; its own controls take the rest.
  if (modalOpen() && event.key !== 'Escape') return;
  const action = actionFor(event, inField);
  if (action && act(action, event)) event.preventDefault();
});

// --- start --------------------------------------------------------------------------------------

function wire(): void {
  for (const tab of document.querySelectorAll<HTMLElement>('[data-view]')) tab.addEventListener('click', () => switchView(tab.dataset.view as 'sessions' | 'chat'));
  $('scan').addEventListener('click', () => void scanFolder());
  $('import').addEventListener('click', () => importDocument());
  $('reload').addEventListener('click', () => void loadInventory([]));
  $('theme').addEventListener('click', () => {
    const current = (document.documentElement.dataset.theme ?? 'auto') as (typeof THEMES)[number];
    applyTheme(THEMES[(THEMES.indexOf(current) + 1) % THEMES.length]);
  });
  const search = $('search') as HTMLInputElement;
  search.addEventListener('input', () => {
    state.query = search.value;
    const places = visiblePlaces();
    if (!places.some((place) => place.row.id === state.selected)) state.selected = places[0]?.row.id ?? null;
    drawSessions();
    if (!state.reader) drawDetail();
  });
  search.addEventListener('focus', () => setFocus('sessions'));
  columns.sessions.addEventListener('mousedown', () => setFocus('sessions'));
  columns.detail.addEventListener('mousedown', () => { if (state.selected) setFocus('detail'); });
  makeResizer($('resize-installations'), 'installations');
  makeResizer($('resize-sessions'), 'sessions');
  for (const back of document.querySelectorAll<HTMLElement>('[data-back]')) back.addEventListener('click', () => column(-1));
  onCloseBlocked(async () => {
    if (await confirmModal('A hop is still running', 'Closing now stops the window while it writes. The destination may be left with a session the receipt never reports.', 'Close anyway')) void api.quit();
  });
}

(async () => {
  let theme: (typeof THEMES)[number] = 'auto';
  try { theme = (localStorage.getItem('mukade.theme') as typeof theme) || 'auto'; } catch { /* default */ }
  applyTheme(THEMES.includes(theme) ? theme : 'auto');
  restoreWidths();
  wire();
  drawAll();
  try {
    const info = await api.appInfo();
    $('version').textContent = info.version;
    state.defaultTurns = info.turns;
    state.scan = info.scan;
  } catch (error) {
    setStatus(errorText(error), true);
  }
  await loadInventory(state.scan);
  setFocus('sessions');
})();
