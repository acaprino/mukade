// Rust owns a hop or an import until the person dismisses its result. A replacement WebView watches the same operation through IPC
// without asking the destination to write again; a dialog of the window holds one `WriteWatch` for as long as it is open, and
// acknowledges the result when it closes.
import { api, errorText } from './api.ts';
import type { WriteOperation } from './types.ts';

export type Terminal = Exclude<WriteOperation, { status: 'running' }>;

let dismissal: Promise<void> = Promise.resolve();

export class WriteWatch {
  private terminalId: number | undefined;

  constructor(private readonly report: (message: string) => void) {}

  /** Tells Rust the result was seen, so that the operation is forgotten; the next write waits for it. */
  acknowledge(): Promise<void> {
    if (this.terminalId !== undefined) {
      const id = this.terminalId;
      this.terminalId = undefined;
      dismissal = api.acknowledgeWrite(id).catch((error) => this.report(errorText(error)));
    }
    return dismissal;
  }

  /** Follows the operation to its end: the one `start` began (or asks to begin), or the one Rust already holds. */
  async observe(start: WriteOperation | (() => Promise<WriteOperation>)): Promise<Terminal> {
    // A retry dismisses the failed operation before it submits the form again.
    await this.acknowledge();
    let operation: WriteOperation;
    if (typeof start === 'function') {
      try {
        operation = await start();
      } catch (error) {
        // The command may have reached Rust before its IPC response was lost.
        const current = await api.writeStatus();
        if (!current) throw error;
        operation = current;
      }
    } else {
      operation = start;
    }
    while (operation.status === 'running') {
      await new Promise((resolve) => setTimeout(resolve, 300));
      let current: WriteOperation | null;
      try {
        current = await api.writeStatus();
      } catch (error) {
        this.report(`Reconnecting to ${operation.kind}… ${errorText(error)}`);
        continue;
      }
      if (!current || current.id !== operation.id) throw new Error('The write is no longer available in this window.');
      operation = current;
    }
    this.terminalId = operation.id;
    return operation;
  }
}
