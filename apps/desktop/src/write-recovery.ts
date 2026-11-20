// Rust owns the operation until the user dismisses its result. A replacement WebView
// watches the same operation through IPC without asking the destination to write again.
import { api, errorText } from './api.ts';
import { openModal, type Modal } from './modal.ts';
import type { WriteOperation } from './types.ts';

type Terminal = Exclude<WriteOperation, { status: 'running' }>;
export type WriteDialog = Modal & {
  observe(start: WriteOperation | (() => Promise<WriteOperation>)): Promise<Terminal>;
};

let dismissal: Promise<void> = Promise.resolve();

export function openWriteDialog(options: Parameters<typeof openModal>[0], report: (message: string) => void): WriteDialog {
  let terminalId: number | undefined;
  function acknowledge(): Promise<void> {
    if (terminalId !== undefined) {
      const id = terminalId;
      terminalId = undefined;
      dismissal = api.acknowledgeWrite(id).catch(error => report(errorText(error)));
    }
    return dismissal;
  }
  const modal = openModal({ ...options, onClose: () => { void acknowledge(); } });
  return Object.assign(modal, {
    async observe(start: WriteOperation | (() => Promise<WriteOperation>)): Promise<Terminal> {
      // A retry dismisses the failed operation before it submits the form again.
      await acknowledge();
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
        await new Promise(resolve => setTimeout(resolve, 300));
        let current: WriteOperation | null;
        try {
          current = await api.writeStatus();
        } catch (error) {
          report(`Reconnecting to ${operation.kind}… ${errorText(error)}`);
          continue;
        }
        if (!current || current.id !== operation.id) throw new Error('The write is no longer available in this window.');
        operation = current;
      }
      terminalId = operation.id;
      return operation;
    },
  });
}
