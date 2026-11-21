// A small element builder: the views build their DOM with it, never with innerHTML, so text from a
// session is always text.

import { chipHtml, componentElement, emptyStateHtml, keyCaps, noteHtml, roving, segmentedControl, type ChipTone, type Tone } from '@acaprino/imago/ui';

type Child = Node | string | number | null | undefined | false;
type Attrs = Record<string, string | number | boolean | null | undefined | EventListener>;

export function h<K extends keyof HTMLElementTagNameMap>(tag: K, attrs: Attrs = {}, ...children: Child[]): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  for (const [name, value] of Object.entries(attrs)) {
    if (value === null || value === undefined || value === false) continue;
    if (typeof value === 'function') node.addEventListener(name.replace(/^on/, '').toLowerCase(), value);
    else if (name === 'class') node.className = String(value);
    else if (value === true) node.setAttribute(name, '');
    else node.setAttribute(name, String(value));
  }
  // Each native control carries the class of its drawing: input.im-input for a text, textarea.im-textarea, input.im-check, input.im-radio, select.im-select.
  if (tag === 'select') node.classList.add('im-select');
  if (tag === 'textarea') node.classList.add('im-textarea');
  if (tag === 'input') node.classList.add(attrs.type === 'checkbox' ? 'im-check' : attrs.type === 'radio' ? 'im-radio' : 'im-input');
  append(node, children);
  return node;
}

export function append(node: Node, children: Child[]): void {
  for (const child of children) {
    if (child === null || child === undefined || child === false) continue;
    node.appendChild(typeof child === 'object' ? child : document.createTextNode(String(child)));
  }
}

/** Replace everything inside `node`. */
export function fill(node: Element, ...children: Child[]): void {
  node.replaceChildren();
  append(node, children);
}

const SVG = 'http://www.w3.org/2000/svg';

/** One of the `<symbol>`s in index.html. */
export function icon(name: string, extra = ''): SVGSVGElement {
  const svg = document.createElementNS(SVG, 'svg');
  svg.setAttribute('class', `im-icon ${extra}`.trim());
  svg.setAttribute('aria-hidden', 'true');
  const use = document.createElementNS(SVG, 'use');
  use.setAttribute('href', `#i-${name}`);
  svg.appendChild(use);
  return svg;
}

export function button(label: Child | Child[], attrs: Attrs = {}): HTMLButtonElement {
  const node = h('button', { type: 'button', ...attrs }, ...(Array.isArray(label) ? label : [label]));
  node.classList.add(attrs.role === 'tab' ? 'im-tab' : 'im-btn');
  if (node.classList.contains('primary')) node.classList.add('im-btn--primary');
  if (node.classList.contains('ghost')) node.classList.add('im-btn--ghost');
  if (node.classList.contains('danger')) node.classList.add('im-btn--danger');
  return node;
}

/** The key caps of a binding (`Escape`, `Ctrl+Enter`, `H`): Imago writes them, in the one form every app has. */
export function caps(keys: string): HTMLElement {
  return keyCaps(document, keys);
}

/** A copy button for `text`. */
export function copyButton(text: string): HTMLButtonElement {
  const node = button('Copy', { title: 'Copy to the clipboard' });
  node.addEventListener('click', async () => {
    try { await navigator.clipboard.writeText(text); node.textContent = 'Copied'; }
    catch { node.textContent = 'Copy failed'; }
    setTimeout(() => { node.textContent = 'Copy'; }, 1400);
  });
  return node;
}

