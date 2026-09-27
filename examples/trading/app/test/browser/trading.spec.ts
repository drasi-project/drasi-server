// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { test, expect, expectSymbols, openTrading, panel, row, tradingDialog, prepareTradingPage } from './fixtures';
import { QUERY_IDS } from '../fixtures/synthetic/trading';

test('partial setup repairs only the known missing/stopped resources before reconnecting', async ({ page, request }) => {
  await openTrading(page);
  expect((await request.post('/__fixture/partial')).ok()).toBe(true);
  await page.reload();
  await expect(page.getByText('Connected', { exact: true })).toBeVisible();
  await expectSymbols(page, 'Watchlist', ['AAPL', 'MSFT']);
  await expect(panel(page, 'Portfolio').getByText('$2,000.00')).toBeVisible();
  const state = await (await request.get('/__fixture/state')).json();
  const writes = state.requests.filter((item: { method: string }) => item.method === 'POST');
  expect(writes.slice(12).map((item: { path: string }) => item.path)).toEqual([
    '/api/v1/instances/trading-server/queries/watchlist-query/start',
    '/api/v1/instances/trading-server/queries',
    '/api/v1/instances/trading-server/reactions/sse-stream/start',
  ]);
  expect(state.connections).toBe(1);
});

test('concurrent tabs share app-owned setup without duplicate start storms', async ({ page, context, request, baseURL }) => {
  const second = await context.newPage();
  const errors: string[] = [];
  try {
    await prepareTradingPage(second, baseURL!, errors);
    // Lifecycle tests need real retry timers; only visual/animation cases freeze them.
    await page.clock.resume();
    await second.clock.resume();
    await Promise.all([openTrading(page), openTrading(second)]);
    const state = await (await request.get('/__fixture/state')).json();
    const writes = state.requests.filter((item: { method: string }) => item.method === 'POST');
    expect(writes.map((item: { body: { id: string } }) => item.body.id)).toEqual([...QUERY_IDS, 'sse-stream']);
    expect(state.connections).toBe(2); // One per independent tab, never a setup connection.
    expect(errors).toEqual([]);
  } finally {
    await second.close();
  }
});

test('a second tab retries an unavailable existing stream without provisioning', async ({ page, context, request, baseURL }) => {
  await openTrading(page);
  const before = await (await request.get('/__fixture/state')).json();
  const second = await context.newPage();
  const errors: string[] = [];
  try {
    await prepareTradingPage(second, baseURL!, errors);
    await second.clock.resume();
    let rejectedStreams = 0;
    await second.route('http://localhost:8281/events', route => {
      rejectedStreams += 1;
      return route.fulfill({
        status: 503, headers: { 'Access-Control-Allow-Origin': '*' },
        contentType: 'text/plain', body: 'Controlled transient stream outage',
      });
    }, { times: 1 });
    await openTrading(second);
    const state = await (await request.get('/__fixture/state')).json();
    expect(state.requests.slice(before.requests.length).every((item: { method: string }) => item.method === 'GET')).toBe(true);
    expect(rejectedStreams).toBe(1);
    expect(state.connections).toBe(2);
    expect(errors).toEqual([]);
  } finally {
    await second.close();
  }
});

test('fresh automatic setup and existing-resource reload use the unchanged startup contract', async ({ page, request }) => {
  await openTrading(page);
  const initial = await (await request.get('/__fixture/state')).json();
  const created = initial.requests.filter((item: { method: string; path: string }) => item.method === 'POST' && item.path === '/api/v1/instances/trading-server/queries');
  expect(created.map((item: { body: { id: string } }) => item.body.id)).toEqual(QUERY_IDS);
  expect(initial.connections).toBe(1);
  await page.reload();
  await expect(page.getByText('Connected', { exact: true })).toBeVisible();
  await expectSymbols(page, 'Watchlist', ['AAPL', 'MSFT']);
  const reloaded = await (await request.get('/__fixture/state')).json();
  expect(reloaded.requests.filter((item: { method: string }) => item.method === 'POST')).toHaveLength(12);
  expect(reloaded.connections).toBe(1);
});

