// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { test, expect } from '@playwright/test';
import { writeFile } from 'node:fs/promises';
import { expectSymbols, panel, row, tradingDialog } from '../browser/locators';
import { ALL_QUERIES } from '../../src/services/queries';

interface Endpoints {
  rest: string;
  events: string;
  api: string;
  price: string;
  control: string;
  reactionPort?: number;
}

function readEndpoints(): Endpoints {
  const raw: unknown = JSON.parse(process.env.P1_LIVE_ENDPOINTS ?? 'null');
  if (!raw || typeof raw !== 'object') throw new Error('Missing real-server runtime endpoints');
  const input = raw as Record<string, unknown>;
  for (const key of ['rest', 'events', 'api', 'price', 'control']) {
    if (typeof input[key] !== 'string' || !/^http:\/\/127\.0\.0\.1:\d+$/.test(input[key])) {
      throw new Error(`Unsafe or missing test endpoint ${key}`);
    }
    if (input.reactionPort !== undefined &&
        (!Number.isInteger(input.reactionPort) || Number(input.reactionPort) < 1024 || Number(input.reactionPort) > 65535)) {
      throw new Error('Invalid isolated native reaction port');
    }
  }
  return {
    rest: String(input.rest), events: String(input.events), api: String(input.api),
    price: String(input.price), control: String(input.control),
    ...(input.reactionPort !== undefined ? { reactionPort: Number(input.reactionPort) } : {}),
  };
}

const endpoints = readEndpoints();
const instancePath = '/api/v1/instances/trading-server';

