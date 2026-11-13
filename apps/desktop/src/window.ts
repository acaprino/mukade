// The window's own frame, as Shinatama's: minimize, maximize and close in the title bar, and the
// acrylic under the glass. Outside Tauri (the browser, the smoke test) the buttons stay disabled
// and the page keeps its opaque stage.

import { inTauri } from './api.ts';

type Tint = [number, number, number, number];
// The stage of each theme. Dark is dense: the acrylic only softens a black that stays black.
const TINT: Record<'dark' | 'light', Tint> = { dark: [0, 0, 0, 205], light: [233, 233, 236, 150] };

export async function initializeWindow(reportError: (error: unknown) => void): Promise<void> {
  if (!inTauri) return;
  document.documentElement.classList.add('native-glass');
  const { getCurrentWindow } = await import('@tauri-apps/api/window');
  const appWindow = getCurrentWindow();
  const minimize = document.querySelector<HTMLButtonElement>('#window-minimize')!;
  const maximize = document.querySelector<HTMLButtonElement>('#window-maximize')!;
  const close = document.querySelector<HTMLButtonElement>('#window-close')!;
  let revision = 0;
  async function syncMaximized(): Promise<void> {
    const request = ++revision;
    const maximized = await appWindow.isMaximized();
    if (request !== revision) return;
    maximize.setAttribute('aria-label', maximized ? 'Restore window' : 'Maximize window');
    maximize.title = maximized ? 'Restore' : 'Maximize';
    maximize.querySelector('.maximize-icon')!.toggleAttribute('hidden', maximized);
    maximize.querySelector('.restore-icon')!.toggleAttribute('hidden', !maximized);
  }
  minimize.onclick = () => { void appWindow.minimize().catch(reportError); };
  maximize.onclick = () => { void appWindow.toggleMaximize().then(syncMaximized).catch(reportError); };
  // The backend's CloseRequested handler still refuses while a hop or import writes.
  close.onclick = () => { void appWindow.close().catch(reportError); };
  await appWindow.onResized(() => { void syncMaximized().catch(reportError); });
  await syncMaximized();
  for (const control of [minimize, maximize, close]) control.disabled = false;
}

/** The acrylic takes the tint of the theme in use. */
export function tintWindow(theme: 'light' | 'dark'): void {
  if (!inTauri) return;
  void import('@tauri-apps/api/window').then(({ getCurrentWindow, Effect }) =>
    getCurrentWindow().setEffects({ effects: [Effect.Acrylic], color: TINT[theme] })).catch(() => { /* the glass stays as it opened */ });
}
