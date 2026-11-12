// F6, F8: the hop dialog (destination, compaction, subagents, branches, reasoning, shell, prompt,
// and the plan before anything runs), the import dialog, and the receipt both end in.

import { api, errorText } from './api.ts';
import { button, commandLine, copyButton, fill, h, kbd } from './dom.ts';
import { openModal, type Modal } from './modal.ts';
import type { HopArgs, HopOptions, Installation, Receipt } from './types.ts';

export interface JobHooks {
  /** A job started (`label` for the status line) or ended. */
  job(label: string | null): void;
  status(text: string, error?: boolean): void;
  /** Show a written session in the tree (its installation listed again first). */
  showSession(key: number, id: string): void;
  resumeIn(key: number, id: string): void;
}

const COMPACTION_HELP: Record<string, string> = {
  none: 'the whole history',
  portable: 'the source’s own last summary, then what follows it',
  summary: 'the source’s model writes a summary; the history travels as it',
};
const SHELLS = ['default', 'posix', 'powershell', 'bash', 'pwsh', 'none'];

function segmented<T extends string>(label: string, values: T[], current: T, onPick: (value: T) => void): HTMLElement {
  const group = h('div', { class: 'seg', role: 'group', 'aria-label': label });
  for (const value of values) {
    group.append(button(value, { 'aria-pressed': String(value === current), onclick: () => onPick(value) }));
  }
  return group;
}

export async function openHop(source: number, id: string, hooks: JobHooks): Promise<void> {
  const modal = openModal({ title: 'Hop', subtitle: 'reading the session…', icon: 'hop', wide: true });
  fill(modal.body, h('p', { class: 'muted' }, 'Looking at where it can go…'));
  let options: HopOptions;
  try {
    options = await api.hopOptions(source, id);
  } catch (error) {
    fill(modal.body, h('p', { class: 'note err' }, errorText(error)));
    fill(modal.foot, h('span', { class: 'spring' }), button(['Close ', kbd('esc')], { onclick: () => modal.close() }));
    return;
  }
  modal.setTitle('Hop', `${options.source_label} · ${options.title} · ${id.slice(0, 8)}`);
  if (!options.destinations.length) {
    fill(modal.body, h('p', { class: 'note warn' }, 'No other installation mukade can write into.'));
    return;
  }
  const choice: HopArgs = {
    source, id, dest: options.destinations[0].key, compaction: 'none',
    children: options.subagents.length > 0, all_branches: false, reasoning: 'drop_foreign', dest_shell: 'default', prompt: '',
  };
  const draw = () => drawHop(modal, options, choice, draw, () => void runHop(modal, options, choice, hooks));
  draw();
}

