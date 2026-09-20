// Copyright 2026 The Drasi Authors. Licensed under the Apache License, Version 2.0.
import assert from 'node:assert/strict';
import { test as base, expect, type APIRequestContext, type Page } from '@playwright/test';
import { probes, sendReading, type FeedOperation, type ProbeReading } from '../scripts/feed.mjs';
import { audit } from './audit';

function object(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}
function loopback(value: unknown): string {
  assert(typeof value === 'string' && /^http:\/\/127\.0\.0\.1:\d+$/.test(value), 'Expected an owned loopback endpoint');
  const port = Number(new URL(value).port);
  assert(port > 0 && port <= 65535, 'Expected an allocated port');
  return value;
}
const configured: unknown = JSON.parse(process.env.P7_LIVE_ENDPOINTS ?? 'null');
assert(object(configured), 'Run this suite through test/run-live.mjs');
const endpoints = {
  rest: loopback(configured.rest), feed: loopback(configured.feed), control: loopback(configured.control),
};
const web = loopback(process.env.P7_WEB_URL);
const instance = '/api/v1/instances/cold-chain';
const reaction = `${instance}/reactions/cold-chain-events`;
const queryIds = ['north-room', 'south-room'] as const;
type QueryId = typeof queryIds[number];
const entries = ['/', '/hooks.html'] as const;
type Entry = typeof entries[number];
type WireMessage = { requestId: string; timestamp: number; eventName: string; eventId: string; data: string };
interface Evidence {
  phase: string;
  requests: { phase: string; method: string; url: string; resourceType: string }[];
  responses: { owner: 'browser' | 'test'; phase: string; method: string; url: string; status: number; body: string }[];
  streamResponses: { phase: string; url: string; status: number; contentType: string | undefined }[];
  pageErrors: string[];
  consoleErrors: string[];
  failedRequests: { url: string; error: string | null }[];
  unreadResponses: { url: string; error: string }[];
  navigations: string[];
  wire: WireMessage[];
  states: { phase: string; visible: string[] }[];
  feedCommands: { phase: string; probe: ProbeReading; operation: FeedOperation; completed: boolean }[];
}

const test = base.extend<{ evidence: Evidence }>({
  evidence: async ({ page, browserName }, use, info) => {
    const evidence: Evidence = {
      phase: 'setup', requests: [], responses: [], streamResponses: [], pageErrors: [], consoleErrors: [],
      failedRequests: [], unreadResponses: [], navigations: [], wire: [], states: [], feedCommands: [],
    };
    const pending = new Set<Promise<void>>();
    page.on('request', request => {
      evidence.requests.push({
        phase: evidence.phase, method: request.method(), url: request.url(), resourceType: request.resourceType(),
      });
    });
    page.on('response', response => {
      const phase = evidence.phase;
      const path = new URL(response.url()).pathname;
      if (path === '/events') evidence.streamResponses.push({
        phase, url: response.url(), status: response.status(), contentType: response.headers()['content-type'],
      });
      if (!path.startsWith('/api/')) return;
      const reading: Promise<void> = response.text().then(body => {
        evidence.responses.push({
          owner: 'browser', phase, method: response.request().method(),
          url: response.url(), status: response.status(), body,
        });
      }, error => {
        evidence.unreadResponses.push({ url: response.url(), error: String(error) });
      }).finally(() => { pending.delete(reading); });
      pending.add(reading);
    });
    page.on('pageerror', error => { evidence.pageErrors.push(error.message); });
    page.on('console', message => {
      if (message.type() === 'error') evidence.consoleErrors.push(message.text());
    });
    page.on('requestfailed', request => {
      evidence.failedRequests.push({ url: request.url(), error: request.failure()?.errorText ?? null });
    });
    page.on('framenavigated', frame => {
      if (frame === page.mainFrame() && frame.url() !== 'about:blank') evidence.navigations.push(frame.url());
    });
    const session = browserName === 'chromium' ? await page.context().newCDPSession(page) : null;
    if (session) {
      await session.send('Network.enable');
      session.on('Network.eventSourceMessageReceived', (message: WireMessage) => { evidence.wire.push(message); });
    }
    try {
      await use(evidence);
    } finally {
      await Promise.all(pending);
      await info.attach('actual-server-and-browser-observations', {
        body: JSON.stringify({
          browser: info.project.name, version: page.context().browser()?.version(), endpoints, web,
          nativeSseCapture: session ? 'Chromium CDP, unmodified event payloads' :
            'Not exposed by this engine; actual REST response bodies and browser requests are retained.',
          feedCommands: 'Test-owned helper calls, not a fabricated wire recording.',
          humanReview: 'Not performed. Automated evidence is not AT acceptance.',
          observations: evidence,
        }, null, 2),
        contentType: 'application/json',
      });
      await session?.detach();
    }
  },
});