test('real Trading services: fresh provisioning, CRUD/live/delete, existing reload and reconnect', async ({ page, request, baseURL }, testInfo) => {
  const wire: Array<{ eventName: string; data: string }> = [];
  const snapshots: Array<{ path: string; recordedAt: string; body: unknown }> = [];
  const observations: Record<string, string> = {};
  const mutations: Array<{ url: string; method: string; body: unknown }> = [];
  const errors: string[] = [];
  const cdp = await page.context().newCDPSession(page);
  await cdp.send('Network.enable');
  cdp.on('Network.eventSourceMessageReceived', event => {
    wire.push({ eventName: event.eventName, data: event.data });
  });
  page.on('pageerror', error => errors.push(error.message));
  await page.route('**/*', async route => {
    const url = new URL(route.request().url());
    const method = route.request().method();
    const targets: Record<string, string> = {
      '8280': endpoints.rest,
      '8281': endpoints.events,
      '9200': endpoints.api,
    };
    if (endpoints.reactionPort) targets[String(endpoints.reactionPort)] = endpoints.events;
    const nativeSse = endpoints.reactionPort && url.hostname === '127.0.0.1' && url.port === String(endpoints.reactionPort);
    if ((url.hostname === 'localhost' || nativeSse) && targets[url.port]) {
      const postData = route.request().postData();
      if (method !== 'GET' && method !== 'OPTIONS') {
        mutations.push({ url: url.pathname, method, body: postData ? JSON.parse(postData) : null });
      }
      const target = `${targets[url.port]}${url.pathname}${url.search}`;
      // A native host cannot bind the normal demo's SSE port. Only the test bind
      // address changes; query definitions, resource creation and rows stay real.
      if (endpoints.reactionPort && method === 'POST' && url.pathname === `${instancePath}/reactions` && postData) {
        await route.continue({ url: target, postData: JSON.stringify({
          ...JSON.parse(postData), host: '127.0.0.1', port: endpoints.reactionPort,
        }) });
      } else if (endpoints.reactionPort && method === 'GET' &&
          url.pathname === `${instancePath}/reactions/sse-stream` && url.search === '?view=full') {
        // Reverse the isolated bind translation for the app's desired-definition
        // check. All runtime status, membership, query configs and rows stay real.
        const response = await route.fetch({ url: target });
        const body = await response.json();
        if (response.ok() && body.data?.config?.port === endpoints.reactionPort &&
            body.data.config.host === '127.0.0.1') {
          body.data.config.port = 8281;
          body.data.config.host = '0.0.0.0';
        }
        await route.fulfill({ response, json: body });
      } else {
        await route.continue({ url: target });
      }
      return;
    }
    if (url.origin === new URL(baseURL!).origin && !url.pathname.startsWith('/api/') && url.pathname !== '/events') {
      await route.continue();
      return;
    }
    errors.push(`Blocked unexpected outbound request: ${url.origin}${url.pathname}`);
    await route.abort('blockedbyclient');
  });

  const get = async (path: string) => {
    const response = await request.get(`${endpoints.rest}${path}`);
    expect(response.ok(), `${path}: ${await response.text()}`).toBe(true);
    const body: unknown = await response.json();
    snapshots.push({ path, recordedAt: new Date().toISOString(), body });
    if (body && typeof body === 'object' && 'data' in body) return body.data;
    return body;
  };
  const price = async (value: number) => {
    const response = await request.post(`${endpoints.price}/sources/price-feed/events`, { data: {
      operation: 'update',
      element: { type: 'node', id: 'price_AAPL', labels: ['stock_prices'], properties: {
        symbol: 'AAPL', price: value, previous_close: 100, volume: 12_000_000,
        timestamp: new Date().toISOString(),
      } },
      timestamp: Date.now() * 1_000_000,
    } });
    expect(response.ok(), await response.text()).toBe(true);
  };
  try {
    expect(await get('/api/v1/queries')).toEqual([]);
    expect(await get('/api/v1/reactions')).toEqual([]);
    await page.goto('/');
    await expect(page.getByText('Connected', { exact: true })).toBeVisible();
    await expect(page.getByRole('table')).toHaveCount(7);
    await expectSymbols(page, 'Watchlist', ['AAPL', 'MSFT']);
    await expect.poll(() => get('/api/v1/queries/portfolio-summary-query/results')).toEqual(
      [expect.objectContaining({ totalValue: 2000, totalCost: 1800, positionCount: 2 })],
    );
    observations.initialSummary = await panel(page, 'Portfolio').locator('.drasi-query-table__header-slot').innerText();
    // Soft assertions still fail the gate, but retain later reconnect evidence
    // when the baseline already has an incorrect initial aggregate snapshot.
    await expect.soft(panel(page, 'Portfolio').getByText('$2,000.00')).toBeVisible();
    const createdQueries = mutations.filter(item => item.url === `${instancePath}/queries`);
    expect(createdQueries).toHaveLength(11);
    expect(createdQueries.map(item => item.body)).toEqual(ALL_QUERIES.map(query => ({
      id: query.id, query: query.query, sources: query.sources, joins: query.joins,
      autoStart: true, queryLanguage: 'Cypher',
    })));
    expect(mutations.filter(item => item.url === `${instancePath}/reactions`)).toHaveLength(1);
    await get(`${instancePath}/queries/portfolio-summary-query?view=full`);
    await get(`${instancePath}/queries/watchlist-query?view=full`);
    await get('/api/v1/reactions/sse-stream?view=full');
    await get('/api/v1/queries/watchlist-query/results');
    await get('/api/v1/queries/portfolio-query/results');
    expect(await get('/api/v1/queries/portfolio-summary-query/results')).toEqual([
      expect.objectContaining({ totalValue: 2000, totalCost: 1800, positionCount: 2 }),
    ]);

    await page.getByTitle('Add to watchlist', { exact: true }).click();
    const watchlist = tradingDialog(page, 'Add to Watchlist');
    await watchlist.getByRole('combobox').selectOption('NVDA');
    await watchlist.getByRole('button', { name: 'Add', exact: true }).click();
    await expectSymbols(page, 'Watchlist', ['AAPL', 'MSFT', 'NVDA']);
    await row(page, 'Watchlist', 'NVDA').getByRole('button', { name: 'Remove from watchlist' }).click();
    await expectSymbols(page, 'Watchlist', ['AAPL', 'MSFT']);

    await page.getByTitle('Add position', { exact: true }).click();
    const position = tradingDialog(page, 'Add Position');
    await position.getByRole('combobox').selectOption('NVDA');
    await position.getByPlaceholder('e.g., 100').fill('2');
    await position.getByPlaceholder('e.g., 150.00').fill('100');
    await position.getByRole('button', { name: 'Add', exact: true }).click();
    await expect(row(page, 'Portfolio', 'NVDA')).toContainText('$240.00');
    await expect(panel(page, 'Portfolio').getByText('$2,240.00')).toBeVisible();
    await row(page, 'Portfolio', 'NVDA').getByRole('button', { name: 'Edit position' }).click();
    const edit = tradingDialog(page, 'Edit Position');
    await edit.getByPlaceholder('e.g., 100').fill('3');
    await edit.getByRole('button', { name: 'Save', exact: true }).click();
    await expect(row(page, 'Portfolio', 'NVDA')).toContainText('$360.00');
    await row(page, 'Portfolio', 'NVDA').getByRole('button', { name: 'Delete position' }).click();
    await tradingDialog(page, 'Delete Position').getByRole('button', { name: 'Delete', exact: true }).click();
    await expectSymbols(page, 'Portfolio', ['AAPL', 'MSFT']);

    await page.getByTitle('New limit order', { exact: true }).click();
    const order = tradingDialog(page, 'New Limit Order');
    await order.getByRole('combobox').selectOption('NVDA');
    await order.getByRole('button', { name: 'Sell', exact: true }).click();
    await order.getByPlaceholder('e.g., 150.00').fill('125');
    await order.getByPlaceholder('e.g., 100').fill('2');
    await order.getByPlaceholder('e.g., 60').fill('3600');
    await order.getByRole('button', { name: 'Create Order' }).click();
    await expect(row(page, 'Limit Orders', 'NVDA')).toContainText('pending');
    await expect(row(page, 'Limit Orders', 'NVDA')).toContainText('+4.17%');
    await row(page, 'Limit Orders', 'NVDA').getByRole('button', { name: 'Cancel order' }).click();
    await tradingDialog(page, 'Cancel Order').getByRole('button', { name: 'Cancel Order', exact: true }).click();
    await expectSymbols(page, 'Limit Orders', ['AMD', 'AMZN']);

    await price(115);
    await expect(row(page, 'Watchlist', 'AAPL')).toContainText('$115.00');
    await expect(panel(page, 'Portfolio').getByText('$2,050.00')).toBeVisible();
    observations.liveSummary = await panel(page, 'Portfolio').locator('.drasi-query-table__header-slot').innerText();
    const creationCount = mutations.filter(item => item.url.startsWith('/api/v1/')).length;
    await page.reload();
    await expect(page.getByText('Connected', { exact: true })).toBeVisible();
    await expect(row(page, 'Watchlist', 'AAPL')).toContainText('$115.00');
    await expect(panel(page, 'Portfolio').getByText('Total Value', { exact: true })).toBeVisible();
    observations.reloadedSummary = await panel(page, 'Portfolio').locator('.drasi-query-table__header-slot').innerText();
    expect(await get('/api/v1/queries/portfolio-summary-query/results')).toEqual([
      expect.objectContaining({ totalValue: 2050, totalCost: 1800, positionCount: 2 }),
    ]);
    await expect.soft(panel(page, 'Portfolio').getByText('$2,050.00')).toBeVisible();
    expect(mutations.filter(item => item.url.startsWith('/api/v1/'))).toHaveLength(creationCount);

    expect((await request.post(`${endpoints.control}/disconnect`)).ok()).toBe(true);
    await expect(page.getByText('Reconnecting...', { exact: true })).toBeVisible();
    let reconnectNavigations = 0;
    page.on('framenavigated', frame => {
      if (frame === page.mainFrame()) reconnectNavigations += 1;
    });
    await price(125);
    expect((await request.delete(`${endpoints.api}/api/watchlist/MSFT`)).ok()).toBe(true);
    // Wait for server-side processing, not an assumed fixed sleep or browser refresh.
    await expect.poll(() => get('/api/v1/queries/portfolio-summary-query/results')).toEqual(
      [expect.objectContaining({ totalValue: 2150, totalCost: 1800, positionCount: 2 })],
    );
    const settledSummary = await get('/api/v1/queries/portfolio-summary-query/results');
    await expectSymbols(page, 'Watchlist', ['AAPL', 'MSFT']);
    expect((await request.post(`${endpoints.control}/reconnect`)).ok()).toBe(true);
    await expect(page.getByText('Connected', { exact: true })).toBeVisible();
    await expectSymbols(page, 'Watchlist', ['AAPL']);
    await expect(row(page, 'Watchlist', 'AAPL')).toContainText('$125.00');
    observations.reconnectedSummary = await panel(page, 'Portfolio').locator('.drasi-query-table__header-slot').innerText();
    await get('/api/v1/queries/watchlist-query/results');
    expect(wire.length).toBeGreaterThan(0);
    expect(reconnectNavigations).toBe(0);
    expect(errors).toEqual([]);
    expect.soft(settledSummary, 'Authoritative aggregate snapshot must not contain historical intermediate rows').toEqual([
      expect.objectContaining({ totalValue: 2150, totalCost: 1800, positionCount: 2 }),
    ]);
    await expect(panel(page, 'Portfolio').getByText('$2,150.00')).toBeVisible();
  } finally {
    // Raw server/CDP records are kept apart from synthetic transport fixtures.
    await writeFile(testInfo.outputPath('server-sse.ndjson'), wire.map(event => JSON.stringify(event)).join('\n') + '\n');
    await writeFile(testInfo.outputPath('server-rest.json'), JSON.stringify(snapshots, null, 2) + '\n');
    await writeFile(testInfo.outputPath('app-mutations.json'), JSON.stringify(mutations, null, 2) + '\n');
    await writeFile(testInfo.outputPath('ui-observations.json'), JSON.stringify(observations, null, 2) + '\n');
    await testInfo.attach('server-sse.ndjson', { path: testInfo.outputPath('server-sse.ndjson'), contentType: 'application/x-ndjson' });
    await testInfo.attach('server-rest.json', { path: testInfo.outputPath('server-rest.json'), contentType: 'application/json' });
  }
});
