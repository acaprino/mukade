// The window: wires the views (sidebar, sessions, detail, reader, chat, dialogs) to the commands
// and the keyboard (spec §2, §4).

import '@acaprino/imago/css/imago.css';
import { parseLook } from '@acaprino/imago/shell';
import { attachLayers, createRegistry, hasModal, inspector, openMenu, openPalette, regions, roving, sidebarCollapse, sidebarDrawer, topLayer } from '@acaprino/imago/ui';
import './styles.css';
import { api, errorText, onCloseBlocked, onFilesDropped } from './api.ts';
import { ChatView } from './chat.ts';
import { boundTo, chatIndex } from './chatindex.ts';
import { inRoomsBox, renderDetail } from './detail.ts';
import { button, commandLine, fill, h, kbd } from './dom.ts';
import { openHop, openImport, segmented, type JobHooks } from './hop.ts';
import { HINTS, actionFor, type Action } from './keys.ts';
import { stepIn } from './lists.ts';
import { LOOK_KEY, THEMES, THEME_KEY, nextTheme, storedTheme, type Look, type Theme } from './look.ts';
import { closeModal, confirmModal, modalOpen, openModal } from './modal.ts';
import { Reader } from './reader.ts';
import { HarnessesView } from './harnesses.ts';
import { openSearch } from './search.ts';
import { renderSessions, visiblePlaces } from './sessions.ts';
import { folderKey, renderSidebar } from './sidebar.ts';
import { currentInstallation, selectedRow, state, type Column } from './state.ts';
import { toTranscript } from './transcript.ts';
import { ancestors, folders } from './tree.ts';
import { initializeWindow, paintWindow } from './window.ts';


const $ = (id: string) => document.getElementById(id)!;
const registry = createRegistry();
const sessionInspector = inspector($('app'));
const narrowSessions = matchMedia('(max-width: 999.98px)');
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
  const hints = state.view === 'chat' ? HINTS.chat : state.view === 'harnesses' ? HINTS.harnesses : state.reader ? HINTS.reader : HINTS.sessions;
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
  retry: () => void openReader(state.reader?.turns),
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
    pick: (id) => { pickSession(id); if (narrowSessions.matches) setFocus('detail'); },
    reload: () => void loadInventory(),
    open: (id) => { pickSession(id); void openReader(); },
    fold: (id) => fold(id),
    focus: () => setFocus('sessions'),
  });
}

function syncSidebar(): void {
  const visible = state.view === 'sessions' && !state.reader;
  $('app').dataset.imSidebar = visible ? 'on' : 'off';
  $('session-sidebar').hidden = !visible;
  document.querySelector<HTMLElement>('.im-sidebar-toggle')!.hidden = !visible;
}

function drawDetail(): void {
  syncSidebar();
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
  void drawInRooms();
}

let roomsTicket = 0;
/** The rooms where a member is bound to the selected session, under its details. */
async function drawInRooms(): Promise<void> {
  const ticket = ++roomsTicket;
  const row = selectedRow();
  const installation = currentInstallation();
  if (!row || !installation || state.reader) return;
  const index = await chatIndex().catch(() => null);
  const pad = columns.detail.querySelector('.pad');
  if (ticket !== roomsTicket || !index || !pad) return;
  const found = boundTo(index, installation.harness, row.id);
  pad.querySelector('.in-rooms')?.remove();
  if (found.length) pad.append(inRoomsBox(found, (roomId, memberId) => { chat.openRoom(roomId, { member: memberId }); switchView('chat'); }));
}

/** Search everything: rooms, members, messages, sessions. */
function searchEverything(): void {
  openSearch({
    installations: () => state.installations,
    openSession: (key, id) => void revealSession(key, id, false),
    openRoom: (roomId, at) => { chat.openRoom(roomId, at); switchView('chat'); },
  });
}

function syncInspector(): void {
  const visible = !narrowSessions.matches || state.focus === 'detail' || Boolean(state.reader);
  if (visible && !sessionInspector.isOpen()) sessionInspector.open(columns.detail);
  if (!visible && sessionInspector.isOpen()) sessionInspector.close();
}

