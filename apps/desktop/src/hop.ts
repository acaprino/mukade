// F6, F8: the hop dialog (destination, compaction and its own options, subagents, branches,
// reasoning, shell, prompt, model and tools, and the plan before anything runs), the import dialog, and the receipt both end in.

import { pathField } from '@acaprino/imago/ui';
import { api, errorText } from './api.ts';
import { button, commandLine, copyButton, fill, h, caps } from './dom.ts';
import type { Modal } from './modal.ts';
import { openWriteDialog, type WriteDialog } from './write-recovery.ts';
import type { HopArgs, HopOptions, Installation, Receipt, WriteOperation } from './types.ts';

export interface JobHooks {
  /** A job started (`label` for the status line) or ended. */
  job(label: string | null): void;
  status(text: string, error?: boolean): void;
  /** Show a written session in the tree (its installation listed again first). */
  showSession(key: string, id: string): void;
  resumeIn(key: string, id: string): void;
}

const COMPACTION_HELP: Record<string, string> = {
  none: 'the whole history',
  portable: 'the source’s own last summary, then what follows it',
  summary: 'the source’s model writes a summary; the history travels as it',
  native: 'the whole history, then the destination’s own compaction record',
};
/** Whether the hop dialog's More options are open: kept while the window lives. */
let moreOpen = false;
const SHELLS = ['default', 'posix', 'powershell', 'bash', 'pwsh', 'none'];

/** Enter runs a dialog's primary action only from a one-line text field or the dialog itself; on a
 *  button, a choice, a list or a multi-line field it is that control's own (a hop is a write). */
function submits(event: KeyboardEvent): boolean {
  if (event.key !== 'Enter' || event.isComposing || event.defaultPrevented || event.repeat) return false;
  const target = event.target;
  return target instanceof HTMLInputElement
    ? ['text', 'search', 'url', 'tel', 'email', 'password', 'number'].includes(target.type)
    : target instanceof HTMLElement && target.matches('.im-form-dialog');
}

export function segmented<T extends string>(label: string, values: readonly T[], current: T, onPick: (value: T) => void): HTMLElement {
  const group = h('div', { class: 'seg im-seg', role: 'group', 'aria-label': label });
  for (const value of values) {
    group.append(button(value, { 'aria-pressed': String(value === current), onclick: () => onPick(value) }));
  }
  return group;
}

