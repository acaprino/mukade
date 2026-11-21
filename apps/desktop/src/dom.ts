// A small element builder: the views build their DOM with it, never with innerHTML, so text from a
// session is always text.

import { keyCaps } from '@acaprino/imago/ui';

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
  if (tag === 'select') node.classList.add('im-select');
  if (tag === 'textarea' || (tag === 'input' && !['checkbox', 'radio'].includes(String(attrs.type)))) node.classList.add('im-input');
  if (node.classList.contains('im-chip')) {
    if (node.classList.contains('live')) node.dataset.imTone = 'ok';
    if (node.classList.contains('bad')) node.dataset.imTone = 'err';
    if (node.classList.contains('warn')) node.dataset.imTone = 'warn';
  }
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
  svg.setAttribute('class', `icon ${extra}`.trim());
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