async function observe(
  request: APIRequestContext, evidence: Evidence, origin: string, path: string,
  method: 'GET' | 'POST' | 'DELETE' = 'GET', data?: Record<string, unknown>,
) {
  const response = await request.fetch(origin + path, { method, data, timeout: 5000 });
  const body = await response.text();
  const result = { owner: 'test' as const, phase: evidence.phase, method, url: response.url(), status: response.status(), body };
  evidence.responses.push(result);
  await response.dispose();
  return result;
}
async function successful(
  request: APIRequestContext, evidence: Evidence, path: string,
  method: 'GET' | 'POST' | 'DELETE' = 'GET', data?: Record<string, unknown>,
) {
  const response = await observe(request, evidence, endpoints.rest, path, method, data);
  expect(response.status, `${method} ${path}: ${response.body}`).toBeGreaterThanOrEqual(200);
  expect(response.status, `${method} ${path}: ${response.body}`).toBeLessThan(300);
  return response;
}
function data(body: string): unknown {
  const value: unknown = JSON.parse(body);
  assert(object(value) && value.success === true && 'data' in value, 'Expected an actual successful REST envelope');
  return value.data;
}
async function full(request: APIRequestContext, evidence: Evidence, path: string) {
  const value = data((await successful(request, evidence, `${path}?view=full`)).body);
  assert(object(value) && object(value.config), 'Expected a full component read');
  return value;
}
async function transport(request: APIRequestContext, evidence: Evidence, state: 'offline' | 'online') {
  const response = await observe(request, evidence, endpoints.control, `/${state}`, 'POST');
  expect(response.status, response.body).toBe(200);
}
async function feed(evidence: Evidence, probe: ProbeReading, operation: FeedOperation = 'update') {
  const command = { phase: evidence.phase, probe, operation, completed: false };
  evidence.feedCommands.push(command);
  await sendReading(endpoints.feed, probe, operation);
  command.completed = true;
}
async function baseline(request: APIRequestContext, evidence: Evidence, north: number, south: number | null) {
  await expect.poll(async () => Promise.all(queryIds.map(async queryId =>
    data((await successful(request, evidence, `${instance}/queries/${queryId}/results`)).body),
  )), { message: 'The actual REST baseline must contain the processed source mutations' }).toEqual([
    [{ probeId: probes[0].probeId, room: 'North', temperatureC: north }],
    south === null ? [] : [{ probeId: probes[1].probeId, room: 'South', temperatureC: south }],
  ]);
}
async function seed(request: APIRequestContext, evidence: Evidence) {
  for (const probe of probes) await feed(evidence, probe, 'insert');
  await baseline(request, evidence, 3, 5);
}
const room = (page: Page, name: 'North' | 'South') => page.getByRole('region', { name: `${name} room`, exact: true });
const connection = (page: Page) => page.getByRole('region', { name: 'Connection', exact: true });
const streams = (evidence: Evidence) => evidence.requests.filter(request => new URL(request.url).pathname === '/events');
const reads = (evidence: Evidence, queryId: QueryId) => evidence.requests.filter(request =>
  new URL(request.url).pathname === `${instance}/queries/${queryId}/results`).length;
