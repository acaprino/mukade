// What the page does with the stores: the installations and their sessions, the folder, the search, the transcript being read, the
// jobs the status row names, and the one dialog that is open. Plain TypeScript with no screen in it: the screens (src/screens) read
// it through `useStore` and ask it for changes, and it tells them when something changed. The chat and the harnesses are the
// controllers of their own (chatcontroller.ts, harnesscontroller.ts); this holds the other two beside the window's state, which
// `state.ts` defines and node tests.

import { api, errorText, onCloseBlocked, onFilesDropped } from './api.ts';
import { ChatController } from './chatcontroller.ts';
import { HarnessController } from './harnesscontroller.ts';
import { createState, currentInstallation, selectedRow, visiblePlaces, type ReaderState, type State, type ViewId } from './state.ts';
import { Store } from './store.ts';
import { toTranscript } from './transcript.ts';
import { ancestors } from './tree.ts';
import type { Exported, HarnessView, Installation, Resumed, SessionRow, Work } from './types.ts';
import { canStartIn, workCache } from './work.ts';

/** What the one open dialog is. A dialog holds its own form; the controller only says which one is open and what it was opened for. */
export type Dialog =
  | { kind: 'hop'; key: string; id: string }
  | { kind: 'import'; document: string | null; into: string | null }
  | { kind: 'recover' }
  | { kind: 'exported'; exported: Exported }
  | { kind: 'resume'; result: Resumed }
  | { kind: 'start-in'; key: string; row: SessionRow }
  | { kind: 'keys' }
  | { kind: 'search' }
  | { kind: 'settings' }
  | { kind: 'new-room' }
  | { kind: 'launch' }
  | { kind: 'install'; harness: HarnessView }
  | { kind: 'confirm'; title: string; description: string; confirmLabel: string; destructive?: boolean; run: () => void | Promise<void> };

/** What a notice may carry besides its words: an error stays until it is closed, and Undo takes the action back. */
export interface Say { error?: boolean; undo?: () => void | Promise<void> }

/** The work of the selected session's folder, read once per selection. */
export type WorkRead = { key: string; id: string } & ({ kind: 'loading' } | { kind: 'ready'; work: Work } | { kind: 'error'; message: string });

/** How long the details wait on a selection before they read its folder: moving down the list asks for no repository it does not stop on. */
const WORK_DELAY = 200;
/** A live session open in the reader is read again when its file changes, looked at this often. */
const LIVE_MS = 3000;
const SCAN_KEY = 'mukade.scan';

function keptFolders(): string[] {
  try { const kept = JSON.parse(localStorage.getItem(SCAN_KEY) ?? '[]'); return Array.isArray(kept) ? kept.map(String) : []; } catch { return []; }
}

export class Controller extends Store {
  readonly state: State = createState();
  readonly chat: ChatController;
  readonly harnesses: HarnessController;
  /** The one dialog that is open, or none. */
  dialog: Dialog | null = null;
  /** The work in the selected session's folder. */
  work: WorkRead | null = null;
  /** Says something in a notice (the window sets it: the shell's `useNotify`). */
  notice: (text: string, say?: Say) => void = () => {};
  /** Asks the window to draw the view it is told (its title bar is the shell's). */
  onView: (view: ViewId) => void = () => {};
  /** The transcript is drawn and may take the focus (the reader sets it). */
  focusReader: () => void = () => {};
  /** The inspector is docked beside the content (the width says so): the details open by themselves. Set by the inspectors as they draw. */
  docked = true;
  /** The list of sessions takes the focus on its selected row (the list sets it). */
  focusList: () => void = () => {};
  /** What the keys do to the conversation drawn (the reader sets it while it is on the page). */
  readerKeys: { top(): void; end(): void; by(pixels: number): void } | null = null;
  private startupScan: string[] = [];
  private listing = 0;
  private sessionListing = 0;
  private liveTimer = 0;
  private workToken = 0;
  /** A hop or import cannot start before the write left over from an earlier window was looked at. */
  private writeChecked = false;
  private started = false;

  constructor() {
    super();
    this.chat = new ChatController(this);
    this.harnesses = new HarnessController(this);
  }

  // --- the dialog ---------------------------------------------------------------------------------

  open(dialog: Dialog): void {
    this.dialog = dialog;
    this.changed();
  }

  close(): void {
    if (this.dialog === null) return;
    this.dialog = null;
    this.changed();
  }