test('watchlist add/remove and portfolio add/edit/delete flow into rows and summaries', async ({ page }) => {
  await openTrading(page);
  await page.getByTitle('Add to watchlist', { exact: true }).click();
  const watchlist = tradingDialog(page, 'Add to Watchlist');
  await watchlist.getByRole('combobox').selectOption('NVDA');
  await watchlist.getByRole('button', { name: 'Add', exact: true }).click();
  await expectSymbols(page, 'Watchlist', ['AAPL', 'MSFT', 'NVDA']);
  await row(page, 'Watchlist', 'NVDA').getByRole('button', { name: 'Remove from watchlist' }).click();
  await expectSymbols(page, 'Watchlist', ['AAPL', 'MSFT']);

  await page.getByTitle('Add position', { exact: true }).click();
  const add = tradingDialog(page, 'Add Position');
  await add.getByRole('combobox').selectOption('NVDA');
  await add.getByPlaceholder('e.g., 100').fill('2');
  await add.getByPlaceholder('e.g., 150.00').fill('100');
  await add.getByRole('button', { name: 'Add', exact: true }).click();
  await expectSymbols(page, 'Portfolio', ['AAPL', 'MSFT', 'NVDA']);
  await expect(panel(page, 'Portfolio').getByText('$2,240.00')).toBeVisible();
  await expect(panel(page, 'Portfolio').getByText('+12.00%')).toBeVisible();
  await row(page, 'Portfolio', 'NVDA').getByRole('button', { name: 'Edit position' }).click();
  const edit = tradingDialog(page, 'Edit Position');
  await edit.getByPlaceholder('e.g., 100').fill('3');
  await edit.getByPlaceholder('e.g., 150.00').fill('110');
  await edit.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(panel(page, 'Portfolio').getByText('$2,360.00')).toBeVisible();
  await expect(panel(page, 'Portfolio').getByText('$2,130.00')).toBeVisible();
  await row(page, 'Portfolio', 'NVDA').getByRole('button', { name: 'Delete position' }).click();
  const deletion = tradingDialog(page, 'Delete Position');
  await expect(deletion.getByText('3 shares')).toBeVisible();
  await deletion.getByRole('button', { name: 'Delete', exact: true }).click();
  await expectSymbols(page, 'Portfolio', ['AAPL', 'MSFT']);
  await expect(panel(page, 'Portfolio').getByText('$2,000.00')).toBeVisible();
});

test('limit-order validation, creation, cancellation and row status presentation', async ({ page }) => {
  await openTrading(page);
  await page.getByTitle('New limit order', { exact: true }).click();
  const order = tradingDialog(page, 'New Limit Order');
  await order.getByRole('combobox').selectOption('NVDA');
  await order.getByRole('button', { name: 'Create Order' }).click();
  await expect(order.getByText('Target price is required')).toBeVisible();
  await expect(order.getByText('Quantity is required')).toBeVisible();
  await order.getByRole('button', { name: 'Sell', exact: true }).click();
  await order.getByPlaceholder('e.g., 150.00').fill('125');
  await order.getByPlaceholder('e.g., 100').fill('2');
  await order.getByRole('button', { name: 'Create Order' }).click();
  await expect(row(page, 'Limit Orders', 'NVDA')).toContainText('pending');
  await expect(row(page, 'Limit Orders', 'NVDA')).toContainText('+4.17%');
  await expect(row(page, 'Limit Orders', 'AMD')).toContainText('expired');
  await expect(row(page, 'Limit Orders', 'AMZN')).toContainText('filled');
  await row(page, 'Limit Orders', 'NVDA').getByRole('button', { name: 'Cancel order' }).click();
  await tradingDialog(page, 'Cancel Order').getByRole('button', { name: 'Cancel Order', exact: true }).click();
  await expectSymbols(page, 'Limit Orders', ['AMD', 'AMZN']);
});

