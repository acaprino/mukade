// The harnesses page: every harness with its binary, version and standard store; the other
// installations found for it (portable ones, sandboxes); a scan of the whole computer, one folder
// at a time; installs: the harness's own command in a terminal, or a sandbox the window installs
// with npm, with a store of its own.

import './harnesses.css';
import { loader, setPanelState } from '@acaprino/imago/ui';
import { api, errorText } from './api.ts';
import { mark } from './chatviews.ts';
import { button, fill, h, icon, caps } from './dom.ts';
import { openModal } from './modal.ts';
import type { HarnessView, Installation, Sandbox } from './types.ts';

export interface HarnessesHooks {
  installations(): Installation[];
  /** The folders and stores scanned and kept (`Scan folder`, the system scan). */
  folders(): string[];
  /** Keep these folders or stores, then read every store again. */
  addFolders(dirs: string[]): Promise<void>;
  forgetFolder(dir: string): Promise<void>;
  /** Read every store again (after an install). */
  reload(): Promise<void>;
  scanFolder(): void;
  showInstallation(key: string): void;
  report(text: string, error?: boolean): void;
}

const STATUS_TEXT: Record<HarnessView['status'], string> = { installed: 'installed', configured: 'configured', missing: 'not found' };

export class HarnessesView {
  private inventory: HarnessView[] | null = null;
  private sandboxes: Sandbox[] = [];
  private error: string | null = null;
  private reading = false;
  private scan: { root: string; done: number; total: number; found: number } | null = null;
  private stop = false;
  /** Sandboxes on their way, by harness: npm takes minutes. */
  private installing = new Set<string>();

  constructor(private readonly root: HTMLElement, private readonly hooks: HarnessesHooks) {}

  show(): void {
    this.draw();
    if (!this.inventory) void this.refresh();
  }

  /** Read the binaries, versions and sandboxes again. */
  async refresh(): Promise<void> {
    if (this.reading) return;
    this.reading = true;
    this.draw();
    try {
      const [inventory, sandboxes] = await Promise.all([api.harnessInventory(), api.sandboxes()]);
      this.inventory = inventory;
      this.sandboxes = sandboxes;
      this.error = null;
    } catch (error) {
      this.error = errorText(error);
    } finally {
      this.reading = false;
    }
    this.draw();
  }

  draw(): void {
    if (!this.root.isConnected || this.root.hidden) return;
    const installations = this.hooks.installations();
    const found = installations.filter((item) => item.scanned).length;
    const head = h('div', { class: 'harness-bar' },
      h('h2', {}, 'Harnesses'),
      h('span', { class: 'muted' }, this.inventory
        ? `${this.inventory.filter((one) => one.status !== 'missing').length} of ${this.inventory.length} found · ${found} more installation${found === 1 ? '' : 's'}`
        : this.reading ? 'looking…' : ''),
      h('span', { class: 'spring' }),
      this.scan
        ? h('span', { class: 'scan-progress' }, loader(this.root.ownerDocument, `Scanning ${this.scan.root} · ${this.scan.done} of ${this.scan.total} folders · ${this.scan.found} found`))
        : null,
      this.scan
        ? button('Stop the scan', { onclick: () => { this.stop = true; this.draw(); } })
        : button([icon('search'), 'Scan this computer'], { class: 'primary', title: 'Look in your home and in every folder of every drive', onclick: () => void this.scanComputer() }),
      button([icon('folder'), 'Scan folder… ', caps('O')], { onclick: () => this.hooks.scanFolder() }),
      button([icon('refresh'), 'Check again'], { disabled: this.reading, onclick: () => void this.refresh() }));
    if (this.error || !this.inventory) {
      fill(this.root, head, h('div', { class: 'im-panel-body' }));
      setPanelState(this.root, this.error
        ? { kind: 'error', text: this.error, retry: { label: 'Check again', run: () => void this.refresh() } }
        : { kind: 'loading', rows: 4 });
      return;
    }
    this.root.removeAttribute('aria-busy');
    const cards = h('div', { class: 'harness-grid' }, ...this.inventory.map((harness) => this.card(harness, installations)));
    const folders = this.hooks.folders();
    const kept = h('section', { class: 'box harness-folders' },
      h('p', { class: 'label' }, `Folders and stores scanned · ${folders.length}`),
      folders.length
        ? h('ul', { class: 'plain' }, ...folders.map((dir) => h('li', {}, h('span', { class: 'mono grow' }, dir),
          button('Forget', { class: 'ghost', title: 'Stop listing what this folder holds', onclick: () => void this.hooks.forgetFolder(dir) }))))
        : h('p', { class: 'hint muted pad-x' }, 'Scan this computer, or one folder, to find portable and custom installations.'));
    fill(this.root, head, h('div', { class: 'harness-body' }, cards, kept));
  }

