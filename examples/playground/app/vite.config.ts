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
        rewrite: (path) => {
          // Extract source ID from path like /api/inject/data-feed
          const match = path.match(/^\/api\/inject\/(.+)$/);
          if (match) {
            // Forward to HTTP source endpoint
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
