// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { defineConfig, devices } from '@playwright/test';

const port = Number(process.env.P1_WEB_PORT ?? 15273);

export default defineConfig({
  testDir: './test/browser',
  fullyParallel: false,
  workers: 1,
  retries: 0,
  forbidOnly: !!process.env.CI,
  timeout: 30_000,
  expect: {
    timeout: 5_000,
    toHaveScreenshot: { maxDiffPixels: 0, animations: 'disabled', caret: 'hide' },
  },
  reporter: [['list'], ['html', { open: 'never' }]],
  use: {
    baseURL: `http://127.0.0.1:${port}`,
    viewport: { width: 1440, height: 1000 },
    locale: 'en-US',
    timezoneId: 'UTC',
    colorScheme: 'dark',
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
    video: 'retain-on-failure',
    actionTimeout: 5_000,
  },
  projects: [
    { name: 'chromium', use: { ...devices['Desktop Chrome'], viewport: { width: 1440, height: 1000 } }, testMatch: 'trading.spec.ts' },
    { name: 'firefox', use: { ...devices['Desktop Firefox'], viewport: { width: 1440, height: 1000 } }, testMatch: 'trading.spec.ts' },
    { name: 'webkit', use: { ...devices['Desktop Safari'], viewport: { width: 1440, height: 1000 } }, testMatch: 'trading.spec.ts' },
    { name: 'visual', use: { ...devices['Desktop Chrome'], viewport: { width: 1440, height: 1000 } }, testMatch: 'visual.spec.ts' },
  ],
  snapshotPathTemplate: '{testDir}/__screenshots__/{projectName}/{arg}{ext}',
  webServer: {
    command: 'node --import tsx test/browser/server.ts',
    url: `http://127.0.0.1:${port}/__fixture/health`,
    timeout: 30_000,
    reuseExistingServer: false,
    stdout: 'pipe',
    stderr: 'pipe',
  },
});
