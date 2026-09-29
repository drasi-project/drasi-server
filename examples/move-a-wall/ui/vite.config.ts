import { defineConfig } from 'vite';
export default defineConfig({
  server: {
    proxy: {
      '/api/v1': 'http://127.0.0.1:8421',
      '/commands': 'http://127.0.0.1:8423',
      '/events': { target: 'http://127.0.0.1:8422', timeout: 0, proxyTimeout: 0 },
    },
  },
});
