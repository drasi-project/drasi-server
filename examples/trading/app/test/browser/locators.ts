// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { expect, type Locator, type Page } from '@playwright/test';

export function panel(page: Page, title: string): Locator {
  return page.locator('.drasi-query-table').filter({
    has: page.getByRole('heading', { name: title, exact: true }),
  });
}

export function row(page: Page, title: string, symbol: string): Locator {
  return panel(page, title).getByRole('row').filter({
    has: page.getByRole('cell', { name: symbol, exact: true }),
  });
}

// Trading's legacy dialogs have no dialog role or associated input labels yet (P6).
export function tradingDialog(page: Page, title: string): Locator {
  return page.getByRole('heading', { level: 3, name: title, exact: true }).locator('..');
}

export async function openTrading(page: Page): Promise<void> {
  await page.goto('/');
  await expect(page.getByText('Connected', { exact: true })).toBeVisible();
  await expect(page.getByRole('table')).toHaveCount(7);
  await expect(panel(page, 'Portfolio').getByText('$2,000.00')).toBeVisible();
  await expect(panel(page, 'Watchlist').locator('tbody tr')).toHaveCount(2);
}

export async function expectSymbols(page: Page, title: string, values: string[]): Promise<void> {
  await expect(panel(page, title).locator('tbody tr td:first-child')).toHaveText(values);
}
