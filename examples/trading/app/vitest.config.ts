// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { defineConfig } from 'vitest/config';
import { fileURLToPath } from 'node:url';

const emulatedX64 = process.env.DRASI_EMULATED_X64 === '1';

export default defineConfig({
  resolve: {
    mainFields: ['module', 'main'],
    dedupe: ['react', 'react-dom'],
    alias: { '@': fileURLToPath(new URL('./src', import.meta.url)) },
  },
  test: {
    environment: 'jsdom',
    include: ['test/integration/**/*.test.{ts,tsx}'],
    setupFiles: ['test/setup.ts'],
    clearMocks: true,
    restoreMocks: true,
    unstubGlobals: true,
    fileParallelism: !emulatedX64,
    maxWorkers: emulatedX64 ? 1 : undefined,
    minWorkers: emulatedX64 ? 1 : undefined,
    server: {
      deps: {
        // Linked local packages must use Vite's React deduplication, not Node's
        // separate dev-tools/react peer. Packed installs still use these modules.
        inline: [/@radix-ui\//, /react-remove-scroll/, /react-style-singleton/, /use-callback-ref/, /use-sidecar/],
      },
    },
    coverage: {
      provider: 'v8',
      // Match the package's stable AST-based statement and branch identities.
      experimentalAstAwareRemapping: true,
      include: ['src/**/*.{ts,tsx}'],
      reporter: ['text', 'json-summary', 'html'],
    },
  },
});
