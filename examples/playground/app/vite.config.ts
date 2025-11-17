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
      '/api/inject': {
        target: 'http://localhost:9000',
        changeOrigin: true,
        bypass: (req, _res, options) => {
          // Extract port from query parameter if provided
          const url = new URL(req.url!, `http://${req.headers.host}`);
          const port = url.searchParams.get('port');
          if (port && port !== '9000') {
            // If a different port is specified, update the target
            (options as any).target = `http://localhost:${port}`;
          }
          return null; // Continue with proxy
        },
        rewrite: (path) => {
          // Extract source ID from path like /api/inject/data-feed?port=9001
          const match = path.match(/^\/api\/inject\/([^?]+)/);
          if (match) {
            // Forward to HTTP source endpoint (remove query params)
            return `/sources/${match[1]}/events`;
          }
          return path;
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
