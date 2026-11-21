// The window: wires the views (sidebar, sessions, detail, reader, chat, dialogs) to the commands
// and the keyboard (spec §2, §4).

import '@acaprino/imago/css/imago.css';
import { parseLook } from '@acaprino/imago/shell';
import { claimKey, closeLayers, createRegistry, footerKeys, formatKeys, hasModal, inspector, keyCaps, keyClaimed, mountApp, openMenu, roving, sidebarResize, splitView } from '@acaprino/imago/ui';
import './styles.css';
import { api, errorText, onCloseBlocked, onFilesDropped } from './api.ts';
import { ChatView } from './chat.ts';
import { boundTo, chatIndex } from './chatindex.ts';
import { inRoomsBox, renderDetail } from './detail.ts';
import { button, caps, commandLine, fill, h } from './dom.ts';
import { canStartIn, openHandoff } from './handoff.ts';
import { openHop, openImport, recoverWrite, type JobHooks } from './hop.ts';
import { VIEWS, writeFrame } from './frame.ts';
import { COMMANDS, FOOTER, type Place } from './keys.ts';
import { LOOK_KEY, THEME_KEY, storedTheme, type Look, type Theme } from './look.ts';
import { confirmModal, modalOpen, openModal } from './modal.ts';
import { Reader } from './reader.ts';
import { HarnessesView } from './harnesses.ts';
import { openSearch } from './search.ts';
import { renderSessions, visiblePlaces } from './sessions.ts';
import { renderSidebar } from './sidebar.ts';
import { currentInstallation, selectedRow, state, type Column } from './state.ts';
import { toTranscript } from './transcript.ts';
import type { SessionRow } from './types.ts';
import { ancestors } from './tree.ts';
import { initializeWindow, paintWindow } from './window.ts';


const $ = (id: string) => document.getElementById(id)!;
const registry = createRegistry();
const frame = writeFrame($('app'));
const sessionInspector = inspector($('app'));
const columns = { installations: frame.sidebar.body, sessions: frame.views.sessions, reader: frame.views.reader, detail: frame.detail };

/** Under 1000 the inspector is a drawer over the content: its drawn position says so (the package owns the breakpoint). */
const isDrawer = (): boolean => getComputedStyle($('app').querySelector<HTMLElement>('.im-inspector')!).position === 'absolute';
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
  const failed = state.job === null && state.statusError;
  const text = state.job ?? state.status;
  frame.footer.text.textContent = text;
  frame.footer.dot.dataset.imTone = state.job !== null ? 'info' : failed ? 'err' : 'ok';
  frame.footer.counts.textContent = state.counts;
  frame.status.chip.dataset.imTone = state.job !== null ? 'info' : failed ? 'err' : 'ok';
  frame.status.chip.textContent = state.job !== null ? 'Working' : failed ? 'Error' : 'Ready';
  frame.status.text.textContent = text;
  const place = state.view === 'chat' ? 'chat' : state.view === 'harnesses' ? 'harnesses' : state.reader ? 'reader' : 'sessions';
  const keys = footerKeys(document, registry, FOOTER[place]);
  keys.id = 'status-right';
  $('status-right').replaceWith(keys);
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
  const focused = columns.installations.querySelector('[role="listbox"]:focus');
  renderSidebar(columns.installations, {
    pickInstallation: (key) => void pickInstallation(key),
    pickFolder: (cwd) => pickFolder(cwd),
    focusSessions: () => focusColumn('sessions'),
  });
  if (focused) {
    const lists = [...columns.installations.querySelectorAll<HTMLElement>('[role="listbox"]')];
    const replacement = lists.find(list => list.getAttribute('aria-label') === focused.getAttribute('aria-label'));
    replacement?.focus({ preventScroll: true });
  }
}

function drawSessions(): void {
  const hadFocus = columns.sessions.contains(document.activeElement);
  renderSessions(columns.sessions, {
    pick: (id) => { pickSession(id); if (isDrawer()) focusColumn('detail'); },
    reload: () => void loadInventory(),
    retry: (key) => void pickInstallation(key, true),
    open: (id) => { pickSession(id); void openReader(); },
    fold: (id) => fold(id),
  });
  if (hadFocus && !columns.sessions.contains(document.activeElement)) {
    const target = columns.sessions.querySelector<HTMLElement>('[role="listbox"], [data-im-state-layer] button');
    (target ?? columns.sessions).focus({ preventScroll: true });
  }
}

