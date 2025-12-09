import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import path from 'path'

// https://vite.dev/config/
export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    proxy: {
      // Proxy data injection requests to HTTP sources
      // Support multiple source ports (9000-9009)
      '/sources': {
        target: 'http://localhost:9000',
        changeOrigin: true,
        ws: false,
        router: (req) => {
          // Extract port from query parameter
          const url = new URL(req.url || '', `http://${req.headers?.host || 'localhost'}`);
          const port = url.searchParams.get('port') || '9000';
          return `http://localhost:${port}`;
        },
        rewrite: (path) => {
          // Remove query parameters from the path
          return path.split('?')[0];
        },
      },
    },
  },
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
    },
  },
})