/** A command line as one shell-ish line, each part quoted when it holds a space or a quote. */
export function commandLine(argv: string[]): string {
  return argv.map((part) => (/[\s"']/.test(part) || part === '' ? `"${part.replace(/"/g, '\\"')}"` : part)).join(' ');
}

// --- fields and choices: one markup each, the package's -------------------------------------------

/** A labelled control: the label sits above it, tied by `for`. */
export function field(label: string, forId: string, ...controls: Child[]): HTMLElement {
  return h('div', { class: 'im-field' }, h('label', { class: 'im-field-label', for: forId }, label), ...controls);
}

/** A labelled group of controls (radios, a choice, a receipt part): a fieldset with its legend. */
export function group(label: string, ...controls: Child[]): HTMLElement {
  return h('fieldset', { class: 'im-field' }, h('legend', { class: 'im-field-label' }, label), ...controls);
}

/** A checkbox with its text, in an option. Disabled by `disabled`, never by a class. */
export function checkbox(label: Child, attrs: Attrs, onChange: (checked: boolean) => void, title?: string): { element: HTMLElement; input: HTMLInputElement } {
  const input = h('input', { type: 'checkbox', ...attrs }) as HTMLInputElement;
  input.addEventListener('change', () => onChange(input.checked));
  return { input, element: h('label', { class: 'im-option', title }, input, label) };
}

/** The checked input of the choice named `key`, focused again: a choice redraws its window, so the focus is given back. */
function refocus(key: string): void {
  queueMicrotask(() => document.querySelector<HTMLElement>(`[data-mk-choice="${CSS.escape(key)}"] input:checked`)?.focus());
}

/** Radios in a fieldset, each with its content (a destination and its shell, say). */
export function radios<T extends string>(label: string, items: readonly { value: T; content: Child[] }[], current: T, onPick: (value: T) => void): HTMLElement {
  const name = `mk-${label.toLowerCase().replace(/[^a-z0-9]+/g, '-')}`;
  const legend = h('legend', { class: 'im-field-label' }, label);
  const set = h('fieldset', { class: 'im-field', 'data-mk-choice': name }, legend, ...items.map((item) => {
    const input = h('input', { type: 'radio', name, value: item.value, checked: item.value === current }) as HTMLInputElement;
    input.addEventListener('change', () => { if (input.checked) { onPick(item.value); refocus(name); } });
    return h('label', { class: 'im-option' }, input, ...item.content);
  }));
  return set;
}

/** One choice among a few values, as the package's segmented control (radios in `im-seg`). */
export function choice<T extends string>(label: string, values: readonly { value: T; label: string }[], current: T, onPick: (value: T) => void): HTMLElement {
  const control = segmentedControl(document, { label, choices: values, value: current, onChange: (value) => { onPick(value as T); refocus(name); } });
  const name = `mk-${label.toLowerCase().replace(/[^a-z0-9]+/g, '-')}`;
  control.element.setAttribute('data-mk-choice', name);
  return control.element;
}

/** A choice of view: buttons with `aria-selected` in a tablist of `im-seg`. */
export function viewSwitch<T extends string>(label: string, items: readonly { value: T; content: Child[] }[], current: T, onPick: (value: T) => void): HTMLElement {
  const strip = h('div', { class: 'im-seg', role: 'tablist', 'aria-label': label }, ...items.map((item) => {
    const node = h('button', { type: 'button', role: 'tab', 'aria-selected': String(item.value === current) }, ...item.content);
    node.addEventListener('click', () => onPick(item.value));
    return node;
  }));
  // One tab stop (the selected tab) and the arrows between the tabs, as every tab strip.
  roving(strip, { item: '[role="tab"]', orientation: 'horizontal', onSelect: (tab) => tab.click() });
  return strip;
}

/** A toggle: a small button that is pressed or not (the kinds of events, a member, a recipient). */
export function toggle(label: Child | Child[], pressed: boolean, onClick: () => void, attrs: Attrs = {}): HTMLButtonElement {
  return button(label, { ...attrs, class: 'im-btn--sm', 'aria-pressed': String(pressed), onclick: onClick });
}

// --- facts, notes, chips and small controls: the package's markup ----------------------------------

/** A list of names and values: the values are content and can be selected. A code value is `im-facts-code` on its dd. */
export function facts(...children: Child[]): HTMLElement {
  return h('dl', { class: 'im-facts' }, ...children);
}

/** A message that stays in the panel it is about: a warning or an error is announced (role alert). Content may be nodes. */
export function inlineNote(content: Child | Child[], tone?: 'warn' | 'err'): HTMLElement {
  const parts = Array.isArray(content) ? content : [content];
  const plain = parts.length === 1 && typeof parts[0] === 'string';
  const element = componentElement<HTMLElement>(noteHtml({ message: plain ? (parts[0] as string) : ' ', tone }));
  if (!plain) {
    const span = element.querySelector('span')!;
    span.replaceChildren();
    append(span, parts);
  }
  return element;
}

/** A status chip with its tone (the accent is data, not a status). */
export function chip(label: string, tone: ChipTone = '', title?: string): HTMLElement {
  const element = componentElement<HTMLElement>(chipHtml(label, tone));
  if (title) element.title = title;
  return element;
}

/** A button that is an icon alone: its name is its accessible name and its tooltip. */
export function iconButton(label: string, glyph: string, attrs: Attrs = {}): HTMLButtonElement {
  const node = h('button', { type: 'button', class: 'im-icon-btn', 'aria-label': label, title: label, ...attrs }, icon(glyph));
  return node;
}

/** The empty layer of a panel (what setPanelState draws), with the app's own actions and notes after its text. */
export function emptyState(title: string, description: string, ...extras: Child[]): HTMLElement {
  const element = componentElement<HTMLElement>(emptyStateHtml(title, description));
  append(element, extras);
  return element;
}

/** A list of rows that is one tab stop, with the arrows inside. */
export function rovingList(list: HTMLElement): HTMLElement {
  list.setAttribute('role', 'group');
  roving(list, { item: '.im-row', orientation: 'vertical' });
  return list;
}