function syncSidebar(): void {
  const visible = state.view === 'sessions';
  const toggle = document.querySelector<HTMLButtonElement>('.im-sidebar-toggle')!;
  if (!visible && $('app').dataset.imSidebarOpen === 'true') toggle.click();
  $('app').dataset.imSidebar = visible ? 'on' : 'off';
  $('sidebar').hidden = !visible;
  toggle.hidden = !visible;
}

// The list and the reader stand side by side in the content region (splitView, the width kept under mukade.width.sessions);
// where the inspector is a drawer the window is narrow, and the reader takes the place of the list while it is open.
const SESSIONS_WIDTH = 'mukade.width.sessions';
let split: ReturnType<typeof splitView> | null = null;

function storedWidth(key: string): number | null {
  const stored = localStorage.getItem(key);
  return stored && /^\d+(?:\.\d+)?px$/.test(stored) ? Number.parseFloat(stored) : null;
}

function unsplit(): void {
  if (!split) return;
  split.destroy();
  split.element.replaceWith();
  frame.views.sessionsView.append(frame.views.sessions, frame.views.reader);
  split = null;
}

function mountSplit(fraction?: number): void {
  const view = frame.views.sessionsView;
  const width = view.clientWidth;
  let saved: number | null = null;
  try { saved = storedWidth(SESSIONS_WIDTH); } catch { /* the default fraction stands */ }
  const start = fraction ?? (saved && width ? Math.min(0.8, Math.max(0.2, saved / width)) : 0.5);
  split = splitView(frame.views.sessions, frame.views.reader, {
    label: 'Resize the sessions list', fraction: start, minFirst: 180, minSecond: 240,
    onCommit: () => {
      try { localStorage.setItem(SESSIONS_WIDTH, `${Math.round(frame.views.sessions.getBoundingClientRect().width)}px`); } catch { /* the resize still applies */ }
    },
  });
  split.element.classList.add('mk-split');
  const handle = split.element.querySelector<HTMLElement>('[role="separator"]')!;
  handle.id = 'resize-sessions';
  handle.title = 'Drag or use arrow keys to resize · double-click to reset';
  handle.addEventListener('dblclick', () => {
    try { localStorage.removeItem(SESSIONS_WIDTH); } catch { /* the reset still applies */ }
    unsplit();
    mountSplit(0.5);
    layoutSessions();
  });
  view.append(split.element);
}

function layoutSessions(): void {
  const reading = state.reader !== null;
  const drawer = isDrawer();
  if (reading && !drawer) {
    if (!split) mountSplit();
    frame.views.sessions.hidden = false;
    frame.views.reader.hidden = false;
    return;
  }
  unsplit();
  frame.views.sessions.hidden = reading;
  frame.views.reader.hidden = !reading;
}