  /** Asks before something that cannot be taken back; the window draws it as a `ConfirmDialog`. */
  confirm(title: string, description: string, confirmLabel: string, run: () => void | Promise<void>): void {
    this.open({ kind: 'confirm', title, description, confirmLabel, run });
  }

  // --- the status row -----------------------------------------------------------------------------

  /** A state of the window: the status row's text only. */
  summary(text: string): void {
    this.state.status = text;
    this.state.statusError = false;
    this.changed();
  }

  /** The result of something the person did: a notice (an error stays until it is closed; Undo takes the action back), then the status row until the next one. */
  report(text: string, error = false, undo?: Say['undo']): void {
    this.notice(text, { error, undo });
    this.state.status = text;
    this.state.statusError = error;
    this.changed();
  }

  setJob(label: string | null): void {
    this.state.job = label;
    this.changed();
  }

  // --- start --------------------------------------------------------------------------------------

  /** The first reads: the app's facts, every store, and a write an earlier window left running. */
  async start(): Promise<void> {
    if (this.started) return;
    this.started = true;
    onFilesDropped((paths) => this.dropped(paths));
    onCloseBlocked(() => this.confirm(
      'A hop is still running',
      'Closing now stops the window while it writes. The destination may be left with a session the receipt never reports.',
      'Close the window',
      () => { void api.quit(); },
    ));
    try {
      const startup = await api.startup();
      this.state.defaultTurns = startup.turns;
      this.startupScan = [...startup.scan];
      this.state.scan = [...new Set([...startup.scan, ...keptFolders()])];
      this.programVersion = startup.version;
    } catch (error) {
      this.report(errorText(error), true);
    }
    await this.loadInventory();
    await this.recoverWrite();
  }

  /** The version of the program, for About. */
  programVersion = '';

  /** The write that the window of an earlier run left running is shown again, so that no new write starts before it is told. */
  private async recoverWrite(): Promise<void> {
    try {
      const current = await api.writeStatus();
      if (current) {
        this.setJob(`${current.kind === 'import' ? 'Import' : 'Hop'} in progress…`);
        this.open({ kind: 'recover' });
      }
    } catch (error) {
      this.report(errorText(error), true);
    } finally {
      this.writeChecked = true;
      this.changed();
    }
  }

  /** Whether a hop or import may start. */
  get canWrite(): boolean { return this.writeChecked; }

  /** A document dropped on the window opens the import dialog with it. */
  private dropped(paths: string[]): void {
    if (this.dialog !== null) return;
    const document = paths.find((path) => path.toLowerCase().endsWith('.json'));
    if (!document) { this.report('Drop a mukade document (.json) to import it.', true); return; }
    if (this.state.job) { this.report('A job is running: drop the document again when it ends.', true); return; }
    this.importDocument(document);
  }

  // --- views --------------------------------------------------------------------------------------

  get view(): ViewId { return this.state.view; }

  switchView(view: ViewId): void {
    if (this.state.view === view) return;
    this.state.view = view;
    this.onView(view);
    this.changed();
  }

  /** The window's own choice of a view, from its title bar or its palette. */
  chooseView(view: ViewId): void {
    this.switchView(view);
    if (view === 'harnesses') void this.harnesses.show();
  }

  // --- loading ------------------------------------------------------------------------------------

  /** Read every store again, with every folder scanned so far (`state.scan`). */
  async loadInventory(): Promise<void> {
    const state = this.state;
    const ticket = ++this.listing;
    state.inventoryRead = { kind: 'loading' };
    this.setJob(state.scan.length ? `Scanning ${state.scan.join(', ')} for installations…` : 'Reading every store…');
    try {
      const list = await api.installations(state.scan);
      if (ticket !== this.listing) return; // a newer reload replaced this one
      const before = new Set(state.installations.map((item) => item.key));
      state.inventoryRead = { kind: 'ready' };
      state.installations = list.installations;
      const sessions = list.installations.reduce((sum, item) => sum + item.count, 0);
      state.counts = `${list.installations.length} installations · ${sessions} sessions`;
      const notes = list.scans.filter((scan) => scan.added || scan.stopped_early)
        .map((scan) => `${scan.added} installation(s) in ${scan.dir}${scan.stopped_early ? ' (the scan stopped early: depth or folder budget)' : ''}`);
      const errors = list.installations.filter((item) => item.error).length;
      if (errors) notes.push(`${errors} store(s) could not be read`);
      this.setJob(null);
      this.summary(notes.join(' · ') || 'Ready');
      const added = list.installations.find((item) => item.scanned && !before.has(item.key) && before.size > 0);
      const keep = list.installations.some((item) => item.key === state.installation);
      const first = added?.key ?? (keep ? state.installation! : list.installations.find((item) => item.count > 0)?.key ?? list.installations[0]?.key);
      if (first !== undefined) await this.pickInstallation(first, true);
      else {
        ++this.sessionListing;
        state.installation = null;
        state.sessionsRead = { kind: 'ready' };
        state.rows = [];
        state.selected = null;
        state.folder = null;
        state.collapsed.clear();
        this.closeReader();
      }
    } catch (error) {
      if (ticket !== this.listing) return;
      this.setJob(null);
      const message = `Reading the stores failed: ${errorText(error)}`;
      state.inventoryRead = { kind: 'error', message };
      this.report(message, true);
    }
    this.changed();
  }

