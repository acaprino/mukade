import './styles.css';
import { invoke } from './api.ts';

(async () => {
  const info = await invoke<{ name: string; version: string }>('app_info');
  document.getElementById('version')!.textContent = info.version;
})().catch(() => {});