function drawDetail(): void {
  syncSidebar();
  layoutSessions();
  if (state.reader) {
    const r = state.reader;
    reader.render(r.transcript, {
      tab: r.tab, turns: r.turns, defaultTurns: state.defaultTurns, title: r.transcript?.title ?? selectedRow()?.title ?? r.id,
      error: r.error, raw: rawRecords, rawEntry: r.rawEntry,
    });
    if (!columns.reader.contains(reader.root)) fill(columns.reader, reader.root);
  } else if (columns.reader.firstChild) {
    columns.reader.replaceChildren();
  }
  renderDetail(columns.detail, {
    close: () => closeDetail(),
    open: () => void openReader(),
    hop: () => startHop(),
    exportTo: (saveAs) => void exportSelected(saveAs),
    resume: () => { const row = selectedRow(); if (row && state.installation !== null) void resume(state.installation, row.id); },
    startIn: () => { const row = selectedRow(); if (row) startIn(row); },
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
  if (!row || !installation) return;
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
  const visible = isDrawer() ? state.focus === 'detail' : !state.detailClosed;
  if (visible && !sessionInspector.isOpen()) sessionInspector.open(columns.detail);
  if (!visible && sessionInspector.isOpen()) sessionInspector.close();
}

/** The close button of the detail: a drawer goes away and the list takes the focus; the docked inspector stays closed until
 *  another session is picked. */
function closeDetail(): void {
  if (!isDrawer()) state.detailClosed = true;
  state.focus = 'sessions';
  syncInspector();
  focusColumn('sessions');
  drawFocus();
}

function drawFocus(): void {
  syncInspector();
  const focused: Record<Column, HTMLElement> = { installations: columns.installations, folders: columns.installations, sessions: columns.sessions, reader: columns.reader, detail: columns.detail };
  for (const column of Object.values(columns)) column.classList.toggle('focused', column === focused[state.focus]);
  for (const pane of columns.installations.querySelectorAll<HTMLElement>('[data-pane]')) {
    pane.classList.toggle('focused', pane.dataset.pane === state.focus);
  }
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
  drawFocus();
}

// --- loading ------------------------------------------------------------------------------------

let listing = 0;
let sessionListing = 0;

/** Read every store again, with every folder scanned so far (`state.scan`). */
async function loadInventory(): Promise<void> {
  const ticket = ++listing;
  state.inventoryRead = { kind: 'loading' };
  setJob(state.scan.length ? `scanning ${state.scan.join(', ')} for installations…` : 'reading every store…');
  if (!currentInstallation()) drawSessions();
  try {
    const list = await api.installations(state.scan);
    if (ticket !== listing) return; // a newer reload replaced this one
    const before = new Set(state.installations.map((item) => item.key));
    state.inventoryRead = { kind: 'ready' };
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
    else {
      ++sessionListing;
      state.installation = null;
      state.sessionsRead = { kind: 'ready' };
      state.rows = [];
      state.selected = null;
      state.folder = null;
      state.collapsed.clear();
      closeReader(false);
      if (state.focus === 'detail') state.focus = 'sessions';
      drawAll();
    }
  } catch (error) {
    if (ticket !== listing) return;
    setJob(null);
    const message = `reading the stores failed: ${errorText(error)}`;
    state.inventoryRead = { kind: 'error', message };
    setStatus(message, true);
    drawSessions();
  }
}

async function pickInstallation(key: string, keepSelection = false): Promise<boolean> {
  if (!state.installations.some(item => item.key === key)) return false;
  const ticket = ++sessionListing;
  const changed = key !== state.installation;
  const previousError = state.sessionsRead.kind === 'error' ? state.sessionsRead.message : null;
  state.installation = key;
  if (changed) {
    state.folder = null;
    state.collapsed.clear();
    state.rows = [];
    state.selected = null;
  }
  if (changed || !keepSelection) closeReader(false);
  state.sessionsRead = { kind: 'loading' };
  drawAll();
  try {
    const rows = await api.sessions(key);
    if (ticket !== sessionListing) return false;
    state.rows = rows;
    state.sessionsRead = { kind: 'ready' };
    if (state.statusError && state.status === previousError) setStatus('ready');
    if (!rows.some(row => row.id === state.selected)) state.selected = visiblePlaces()[0]?.row.id ?? null;
    if (state.reader && state.reader.id !== state.selected) closeReader(false);
  } catch (error) {
    if (ticket !== sessionListing) return false;
    const message = errorText(error);
    state.sessionsRead = { kind: 'error', message };
    state.rows = [];
    state.selected = null;
    closeReader(false);
    setStatus(message, true);
  }
  if (!state.selected && state.focus === 'detail') setFocus('sessions');
  drawAll();
  return state.sessionsRead.kind === 'ready';
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
  state.detailClosed = false;
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
  if (!await pickInstallation(key)) return false;
  if (!state.rows.some((row) => row.id === id)) { setStatus(`${id} is not listed in ${currentInstallation()?.label}`, true); return false; }
  state.folder = null;
  state.query = '';
  ($('search') as HTMLInputElement).value = '';
  for (const parent of ancestors(state.rows, id)) state.collapsed.delete(parent);
  state.selected = id;
  drawAll();
  focusColumn('sessions');
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
  state.focus = 'reader';
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
    drawDetail();
    focusColumn('sessions');
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
  let records: unknown[];
  try {
    const index = r.transcript?.entries.find((entry) => entry.id === entryId)?.index ?? 0;
    const page = await api.raw(r.key, r.id, index, 1);
    records = page.items[0]?.raw ?? [];
  } catch (error) {
    records = [{ error: errorText(error) }];
  }
  if (state.reader !== r || r.rawEntry !== entryId) return;
  rawRecords = records;
  drawDetail();
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
      button(['Import it… ', caps('I')], { onclick: () => { modal.close(); importDocument(); } }),
      button(['Close ', caps('Escape')], { class: 'primary', onclick: () => modal.close() }));
  } catch (error) {
    setJob(null);
    setStatus(`export failed: ${errorText(error)}`, true);
  }
}

function importDocument(): void {
  openImport(state.installations, state.installation, state.lastExport, hooks);
}