async function visible(page: Page, evidence: Evidence, north: number, south: number | null) {
  await expect(connection(page).getByRole('status')).toHaveText('Connection: stream open');
  for (const [index, probe] of probes.entries()) {
    const section = room(page, probe.room);
    const temperature = index === 0 ? north : south;
    await expect(section.getByRole('status').filter({ hasText: `${probe.room} room query:` }))
      .toHaveText(`${probe.room} room query: ${temperature === null ? 'empty' : 'live'}`);
    await expect(section.getByText(probes[index === 0 ? 1 : 0].probeId, { exact: true })).toHaveCount(0);
    await expect(section.getByText(probe.probeId, { exact: true })).toHaveCount(temperature === null ? 0 : 1);
    if (temperature !== null) await expect(section.getByText(temperature.toFixed(1), { exact: true })).toBeVisible();
  }
  evidence.states.push({ phase: evidence.phase, visible: await page.getByRole('status').allTextContents() });
}
function readOnly(evidence: Evidence) {
  expect(evidence.pageErrors).toEqual([]);
  expect(evidence.requests.filter(request => request.method !== 'GET')).toEqual([]);
  expect(evidence.requests.filter(request => new URL(request.url).origin !== web)).toEqual([]);
  expect(evidence.requests.filter(request =>
    new URL(request.url).pathname.startsWith('/api/') &&
    !/^\/api\/v1\/instances\/cold-chain\/(?:queries\/(?:north-room|south-room)(?:\/results)?|reactions\/cold-chain-events)$/
      .test(new URL(request.url).pathname))).toEqual([]);
}
async function keyboard(page: Page, evidence: Evidence, entry: Entry) {
  if (entry === '/') {
    const heading = room(page, 'North').getByRole('columnheader', { name: 'Temperature (C)' });
    await heading.getByRole('button').focus();
    await page.keyboard.press('Enter');
    await expect(heading).toHaveAttribute('aria-sort', 'ascending');
    await page.keyboard.press('Space');
    await expect(heading).toHaveAttribute('aria-sort', 'descending');
  } else {
    const north = reads(evidence, 'north-room');
    const south = reads(evidence, 'south-room');
    const count = streams(evidence).length;
    await page.getByRole('button', { name: 'Refresh north room', exact: true }).focus();
    await page.keyboard.press('Enter');
    await expect.poll(() => reads(evidence, 'north-room')).toBeGreaterThan(north);
    await visible(page, evidence, 3, 5);
    expect(reads(evidence, 'south-room')).toBe(south);
    expect(streams(evidence)).toHaveLength(count);
  }
}

test.beforeEach(async ({ request, evidence }) => {
  await transport(request, evidence, 'online');
  await seed(request, evidence);
});
test.afterEach(async ({ request, evidence }) => {
  evidence.phase = 'cleanup';
  await transport(request, evidence, 'online');
  await seed(request, evidence);
  readOnly(evidence);
});

