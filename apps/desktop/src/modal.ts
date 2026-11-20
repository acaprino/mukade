import { confirm, hasModal, inertShell, pushLayer, trapTab } from '@acaprino/imago/ui';
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

export function modalOpen(): boolean { return hasModal(); }

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
  const foot = h('div', { class: 'im-dialog-actions dialog-foot' });
  const dialog = h('section', {
    class: `im-float im-dialog dialog tone-${opts.tone ?? 'accent'}${opts.wide ? ' wide' : ''}`,
    role: 'dialog', 'aria-modal': 'true', 'aria-labelledby': 'modal-title',
  },
  h('div', { class: 'dialog-head' }, opts.icon ? icon(opts.icon, 'accent') : null, title, subtitle, h('span', { class: 'spring' }), closeButton),
  body, foot);
  const root = h('div', { class: 'im-scrim', 'data-im-layer': 'dialog' }, dialog);
  const previous = document.activeElement as HTMLElement | null;
  const shell = document.getElementById('app')!;
  const release = inertShell(shell);
  const pop = pushLayer({ el: root, kind: 'dialog', close: () => modal.close() }, previous);
  dialog.addEventListener('keydown', event => trapTab(event, dialog));
  const modal: Modal = {
    root, body, foot,
    close() {
      if (locked || current !== modal) return;
      current = null;
      release();
      pop();
      root.remove();
      opts.onClose?.();
    },
    lock(value) {
      locked = value;
      closeButton.disabled = value;
      body.inert = value;
      foot.inert = value;
      dialog.classList.toggle('locked', value);
    },
    setTitle(text, sub) { title.textContent = text; fill(subtitle, sub ?? ''); },
  };
  modal.setTitle(opts.title, opts.subtitle);
  closeButton.addEventListener('click', () => modal.close());
  root.addEventListener('mousedown', (event) => { if (event.target === root) modal.close(); });
  shell.append(root);
  current = modal;
  queueMicrotask(() => (dialog.querySelector<HTMLElement>('[autofocus], input, select, textarea, button.primary') ?? closeButton).focus());
  return modal;
}

/** A confirm dialog; resolves true on the primary button. */
export function confirmModal(title: string, text: string, yes: string): Promise<boolean> {
  return confirm(document.getElementById('app')!, { title, body: text, confirm: yes, cancel: 'Cancel', danger: true });
}