  private card(harness: HarnessView, installations: Installation[]): HTMLElement {
    const standard = installations.find((item) => item.key === harness.id);
    const others = installations.filter((item) => item.harness === harness.id && item.scanned);
    const sandboxes = this.sandboxes.filter((one) => one.harness === harness.id);
    const installing = this.installing.has(harness.id);
    return h('article', { class: `harness-card st-${harness.status}` },
      h('header', {},
        mark(harness.id),
        h('h3', {}, harness.display),
        h('span', { class: `tag im-chip ${harness.status === 'missing' ? 'bad' : 'live'}` }, STATUS_TEXT[harness.status]),
        harness.version ? h('span', { class: 'mono muted' }, harness.version) : null,
        h('span', { class: 'spring' }),
        button([icon('import'), installing ? 'Installing…' : 'Install…'], { disabled: installing, onclick: () => this.installDialog(harness) })),
      h('dl', { class: 'kv' },
        h('dt', {}, 'binary'), h('dd', { class: harness.binary ? 'mono' : 'muted' }, harness.binary ?? `not on PATH and not at its usual place; name one with ${harness.binary_env}`),
        harness.binary ? h('dt', {}, 'found by') : null, harness.binary ? h('dd', {}, harness.source) : null,
        h('dt', {}, 'store'), h('dd', { class: 'mono' }, harness.store ?? 'none known',
          harness.store && !harness.store_exists ? h('span', { class: 'muted' }, ' · not there yet') : null),
        h('dt', {}, 'sessions'), h('dd', {}, standard?.error ? h('span', { class: 'err-text' }, standard.error) : String(standard?.count ?? 0)),
        h('dt', {}, 'writes'), h('dd', {}, harness.destination ? 'yes: a hop or an import can land here' : 'no: read only')),
      standard && standard.count > 0 ? h('div', { class: 'actions' }, button([icon('list'), 'Show sessions'], { onclick: () => this.hooks.showInstallation(standard.key) })) : null,
      others.length || sandboxes.length
        ? h('div', { class: 'harness-more' }, h('p', { class: 'label' }, `More installations · ${others.length + sandboxes.filter((one) => !others.some((item) => item.store === one.store)).length}`),
          ...others.map((item) => {
            const sandbox = sandboxes.find((one) => one.store === item.store);
            return h('div', { class: 'install-row' },
              h('span', { class: 'grow' }, h('b', {}, item.label), sandbox ? h('span', { class: 'tag im-chip' }, 'sandbox') : null,
                h('span', { class: 'mono muted block' }, item.store ?? ''), item.engine ? h('span', { class: 'mono muted block' }, `engine ${item.engine}`) : null),
              h('span', { class: 'num im-num' }, item.error ? 'error' : `${item.count} sessions`),
              item.count > 0 ? button('Show', { class: 'ghost', onclick: () => this.hooks.showInstallation(item.key) }) : null,
              sandbox ? button([icon('term'), 'Run'], { class: 'ghost', title: 'Open it in a terminal, in its sandbox', onclick: () => void this.runSandbox(sandbox) }) : null,
              sandbox ? button('Forget', { class: 'ghost', title: 'Stop listing it; its folder stays', onclick: () => void this.forgetSandbox(sandbox) }) : null);
          }),
          ...sandboxes.filter((one) => !others.some((item) => item.store === one.store)).map((sandbox) => h('div', { class: 'install-row' },
            h('span', { class: 'grow' }, h('b', {}, `sandbox ${sandbox.name}`), h('span', { class: 'tag im-chip' }, 'sandbox'),
              h('span', { class: 'mono muted block' }, sandbox.root), h('span', { class: 'muted block' }, 'Its store appears at the first run.')),
            button([icon('term'), 'Run'], { class: 'ghost', onclick: () => void this.runSandbox(sandbox) }),
            button('Forget', { class: 'ghost', onclick: () => void this.forgetSandbox(sandbox) }))))
        : null);
  }

  /** Every folder of the home and of every drive, one at a time, through `mukade::detect`. */
  private async scanComputer(): Promise<void> {
    if (this.scan) return;
    let roots: string[];
    try {
      roots = await api.systemRoots();
    } catch (error) {
      this.hooks.report(`the system scan could not start: ${errorText(error)}`, true);
      return;
    }
    this.stop = false;
    this.scan = { root: '', done: 0, total: roots.length, found: 0 };
    const stores = new Set<string>();
    const standard = new Set(this.inventory?.map((one) => one.store).filter((store): store is string => Boolean(store)));
    let truncated = 0;
    for (const root of roots) {
      if (this.stop) break;
      this.scan.root = root;
      this.draw();
      try {
        const result = await api.scanFolder(root);
        if (result.truncated) truncated += 1;
        for (const one of result.found) if (!standard.has(one.store)) stores.add(one.store);
        this.scan.found = stores.size;
      } catch {
        // A folder that cannot be read is skipped.
      }
      this.scan.done += 1;
    }
    const stopped = this.stop;
    const done = this.scan.done;
    this.scan = null;
    this.draw();
    const fresh = [...stores].filter((store) => !this.hooks.folders().includes(store));
    if (fresh.length) await this.hooks.addFolders(fresh);
    this.hooks.report(`system scan ${stopped ? 'stopped' : 'done'}: ${stores.size} installation${stores.size === 1 ? '' : 's'} outside the standard stores in ${done} folder${done === 1 ? '' : 's'}${fresh.length ? `, ${fresh.length} new` : ''}${truncated ? `; ${truncated} folder${truncated === 1 ? '' : 's'} too large to read whole` : ''}`);
    this.draw();
  }

