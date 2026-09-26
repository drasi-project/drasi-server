// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { test as base, expect, type Page } from '@playwright/test';
import { FIXED_TIME } from '../fixtures/synthetic/trading';

export { expect };
export { panel, row, tradingDialog, openTrading, expectSymbols } from './locators';

export async function prepareTradingPage(page: Page, baseURL: string, errors: string[]): Promise<void> {
  page.on('pageerror', error => errors.push(error.message));
  // Only addresses change. The built app, fetch, EventSource and SSE parser stay real.
  await page.route('**/*', route => {
    const url = new URL(route.request().url());
    if (url.hostname === 'localhost' && ['8280', '8281', '9200'].includes(url.port)) {
      return route.continue({ url: `${baseURL}${url.pathname}${url.search}` });
    }
    if (url.origin === new URL(baseURL).origin) return route.continue();
    errors.push(`Blocked unexpected outbound request: ${url.origin}${url.pathname}`);
    return route.abort('blockedbyclient');
  });
  await page.clock.install({ time: new Date(FIXED_TIME) });
  await page.clock.pauseAt(new Date(FIXED_TIME));
}

export const test = base.extend({
  page: async ({ page, request, baseURL }, use, testInfo) => {
    const reset = await request.post('/__fixture/reset');
    expect(reset.ok()).toBe(true);
    const errors: string[] = [];
    await prepareTradingPage(page, baseURL!, errors);
    await use(page);
    const state = await request.get('/__fixture/state');
    const diagnostics = await state.json();
    await testInfo.attach('synthetic-requests.json', {
      body: JSON.stringify(diagnostics, null, 2), contentType: 'application/json',
    });
    expect(diagnostics.failures).toEqual([]);
    expect(errors).toEqual([]);
  },
});