export async function openHop(source: string, id: string, hooks: JobHooks): Promise<void> {
  if (!writeChecked) return;
  const modal = openWriteDialog({ title: 'Hop', subtitle: 'reading the session…', icon: 'hop', wide: true }, message => hooks.status(message, true));
  fill(modal.body, h('p', { class: 'muted' }, 'Looking at where it can go…'));
  let options: HopOptions;
  try {
    options = await api.hopOptions(source, id);
  } catch (error) {
    fill(modal.body, h('p', { class: 'note err' }, errorText(error)));
    fill(modal.foot, h('span', { class: 'spring' }), button(['Close ', caps('Escape')], { onclick: () => modal.close() }));
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
    interrupted: 'keep', without_tools: false,
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
  const shell = h('select', { id: 'hop-shell' }) as HTMLSelectElement;
  for (const name of SHELLS) {
    shell.append(h('option', { value: name, selected: name === choice.dest_shell }, name === 'default' ? `default · ${dest.default_shell ?? 'none known'}` : name));
  }
  shell.addEventListener('change', () => { choice.dest_shell = shell.value; redraw(); });
  const branches = h('input', { type: 'checkbox', id: 'hop-branches', checked: choice.all_branches, disabled: !options.all_branches_supported }) as HTMLInputElement;
  branches.addEventListener('change', () => { choice.all_branches = branches.checked; if (branches.checked && choice.compaction === 'summary') choice.compaction = 'none'; redraw(); });

  // The plan and the primary button follow what is typed without a redraw, which would take the
  // caret out of the field.
  const plan = h('ul', { class: 'plan' });
  const go = button(['Hop to ', dest.label, ' ', caps('Enter')], { class: 'primary', onclick: run });
  const summary = choice.compaction === 'summary';
  const native = choice.compaction === 'native';
  const drawPlan = (): void => {
    fill(plan,
      h('li', {}, h('b', {}, 'write '), `1 session into ${dest.label}`),
      choice.children && options.subagents.length ? h('li', {}, h('b', {}, 'write '), `${options.subagents.length} subagent session${options.subagents.length === 1 ? '' : 's'} with it`) : null,
      ...(choice.children ? options.subagents.map((name) => h('li', { class: 'sub' }, `↳ ${name}`)) : []),
      h('li', {}, h('b', {}, 'history '), COMPACTION_HELP[choice.compaction] ?? choice.compaction),
      summary ? h('li', {}, h('b', {}, 'summary '), `intent ${choice.intent ?? options.intents[0]}, by ${choice.summary_model?.trim() || 'the session’s own model'}${choice.keep_turns ? `, then the last ${choice.keep_turns} turn${choice.keep_turns === 1 ? '' : 's'} whole` : ''}`) : null,
      native ? h('li', { class: choice.context_window ? '' : 'warn-text' }, h('b', {}, 'window '), choice.context_window ? `${choice.context_window} tokens${choice.max_output_tokens ? `, ${choice.max_output_tokens} of output` : ''}` : 'the context window is needed') : null,
      choice.all_branches ? h('li', {}, h('b', {}, 'branches '), 'every branch the source keeps') : null,
      h('li', {}, h('b', {}, 'reasoning '), choice.reasoning),
      h('li', {}, h('b', {}, 'shell '), choice.dest_shell === 'default' ? `tool calls for ${dest.default_shell ?? 'the destination’s own shell'}` : choice.dest_shell === 'none' ? 'commands left as written' : `tool calls for ${choice.dest_shell}`),
      choice.model?.trim() || choice.model_provider?.trim() ? h('li', {}, h('b', {}, 'model '), [choice.model_provider?.trim(), choice.model?.trim()].filter(Boolean).join(' · ')) : null,
      choice.without_tools ? h('li', {}, h('b', {}, 'tools '), 'left out: the conversation alone') : null,
      choice.interrupted === 'drop' ? h('li', {}, h('b', {}, 'interrupted '), 'the work of unfinished turns is left out') : null,
      choice.prompt?.trim() ? h('li', {}, h('b', {}, 'then '), `run the destination once: “${choice.prompt.trim()}”`) : null,
      h('li', {}, h('b', {}, 'save '), 'the manifest in mukade-data/manifests'));
    go.disabled = native && !choice.context_window;
  };
  prompt.addEventListener('input', () => { choice.prompt = prompt.value; drawPlan(); });
  const textField = (id: string, label: string, value: string | undefined, placeholder: string, set: (value: string) => void): HTMLElement => {
    const input = h('input', { id, type: 'text', placeholder, value: value ?? '', autocomplete: 'off', spellcheck: 'false' }) as HTMLInputElement;
    input.addEventListener('input', () => { set(input.value); drawPlan(); });
    return h('div', { class: 'field im-field' }, h('label', { for: id }, label), input);
  };
  /** A whole number of at least 1, or nothing. */
  const count = (text: string): number | undefined => { const n = Number(text.trim()); return text.trim() && Number.isInteger(n) && n >= 1 ? n : undefined; };
  const intent = h('select', { id: 'hop-intent' }) as HTMLSelectElement;
  for (const name of options.intents) intent.append(h('option', { value: name, selected: name === (choice.intent ?? options.intents[0]) }, name));
  intent.addEventListener('change', () => { choice.intent = intent.value; drawPlan(); });
  const noTools = h('input', { type: 'checkbox', id: 'hop-no-tools', checked: Boolean(choice.without_tools) }) as HTMLInputElement;
  noTools.addEventListener('change', () => { choice.without_tools = noTools.checked; drawPlan(); });
  const more = h('details', { class: 'more', open: moreOpen },
    h('summary', {}, 'More options'),
    h('div', { class: 'more-body' },
      h('div', { class: 'row-fields' },
        textField('hop-model', 'Destination model', choice.model, dest.model ?? 'the destination’s own', (value) => { choice.model = value; }),
        textField('hop-provider', 'Provider', choice.model_provider, 'the destination’s own', (value) => { choice.model_provider = value; })),
      h('div', { class: 'field im-field' }, h('span', { class: 'lab' }, 'Unfinished turns'),
        segmented('Unfinished turns', ['keep', 'drop'], choice.interrupted ?? 'keep', (value) => { choice.interrupted = value; redraw(); })),
      h('label', { class: 'check', for: 'hop-no-tools' }, noTools, 'Without tool calls and results')));
  more.addEventListener('toggle', () => { moreOpen = more.open; });

  const left = h('div', { class: 'col' },
    h('div', { class: 'field im-field' }, h('span', { class: 'lab' }, 'Destination'), destList),
    h('div', { class: 'field im-field' }, h('span', { class: 'lab' }, 'Compaction'),
      segmented('Compaction', options.compactions.filter((c) => !(choice.all_branches && c === 'summary')), choice.compaction, (value) => { choice.compaction = value; redraw(); }),
      h('span', { class: 'muted mono hint' }, COMPACTION_HELP[choice.compaction] ?? '')),
    summary ? h('div', { class: 'row-fields' },
      h('div', { class: 'field im-field' }, h('label', { for: 'hop-intent' }, 'Intent'), intent),
      textField('hop-summary-model', 'Summary model', choice.summary_model, 'the session’s own', (value) => { choice.summary_model = value; }),
      textField('hop-keep-turns', 'Turns kept whole', choice.keep_turns ? String(choice.keep_turns) : '', '0', (value) => { choice.keep_turns = count(value); })) : null,
    native ? h('div', { class: 'row-fields' },
      textField('hop-context', 'Context window · tokens', choice.context_window ? String(choice.context_window) : '', 'of the destination’s model, e.g. 200000', (value) => { choice.context_window = count(value); }),
      textField('hop-max-output', 'Output limit · optional', choice.max_output_tokens ? String(choice.max_output_tokens) : '', 'tokens', (value) => { choice.max_output_tokens = count(value); })) : null,
    options.subagents.length ? h('div', { class: 'field im-field' }, h('span', { class: 'lab' }, `Subagents · ${options.subagents.length} found`),
      segmented('Subagents', ['carry', 'leave'], choice.children ? 'carry' : 'leave', (value) => { choice.children = value === 'carry'; redraw(); })) : null,
    h('div', { class: 'field im-field' }, h('span', { class: 'lab' }, 'Reasoning'),
      segmented('Reasoning', ['drop_foreign', 'as_text', 'drop'], choice.reasoning, (value) => { choice.reasoning = value; redraw(); })),
    h('div', { class: 'row-fields' },
      h('div', { class: 'field im-field' }, h('label', { for: 'hop-shell' }, 'Destination shell'), shell),
      h('label', { class: `check${options.all_branches_supported ? '' : ' off'}`, for: 'hop-branches', title: options.all_branches_supported ? 'Carry every branch the source keeps' : 'This source keeps no branches mukade reads' },
        branches, options.all_branches_supported ? 'All branches' : 'All branches · not kept by this source')),
    h('div', { class: 'field im-field' }, h('label', { for: 'hop-prompt' }, 'Prompt · optional, runs the destination once'), prompt),
    more);

  drawPlan();
  const notTravelling = [
    options.forks.length ? `${options.forks.length === 1 ? 'the fork' : `${options.forks.length} forks`} (${options.forks.join(', ')}): hop each on its own` : '',
    !choice.children && options.subagents.length ? `the ${options.subagents.length} subagent session${options.subagents.length === 1 ? '' : 's'}` : '',
  ].filter(Boolean);
  const right = h('div', { class: 'col' },
    h('div', { class: 'field im-field' }, h('span', { class: 'lab' }, 'Plan · nothing is written yet'), plan),
    notTravelling.length ? h('p', { class: 'note warn' }, `Will not travel: ${notTravelling.join('; ')}.`) : null,
    h('p', { class: 'note' }, 'What the destination cannot hold is listed as a loss in the receipt; no loss is silent.'),
    options.live ? h('p', { class: 'note' }, 'The source session is live. The hop copies what is on disk now.') : null);
  const focused = document.activeElement;
  fill(modal.body, left, right);
  fill(modal.foot, h('span', { class: 'mono muted' }, 'A hop cannot be stopped once it starts.'), h('span', { class: 'spring' }),
    button(['Cancel ', caps('Escape')], { onclick: () => modal.close() }), go);
  // The first draw starts on the primary action; a redraw keeps the control the person was on.
  if (!focused || !focused.isConnected || !modal.body.contains(focused)) go.focus({ preventScroll: true });
  modal.root.onkeydown = (event) => { if (submits(event) && !go.disabled) { event.preventDefault(); run(); } };
}

async function runHop(modal: WriteDialog, options: HopOptions, choice: HopArgs, hooks: JobHooks): Promise<void> {
  const dest = options.destinations.find((d) => d.key === choice.dest)!;
  modal.lock(true);
  modal.root.onkeydown = null;
  fill(modal.foot, h('span', { class: 'mono' }, `Hopping to ${dest.label}…`), h('span', { class: 'spring' }));
  hooks.job(`hopping → ${dest.label}…`);
  const text = (value?: string) => value?.trim() || undefined;
  const summary = choice.compaction === 'summary';
  const native = choice.compaction === 'native';
  await showWriteResult(modal, () => api.hop({
    ...choice, prompt: text(choice.prompt), model: text(choice.model), model_provider: text(choice.model_provider),
    intent: summary ? choice.intent : undefined, summary_model: summary ? text(choice.summary_model) : undefined, keep_turns: summary ? choice.keep_turns : undefined,
    context_window: native ? choice.context_window : undefined, max_output_tokens: native ? choice.max_output_tokens : undefined,
  }), hooks, 'hop', options.source_label);
}

let writeChecked = false;

/** Restore the native operation before accepting a new write from this renderer. */
export async function recoverWrite(hooks: JobHooks): Promise<void> {
  try {
    const current = await api.writeStatus();
    if (current) {
      const title = current.kind === 'import' ? 'Import' : 'Hop';
      const modal = openWriteDialog({ title, icon: current.kind, wide: true }, message => hooks.status(message, true));
      modal.lock(true);
      const message = `${title} in progress…`;
      fill(modal.body, h('p', { class: 'note' }, message));
      hooks.job(message);
      void showWriteResult(modal, current, hooks, current.kind);
    }
  } finally {
    writeChecked = true;
  }
}

async function showWriteResult(modal: WriteDialog, start: WriteOperation | (() => Promise<WriteOperation>), hooks: JobHooks,
  kind: WriteOperation['kind'], source?: string, retry?: () => void): Promise<boolean> {
  try {
    const result = await modal.observe(start);
    // A retransmission can reattach to an earlier hop or import.
    if (kind !== result.kind) { source = undefined; retry = undefined; }
    kind = result.kind;
    if (result.status === 'failed') throw result.error;
    const receipt = result.receipt;
    hooks.status(receipt.warning ? `${kind} written, with a warning: ${receipt.warning}` : `${kind} done → ${receipt.dest_label} · ${receipt.dest_id}`, Boolean(receipt.warning));
    modal.lock(false);
    showReceipt(modal, receipt, `${source ?? (kind === 'import' ? 'document' : 'session')} → ${receipt.dest_label}`, hooks);
    return true;
  } catch (error) {
    const reason = errorText(error);
    hooks.status(`${kind} failed: ${reason}`, true);
    modal.lock(false);
    if (!retry) fill(modal.body, h('p', { class: 'note err' }, reason));
    fill(modal.foot, retry ? h('p', { class: 'note err grow' }, reason) : h('span', { class: 'spring' }),
      button(['Close ', caps('Escape')], { onclick: () => modal.close() }),
      retry ? button('Try again', { class: 'primary', onclick: retry }) : null);
    return false;
  } finally {
    hooks.job(null);
  }
}

function commandField(label: string, argv: string[]): HTMLElement {
  const text = commandLine(argv);
  return h('div', { class: 'field im-field' }, h('span', { class: 'lab' }, label), h('div', { class: 'cmd' }, h('pre', { class: 'code' }, text), copyButton(text)));
}

/** The receipt of a hop or import, in `modal` (spec F6 result). */
export function showReceipt(modal: Modal, receipt: Receipt, subtitle: string, hooks: JobHooks): void {
  modal.setTitle(receipt.warning ? 'Written, with a warning' : 'Done', subtitle);
  const left = h('div', { class: 'col' },
    receipt.warning ? h('p', { class: 'note warn' }, receipt.warning) : null,
    h('dl', { class: 'kv' },
      h('dt', {}, 'new id'), h('dd', {}, receipt.dest_id),
      h('dt', {}, 'into'), h('dd', {}, receipt.dest_label),
      h('dt', {}, 'location'), h('dd', {}, receipt.location),
      h('dt', {}, 'manifest'), h('dd', {}, receipt.manifest_path ?? 'not saved (no mukade-data folder)')),
    receipt.refusal
      ? h('div', { class: 'field im-field' }, h('span', { class: 'lab' }, 'Resume · interactive'), h('p', { class: 'note' }, receipt.refusal))
      : commandField('Resume · interactive', receipt.interactive),
    receipt.headless.length ? commandField('Resume · headless', receipt.headless) : null,
    receipt.children.length ? h('div', { class: 'field im-field' }, h('span', { class: 'lab' }, `Subagents written · ${receipt.children.length}`),
      h('ul', { class: 'plan' }, ...receipt.children.map((child) => h('li', {}, h('span', { class: 'spawn' }, '↳ '), `${child.source_id} → ${child.dest_id}`,
        h('span', { class: child.linked ? 'muted' : 'warn-text' }, child.linked ? ' · linked to its parent' : ' · not linked: the destination has no link'))))) : null);
  const right = h('div', { class: 'col' },
    h('div', { class: 'field im-field' }, h('span', { class: 'lab' }, receipt.losses.length ? `Losses · ${receipt.losses.length}` : 'Losses'),
      receipt.losses.length ? h('ul', { class: 'plan losses' }, ...receipt.losses.map((loss) => h('li', {}, loss))) : h('p', { class: 'note' }, 'None: everything travelled.')),
    receipt.reply ? h('div', { class: 'field im-field' }, h('span', { class: 'lab' }, `Reply from ${receipt.dest_label}`), h('pre', { class: 'code' }, receipt.reply)) : null);
  fill(modal.body, left, right);
  const close = button(['Close ', caps('Enter')], { class: 'primary', onclick: () => modal.close() });
  fill(modal.foot,
    receipt.manifest_path ? copyButton(receipt.manifest_path) : null,
    receipt.manifest_path ? h('span', { class: 'mono muted' }, 'manifest path') : null,
    h('span', { class: 'spring' }),
    button('Show in sessions', { onclick: () => { modal.close(); hooks.showSession(receipt.dest_key, receipt.dest_id); } }),
    receipt.refusal ? null : button(['Resume in terminal'], { onclick: () => hooks.resumeIn(receipt.dest_key, receipt.dest_id) }),
    close);
  modal.root.onkeydown = (event) => { if (submits(event)) { event.preventDefault(); modal.close(); } };
  queueMicrotask(() => close.focus());
}

/** F8: a document written into an installation. */
export function openImport(installations: Installation[], into: string | null, document: string | null, hooks: JobHooks): void {
  if (!writeChecked) return;
  const writable = installations.filter((item) => item.destination);
  const modal = openWriteDialog({ title: 'Import', subtitle: 'a dump document into an installation', icon: 'import' }, message => hooks.status(message, true));
  const path = Object.assign(modal.body.ownerDocument.createElement('input'), { id: 'import-path', type: 'text', placeholder: 'C:\\…\\mukade-data\\exports\\<harness>-<id>.json', value: document ?? '' });
  const dest = h('select', { id: 'import-into' }) as HTMLSelectElement;
  for (const item of writable) dest.append(h('option', { value: item.key, selected: item.key === into }, item.label));
  const prompt = h('textarea', { id: 'import-prompt', rows: '2', placeholder: 'continue' }) as HTMLTextAreaElement;
  const source = pathField(path, {
    buttonLabel: 'Choose…',
    choose: () => api.pick('open'),
    onError: error => hooks.status(errorText(error), true),
  });
  fill(modal.body, h('div', { class: 'col' },
    h('div', { class: 'field im-field' }, h('label', { for: 'import-path' }, 'Document'), source),
    h('div', { class: 'field im-field' }, h('label', { for: 'import-into' }, 'Into'), dest),
    h('div', { class: 'field im-field' }, h('label', { for: 'import-prompt' }, 'Prompt · optional, runs the destination once'), prompt),
    h('p', { class: 'note' }, 'The document becomes a new session of the installation, with its own id; the source is not touched.')));
  let importing = false;
  const run = async () => {
    if (importing) return;
    if (!path.value.trim()) { path.focus(); return; }
    const target = writable.find((item) => item.key === dest.value);
    importing = true;
    modal.lock(true);
    fill(modal.foot, h('span', { class: 'mono' }, `Importing into ${target?.label}…`), h('span', { class: 'spring' }));
    hooks.job(`importing → ${target?.label}…`);
    importing = await showWriteResult(modal, () => api.import(path.value.trim(), dest.value, prompt.value.trim() || undefined), hooks,
      'import', 'document', () => void run());
  };
  fill(modal.foot, h('span', { class: 'spring' }), button(['Cancel ', caps('Escape')], { onclick: () => modal.close() }), button(['Import ', caps('Enter')], { class: 'primary', onclick: run }));
  modal.root.onkeydown = (event) => { if (submits(event)) { event.preventDefault(); void run(); } };
}
