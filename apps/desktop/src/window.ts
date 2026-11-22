import { invoke } from '@tauri-apps/api/core';
import { tauriWindowApi } from '@acaprino/imago/tauri';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { applyLook, guardDesktop, mountWindowControls, mountZoom, type Look, type ThemeChoice } from '@acaprino/imago/shell';
import { inTauri } from './api.ts';

export function paintWindow(look: Look, theme: ThemeChoice): void {
  applyLook(document.documentElement, { app: 'mukade', look, theme, glass: inTauri }, {
    setWindowTheme: inTauri ? (word, detail) => invoke('set_window_theme', { theme: word, look: detail.look, resolved: detail.resolved }) : undefined,
  });
}

export function initializeWindow(fail: (text: string) => void): void {
  if (!inTauri) return;
  const bar = document.querySelector<HTMLElement>('.im-titlebar')!;
  // A failed minimize, maximize, close, drag or zoom is a notice with the action named.
  const onError = (action: string, error: unknown): void => fail(`${action} failed: ${String(error)}`);
  mountWindowControls(bar, tauriWindowApi(onError), { onError });
  mountZoom(document, { apply: level => getCurrentWebview().setZoom(level), storageKey: 'mukade.zoom' });
  if (!import.meta.env.DEV) guardDesktop(document);
}
