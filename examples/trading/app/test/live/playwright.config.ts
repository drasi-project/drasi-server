// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { defineConfig } from '@playwright/test';
import { fileURLToPath } from 'node:url';

if (!process.env.P1_LIVE_ENDPOINTS) throw new Error('Run npm run test:live; real runtime endpoints are mandatory');
const port = Number(process.env.P1_WEB_PORT ?? 15273);

export default defineConfig({
  testDir: '.',
  testMatch: 'trading.live.spec.ts',
  workers: 1,
  retries: 0,
  forbidOnly: true,
  timeout: 120_000,
  expect: { timeout: 15_000 },
  reporter: [['list'], ['html', { outputFolder: fileURLToPath(new URL('../../playwright-report/live', import.meta.url)), open: 'never' }]],
  outputDir: fileURLToPath(new URL('../../test-results/live', import.meta.url)),
  use: {
    browserName: 'chromium',
    baseURL: `http://127.0.0.1:${port}`,
    viewport: { width: 1440, height: 1000 },
    locale: 'en-US',
    timezoneId: 'UTC',
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
    video: 'retain-on-failure',
    actionTimeout: 10_000,
  },
  webServer: {
    command: 'node --import tsx test/browser/server.ts',
    cwd: fileURLToPath(new URL('../../', import.meta.url)),
    env: { P1_REAL_SERVER_ONLY: '1' },
    url: `http://127.0.0.1:${port}/__fixture/health`,
    timeout: 30_000,
    reuseExistingServer: false,
    stdout: 'pipe',
    stderr: 'pipe',
  },
});
