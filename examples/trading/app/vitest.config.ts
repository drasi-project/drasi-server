// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { defineConfig } from 'vitest/config';
import { fileURLToPath } from 'node:url';

export default defineConfig({
  resolve: {
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
    coverage: {
      provider: 'v8',
      // Match the package's stable AST-based statement and branch identities.
      experimentalAstAwareRemapping: true,
      include: ['src/**/*.{ts,tsx}'],
      reporter: ['text', 'json-summary', 'html'],
    },
  },
});
