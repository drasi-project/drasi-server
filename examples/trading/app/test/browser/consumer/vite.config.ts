// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  root: fileURLToPath(new URL('.', import.meta.url)),
  base: '/__components/',
  publicDir: false,
  plugins: [react()],
  resolve: { dedupe: ['react', 'react-dom'] },
  // Exercise real StrictMode effect replay, not production's inert StrictMode wrapper.
  define: { 'process.env.NODE_ENV': JSON.stringify('development') },
  // This consumer deliberately has no Trading/Tailwind build contract.
  css: { postcss: { plugins: [] } },
  build: {
    outDir: fileURLToPath(new URL('../../../.test-runtime/consumer-dist/', import.meta.url)),
    emptyOutDir: true,
    minify: false,
  },
});
