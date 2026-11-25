// The harnesses page (state, no screen): every harness with its binary, version and standard store; the other installations found for
// it (portable ones, sandboxes); a scan of the whole computer, one folder at a time; installs: the harness's own command in a
// terminal, or a sandbox the window installs with npm, with a store of its own. screens/harnesses.tsx draws it.

import { api, errorText } from './api.ts';
import type { Controller } from './controller.ts';
import { Store } from './store.ts';
import type { HarnessView, Sandbox } from './types.ts';

export interface ScanState { root: string; done: number; total: number; found: number }

export class HarnessController extends Store {
  inventory: HarnessView[] | null = null;
  sandboxes: Sandbox[] = [];
  error: string | null = null;
  reading = false;
  scan: ScanState | null = null;
  private stopped = false;
  /** Sandboxes on their way, by harness: npm takes minutes. */
  installing = new Set<string>();

  constructor(private readonly app: Controller) { super(); }

  /** The page was opened: it reads the binaries once. */
  async show(): Promise<void> {
    if (!this.inventory && !this.reading) await this.refresh();
  }

  /** Reads the binaries, versions and sandboxes again. */
  async refresh(): Promise<void> {
    if (this.reading) return;
    this.reading = true;
    this.changed();
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
    this.changed();
  }

  /** Every folder of the home and of every drive, one at a time, through `mukade::detect`. */
  async scanComputer(): Promise<void> {
    if (this.scan) return;
    let roots: string[];
    try {
      roots = await api.systemRoots();
    } catch (error) {
      this.app.report(`The system scan could not start: ${errorText(error)}`, true);
      return;
    }
    this.stopped = false;
    this.scan = { root: '', done: 0, total: roots.length, found: 0 };
    const stores = new Set<string>();
    const standard = new Set(this.inventory?.map((one) => one.store).filter((store): store is string => Boolean(store)));
    let truncated = 0;
    for (const root of roots) {
      if (this.stopped) break;
      this.scan.root = root;
      this.changed();
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
    const stopped = this.stopped;
    const done = this.scan.done;
    this.scan = null;
    this.changed();
    const fresh = [...stores].filter((store) => !this.app.state.scan.includes(store));
    if (fresh.length) await this.app.addFolders(fresh);
    this.app.report(`System scan ${stopped ? 'stopped' : 'done'}: ${stores.size} installation${stores.size === 1 ? '' : 's'} outside the standard stores in ${done} folder${done === 1 ? '' : 's'}${fresh.length ? `, ${fresh.length} new` : ''}${truncated ? `; ${truncated} folder${truncated === 1 ? '' : 's'} too large to read whole` : ''}`);
    this.changed();
  }

  stopScan(): void {
    this.stopped = true;
    this.changed();
  }

  /** A kept folder is forgotten at once, and the notice has Undo: it keeps the folder again. */
  async forgetFolder(dir: string): Promise<void> {
    await this.app.forgetFolder(dir);
    this.app.report(`Forgot the folder ${dir}`, false, () => this.app.addFolders([dir]));
  }

  /** The harness's own install command, in a new terminal. */
  async installSystem(harness: HarnessView): Promise<boolean> {
    try {
      await api.installSystem(harness.id);
      this.app.report(`The install of ${harness.display} runs in a new terminal; check again when it ends`);
      return true;
    } catch (error) {
      this.app.report(`The install of ${harness.display} could not start: ${errorText(error)}`, true);
      return false;
    }
  }

  /** npm installs the harness into a sandbox of its own: answers the sandbox, or throws what went wrong. */
  async installSandbox(harness: HarnessView, name: string): Promise<Sandbox> {
    if (this.installing.has(harness.id)) throw new Error(`${harness.display} is being installed already`);
    this.installing.add(harness.id);
    this.changed();
    try {
      const made = await api.sandboxInstall(harness.id, name);
      this.app.report(`${harness.display} installed in a sandbox: ${made.root}`);
      await this.app.afterSandboxChange();
      return made;
    } catch (error) {
      this.app.report(`The sandbox install of ${harness.display} failed: ${errorText(error)}`, true);
      throw error;
    } finally {
      this.installing.delete(harness.id);
      this.changed();
    }
  }

  async runSandbox(sandbox: Sandbox): Promise<void> {
    try {
      await api.sandboxRun(sandbox.root);
      this.app.report(`Sandbox ${sandbox.name} opened in a terminal`);
    } catch (error) {
      this.app.report(errorText(error), true);
    }
  }

  /** A sandbox is forgotten at once (its folder stays), and the notice has Undo: it lists the sandbox again. */
  async forgetSandbox(sandbox: Sandbox): Promise<void> {
    try {
      await api.sandboxForget(sandbox.root);
      this.app.report(`Sandbox ${sandbox.name} is no longer listed; its folder stays: ${sandbox.root}`, false, async () => {
        try {
          await api.sandboxRestore(sandbox);
          await this.app.afterSandboxChange();
        } catch (error) {
          this.app.report(errorText(error), true);
        }
      });
      await this.app.afterSandboxChange();
    } catch (error) {
      this.app.report(errorText(error), true);
    }
  }
}
