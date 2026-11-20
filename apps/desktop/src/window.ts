import { invoke } from '@tauri-apps/api/core';
import { tauriWindowApi } from '@acaprino/imago/tauri';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { applyLook, guardDesktop, mountWindowControls, mountZoom, type Look, type ThemeChoice } from '@acaprino/imago/shell';
import { inTauri } from './api.ts';

export function paintWindow(look: Look, theme: ThemeChoice): void {
  applyLook(document.documentElement, { app: 'mukade', look, theme, glass: inTauri }, {
    setWindowTheme: inTauri ? (theme) => invoke('set_window_theme', { theme }) : undefined,
  });
}

export function initializeWindow(reportError: (error: unknown) => void): void {
  if (!inTauri) return;
  const bar = document.querySelector<HTMLElement>('.im-titlebar')!;
  for (const child of bar.querySelectorAll('.im-brand *')) child.setAttribute('data-tauri-drag-region', '');
  for (const button of bar.querySelectorAll<HTMLButtonElement>('[data-im-window]')) button.disabled = false;
  mountWindowControls(bar, tauriWindowApi((_what, error) => reportError(error)), { onError: (_action, error) => reportError(error) });
  mountZoom(document, { apply: level => getCurrentWebview().setZoom(level), storageKey: 'mukade.zoom' });
  if (!import.meta.env.DEV) guardDesktop(document, { keyPhase: 'bubble' });
}
