// Copyright 2026 The Drasi Authors. Licensed under the Apache License, Version 2.0.
import { defineConfig, devices } from '@playwright/test';

const live = process.env.P7_LIVE_ENDPOINTS !== undefined;
const port = Number(process.env.P7_WEB_PORT ?? 15373);
export default defineConfig({
  testDir: './test',
  testMatch: live ? 'live.spec.ts' : 'showcase.spec.ts',
  fullyParallel: false,
  workers: 1,
  retries: 0,
  forbidOnly: !!process.env.CI,
  timeout: live ? 60_000 : 30_000,
  expect: { timeout: 5000 },
  outputDir: `test-results/${live ? 'live' : 'showcase'}`,
  reporter: [
    ['list'],
    ['html', { open: 'never', outputFolder: `playwright-report/${live ? 'live' : 'showcase'}` }],
    ['json', { outputFile: `test-results/${live ? 'live' : 'showcase'}/results.json` }],
  ],
  use: {
    baseURL: process.env.P7_WEB_URL ?? `http://127.0.0.1:${port}`,
    viewport: { width: 1100, height: 850 },
    locale: 'en-US', timezoneId: 'UTC', colorScheme: 'light',
    actionTimeout: 5000, trace: 'retain-on-failure', screenshot: 'only-on-failure',
  },
  projects: [
    { name: 'chromium', use: { ...devices['Desktop Chrome'] } },
    { name: 'firefox', use: { ...devices['Desktop Firefox'] } },
    { name: 'webkit', use: { ...devices['Desktop Safari'] } },
  ],
  webServer: live ? undefined : {
    command: 'node test/preview.mjs',
    env: { P7_SOURCE_ROOT: `${process.cwd()}/.runtime/no-backend-for-static-preview` },
    url: `http://127.0.0.1:${port}/showcase.html`,
    timeout: 30_000, reuseExistingServer: false, stdout: 'pipe', stderr: 'pipe',
  },
});