function drawFocus(): void {
  syncInspector();
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

/** Read every store again, with every folder scanned so far (`state.scan`). */
async function loadInventory(): Promise<void> {
  const ticket = ++listing;
  setJob(state.scan.length ? `scanning ${state.scan.join(', ')} for installations…` : 'reading every store…');
  if (!currentInstallation()) drawSessions();
  try {
    const list = await api.installations(state.scan);
    if (ticket !== listing) return; // a newer reload replaced this one
    const before = new Set(state.installations.map((item) => item.key));
    state.installations = list.installations;
    const sessions = list.installations.reduce((sum, item) => sum + item.count, 0);
    state.counts = `${list.installations.length} installations · ${sessions} sessions`;
    const notes = list.scans.filter((scan) => scan.added || scan.stopped_early)
      .map((scan) => `${scan.added} installation(s) in ${scan.dir}${scan.stopped_early ? ' (the scan stopped early: depth or folder budget)' : ''}`);
    const errors = list.installations.filter((item) => item.error).length;
    if (errors) notes.push(`${errors} store(s) could not be read`);
    setJob(null);
    setStatus(notes.join(' · ') || 'ready');
    const added = list.installations.find((item) => item.scanned && !before.has(item.key) && before.size > 0);
    const keep = list.installations.some((item) => item.key === state.installation);
    const first = added?.key ?? (keep ? state.installation! : list.installations.find((item) => item.count > 0)?.key ?? list.installations[0]?.key);
    if (first !== undefined) await pickInstallation(first, true);
    else drawAll();
  } catch (error) {
    if (ticket !== listing) return;
    setJob(null);
    setStatus(`reading the stores failed: ${errorText(error)}`, true);
    drawSessions();
  }
}

async function pickInstallation(key: string, keepSelection = false): Promise<void> {
  const changed = key !== state.installation;
  state.installation = key;
  if (changed) { state.folder = null; state.collapsed.clear(); }
  if (changed || !keepSelection) closeReader(false);
  let rows: typeof state.rows;
  try {
    rows = await api.sessions(key);
  } catch (error) {
    rows = [];
    if (state.installation === key) setStatus(errorText(error), true);
  }
  // A slower reply for an installation no longer selected must not replace the current rows.
  if (state.installation !== key) return;
  state.rows = rows;
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
async function revealSession(key: string, id: string, reload: boolean): Promise<boolean> {
  if (state.view !== 'sessions') switchView('sessions');
  if (reload) await loadInventory();
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
  const mine: NonNullable<typeof state.reader> = { key, id: row.id, transcript: null, turns, tab: 'conversation', rawEntry: null, error: null };
  state.reader = mine;
  rawRecords = null;
  state.focus = 'detail';
  drawDetail();
  drawFocus();
  setJob(`reading ${row.title ?? row.id}…`);
  try {
    const transcript = toTranscript(await api.transcript(key, row.id, turns), row.title ?? row.id);
    // An older read (the same session at another turn limit, say) must not replace a newer one.
    if (state.reader !== mine) return;
    mine.transcript = transcript;
    const shown = transcript.total_turns - transcript.hidden_turns;
    setStatus(transcript.hidden_turns ? `showing the last ${shown} of ${transcript.total_turns} turns` : `${transcript.total_turns} turns`);
  } catch (error) {
    if (state.reader !== mine) return;
    mine.error = errorText(error);
    setStatus(`read failed: ${errorText(error)}`, true);
  } finally {
    if (state.reader === mine) setJob(null);
  }
  if (state.reader !== mine) return;
  drawDetail();
  reader.focus();
  watchLive(mine);
}

let liveTimer = 0;
/** A live session open in the reader is read again when its file changes, checked every 3 s. */
function watchLive(mine: NonNullable<typeof state.reader>): void {
  window.clearInterval(liveTimer);
  liveTimer = 0;
  if (!selectedRow()?.live || mine.error) return;
  let stamp: string | null = null;
  let reading = false;
  liveTimer = window.setInterval(async () => {
    if (state.reader !== mine) { window.clearInterval(liveTimer); return; }
    if (reading || document.hidden || state.view !== 'sessions' || mine.tab !== 'conversation') return;
    reading = true;
    try {
      const now = await api.sessionStamp(mine.key, mine.id).catch(() => null);
      if (now === null || state.reader !== mine) return;
      // The first look is the file the reader has just read.
      if (stamp === null || now === stamp) { stamp = now; return; }
      stamp = now;
      const transcript = toTranscript(await api.transcript(mine.key, mine.id, mine.turns, true), mine.transcript?.title ?? mine.id);
      if (state.reader !== mine) return;
      mine.transcript = transcript;
      reader.keepPlace = true;
      try { drawDetail(); } finally { reader.keepPlace = false; }
      setStatus(`live: read again at ${new Date().toLocaleTimeString()}, ${transcript.total_turns} turns`);
    } catch {
      // The next look tries again.
    } finally {
      reading = false;
    }
  }, 3000);
}

function closeReader(draw = true): void {
  if (!state.reader) return;
  state.reader = null;
  window.clearInterval(liveTimer);
  liveTimer = 0;
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
    const index = r.transcript?.entries.find((entry) => entry.id === entryId)?.index ?? 0;
    const page = await api.raw(r.key, r.id, index, 1);
    rawRecords = page.items[0]?.raw ?? [];
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
    const modal = openModal({ title: 'Exported', subtitle: `${exported.messages} messages`, icon: 'export' });
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

async function resume(key: string, id: string): Promise<void> {
  setJob('opening a terminal…');
  try {
    const result = await api.resume(key, id);
    setJob(null);
    if (result.refusal) {
      const modal = openModal({ title: 'Resume', subtitle: 'this harness has no terminal UI', icon: 'play' });
      // The backend sends the command that continues it instead, as data.
      const command = commandLine(result.command);
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
  if (!dir) return;
  if (!state.scan.includes(dir)) state.scan.push(dir);
  keepFolders();
  await loadInventory();
  harnesses.draw();
}

// --- views and theme ----------------------------------------------------------------------------

const chat = new ChatView($('view-chat'), {
  showSession: (harness, id) => {
    const installation = state.installations.find((item) => item.harness === harness && !item.scanned)
      ?? state.installations.find((item) => item.harness === harness);
    if (!installation) return false;
    void revealSession(installation.key, id, false);
    return true;
  },
  status: setStatus,
});

/** The folders and stores scanned and kept across restarts (Scan folder, the system scan). */
const SCAN_KEY = 'mukade.scan';
function keptFolders(): string[] {
  try { const kept = JSON.parse(localStorage.getItem(SCAN_KEY) ?? '[]'); return Array.isArray(kept) ? kept.map(String) : []; } catch { return []; }
}
function keepFolders(): void {
  try { localStorage.setItem(SCAN_KEY, JSON.stringify(state.scan.filter((dir) => !startupScan.includes(dir)))); } catch { /* private window: not kept */ }
}
let startupScan: string[] = [];

const harnesses = new HarnessesView($('view-harnesses'), {
  installations: () => state.installations,
  folders: () => state.scan,
  addFolders: async (dirs) => {
    for (const dir of dirs) if (!state.scan.includes(dir)) state.scan.push(dir);
    keepFolders();
    await loadInventory();
    harnesses.draw();
  },
  forgetFolder: async (dir) => {
    state.scan = state.scan.filter((one) => one !== dir);
    keepFolders();
    await loadInventory();
    harnesses.draw();
  },
  reload: async () => { await loadInventory(); harnesses.draw(); },
  scanFolder: () => void scanFolder(),
  showInstallation: (key) => { switchView('sessions'); void pickInstallation(key).then(() => setFocus('sessions')); },
  status: setStatus,
});

function switchView(view: 'sessions' | 'chat' | 'harnesses'): void {
  if (state.view === view) return;
  state.view = view;
  syncSidebar();
  $('view-sessions').hidden = view !== 'sessions';
  $('view-chat').hidden = view !== 'chat';
  $('view-harnesses').hidden = view !== 'harnesses';
  if (view === 'harnesses') harnesses.show();
  for (const tab of document.querySelectorAll<HTMLElement>('[data-view]')) tab.setAttribute('aria-selected', String(tab.dataset.view === view));
  // The chat view has its own filter, on its messages; the harnesses page has none.
  $('search').hidden = view !== 'sessions';
  for (const content of [columns.sessions, $('view-chat'), $('view-harnesses')]) {
    content.classList.toggle('im-content', content === (view === 'sessions' ? columns.sessions : $(`view-${view}`)));
  }
  if (view === 'chat') chat.show(); else chat.hide();
  drawStatus();
}

const stored = (key: string): string | null => { try { return localStorage.getItem(key); } catch { return null; } };
const keep = (key: string, value: string): void => { try { localStorage.setItem(key, value); } catch { /* private window: the choice is not kept */ } };
let look: Look = parseLook(stored(LOOK_KEY));
const themes: Record<Look, Theme> = { oni: storedTheme('oni', stored(THEME_KEY.oni)), smooth: storedTheme('smooth', stored(THEME_KEY.smooth)) };
const systemLight = matchMedia('(prefers-color-scheme: light)');
function applyLook(): void {
  const theme = themes[look];
  paintWindow(look, theme);
  $('theme-name').textContent = theme[0].toUpperCase() + theme.slice(1);
  $('theme').title = `Theme: ${theme}`;
}

/** Settings: the look and its theme. Each pick applies and is kept at once. */
function openSettings(): void {
  const modal = openModal({ title: 'Settings', icon: 'settings' });
  const draw = (): void => {
    fill(modal.body, h('div', { class: 'col' },
      h('div', { class: 'field im-field' }, h('span', { class: 'lab' }, 'Look'),
        segmented('Look', ['smooth', 'oni'], look, (value) => { look = value; keep(LOOK_KEY, look); applyLook(); draw(); }),
        h('span', { class: 'muted hint' }, look === 'smooth' ? 'Clean and modern, the default.' : 'The look Mukade was born with.')),
      h('div', { class: 'field im-field' }, h('span', { class: 'lab' }, 'Theme'),
        segmented('Theme', THEMES[look], themes[look], (value) => { themes[look] = value; keep(THEME_KEY[look], value); applyLook(); draw(); }))));
    // The pick keeps the keyboard: the redraw made a new button of it.
    modal.body.querySelector<HTMLElement>('[aria-label="Look"] [aria-pressed="true"]')?.focus();
  };
  fill(modal.foot, h('span', { class: 'spring' }), button(['Close ', kbd('esc')], { class: 'primary', onclick: () => modal.close() }));
  draw();
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
    const next = stepIn(state.installations.map((item) => item.key), state.installation, step);
    if (next !== null) void pickInstallation(next);
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
  if (action === 'searchAll') { if (!modalOpen()) searchEverything(); return true; }
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
  if (action === 'harnesses') { switchView(state.view === 'harnesses' ? 'sessions' : 'harnesses'); return true; }
  if (action === 'reload') { void loadInventory(); return true; }
  if (action === 'scan') { void scanFolder(); return true; }
  if (action === 'import') { importDocument(); return true; }
  if (state.view === 'chat') {
    if (action === 'down' || action === 'up') { chat.move(action === 'down' ? 1 : -1); return true; }
    if (action === 'left' || action === 'right') { chat.column(action === 'right' ? 1 : -1); return true; }
    if (action === 'graph' || action === 'lanes' || action === 'feed') { chat.setView(action); return true; }
    if (action === 'fold') { chat.togglePlay(); return true; }
    if (action === 'newRoom') { chat.newRoomDialog(); return true; }
    if (action === 'launch') { chat.launchDialog(); return true; }
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
    ['g G · PgUp PgDn', 'reader top · bottom · page'], ['o · r', 'scan a folder · reload'], ['/ · ctrl+f', 'filter'], ['ctrl+k', 'search everything'], ['c', 'chat ↔ sessions'], ['m', 'harnesses ↔ sessions'], ['space', 'fold a parent; in the chat, play the room'],
    ['1 2 3', 'chat: graph · lanes · feed'], ['n · l', 'chat: new room · launch an agent'], ['ctrl+⏎', 'chat: send'],
  ];
  fill(modal.body, h('dl', { class: 'kv keys' }, ...rows.flatMap(([key, what]) => [h('dt', {}, key), h('dd', {}, what)])));
}

/** The WebView's own page commands, which a desktop window does not have (tauri-native-desktop):
 *  F5 and ctrl+R would reload the page and drop the window's state mid-hop, so they read the
 *  stores again instead, as `r` does; ctrl+P would print the page. Zoom (ctrl +/−) stays. */
function pageCommand(event: KeyboardEvent): boolean {
  const ctrl = event.ctrlKey || event.metaKey;
  const key = event.key.toLowerCase();
  if (event.key === 'F5' || (ctrl && key === 'r')) {
    if (!modalOpen()) void loadInventory();
    return true;
  }
  if (ctrl && (key === 'p' || key === 's' || key === 'o' || key === 'u' || key === 'j')) return true;
  if (!import.meta.env.DEV && (event.key === 'F12' || (ctrl && event.shiftKey && (key === 'i' || key === 'c')))) return true;
  return false;
}

document.addEventListener('keydown', (event) => {
  if (event.defaultPrevented || event.isComposing || topLayer()) return;
  if (event.key === 'Escape' && $('app').dataset.imSidebarOpen === 'true') return;
  if ((event.key === 'Enter' || event.key === ' ') && event.target instanceof Element && event.target.closest('button, summary, [role="tab"]')) return;
  const command = registry.match(event);
  if (command) {
    event.preventDefault();
    void Promise.resolve(command.run()).catch(error => setStatus(errorText(error), true));
    return;
  }
  if (pageCommand(event)) { event.preventDefault(); return; }
  const target = event.target as HTMLElement;
  const inField = target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement || target instanceof HTMLSelectElement;
  // Inside a dialog only Escape is the window's; its own controls take the rest.
  if (modalOpen() && event.key !== 'Escape') return;
  const action = actionFor(event, inField);
  if (action && act(action, event)) event.preventDefault();
});

// --- start --------------------------------------------------------------------------------------

function wire(): void {
  const shell = $('app');
  attachLayers(shell);
  regions(shell);
  roving(shell.querySelector<HTMLElement>('.im-tabs')!, { item: '[role="tab"]', orientation: 'horizontal', onSelect: tab => tab.click() });
  narrowSessions.addEventListener('change', syncInspector);
  columns.detail.addEventListener('keydown', event => {
    if (event.key === 'Escape' && !event.isComposing && !topLayer()) {
      event.preventDefault();
      act('back', event);
    }
  });
  sidebarDrawer(shell);
  sidebarCollapse(shell, { collapsed: stored('mukade.sidebar.collapsed') === 'true', onChange: value => keep('mukade.sidebar.collapsed', String(value)) });

  const views = ['sessions', 'chat', 'harnesses'] as const;
  for (const view of views) registry.register({ id: view, title: $(`tab-${view}`).textContent!.split(' ')[0].trim(), run: () => switchView(view) });
  registry.register({ id: 'search', keys: 'Ctrl+K', title: 'Search rooms, messages and sessions', run: searchEverything });
  registry.register({ id: 'scan', keys: 'O', title: 'Scan folder', run: scanFolder });
  registry.register({ id: 'import', keys: 'I', title: 'Import document', run: importDocument });
  registry.register({ id: 'reload', keys: 'R', title: 'Reload stores', run: loadInventory });
  registry.register({ id: 'settings', title: 'Look and theme', run: openSettings });
  registry.register({ id: 'keys', title: 'Keyboard shortcuts', run: showHelp });
  const hasSession = () => state.view === 'sessions' && Boolean(selectedRow());
  registry.register({ id: 'open-reader', title: 'Open transcript', when: hasSession, run: () => openReader() });
  registry.register({ id: 'hop', title: 'Hop to another installation', keys: 'H', when: hasSession, run: startHop });
  registry.register({ id: 'export', title: 'Export session', keys: 'E', when: hasSession, run: () => exportSelected(false) });
  registry.register({ id: 'new-room', title: 'Create chat room', keys: 'N', when: () => state.view === 'chat', run: () => chat.newRoomDialog() });
  const commands = () => { if (!hasModal()) openPalette(shell, registry); };
  registry.register({ id: 'commands', title: 'Commands', keys: 'Ctrl+Shift+K', run: commands });
  $('commands').addEventListener('click', commands);
  $('views-menu').addEventListener('click', () => {
    if (!hasModal()) openMenu($('views-menu'), views.map(view => ({ label: registry.get(view)!.title, run: () => switchView(view) })));
  });
  for (const tab of document.querySelectorAll<HTMLElement>('[data-view]')) tab.addEventListener('click', () => { if (!hasModal()) void registry.get(tab.dataset.view!)?.run(); });
  for (const [buttonId, commandId] of [['scan', 'scan'], ['import', 'import'], ['reload', 'reload'], ['settings', 'settings'], ['search-everything', 'search']]) {
    $(buttonId).addEventListener('click', () => { if (!hasModal()) void registry.get(commandId)!.run(); });
  }
  $('theme').addEventListener('click', () => {
    themes[look] = nextTheme(look, themes[look]);
    keep(THEME_KEY[look], themes[look]);
    applyLook();
  });
  systemLight.addEventListener('change', applyLook);
  const search = $('search') as HTMLInputElement;
  search.addEventListener('input', () => {
    state.query = search.value;
    const places = visiblePlaces();
    if (!places.some((place) => place.row.id === state.selected)) state.selected = places[0]?.row.id ?? null;
    drawSessions();
    if (!state.reader) drawDetail();
  });
  search.addEventListener('focus', () => setFocus('sessions'));
  columns.sessions.addEventListener('mousedown', () => setFocus('sessions'), { capture: true });
  columns.detail.addEventListener('mousedown', () => { if (state.selected) setFocus('detail'); });
  makeResizer($('resize-installations'), 'installations');
  makeResizer($('resize-sessions'), 'sessions');
  for (const back of document.querySelectorAll<HTMLElement>('[data-back]')) back.addEventListener('click', () => { if (state.reader) closeReader(); else setFocus('sessions'); });
  // A document dropped on the window opens the import dialog with it.
  onFilesDropped((paths) => {
    const document = paths.find((path) => path.toLowerCase().endsWith('.json'));
    if (!document) { setStatus('drop a mukade document (.json) to import it', true); return; }
    if (state.job) { setStatus('a job is running: drop the document again when it ends', true); return; }
    openImport(state.installations, state.installation, document, hooks);
  });
  onCloseBlocked(async () => {
    if (await confirmModal('A hop is still running', 'Closing now stops the window while it writes. The destination may be left with a session the receipt never reports.', 'Close anyway')) void api.quit();
  });
}

(async () => {
  applyLook();
  restoreWidths();
  wire();
  void initializeWindow((error) => setStatus(errorText(error), true));
  drawAll();
  try {
    const startup = await api.startup();
    $('version').textContent = startup.version;
    state.defaultTurns = startup.turns;
    startupScan = [...startup.scan];
    state.scan = [...new Set([...startup.scan, ...keptFolders()])];
  } catch (error) {
    setStatus(errorText(error), true);
  }
  await loadInventory();
  setFocus('sessions');
})();