for (const entry of entries) {
  test(`${entry} uses one GET-only stream for real updates, deletes, keyboard interaction and reload`, async ({ page, request, evidence }, info) => {
    for (const queryId of queryIds) {
      const component = await full(request, evidence, `${instance}/queries/${queryId}`);
      expect(component.status).toBe('Running');
      assert(object(component.config));
      expect(component.config.queryLanguage).toBe('Cypher');
      expect(component.config.sources).toMatchObject([{ sourceId: 'probe-feed' }]);
      expect(component.config.query).toContain(`p.room = '${queryId === 'north-room' ? 'North' : 'South'}'`);
      expect(component.config.query).toContain('RETURN p.probeId AS probeId, p.room AS room, p.temperatureC AS temperatureC');
    }
    evidence.phase = 'initial';
    await page.goto(entry);
    await visible(page, evidence, 3, 5);
    expect(streams(evidence)).toHaveLength(1);
    await keyboard(page, evidence, entry);
    await page.setViewportSize({ width: 390, height: 844 });
    await audit(page, info, 'real-initial-narrow');
    readOnly(evidence);

    evidence.phase = 'source-update';
    await feed(evidence, { ...probes[0], temperatureC: 8 });
    await baseline(request, evidence, 8, 5);
    await visible(page, evidence, 8, 5);
    evidence.phase = 'source-delete';
    await feed(evidence, probes[1], 'delete');
    await baseline(request, evidence, 8, null);
    await visible(page, evidence, 8, null);
    expect(evidence.navigations).toHaveLength(1);
    expect(streams(evidence)).toHaveLength(1);
    await audit(page, info, 'real-empty-south');

    evidence.phase = 'reload-existing-no-provisioning';
    await page.reload();
    await visible(page, evidence, 8, null);
    expect(evidence.navigations).toHaveLength(2);
    expect(streams(evidence)).toHaveLength(2);
    if (info.project.name === 'chromium') expect(evidence.wire.length).toBeGreaterThan(0);
    readOnly(evidence);
  });

  test(`${entry} recovers an owned stream outage from processed REST state without navigation`, async ({ page, request, evidence }, info) => {
    await page.goto(entry);
    await visible(page, evidence, 3, 5);
    expect(streams(evidence)).toHaveLength(1);
    const originalUrl = page.url();
    evidence.phase = 'transport-offline-server-still-running';
    try {
      await transport(request, evidence, 'offline');
      await expect(connection(page).getByRole('status')).toHaveText('Connection: reconnecting');
      for (const probe of probes) {
        await expect(room(page, probe.room).getByRole('status').filter({ hasText: `${probe.room} room query:` }))
          .toHaveText(`${probe.room} room query: reconnecting (last-good data)`);
        await expect(room(page, probe.room).getByText(probe.probeId, { exact: true })).toBeVisible();
      }
      evidence.states.push({ phase: evidence.phase, visible: await page.getByRole('status').allTextContents() });
      await successful(request, evidence, '/health');
      await feed(evidence, { ...probes[0], temperatureC: 9 });
      await feed(evidence, probes[1], 'delete');
      await baseline(request, evidence, 9, null);
      await expect(room(page, 'North').getByText('3.0', { exact: true })).toBeVisible();
      await expect(room(page, 'South').getByText('5.0', { exact: true })).toBeVisible();

      evidence.phase = 'online-baseline-recovery';
      await transport(request, evidence, 'online');
      await visible(page, evidence, 9, null);
      expect(page.url()).toBe(originalUrl);
      expect(evidence.navigations).toHaveLength(1);
      await audit(page, info, 'real-outage-recovered');
      readOnly(evidence);
    } finally {
      await transport(request, evidence, 'online');
    }
  });

  test(`${entry} exposes actual stopped and missing resources until test-owned repair and shared retry`, async ({ page, request, evidence }, info) => {
    const queryPath = `${instance}/queries/north-room`;
    const original = await full(request, evidence, queryPath);
    assert(object(original.config));
    const fields = [
      'id', 'autoStart', 'query', 'queryLanguage', 'middleware', 'sources', 'enableBootstrap',
      'bootstrapBufferSize', 'joins', 'priorityQueueCapacity', 'dispatchBufferCapacity',
      'dispatchMode', 'storageBackend', 'outboxCapacity', 'bootstrapTimeoutSecs',
    ];
    const config = original.config;
    expect(config.id).toBe('north-room');
    expect(config.queryLanguage).toBe('Cypher');
    expect(config.autoStart).toBe(true);
    const creation = Object.fromEntries(fields.filter(field => Object.hasOwn(config, field)).map(field => [field, config[field]]));
    const originalReaction = await full(request, evidence, reaction);
    assert(object(originalReaction.config));
    const reactionConfig = originalReaction.config;
    expect(reactionConfig.id).toBe('cold-chain-events');
    expect(reactionConfig.kind).toBe('sse');
    const reactionFields = ['id', 'kind', 'queries', 'host', 'port', 'ssePath', 'heartbeatIntervalMs'];
    const reactionCreation = {
      ...Object.fromEntries(reactionFields.map(field => [field, reactionConfig[field]])),
      autoStart: true,
    };
    async function restore() {
      const current = await observe(request, evidence, endpoints.rest, `${queryPath}?view=full`);
      if (current.status === 404) {
        const missing: unknown = JSON.parse(current.body);
        assert(object(missing) && missing.code === 'QUERY_NOT_FOUND', 'Repair only the owned, REST-confirmed missing query');
        await successful(request, evidence, `${instance}/queries`, 'POST', creation);
      } else expect(current.status, current.body).toBe(200);
      await expect.poll(async () => (await full(request, evidence, queryPath)).status).toBe('Running');
      const currentReaction = await observe(request, evidence, endpoints.rest, `${reaction}?view=full`);
      if (currentReaction.status === 404) {
        const missing: unknown = JSON.parse(currentReaction.body);
        assert(object(missing) && missing.code === 'REACTION_NOT_FOUND', 'Repair only the owned missing reaction');
        await successful(request, evidence, `${instance}/reactions`, 'POST', reactionCreation);
      } else {
        expect(currentReaction.status, currentReaction.body).toBe(200);
      }
      if ((await full(request, evidence, reaction)).status === 'Stopped') {
        await successful(request, evidence, `${reaction}/start`, 'POST');
      }
      await expect.poll(async () => (await full(request, evidence, reaction)).status).toBe('Running');
    }
    try {
      evidence.phase = 'test-owned-reaction-stop';
      expect((await full(request, evidence, reaction)).status).toBe('Running');
      await successful(request, evidence, `${reaction}/stop`, 'POST');
      await expect.poll(async () => (await full(request, evidence, reaction)).status).toBe('Stopped');
      await page.goto(entry);
      await expect(connection(page).getByRole('alert')).toContainText('RESOURCE_STOPPED');
      evidence.states.push({ phase: evidence.phase, visible: await page.getByRole('status').allTextContents() });
      expect(streams(evidence)).toHaveLength(0);
      await audit(page, info, 'real-stopped-resource');

      evidence.phase = 'test-owned-reaction-removal';
      await successful(request, evidence, reaction, 'DELETE');
      await connection(page).getByRole('button', { name: 'Retry connection', exact: true }).click();
      await expect(connection(page).getByRole('alert')).toContainText('REACTION_NOT_FOUND');
      expect(streams(evidence)).toHaveLength(0);

      evidence.phase = 'test-owned-query-removal';
      await successful(request, evidence, queryPath, 'DELETE');
      const missing = await observe(request, evidence, endpoints.rest, `${queryPath}?view=full`);
      expect(missing.status).toBe(404);
      const failure: unknown = JSON.parse(missing.body);
      assert(object(failure));
      expect(failure.code).toBe('QUERY_NOT_FOUND');
      await connection(page).getByRole('button', { name: 'Retry connection', exact: true }).click();
      await expect(connection(page).getByRole('alert')).toContainText('QUERY_NOT_FOUND');
      await expect(connection(page).getByRole('status')).toHaveText('Connection: failed');
      for (const probe of probes) {
        await expect(room(page, probe.room).getByRole('status').filter({ hasText: `${probe.room} room query:` }))
          .toHaveText(`${probe.room} room query: terminal-error`);
      }
      evidence.states.push({ phase: evidence.phase, visible: await page.getByRole('status').allTextContents() });
      expect(streams(evidence)).toHaveLength(0);
      expect((await observe(request, evidence, endpoints.rest, `${queryPath}?view=full`)).status).toBe(404);
      readOnly(evidence);

      evidence.phase = 'test-owned-repair';
      await restore();
      await seed(request, evidence);
      expect((await full(request, evidence, queryPath)).config).toEqual(config);
      expect((await full(request, evidence, reaction)).config).toEqual(reactionConfig);
      evidence.phase = 'explicit-shared-retry';
      await connection(page).getByRole('button', { name: 'Retry connection', exact: true }).focus();
      await page.keyboard.press('Enter');
      await visible(page, evidence, 3, 5);
      expect(streams(evidence)).toHaveLength(1);
      expect(evidence.navigations).toHaveLength(1);
      await audit(page, info, 'real-resource-repair');
      readOnly(evidence);
    } finally {
      evidence.phase = 'resource-restoration-finally';
      await restore();
    }
  });
}