test('live data, deletes, animations, sector totals, ticker and reconnect recover without refresh', async ({ page, request }) => {
  await openTrading(page);
  await page.clock.runFor(600);
  await request.post('/__fixture/price', { data: { symbol: 'AAPL', price: 90, volume: 9_000_000 } });
  await expect(row(page, 'Watchlist', 'AAPL')).toContainText('$90.00');
  await expect(row(page, 'Watchlist', 'AAPL')).toHaveClass(/drasi-row--down/);
  await expect(panel(page, 'Portfolio').getByText('$1,800.00').first()).toBeVisible();
  await expect(row(page, 'Sector Performance', 'Technology')).toContainText('57.0M');
  await expectSymbols(page, 'Top Gainers', ['NVDA', 'AMZN']);
  await expectSymbols(page, 'High Volume', ['AMZN', 'MSFT']);
  await expect(page.locator('.ticker-price').filter({ hasText: '$90.00' })).toHaveCount(1);
  await page.clock.runFor(500);
  await expect(row(page, 'Watchlist', 'AAPL')).not.toHaveClass(/drasi-row--down/);

  await request.post('/__fixture/disconnect');
  await expect(page.getByText('Reconnecting...', { exact: true })).toBeVisible();
  let reconnectNavigations = 0;
  page.on('framenavigated', frame => {
    if (frame === page.mainFrame()) reconnectNavigations += 1;
  });
  await request.post('/__fixture/price', { data: { symbol: 'AAPL', price: 125 } });
  await request.delete('/api/watchlist/MSFT');
  await expectSymbols(page, 'Watchlist', ['AAPL', 'MSFT']);
  await request.post('/__fixture/reconnect');
  // REST now classifies the opaque SSE failure before scheduling backoff.
  // Keep driving the frozen retry clock while those real HTTP reads complete.
  await expect.poll(async () => {
    await page.clock.runFor(1000);
    return page.getByText('Connected', { exact: true }).isVisible();
  }, { timeout: 5000, intervals: [100] }).toBe(true);
  await expectSymbols(page, 'Watchlist', ['AAPL']);
  await expect(row(page, 'Watchlist', 'AAPL')).toContainText('$125.00');
  expect(reconnectNavigations).toBe(0);
});

test('compatible initial sorting, explicit sorting, tutorial tabs/copy/link and fullscreen', async ({ page, browserName }) => {
  await openTrading(page);
  await expectSymbols(page, 'Top Gainers', ['NVDA', 'AAPL', 'AMZN']);
  // KB-01: preserve observed defaults, separately assert intended user-selected order.
  await expectSymbols(page, 'Top Losers', ['AMD', 'MSFT']);
  await expectSymbols(page, 'High Volume', ['AAPL', 'AMZN', 'MSFT']);
  await panel(page, 'Top Losers').locator('th').filter({ hasText: /^Change$/ }).click();
  await expectSymbols(page, 'Top Losers', ['MSFT', 'AMD']);
  const volume = panel(page, 'High Volume').locator('th').filter({ hasText: /^Volume$/ });
  await volume.click();
  await volume.press('Enter');
  await expectSymbols(page, 'High Volume', ['MSFT', 'AMZN', 'AAPL']);

  await panel(page, 'Watchlist').getByRole('button', { name: 'View code' }).click();
  const dialog = page.getByRole('dialog', { name: 'Watchlist', exact: true });
  await expect(dialog.locator('code')).toContainText('ON_WATCHLIST');
  await expect(dialog.getByRole('link', { name: 'Open in Drasi UI' })).toHaveAttribute('href', 'http://localhost:8280/ui?instance=trading-server');
  await dialog.getByRole('tab', { name: 'React Code' }).click();
  await expect(dialog.locator('code')).toContainText('<QueryTable<Stock>');
  // Native clipboard support/permission differs by engine; do not fake successful copying.
  if (browserName === 'chromium') {
    await page.context().grantPermissions(['clipboard-read', 'clipboard-write']);
    await dialog.getByRole('button', { name: 'Copy', exact: true }).click();
    await expect(dialog.getByRole('button', { name: 'Copied!' })).toBeVisible();
    expect(await page.evaluate(() => navigator.clipboard.readText())).toContain('queryId="watchlist-query"');
  }
  await page.keyboard.press('Escape');
  await expect(dialog).toHaveCount(0);
  await panel(page, 'Watchlist').getByRole('button', { name: 'Expand table' }).click();
  await page.clock.runFor(400);
  await expect(page.locator('.drasi-query-table--expanded')).toBeVisible();
  await expect(page.locator('body')).toHaveCSS('overflow', 'hidden');
  await expect(page.locator('.drasi-query-table--expanded')).toContainText('$110.00');
  await page.getByRole('button', { name: 'Collapse table' }).click();
  await page.clock.runFor(400);
  await expect(page.locator('.drasi-query-table--expanded')).toHaveCount(0);
  await expect(page.locator('body')).not.toHaveCSS('overflow', 'hidden');
});