  async pickInstallation(key: string, keepSelection = false): Promise<boolean> {
    const state = this.state;
    if (!state.installations.some((item) => item.key === key)) return false;
    const ticket = ++this.sessionListing;
    const changed = key !== state.installation;
    const previousError = state.sessionsRead.kind === 'error' ? state.sessionsRead.message : null;
    state.installation = key;
    if (changed) {
      state.folder = null;
      state.collapsed.clear();
      state.rows = [];
      state.selected = null;
    }
    if (changed || !keepSelection) this.closeReader();
    state.sessionsRead = { kind: 'loading' };
    this.changed();
    try {
      const rows = await api.sessions(key);
      if (ticket !== this.sessionListing) return false;
      state.rows = rows;
      state.sessionsRead = { kind: 'ready' };
      if (state.statusError && state.status === previousError) this.summary('Ready');
      if (!rows.some((row) => row.id === state.selected)) state.selected = visiblePlaces(state)[0]?.row.id ?? null;
      if (state.reader && state.reader.id !== state.selected) this.closeReader();
    } catch (error) {
      if (ticket !== this.sessionListing) return false;
      const message = errorText(error);
      state.sessionsRead = { kind: 'error', message };
      state.rows = [];
      state.selected = null;
      this.closeReader();
      this.report(message, true);
    }
    this.changed();
    return state.sessionsRead.kind === 'ready';
  }

  pickFolder(cwd: string | null): void {
    const state = this.state;
    state.folder = cwd;
    const places = visiblePlaces(state);
    if (!places.some((place) => place.row.id === state.selected)) state.selected = places[0]?.row.id ?? null;
    this.closeReader();
    this.changed();
  }

  /** The search of the list: the text typed, and a selection that is still in the list. */
  setQuery(query: string): void {
    const state = this.state;
    state.query = query;
    const places = visiblePlaces(state);
    if (!places.some((place) => place.row.id === state.selected)) state.selected = places[0]?.row.id ?? null;
    this.changed();
  }

  pickSession(id: string): void {
    const state = this.state;
    if (state.selected === id) return;
    state.detailOpen = null;
    state.selected = id;
    if (state.reader && state.reader.id !== id) this.closeReader();
    this.changed();
  }

  /** The tree opened or closed branches: the ones it no longer lists as open are folded. */
  setCollapsed(collapsed: Set<string>): void {
    this.state.collapsed = collapsed;
    this.changed();
  }

  /** The details of the selected session open or close: the person's choice, until another session is picked. */
  setDetailOpen(open: boolean | null): void {
    this.state.detailOpen = open;
    this.changed();
  }

  /** Show `id` of installation `key` in the tree (after a hop, or from a chat member). */
  async revealSession(key: string, id: string, reload: boolean): Promise<boolean> {
    const state = this.state;
    this.switchView('sessions');
    if (reload) await this.loadInventory();
    if (!await this.pickInstallation(key)) return false;
    if (!state.rows.some((row) => row.id === id)) { this.report(`${id} is not listed in ${currentInstallation(state)?.label}`, true); return false; }
    state.folder = null;
    state.query = '';
    for (const parent of ancestors(state.rows, id)) state.collapsed.delete(parent);
    state.selected = id;
    this.changed();
    return true;
  }

  // --- details ------------------------------------------------------------------------------------

