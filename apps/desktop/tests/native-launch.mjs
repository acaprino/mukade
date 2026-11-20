// Only the CI test bundle includes this probe. The production page must render its first
// view and complete real startup IPC before the host can observe the marker and clean exit.
const ready = setInterval(async () => {
  if (!document.querySelector('#version')?.textContent || !document.querySelector('#sessions .im-panel-body')) return;
  clearInterval(ready);
  const ipc = window.__TAURI_INTERNALS__;
  await ipc.invoke('set_window_theme', { theme: 'dark' });
  await ipc.invoke('quit');
}, 100);
