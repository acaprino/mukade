// Only the CI test bundle includes this probe. The production page must draw its first view and complete real startup IPC (the
// stores are read: the status row has their counts, or says why not) before the host can observe the marker and clean exit. The
// theme is told to imago-shell's plugin, which keeps it in the app's data folder: the marker the host looks for.
const ready = setInterval(async () => {
  const shell = document.querySelector('[data-slot="app-shell"]');
  const status = document.querySelector('[data-slot="app-shell-status"]')?.textContent ?? '';
  if (!shell || !shell.querySelector('[data-slot="panel"]') || !/installations|failed|could not/i.test(status)) return;
  clearInterval(ready);
  const ipc = window.__TAURI_INTERNALS__;
  await ipc.invoke('plugin:imago-shell|set_theme', { theme: 'dark', resolved: 'dark' });
  await ipc.invoke('quit');
}, 100);