function drawHop(modal: Modal, options: HopOptions, choice: HopArgs, redraw: () => void, run: () => void): void {
  const dest = options.destinations.find((d) => d.key === choice.dest)!;
  const destList = h('div', { class: 'choices', role: 'radiogroup', 'aria-label': 'Destination' });
  for (const item of options.destinations) {
    const node = button([h('span', { class: 'grow' }, item.label), item.default_shell ? h('span', { class: 'muted' }, item.default_shell) : null], {
      class: 'choice', role: 'radio', 'aria-checked': String(item.key === choice.dest),
      onclick: () => { choice.dest = item.key; redraw(); },
    });
    destList.append(node);
  }
  const prompt = h('textarea', { id: 'hop-prompt', rows: '2', placeholder: 'continue' }) as HTMLTextAreaElement;
  prompt.value = choice.prompt ?? '';
  prompt.addEventListener('input', () => { choice.prompt = prompt.value; });
  const shell = h('select', { id: 'hop-shell' }) as HTMLSelectElement;
  for (const name of SHELLS) {
    shell.append(h('option', { value: name, selected: name === choice.dest_shell }, name === 'default' ? `default · ${dest.default_shell ?? 'none known'}` : name));
  }
  shell.addEventListener('change', () => { choice.dest_shell = shell.value; redraw(); });
  const branches = h('input', { type: 'checkbox', id: 'hop-branches', checked: choice.all_branches, disabled: !options.all_branches_supported }) as HTMLInputElement;
  branches.addEventListener('change', () => { choice.all_branches = branches.checked; if (branches.checked && choice.compaction === 'summary') choice.compaction = 'none'; redraw(); });

  const left = h('div', { class: 'col' },
    h('div', { class: 'field' }, h('span', { class: 'lab' }, 'Destination'), destList),
    h('div', { class: 'field' }, h('span', { class: 'lab' }, 'Compaction'),
      segmented('Compaction', options.compactions.filter((c) => !(choice.all_branches && c === 'summary')), choice.compaction, (value) => { choice.compaction = value; redraw(); }),
      h('span', { class: 'muted mono hint' }, COMPACTION_HELP[choice.compaction] ?? '')),
    options.subagents.length ? h('div', { class: 'field' }, h('span', { class: 'lab' }, `Subagents · ${options.subagents.length} found`),
      segmented('Subagents', ['carry', 'leave'], choice.children ? 'carry' : 'leave', (value) => { choice.children = value === 'carry'; redraw(); })) : null,
    h('div', { class: 'field' }, h('span', { class: 'lab' }, 'Reasoning'),
      segmented('Reasoning', ['drop_foreign', 'as_text', 'drop'], choice.reasoning, (value) => { choice.reasoning = value; redraw(); })),
    h('div', { class: 'row-fields' },
      h('div', { class: 'field' }, h('label', { for: 'hop-shell' }, 'Destination shell'), shell),
      h('label', { class: `check${options.all_branches_supported ? '' : ' off'}`, for: 'hop-branches', title: options.all_branches_supported ? 'Carry every branch the source keeps' : 'This source keeps no branches mukade reads' },
        branches, options.all_branches_supported ? 'All branches' : 'All branches · not kept by this source')),
    h('div', { class: 'field' }, h('label', { for: 'hop-prompt' }, 'Prompt · optional, runs the destination once'), prompt));

  const plan = h('ul', { class: 'plan' },
    h('li', {}, h('b', {}, 'write '), `1 session into ${dest.label}`),
    choice.children && options.subagents.length ? h('li', {}, h('b', {}, 'write '), `${options.subagents.length} subagent session${options.subagents.length === 1 ? '' : 's'} with it`) : null,
    ...(choice.children ? options.subagents.map((name) => h('li', { class: 'sub' }, `↳ ${name}`)) : []),
    h('li', {}, h('b', {}, 'history '), COMPACTION_HELP[choice.compaction] ?? choice.compaction),
    choice.all_branches ? h('li', {}, h('b', {}, 'branches '), 'every branch the source keeps') : null,
    h('li', {}, h('b', {}, 'reasoning '), choice.reasoning),
    h('li', {}, h('b', {}, 'shell '), choice.dest_shell === 'default' ? `tool calls for ${dest.default_shell ?? 'the destination’s own shell'}` : choice.dest_shell === 'none' ? 'commands left as written' : `tool calls for ${choice.dest_shell}`),
    choice.prompt?.trim() ? h('li', {}, h('b', {}, 'then '), `run the destination once: “${choice.prompt.trim()}”`) : null,
    h('li', {}, h('b', {}, 'save '), 'the manifest in mukade-data/manifests'));
  const notTravelling = [
    options.forks.length ? `${options.forks.length === 1 ? 'the fork' : `${options.forks.length} forks`} (${options.forks.join(', ')}): hop each on its own` : '',
    !choice.children && options.subagents.length ? `the ${options.subagents.length} subagent session${options.subagents.length === 1 ? '' : 's'}` : '',
  ].filter(Boolean);
  const right = h('div', { class: 'col' },
    h('div', { class: 'field' }, h('span', { class: 'lab' }, 'Plan · nothing is written yet'), plan),
    notTravelling.length ? h('p', { class: 'note warn' }, `Will not travel: ${notTravelling.join('; ')}.`) : null,
    h('p', { class: 'note' }, 'What the destination cannot hold is listed as a loss in the receipt; no loss is silent.'),
    options.live ? h('p', { class: 'note' }, 'The source session is live. The hop copies what is on disk now.') : null);
  fill(modal.body, left, right);
  const go = button(['Hop to ', dest.label, ' ', kbd('⏎')], { class: 'primary', onclick: run });
  const focused = document.activeElement;
  fill(modal.foot, h('span', { class: 'mono muted' }, 'A hop cannot be stopped once it starts.'), h('span', { class: 'spring' }),
    button(['Cancel ', kbd('esc')], { onclick: () => modal.close() }), go);
  if (!focused || !modal.root.contains(focused) || !focused.isConnected) go.focus({ preventScroll: true });
  modal.root.onkeydown = (event) => {
    if (event.key === 'Enter' && !(event.target instanceof HTMLTextAreaElement) && !(event.target instanceof HTMLSelectElement)) { event.preventDefault(); run(); }
  };
}

