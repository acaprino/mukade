import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { applyLook, guardDesktop, mountWindowControls, mountZoom, type WindowApi, type Look, type ThemeChoice } from '@acaprino/imago/shell';
import { inTauri } from './api.ts';

export function paintWindow(look: Look, theme: ThemeChoice): void {
  applyLook(document.documentElement, { app: 'mukade', look, theme, glass: inTauri }, {
    setWindowTheme: inTauri ? (theme) => invoke('set_window_theme', { theme }) : undefined,
  });
}

function windowApi(fail: (what: string, error: unknown) => void): WindowApi {
  const native = getCurrentWindow();
  const stopOnResolve = (listening: Promise<() => void>): (() => void) => {
    listening.catch((error: unknown) => fail('listen to the window', error));
    return () => { void listening.then((unlisten) => unlisten(), () => {}); };
  };
  return {
    minimize: () => native.minimize(),
    toggleMaximize: () => native.toggleMaximize(),
    close: () => native.close(),
    reportMaximizeRect: (rect) => invoke('window_caption_maximize', rect),
    onOsHover: (callback) => stopOnResolve(listen<{ hover: boolean; pressed: boolean }>('caption-maximize', ({ payload }) => {
      callback(payload.pressed ? 'pressed' : payload.hover ? 'hover' : 'none');
    })),
    onMaximized: (callback) => {
      let revision = 0;
      let live = true;
      // Several answers can be on their way at once (a window resized fast): only the last one asked is shown.
      const sync = (): void => {
        const mine = ++revision;
        native.isMaximized().then((maximized) => { if (live && mine === revision) callback(maximized); }, (error: unknown) => fail('read whether it is maximized', error));
      };
      sync();
      const stop = stopOnResolve(native.onResized(sync));
      return () => { live = false; stop(); };
    },
  };
}

export function initializeWindow(reportError: (error: unknown) => void): void {
  if (!inTauri) return;
  const bar = document.querySelector<HTMLElement>('.im-titlebar')!;
  for (const child of bar.querySelectorAll('.im-brand *')) child.setAttribute('data-tauri-drag-region', '');
  for (const button of bar.querySelectorAll<HTMLButtonElement>('[data-im-window]')) button.disabled = false;
  mountWindowControls(bar, windowApi((_what, error) => reportError(error)), { onError: (_action, error) => reportError(error) });
  mountZoom(document, { apply: level => getCurrentWebview().setZoom(level), storageKey: 'mukade.zoom' });
  if (!import.meta.env.DEV) guardDesktop(document);
}
