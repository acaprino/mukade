// One modal at a time (Shinatama's `modal.ts` pattern): a scrim, a dialog with a head, a body and
// a foot, Escape to close unless the dialog is locked while its job runs.

import './modal.css';
import { button, fill, h, icon, kbd } from './dom.ts';

export interface Modal {
  root: HTMLElement;
  body: HTMLElement;
  foot: HTMLElement;
  close(): void;
  /** While locked, Escape and the close button do nothing (a hop cannot be stopped). */
  lock(locked: boolean): void;
  setTitle(title: string, subtitle?: string): void;
}

let current: Modal | null = null;

export function modalOpen(): boolean { return current !== null; }

export function closeModal(): boolean {
  if (!current) return false;
  current.close();
  return true;
}

export function openModal(opts: { title: string; subtitle?: string; icon?: string; tone?: 'accent' | 'live' | 'error'; wide?: boolean; onClose?: () => void }): Modal {
  current?.close();
  let locked = false;
  const title = h('h2', { id: 'modal-title' });
  const subtitle = h('span', { class: 'mono muted modal-subtitle' });
  const closeButton = button([kbd('esc')], { class: 'ghost', 'aria-label': 'Close' });
  const body = h('div', { class: 'dialog-body' });
  const foot = h('div', { class: 'dialog-foot' });
  const dialog = h('section', {
    class: `dialog tone-${opts.tone ?? 'accent'}${opts.wide ? ' wide' : ''}`,
    role: 'dialog', 'aria-modal': 'true', 'aria-labelledby': 'modal-title',
  },
  h('div', { class: 'dialog-head' }, opts.icon ? icon(opts.icon, 'accent') : null, title, subtitle, h('span', { class: 'spring' }), closeButton),
  body, foot);
  const root = h('div', { class: 'scrim' }, dialog);
  const previous = document.activeElement as HTMLElement | null;
  const modal: Modal = {
    root, body, foot,
    close() {
      if (locked || current !== modal) return;
      current = null;
      root.remove();
      opts.onClose?.();
      previous?.focus?.();
    },
    lock(value) { locked = value; closeButton.disabled = value; dialog.classList.toggle('locked', value); },
    setTitle(text, sub) { title.textContent = text; fill(subtitle, sub ?? ''); },
  };
  modal.setTitle(opts.title, opts.subtitle);
  closeButton.addEventListener('click', () => modal.close());
  root.addEventListener('mousedown', (event) => { if (event.target === root) modal.close(); });
  document.getElementById('app')!.appendChild(root);
  current = modal;
  queueMicrotask(() => (dialog.querySelector<HTMLElement>('[autofocus], input, select, textarea, button.primary') ?? closeButton).focus());
  return modal;
}

/** A confirm dialog; resolves true on the primary button. */
export function confirmModal(title: string, text: string, yes: string): Promise<boolean> {
  return new Promise((resolve) => {
    let answered = false;
    const modal = openModal({ title, tone: 'error', onClose: () => { if (!answered) resolve(false); } });
    fill(modal.body, h('p', { class: 'note warn' }, text));
    const ok = button(yes, { class: 'danger' });
    ok.addEventListener('click', () => { answered = true; modal.close(); resolve(true); });
    const cancel = button(['Cancel ', kbd('esc')]);
    cancel.addEventListener('click', () => modal.close());
    fill(modal.foot, h('span', { class: 'spring' }), cancel, ok);
  });
}
