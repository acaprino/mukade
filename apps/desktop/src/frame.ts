// The window frame, written once at start by Imago (shellFrameHtml, titlebarHtml, tabNavigationHtml, sidebarHtml):
// the seven regions in their canonical markup, with the app's own content in them. Nothing here holds text from a
// session: every string is a label of the app.

import { formatKeys, shellFrameHtml, sidebarHtml, tabNavigationHtml, titlebarHtml } from '@acaprino/imago/ui';
import { inTauri } from './api.ts';
import { h } from './dom.ts';

export type ViewId = 'sessions' | 'chat' | 'harnesses';

/** The views of the title bar, with the label that names the content region while the view is shown. */
export const VIEWS: readonly { id: ViewId; label: string }[] = [
  { id: 'sessions', label: 'Sessions' },
  { id: 'chat', label: 'Chat' },
  { id: 'harnesses', label: 'Harnesses' },
];

export interface Frame {
  shell: HTMLElement;
  /** The sidebar: its count, and the body that holds the installations and the folders. */
  sidebar: { count: HTMLElement; body: HTMLElement };
  /** The content region and the views inside it. */
  main: HTMLElement;
  views: { sessions: HTMLElement; reader: HTMLElement; chat: HTMLElement; harnesses: HTMLElement; sessionsView: HTMLElement };
  /** The sessions list: its header, tools (the filter) and body are written once and repainted in place. */
  list: { head: HTMLElement; tools: HTMLElement; body: HTMLElement; filter: HTMLInputElement };
  /** The detail of the selected session, which the inspector shows. It is not in the page until the inspector opens it. */
  detail: HTMLElement;
  status: { chip: HTMLElement; text: HTMLElement; version: HTMLElement };
  footer: { dot: HTMLElement; text: HTMLElement; counts: HTMLElement; keys: HTMLElement };
}

const need = <T extends Element>(root: ParentNode, selector: string): T => {
  const found = root.querySelector<T>(selector);
  if (!found) throw new Error(`frame: ${selector} is missing`);
  return found;
};

export function writeFrame(shell: HTMLElement): Frame {
  const views = '<div class="mk-views">'
    + '<div id="view-sessions" class="mk-sessions">'
    + '<section id="sessions" class="im-panel mk-view" aria-label="Sessions of the installation"><header class="im-panel-header"></header><div class="im-panel-tools"></div><div class="im-panel-body"></div></section>'
    + '<section id="reader" class="mk-view" aria-label="Transcript" hidden></section>'
    + '</div>'
    + '<section id="view-chat" class="mk-view" tabindex="0" aria-label="Chat" hidden></section>'
    + '<section id="view-harnesses" class="mk-view" tabindex="0" aria-label="Harnesses" hidden></section>'
    + '</div>';
  shell.innerHTML = shellFrameHtml({
    titlebarHtml: titlebarHtml({
      brand: { name: 'Mukade' },
      sidebarToggle: { label: 'Installations and folders', controls: 'sidebar' },
      navigationHtml: tabNavigationHtml(VIEWS, 'sessions', { label: 'Views', action: 'view' }),
      views: { label: 'Views', action: 'views' },
      commands: { label: 'Commands', action: 'palette', shortcut: formatKeys('Ctrl+K').join(' ') },
      windowControls: inTauri,
    }),
    sidebarHtml: sidebarHtml({
      id: 'sidebar', label: 'Installations and folders', rowsHtml: '', count: 0,
      collapse: { collapse: 'Collapse sidebar', expand: 'Expand sidebar' },
    }),
    statusHtml: '<span class="im-chip" data-im-tone="ok">Starting</span><span class="im-status-text"></span><span class="im-spacer"></span><span class="im-status-end"><span id="version"></span></span>',
    contentHtml: views,
    contentLabel: 'Sessions',
    skipLabel: 'Skip to content',
    inspectorLabel: 'Session',
    actionsHtml: '<span class="im-cluster">'
      + '<button type="button" class="im-btn" id="scan" data-command="scan" title="Look for portable or custom installations in a folder">Scan folder </button>'
      + '<button type="button" class="im-btn" id="import" data-command="import" title="Write a dump document into an installation">Import </button>'
      + '<button type="button" class="im-btn" id="reload" data-command="reload" title="Read every store again">Reload </button>'
      + '<button type="button" class="im-btn" id="search-everything" data-command="search-everything" title="Search rooms, members, messages and sessions">Search everything </button>'
      + '</span>',
    footerHtml: '<span class="im-dot" data-im-tone="ok"></span><span class="im-footer-text" id="status">starting…</span><span class="im-muted" id="status-counts"></span><span class="im-spacer"></span><span id="status-right"></span>',
  });
  // What the package markup leaves to the app: the names its code and tests use, and the caps from the registry.
  const bar = need<HTMLElement>(shell, '.im-titlebar');
  need(bar, '.im-views-menu').id = 'views-menu';
  need(bar, '.im-titlebar-command').id = 'commands';
  need(bar, '.im-titlebar-command').setAttribute('data-command', 'imago.palette');
  for (const tab of bar.querySelectorAll<HTMLElement>('.im-tab')) {
    tab.id = `tab-${tab.dataset.id}`;
    if (tab.dataset.id === 'chat' || tab.dataset.id === 'harnesses') tab.dataset.command = tab.dataset.id;
  }
  // The sidebar body holds two lists of its own (installations, folders), not one listbox of rows.
  const sidebar = need<HTMLElement>(shell, '#sidebar');
  const body = need<HTMLElement>(sidebar, ':scope > .im-panel-body');
  body.id = 'installations';
  body.removeAttribute('role');
  body.removeAttribute('aria-label');
  body.classList.remove('im-list');
  const count = need<HTMLElement>(sidebar, '.im-count');
  count.id = 'installation-count';
  const list = need<HTMLElement>(shell, '#sessions');
  const filter = h('input', { type: 'search', id: 'search', 'aria-label': 'Filter sessions', placeholder: 'Filter by title, id, folder', autocomplete: 'off', spellcheck: 'false' });
  const tools = need<HTMLElement>(list, ':scope > .im-panel-tools');
  tools.append(h('label', { class: 'im-input mk-filter' }, filter));
  const detail = h('div', { id: 'detail', tabindex: '-1', class: 'mk-detail' });
  return {
    shell,
    sidebar: { count, body },
    main: need(shell, '#main'),
    views: {
      sessions: list, reader: need(shell, '#reader'), chat: need(shell, '#view-chat'), harnesses: need(shell, '#view-harnesses'),
      sessionsView: need(shell, '#view-sessions'),
    },
    list: { head: need(list, ':scope > .im-panel-header'), tools, body: need(list, ':scope > .im-panel-body'), filter },
    detail,
    status: { chip: need(shell, '.im-status .im-chip'), text: need(shell, '.im-status-text'), version: need(shell, '#version') },
    footer: { dot: need(shell, '.im-footer .im-dot'), text: need(shell, '#status'), counts: need(shell, '#status-counts'), keys: need(shell, '#status-right') },
  };
}
