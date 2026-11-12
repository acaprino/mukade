import { defineConfig } from 'vite';
const here = new URL('.', import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1');
export default defineConfig({
  root: here,
  clearScreen: false,
  server: { host: '127.0.0.1', port: 1421, strictPort: true },
  build: { target: 'es2022', outDir: 'dist', emptyOutDir: true },
});