async function runHop(modal: Modal, options: HopOptions, choice: HopArgs, hooks: JobHooks): Promise<void> {
  const dest = options.destinations.find((d) => d.key === choice.dest)!;
  modal.lock(true);
  modal.root.onkeydown = null;
  fill(modal.foot, h('span', { class: 'mono' }, `Hopping to ${dest.label}…`), h('span', { class: 'spring' }));
  hooks.job(`hopping → ${dest.label}…`);
  try {
    const receipt = await api.hop({ ...choice, prompt: choice.prompt?.trim() || undefined });
    hooks.job(null);
    hooks.status(receipt.warning ? `hop written, with a warning: ${receipt.warning}` : `hop done → ${dest.label} · ${receipt.dest_id}`, Boolean(receipt.warning));
    modal.lock(false);
    showReceipt(modal, receipt, `${options.source_label} → ${dest.label}`, hooks);
  } catch (error) {
    hooks.job(null);
    hooks.status(`hop failed: ${errorText(error)}`, true);
    modal.lock(false);
    fill(modal.body, h('p', { class: 'note err' }, `Nothing was written. ${errorText(error)}`));
    fill(modal.foot, h('span', { class: 'spring' }), button(['Close ', kbd('esc')], { class: 'primary', onclick: () => modal.close() }));
  }
}

function commandField(label: string, argv: string[]): HTMLElement {
  const text = commandLine(argv);
  return h('div', { class: 'field' }, h('span', { class: 'lab' }, label), h('div', { class: 'cmd' }, h('pre', { class: 'code' }, text), copyButton(text)));
}

/** The receipt of a hop or import, in `modal` (spec F6 result). */
export function showReceipt(modal: Modal, receipt: Receipt, subtitle: string, hooks: JobHooks): void {
  modal.setTitle(receipt.warning ? 'Written, with a warning' : 'Done', subtitle);
  modal.root.querySelector('.dialog')?.classList.replace('tone-accent', receipt.warning ? 'tone-error' : 'tone-live');
  const left = h('div', { class: 'col' },
    receipt.warning ? h('p', { class: 'note warn' }, receipt.warning) : null,
    h('dl', { class: 'kv' },
      h('dt', {}, 'new id'), h('dd', {}, receipt.dest_id),
      h('dt', {}, 'into'), h('dd', {}, receipt.dest_label),
      h('dt', {}, 'location'), h('dd', {}, receipt.location),
      h('dt', {}, 'manifest'), h('dd', {}, receipt.manifest_path ?? 'not saved (no mukade-data folder)')),
    receipt.refusal
      ? h('div', { class: 'field' }, h('span', { class: 'lab' }, 'Resume · interactive'), h('p', { class: 'note' }, receipt.refusal))
      : commandField('Resume · interactive', receipt.interactive),
    receipt.headless.length ? commandField('Resume · headless', receipt.headless) : null,
    receipt.children.length ? h('div', { class: 'field' }, h('span', { class: 'lab' }, `Subagents written · ${receipt.children.length}`),
      h('ul', { class: 'plan' }, ...receipt.children.map((child) => h('li', {}, h('span', { class: 'spawn' }, '↳ '), `${child.source_id} → ${child.dest_id}`,
        h('span', { class: child.linked ? 'muted' : 'warn-text' }, child.linked ? ' · linked to its parent' : ' · not linked: the destination has no link'))))) : null);
  const right = h('div', { class: 'col' },
    h('div', { class: 'field' }, h('span', { class: 'lab' }, receipt.losses.length ? `Losses · ${receipt.losses.length}` : 'Losses'),
      receipt.losses.length ? h('ul', { class: 'plan losses' }, ...receipt.losses.map((loss) => h('li', {}, loss))) : h('p', { class: 'note' }, 'None: everything travelled.')),
    receipt.reply ? h('div', { class: 'field' }, h('span', { class: 'lab' }, `Reply from ${receipt.dest_label}`), h('pre', { class: 'code' }, receipt.reply)) : null);
  fill(modal.body, left, right);
  const close = button(['Close ', kbd('⏎')], { class: 'primary', onclick: () => modal.close() });
  fill(modal.foot,
    receipt.manifest_path ? copyButton(receipt.manifest_path) : null,
    receipt.manifest_path ? h('span', { class: 'mono muted' }, 'manifest path') : null,
    h('span', { class: 'spring' }),
    button('Show in sessions', { onclick: () => { modal.close(); hooks.showSession(receipt.dest_key, receipt.dest_id); } }),
    receipt.refusal ? null : button(['Resume in terminal'], { onclick: () => hooks.resumeIn(receipt.dest_key, receipt.dest_id) }),
    close);
  modal.root.onkeydown = (event) => { if (event.key === 'Enter' && event.target === close) return; if (event.key === 'Enter') { event.preventDefault(); modal.close(); } };
  queueMicrotask(() => close.focus());
}

