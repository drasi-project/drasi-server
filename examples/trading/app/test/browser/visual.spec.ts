// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { test, expect, openTrading, panel, tradingDialog } from './fixtures';

test('fixed desktop dashboard, fullscreen and tutorial appearance', async ({ page }) => {
  await openTrading(page);
  await page.clock.runFor(5000);
  await expect(page).toHaveScreenshot('dashboard-desktop.png', { fullPage: true });
  await panel(page, 'Watchlist').getByRole('button', { name: 'Expand table' }).click();
  await page.clock.runFor(400);
  await expect(page.locator('.drasi-query-table--expanded')).toHaveScreenshot('watchlist-fullscreen.png');
  await page.getByRole('button', { name: 'Collapse table' }).click();
  await page.clock.runFor(400);
  await panel(page, 'Portfolio').getByRole('button', { name: 'View code' }).click();
  const inspector = page.getByRole('dialog', { name: 'Portfolio', exact: true });
  await expect(inspector.locator('code')).toContainText('OWNS_STOCK');
  await expect(inspector).toHaveScreenshot('portfolio-query-code.png');
});

test('fixed narrow viewport preserves stacked cards and existing dialog layout', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await openTrading(page);
  await page.clock.runFor(5000);
  await expect(page).toHaveScreenshot('dashboard-mobile.png', { fullPage: true });
  const cards = await page.locator('main .drasi-query-table').evaluateAll(elements =>
    elements.map(element => ({ x: element.getBoundingClientRect().x, width: element.getBoundingClientRect().width })),
  );
  expect(cards.every(card => card.x === 24 && card.width === 342)).toBe(true);
  await page.getByTitle('Add position', { exact: true }).click();
  await expect(tradingDialog(page, 'Add Position').getByRole('combobox')).toBeVisible();
  await expect(tradingDialog(page, 'Add Position')).toHaveScreenshot('add-position-mobile.png');
});