function startIn(row: SessionRow): void {
  openHandoff(row, (result, label) => setStatus(`Started ${label} in ${result.cwd}; note saved to ${result.note_path}`));
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
      fill(modal.foot, h('span', { class: 'spring' }), button(['Close ', caps('Escape')], { onclick: () => modal.close() }), copy);
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
  showInstallation: (key) => { switchView('sessions'); focusColumn('sessions'); void pickInstallation(key); },
  status: setStatus,
});

function switchView(view: 'sessions' | 'chat' | 'harnesses'): void {
  if (state.view === view) return;
  const leavingDrawer = $('app').dataset.imSidebarOpen === 'true';
  const moveFocus = leavingDrawer || $(`view-${state.view}`).contains(document.activeElement)
    || $('sidebar').contains(document.activeElement);
  state.view = view;
  syncSidebar();
  if (leavingDrawer) setFocus('sessions');
  $('view-sessions').hidden = view !== 'sessions';
  $('view-chat').hidden = view !== 'chat';
  $('view-harnesses').hidden = view !== 'harnesses';
  if (view === 'harnesses') harnesses.show();
  for (const tab of document.querySelectorAll<HTMLElement>('.im-tab[data-id]')) {
    const current = tab.dataset.id === view;
    tab.setAttribute('aria-selected', String(current));
    tab.tabIndex = current ? 0 : -1;
    if (current) tab.setAttribute('aria-current', 'page'); else tab.removeAttribute('aria-current');
  }
  frame.main.setAttribute('aria-label', VIEWS.find((one) => one.id === view)!.label);
  if (view === 'chat') chat.show(); else chat.hide();
  if (moveFocus) {
    if (view === 'sessions') focusColumn(state.reader ? 'reader' : 'sessions');
    else $(`view-${view}`).focus();
  }
  drawStatus();
}

const stored = (key: string): string | null => { try { return localStorage.getItem(key); } catch { return null; } };
const keep = (key: string, value: string): void => { try { localStorage.setItem(key, value); } catch { /* private window: the choice is not kept */ } };
let look: Look = parseLook(stored(LOOK_KEY));
const themes: Record<Look, Theme> = { oni: storedTheme('oni', stored(THEME_KEY.oni)), smooth: storedTheme('smooth', stored(THEME_KEY.smooth)) };
const systemLight = matchMedia('(prefers-color-scheme: light)');
function applyLook(): void {
  paintWindow(look, themes[look]);
}

/** The look and the theme are picked from the palette (`mountApp`'s `appearance`): Mukade keeps the choice, a theme for each look. */
const appearance = {
  get: () => ({ look, theme: themes[look] }),
  set: (next: { look: Look; theme: Theme }): void => {
    look = next.look;
    themes[look] = next.theme;
    keep(LOOK_KEY, look);
    keep(THEME_KEY[look], themes[look]);
    applyLook();
  },
};

// --- keys ---------------------------------------------------------------------------------------

const ORDER: Column[] = ['installations', 'folders', 'sessions', 'reader', 'detail'];

function focusColumn(column: Column): void {
  if (column === 'installations' || column === 'folders') {
    const toggle = document.querySelector<HTMLButtonElement>('.im-sidebar-toggle')!;
    if (toggle.getClientRects().length && toggle.getAttribute('aria-expanded') !== 'true') toggle.click();
    const pane = columns.installations.querySelector(`[data-pane="${column}"]`);
    const list = pane?.querySelector<HTMLElement>('[aria-activedescendant]') ?? pane?.querySelector<HTMLElement>('[role="listbox"]');
    list?.focus();
  } else {
    setFocus(column);
    // The list holds the focus (one tab stop, its own keys); the column itself when it has none (an empty state).
    (column === 'sessions' ? columns.sessions.querySelector<HTMLElement>('[role="listbox"]') ?? columns.sessions
      : column === 'reader' ? (state.reader ? reader.root.querySelector<HTMLElement>('.reader-scroll') ?? columns.reader : columns.sessions)
      : columns[column]).focus();
  }
}

function column(step: number): void {
  const order = ORDER.filter((one) => one !== 'reader' || state.reader);
  const index = order.indexOf(state.focus);
  const next = order[Math.max(0, Math.min(order.length - 1, index + step))];
  if (next === 'detail' && !state.selected) return;
  focusColumn(next);
}

/** Escape, when no layer, field or list of the package answered it: leave the field, go back in the chat, in the reader (Raw,
 *  then Conversation, then the list), or one column. */
