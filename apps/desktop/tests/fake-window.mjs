// The window of Tauri as the page finds it, for the tests that run the production build: every call is answered, and the ones of the
// plugin of the shell (`plugin:imago-shell|…`) and of Tauri's own windows and events are answered as the real ones would, with nothing.
// The app's own commands go to `window.mockCommand`, which each test exposes (src/mock.ts, or a fixture round it).
// `window.testIpc` counts what the tests ask about; `window.emit` sends an event to the page.
export function fakeWindow() {
  const state = window.testIpc = { imports: 0, hops: 0, reads: 0, calls: [] };
  let callback = 0;
  const handlers = [];
  const listeners = {};
  window.emit = (event, payload) => { for (const handler of listeners[event] ?? []) handlers[handler]({ event, id: 1, payload }); };
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
  window.__TAURI_INTERNALS__ = {
    metadata: { currentWindow: { label: 'main' }, currentWebview: { label: 'main', windowLabel: 'main' } },
    transformCallback: (handler) => { handlers[++callback] = handler; return callback; },
    unregisterCallback: () => {},
    async invoke(command, args) {
      state.calls.push(command);
      if (command === 'plugin:event|listen') { (listeners[args.event] ??= []).push(args.handler); return 1; }
      if (command === 'plugin:window|is_maximized') return false;
      if (command === 'plugin:imago-shell|kept_glass') return false;
      if (command.startsWith('plugin:')) return null;
      if (command === 'hop') state.hops++;
      if (command === 'installations') state.reads++;
      const result = await window.mockCommand(command, args);
      if (command === 'import') state.imports++;
      if (command === 'acknowledge_write') state.acknowledged = true;
      if (command === 'write_status') state.checked = true;
      return result;
    },
  };
}
