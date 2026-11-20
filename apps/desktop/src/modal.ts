import { confirm, hasModal, openFormDialog, topLayer, type FormDialog } from '@acaprino/imago/ui';
import './modal.css';
import { icon } from './dom.ts';

export type Modal = FormDialog;

const forms = new Set<FormDialog>();

export function modalOpen(): boolean { return hasModal(); }

function topForm(): FormDialog | undefined {
  const root = topLayer()?.el;
  return [...forms].find(form => form.root === root);
}

export function closeModal(): boolean { return topForm()?.close() ?? false; }

export function openModal(opts: { title: string; subtitle?: string; icon?: string; wide?: boolean; onClose?: () => void }): Modal {
  topForm()?.close();
  const form = openFormDialog(document.getElementById('app')!, {
    ...opts,
    icon: opts.icon ? icon(opts.icon, 'accent') : undefined,
    closeLabel: 'Close',
    onClose() { forms.delete(form); opts.onClose?.(); },
  });
  forms.add(form);
  return form;
}

export function confirmModal(title: string, text: string, yes: string): Promise<boolean> {
  return confirm(document.getElementById('app')!, { title, body: text, confirm: yes, cancel: 'Cancel', danger: true });
}