/** F8: a document written into an installation. */
export function openImport(installations: Installation[], into: number | null, document: string | null, hooks: JobHooks): void {
  const writable = installations.filter((item) => item.writable);
  const modal = openModal({ title: 'Import', subtitle: 'a dump document into an installation', icon: 'import' });
  const path = h('input', { id: 'import-path', type: 'text', placeholder: 'C:\\…\\mukade-data\\exports\\<harness>-<id>.json', value: document ?? '' }) as HTMLInputElement;
  const dest = h('select', { id: 'import-into' }) as HTMLSelectElement;
  for (const item of writable) dest.append(h('option', { value: String(item.key), selected: item.key === into }, item.label));
  const prompt = h('textarea', { id: 'import-prompt', rows: '2', placeholder: 'continue' }) as HTMLTextAreaElement;
  const browse = button('Choose…', { onclick: async () => { const picked = await api.pick('open').catch(() => null); if (picked) path.value = picked; } });
  fill(modal.body, h('div', { class: 'col' },
    h('div', { class: 'field' }, h('label', { for: 'import-path' }, 'Document'), h('div', { class: 'cmd' }, path, browse)),
    h('div', { class: 'field' }, h('label', { for: 'import-into' }, 'Into'), dest),
    h('div', { class: 'field' }, h('label', { for: 'import-prompt' }, 'Prompt · optional, runs the destination once'), prompt),
    h('p', { class: 'note' }, 'The document becomes a new session of the installation, with its own id; the source is not touched.')));
  const run = async () => {
    if (!path.value.trim()) { path.focus(); return; }
    const target = writable.find((item) => item.key === Number(dest.value));
    modal.lock(true);
    fill(modal.foot, h('span', { class: 'mono' }, `Importing into ${target?.label}…`), h('span', { class: 'spring' }));
    hooks.job(`importing → ${target?.label}…`);
    try {
      const receipt = await api.import(path.value.trim(), Number(dest.value), prompt.value.trim() || undefined);
      hooks.job(null);
      hooks.status(`import done → ${receipt.dest_label} · ${receipt.dest_id}`);
      modal.lock(false);
      showReceipt(modal, receipt, `document → ${receipt.dest_label}`, hooks);
    } catch (error) {
      hooks.job(null);
      hooks.status(`import failed: ${errorText(error)}`, true);
      modal.lock(false);
      fill(modal.foot, h('p', { class: 'note err grow' }, errorText(error)), button(['Close ', kbd('esc')], { onclick: () => modal.close() }), button('Try again', { class: 'primary', onclick: run }));
    }
  };
  fill(modal.foot, h('span', { class: 'spring' }), button(['Cancel ', kbd('esc')], { onclick: () => modal.close() }), button(['Import ', kbd('⏎')], { class: 'primary', onclick: run }));
  modal.root.onkeydown = (event) => { if (event.key === 'Enter' && !(event.target instanceof HTMLTextAreaElement)) { event.preventDefault(); void run(); } };
}