function back(): void {
  const active = document.activeElement;
  if (active instanceof HTMLInputElement || active instanceof HTMLTextAreaElement) { active.blur(); return; }
  if (state.view === 'chat') { chat.column(-1); return; }
  if (state.reader) { if (state.reader.tab === 'raw') { state.reader.tab = 'conversation'; drawDetail(); } else closeReader(); return; }
  column(-1);
}

/** Ctrl+F: the filter of the current view. The harnesses page has none: it opens the search of everything. */
function filterView(): void {
  if (state.view === 'chat') chat.focusFilter();
  else if (state.view === 'harnesses') searchEverything();
  else ($('search') as HTMLInputElement).focus();
}

function showHelp(): void {
  const modal = openModal({ title: 'Keys', subtitle: 'mouse works everywhere too' });
  const bound = registry.list().filter((command) => command.keys);
  const lists: [string, string][] = [
    ['↑ ↓ · j k', 'move in the focused list, scroll the reader'], ['Home · End · Page Up · Page Down', 'first · last · by ten in a list; page in the reader'],
    ['Enter', 'open the transcript of the selected session'], ['Space', 'fold a parent session; in the chat, play the room'],
    ['Esc', 'close the top layer, leave a field, close the reader, go back'], ['F6 · Shift+F6', 'next · previous region'], ['Ctrl+Enter', 'chat: send'],
  ];
  fill(modal.body, h('dl', { class: 'kv keys' },
    ...bound.flatMap((command) => [h('dt', {}, formatKeys(command.keys!).join(' ')), h('dd', {}, command.title)]),
    ...lists.flatMap(([key, what]) => [h('dt', {}, key), h('dd', {}, what)])));
}

// --- start --------------------------------------------------------------------------------------

const SIDEBAR_WIDTH = 'mukade.width.installations';
let stopSidebarWidth: (() => void) | null = null;

/** The sidebar is resized by Imago's separator; Mukade keeps the width (a saved "240px" reads as 240) and forgets it on a double click. */
function mountSidebarWidth(): void {
  const shell = $('app');
  stopSidebarWidth?.();
  let saved: number | undefined;
  try { saved = storedWidth(SIDEBAR_WIDTH) ?? undefined; } catch { /* the token width stands */ }
  if (saved !== undefined) shell.style.setProperty('--im-w-sidebar', `${saved}px`);
  stopSidebarWidth = sidebarResize(shell, {
    label: 'Resize the sidebar',
    width: saved,
    onChange: (width) => { try { localStorage.setItem(SIDEBAR_WIDTH, `${Math.round(width)}px`); } catch { /* the resize still applies */ } },
  });
  const handle = shell.querySelector<HTMLElement>('.im-sidebar-splitter')!;
  handle.id = 'resize-installations';
  handle.title = 'Drag or use arrow keys to resize · double-click to reset';
  handle.addEventListener('dblclick', () => {
    try { localStorage.removeItem(SIDEBAR_WIDTH); } catch { /* the reset still applies */ }
    mountSidebarWidth();
  });
}

/** Where each command is available (keys.ts says which place a command is for). */
const WHEN: Record<Place, () => boolean> = {
  always: () => true,
  session: () => state.view === 'sessions' && Boolean(selectedRow()),
  reader: () => state.view === 'sessions' && Boolean(state.reader),
  chat: () => state.view === 'chat',
  room: () => state.view === 'chat' && chat.hasRoom(),
};

function runnerOf(id: string): () => void | Promise<void> {
  switch (id) {
    case 'search-everything': return searchEverything;
    case 'sessions': return () => switchView('sessions');
    case 'chat': return () => switchView(state.view === 'chat' ? 'sessions' : 'chat');
    case 'harnesses': return () => switchView(state.view === 'harnesses' ? 'sessions' : 'harnesses');
    case 'scan': return scanFolder;
    case 'import': return importDocument;
    case 'reload': return loadInventory;
    case 'hop': return startHop;
    case 'export': return () => exportSelected(false);
    case 'export-as': return () => exportSelected(true);
    case 'resume': return () => { const row = selectedRow(); if (row && state.installation !== null) return resume(state.installation, row.id); };
    case 'start-in': return () => { const row = selectedRow(); if (row && canStartIn(row)) startIn(row); };
    case 'open-reader': return () => openReader();
    case 'turns': return toggleTurns;
    case 'reader-top': return () => reader.top();
    case 'reader-bottom': return () => reader.bottom();
    case 'chat-graph': return () => chat.setView('graph');
    case 'chat-lanes': return () => chat.setView('lanes');
    case 'chat-feed': return () => chat.setView('feed');
    case 'new-room': return () => chat.newRoomDialog();
    case 'launch': return () => chat.launchDialog();
    case 'play': return () => chat.togglePlay();
    case 'start-service': return () => chat.startService();
    case 'stop-service': return () => chat.stopService();
    case 'back': return back;
    case 'keys': return showHelp;
    default: throw new Error(`no runner for the command ${id}`);
  }
}