  /** Reads the work of the selected session's folder (once per selection; a folder read a moment ago is not read again). */
  readWork(key: string, row: SessionRow): void {
    if (this.work && this.work.key === key && this.work.id === row.id) return;
    const token = ++this.workToken;
    const folder = row.cwd ?? '';
    const cached = workCache.get(folder, Date.now());
    if (cached) {
      this.work = { key, id: row.id, kind: 'ready', work: cached };
      return;
    }
    this.work = { key, id: row.id, kind: 'loading' };
    const settle = (next: { kind: 'ready'; work: Work } | { kind: 'error'; message: string }): void => {
      // The selection moved on: this answer is for a row no longer shown.
      if (this.workToken !== token) return;
      if (next.kind === 'ready') workCache.set(folder, next.work, Date.now());
      this.work = { key, id: row.id, ...next };
      this.changed();
    };
    setTimeout(() => {
      if (this.workToken !== token) return;
      api.work(key, row.id).then((work) => settle({ kind: 'ready', work }), (error) => settle({ kind: 'error', message: errorText(error) }));
    }, WORK_DELAY);
  }

  /** The selection has no folder: no work to read. */
  forgetWork(): void {
    this.workToken += 1;
    this.work = null;
  }

  // --- reader -------------------------------------------------------------------------------------

  async openReader(turns = this.state.defaultTurns): Promise<void> {
    const state = this.state;
    const row = selectedRow(state);
    const key = state.installation;
    if (!row || key === null) return;
    if (row.unreadable) { this.report(`${row.id} cannot be read: ${row.unreadable}`, true); return; }
    const mine: ReaderState = { key, id: row.id, transcript: null, turns, tab: 'conversation', rawEntry: null, raw: null, error: null };
    state.reader = mine;
    this.changed();
    this.setJob(`Reading ${row.title ?? row.id}…`);
    try {
      const transcript = toTranscript(await api.transcript(key, row.id, turns), row.title ?? row.id);
      // An older read (the same session at another turn limit, say) must not replace a newer one.
      if (state.reader !== mine) return;
      mine.transcript = transcript;
      const shown = transcript.total_turns - transcript.hidden_turns;
      this.summary(transcript.hidden_turns ? `Showing the last ${shown} of ${transcript.total_turns} turns` : `${transcript.total_turns} turns`);
    } catch (error) {
      if (state.reader !== mine) return;
      mine.error = errorText(error);
      this.report(`Read failed: ${errorText(error)}`, true);
    } finally {
      if (state.reader === mine) this.setJob(null);
    }
    if (state.reader !== mine) return;
    this.changed();
    this.focusReader();
    this.watchLive(mine);
  }

  /** A live session open in the reader is read again when its file changes, looked at every 3 s. The reader keeps its place. */
  private watchLive(mine: ReaderState): void {
    window.clearInterval(this.liveTimer);
    this.liveTimer = 0;
    if (!selectedRow(this.state)?.live || mine.error) return;
    let stamp: string | null = null;
    let reading = false;
    this.liveTimer = window.setInterval(async () => {
      if (this.state.reader !== mine) { window.clearInterval(this.liveTimer); return; }
      if (reading || document.hidden || this.state.view !== 'sessions' || mine.tab !== 'conversation') return;
      reading = true;
      try {
        const now = await api.sessionStamp(mine.key, mine.id).catch(() => null);
        if (now === null || this.state.reader !== mine) return;
        // The first look is the file the reader has just read.
        if (stamp === null || now === stamp) { stamp = now; return; }
        stamp = now;
        const transcript = toTranscript(await api.transcript(mine.key, mine.id, mine.turns, true), mine.transcript?.title ?? mine.id);
        if (this.state.reader !== mine) return;
        mine.transcript = transcript;
        this.summary(`Live: read again at ${new Date().toLocaleTimeString()}, ${transcript.total_turns} turns`);
      } catch {
        // The next look tries again.
      } finally {
        reading = false;
      }
    }, LIVE_MS);
  }

  /** Closes the transcript; `focus` (the person closed it) gives the focus back to the list, where the transcript had it. */
  closeReader(focus = false): void {
    if (!this.state.reader) return;
    this.state.reader = null;
    window.clearInterval(this.liveTimer);
    this.liveTimer = 0;
    this.changed();
    if (focus) queueMicrotask(() => this.focusList());
  }

  async toggleTurns(): Promise<void> {
    const state = this.state;
    if (!state.reader) return;
    await this.openReader(state.reader.turns === 0 ? Math.max(1, state.defaultTurns) : 0);
  }

  setTab(tab: ReaderState['tab']): void {
    const reader = this.state.reader;
    if (!reader || reader.tab === tab) return;
    reader.tab = tab;
    this.changed();
  }

