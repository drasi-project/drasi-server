import { defineConfig } from 'vite';
import { fileURLToPath } from 'node:url';
export default defineConfig({
  build: {
    rollupOptions: {
      input: {
        demo: fileURLToPath(new URL('./index.html', import.meta.url)),
        about: fileURLToPath(new URL('./about.html', import.meta.url)),
      },
    },
  },
  server: {
    proxy: {
      '/api/v1': 'http://127.0.0.1:8421',
      '/commands': 'http://127.0.0.1:8423',
      '/events': { target: 'http://127.0.0.1:8422', timeout: 0, proxyTimeout: 0 },
    },
  },
});
