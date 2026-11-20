import { splitter } from '@acaprino/imago/ui';

/** Imago controls input and focus. Mukade keeps its saved pixel widths and proportional defaults. */
export function mountColumnSplitters(shell: HTMLElement): void {
  for (const name of ['installations', 'sessions'] as const) {
    const sidebar = name === 'installations';
    const pane = shell.querySelector<HTMLElement>(sidebar ? '#session-sidebar' : '#sessions')!;
    const grid = pane.parentElement!;
    const property = '--w-' + name;
    const key = 'mukade.width.' + name;
    try {
      const stored = localStorage.getItem(key);
      if (stored && /^\d+(?:\.\d+)?px$/.test(stored)) shell.style.setProperty(property, stored);
    } catch { /* Use the layout default when storage is unavailable. */ }

    let measuring = true;
    const handle = splitter(shell.ownerDocument, {
      label: 'Resize the ' + name + ' column', controls: pane.id,
      min: 180, max: 900, value: 180,
      onResize(pixels) { if (!measuring) shell.style.setProperty(property, pixels + 'px'); },
      onCommit(pixels) {
        try { localStorage.setItem(key, pixels + 'px'); } catch { /* The resize still applies. */ }
      },
    });
    const el = handle.element;
    el.id = 'resize-' + name;
    el.title = 'Drag or use arrow keys to resize · double-click to reset';
    shell.querySelector('#' + el.id)!.replaceWith(el);
    const sync = () => {
      if (!el.getClientRects().length) return;
      const width = pane.getBoundingClientRect().width;
      const lower = sidebar ? 180 : Math.max(180, Math.ceil(12 * parseFloat(getComputedStyle(grid).fontSize)));
      const upper = Math.max(lower, Math.min(900, Math.floor(grid.clientWidth * (sidebar ? 0.45 : 0.8))));
      measuring = true;
      handle.setBounds(lower, upper, width);
      measuring = false;
    };
    el.addEventListener('dblclick', () => {
      shell.style.removeProperty(property);
      try { localStorage.removeItem(key); } catch { /* The reset still applies. */ }
      sync();
    });
    measuring = false;
    const observer = new ResizeObserver(sync);
    observer.observe(grid);
    observer.observe(pane);
    sync();
  }
}