  async showRaw(entryId: string): Promise<void> {
    const reader = this.state.reader;
    if (!reader) return;
    reader.tab = 'raw';
    reader.rawEntry = entryId;
    reader.raw = null;
    this.changed();
    let records: unknown[];
    try {
      const index = reader.transcript?.entries.find((entry) => entry.id === entryId)?.index ?? 0;
      const page = await api.raw(reader.key, reader.id, index, 1);
      records = page.items[0]?.raw ?? [];
    } catch (error) {
      records = [{ error: errorText(error) }];
    }
    if (this.state.reader !== reader || reader.rawEntry !== entryId) return;
    reader.raw = records;
    this.changed();
  }

  // --- actions on the selected session ------------------------------------------------------------

  startHop(): void {
    const state = this.state;
    const row = selectedRow(state);
    if (!row || state.installation === null) { this.report('No session selected', true); return; }
    if (row.unreadable) { this.report(`${row.id} cannot be read: ${row.unreadable}`, true); return; }
    if (!this.writeChecked) return;
    this.open({ kind: 'hop', key: state.installation, id: row.id });
  }

  async exportSelected(saveAs: boolean): Promise<void> {
    const state = this.state;
    const row = selectedRow(state);
    const installation = currentInstallation(state);
    if (!row || !installation) { this.report('No session selected', true); return; }
    let to: string | undefined;
    if (saveAs) {
      const picked = await api.pick('save', `${installation.harness}-${row.id.replace(/[^A-Za-z0-9._-]/g, '_')}.json`).catch((error) => { this.report(errorText(error), true); return null; });
      if (!picked) return;
      to = picked;
    }
    this.setJob(`Exporting ${row.title ?? row.id}…`);
    try {
      const exported = await api.export(installation.key, row.id, to);
      state.lastExport = exported.path;
      this.setJob(null);
      this.report(`Exported ${exported.messages} messages → ${exported.path}`);
      this.open({ kind: 'exported', exported });
    } catch (error) {
      this.setJob(null);
      this.report(`Export failed: ${errorText(error)}`, true);
    }
  }

  importDocument(document: string | null = this.state.lastExport): void {
    if (!this.writeChecked) return;
    this.open({ kind: 'import', document, into: this.state.installation });
  }

  startIn(row: SessionRow | undefined = selectedRow(this.state)): void {
    const key = this.state.installation;
    if (!row || key === null || !canStartIn(row)) return;
    this.open({ kind: 'start-in', key, row });
  }

  async resume(key: string, id: string): Promise<void> {
    this.setJob('Opening a terminal…');
    try {
      const result = await api.resume(key, id);
      this.setJob(null);
      if (result.refusal) {
        // The backend sends the command that continues it instead, as data.
        this.open({ kind: 'resume', result });
        this.report('Resume refused: this harness has no terminal UI');
      } else {
        this.report(`Resume → ${result.command.join(' ')}${result.cwd ? `  (in ${result.cwd})` : ''}`);
      }
    } catch (error) {
      this.setJob(null);
      this.report(`Resume failed: ${errorText(error)}`, true);
    }
  }

  // --- folders and stores scanned -----------------------------------------------------------------

  private keepFolders(): void {
    try { localStorage.setItem(SCAN_KEY, JSON.stringify(this.state.scan.filter((dir) => !this.startupScan.includes(dir)))); } catch { /* private window: not kept */ }
  }

  /** Keeps these folders or stores, then reads every store again. */
  async addFolders(dirs: string[]): Promise<void> {
    for (const dir of dirs) if (!this.state.scan.includes(dir)) this.state.scan.push(dir);
    this.keepFolders();
    await this.loadInventory();
  }

  async forgetFolder(dir: string): Promise<void> {
    this.state.scan = this.state.scan.filter((one) => one !== dir);
    this.keepFolders();
    await this.loadInventory();
  }

  async scanFolder(): Promise<void> {
    const dir = await api.pick('folder').catch((error) => { this.report(errorText(error), true); return null; });
    if (!dir) return;
    await this.addFolders([dir]);
  }

  /** The installations a harness page lists, for the screens that need them. */
  get installations(): Installation[] { return this.state.installations; }

  /** A sandbox of the harnesses page was installed, forgotten or restored: the harnesses and the stores are read again. */
  async afterSandboxChange(): Promise<void> {
    await this.harnesses.refresh();
    await this.loadInventory();
  }
}