/** The key caps of a button come from the registry: `data-command` names the command. */
function drawCaps(): void {
  for (const element of document.querySelectorAll<HTMLElement>('[data-command]')) {
    element.querySelector(':scope > .im-keys, :scope > kbd')?.remove();
    const keys = registry.get(element.dataset.command!)?.keys;
    if (keys) element.append(keyCaps(document, keys));
  }
}

function wire(): void {
  const shell = $('app');
  roving(shell.querySelector<HTMLElement>('.im-tabs')!, { item: '[role="tab"]', orientation: 'horizontal', onSelect: tab => tab.click() });
  // Escape in the detail: a drawer closes; the docked inspector stays and the focus goes back to the list. The detail's own
  // handler runs before the inspector's, which closes on any Escape it finds unclaimed.
  columns.detail.addEventListener('keydown', event => {
    if (event.key === 'Escape' && !event.isComposing && !hasModal() && !keyClaimed(event)) {
      event.preventDefault();
      claimKey(event);
      if (isDrawer()) closeDetail(); else focusColumn(state.reader ? 'reader' : 'sessions');
    }
  });
  columns.installations.addEventListener('focusin', event => {
    const pane = (event.target as HTMLElement).closest<HTMLElement>('[data-pane]')?.dataset.pane;
    if (pane === 'installations' || pane === 'folders') setFocus(pane);
  });
  columns.sessions.addEventListener('focusin', () => setFocus('sessions'));
  columns.detail.addEventListener('focusin', () => setFocus('detail'));
  columns.reader.addEventListener('focusin', () => setFocus('reader'));

  const views = ['sessions', 'chat', 'harnesses'] as const;
  for (const spec of COMMANDS) registry.register({ id: spec.id, title: spec.title, keys: spec.keys, when: WHEN[spec.view], run: runnerOf(spec.id) });
  // Registered before mountApp, so the keys reach the registry before the guard refuses them to the browser.
  mountApp(shell, registry, {
    search: filterView,
    labels: { palette: 'Commands', search: 'Search in this view' },
    sidebar: { collapsed: stored('mukade.sidebar.collapsed') === 'true', onChange: value => keep('mukade.sidebar.collapsed', String(value)) },
    appearance,
  });
  drawCaps();
  const run = (id: string) => { if (!hasModal()) void registry.get(id)?.run(); };
  $('commands').addEventListener('click', () => run('imago.palette'));
  $('views-menu').addEventListener('click', () => {
    if (!hasModal()) openMenu($('views-menu'), views.map(view => ({ label: registry.get(view)!.title, checked: () => state.view === view, run: () => { switchView(view); closeLayers(['menu']); } })));
  });
  for (const tab of document.querySelectorAll<HTMLElement>('.im-tab[data-id]')) tab.addEventListener('click', () => run(tab.dataset.id!));
  for (const [buttonId, commandId] of [['scan', 'scan'], ['import', 'import'], ['reload', 'reload'], ['search-everything', 'search-everything']]) {
    $(buttonId).addEventListener('click', () => run(commandId));
  }
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
  search.addEventListener('keydown', (event) => { if (event.key === 'Enter' && !event.isComposing) { event.preventDefault(); focusColumn('sessions'); } });
  columns.sessions.addEventListener('mousedown', () => setFocus('sessions'), { capture: true });
  columns.detail.addEventListener('mousedown', () => { if (state.selected) setFocus('detail'); });
  mountSidebarWidth();
  // The layout follows the window: the inspector turns into a drawer under 1000, and the reader then replaces the list.
  new ResizeObserver(() => { syncInspector(); layoutSessions(); }).observe(shell);
  // A document dropped on the window opens the import dialog with it.
  onFilesDropped((paths) => {
    if (modalOpen()) return;
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
  await recoverWrite(hooks).catch(error => setStatus(errorText(error), true));
})();