  private installDialog(harness: HarnessView): void {
    const modal = openModal({ title: `Install ${harness.display}`, icon: 'import', wide: true });
    const name = h('input', { id: 'sandbox-name', type: 'text', value: 'try', autocomplete: 'off', spellcheck: 'false' }) as HTMLInputElement;
    const note = h('div', { class: 'field-note', 'aria-live': 'polite' });
    const system = h('div', { class: 'col' },
      h('div', { class: 'field im-field' }, h('span', { class: 'lab' }, 'On the system'),
        harness.install_command
          ? h('pre', { class: 'code' }, harness.install_command)
          : h('p', { class: 'note' }, `${harness.display} installs with its own script; its page in docs/harnesses says how.`)),
      h('p', { class: 'note' }, harness.status === 'missing'
        ? 'It runs in a new terminal, with the harness’s own installer; check again when it ends.'
        : `${harness.display} is already installed: the same command updates it. It runs in a new terminal; check again when it ends.`),
      harness.install_command ? button([icon('term'), 'Run in a terminal'], { onclick: () => void this.installSystem(harness, modal.close) }) : null);
    const sandbox = h('div', { class: 'col' },
      h('div', { class: 'field im-field' }, h('span', { class: 'lab' }, 'In a sandbox'),
        harness.sandbox_package
          ? h('p', {}, 'npm installs ', h('code', {}, harness.sandbox_package), ' into ', h('code', {}, `mukade-data/sandboxes/${harness.id}-<name>`),
            ', with a store of its own: it touches neither the installed harness nor its sessions. Hops can land in it, and Run opens it in a terminal. It needs Node.js 22 or later, and its own sign-in.')
          : h('p', { class: 'note' }, `${harness.display} is not an npm package: it has no sandbox.`)),
      harness.sandbox_package ? h('div', { class: 'field im-field' }, h('label', { for: 'sandbox-name' }, 'Name'), name) : null,
      note,
      harness.sandbox_package ? button([icon('import'), 'Install in a sandbox'], { class: 'primary', onclick: () => void run() }) : null);
    const run = async (): Promise<void> => {
      const label = name.value.trim();
      if (!/^[a-z0-9_-]{1,32}$/.test(label)) { fill(note, h('span', { class: 'warn-text' }, 'Lowercase letters, digits, - and _ (up to 32).')); return; }
      if (this.installing.has(harness.id)) return;
      this.installing.add(harness.id);
      fill(note, h('span', { class: 'muted' }, `npm is installing ${harness.sandbox_package}; this can take a few minutes. You can close this dialog.`));
      this.draw();
      try {
        const made = await api.sandboxInstall(harness.id, label);
        this.hooks.report(`${harness.display} installed in a sandbox: ${made.root}`);
        if (modal.root.isConnected) modal.close();
        await this.refresh();
        await this.hooks.reload();
      } catch (error) {
        const text = errorText(error);
        if (modal.root.isConnected) fill(note, h('span', { class: 'err-text', role: 'alert' }, text));
        this.hooks.report(`the sandbox install of ${harness.display} failed: ${text}`, true);
      } finally {
        this.installing.delete(harness.id);
        this.draw();
      }
    };
    fill(modal.body, system, sandbox);
    fill(modal.foot, h('span', { class: 'spring' }), button(['Close ', caps('Escape')], { onclick: () => modal.close() }));
  }

  private async installSystem(harness: HarnessView, close: () => void): Promise<void> {
    try {
      await api.installSystem(harness.id);
      close();
      this.hooks.report(`the install of ${harness.display} runs in a new terminal; check again when it ends`);
    } catch (error) {
      this.hooks.report(`the install of ${harness.display} could not start: ${errorText(error)}`, true);
    }
  }

  private async runSandbox(sandbox: Sandbox): Promise<void> {
    try {
      await api.sandboxRun(sandbox.root);
      this.hooks.report(`sandbox ${sandbox.name} opened in a terminal`);
    } catch (error) {
      this.hooks.report(errorText(error), true);
    }
  }

  private async forgetSandbox(sandbox: Sandbox): Promise<void> {
    try {
      await api.sandboxForget(sandbox.root);
      this.hooks.report(`sandbox ${sandbox.name} is no longer listed; its folder stays: ${sandbox.root}`);
      await this.refresh();
      await this.hooks.reload();
    } catch (error) {
      this.hooks.report(errorText(error), true);
    }
  }
}
