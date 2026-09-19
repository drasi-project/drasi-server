// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { afterEach, describe, expect, it, vi } from 'vitest';
import { DrasiClient, type DrasiClientOptions } from '../src/client/DrasiClient';
import { DrasiError } from '../src/client/errors';
import { fakeEventSourceFactory } from './FakeEventSource';
import { ReadServer, component, deferred, failure, json, queryConfig, refs } from './server';

const clients: DrasiClient[] = [];
const servers: ReadServer[] = [];
function setup(options: Partial<DrasiClientOptions> = {}) {
  const server = new ReadServer();
  const factory = fakeEventSourceFactory();
  const client = new DrasiClient({ ...refs, fetch: server.fetch, eventSourceFactory: factory.create, ...options });
  clients.push(client);
  servers.push(server);
  const open = async () => {
    const connected = client.initialize();
    await vi.waitFor(() => expect(factory.instances.length).toBeGreaterThan(0));
    factory.instances[factory.instances.length - 1].open();
    await connected;
  };
  return { client, server, factory, open };
}

afterEach(async () => {
  for (const client of clients.splice(0)) await client.disconnect();
  // Request-level invariant across every initialize/subscribe/retry/reconnect.
  for (const server of servers.splice(0)) {
    expect(server.fetch.mock.calls.every(([, init]) => init?.method === 'GET')).toBe(true);
    expect(server.fetch.mock.calls.every(([url]) => String(url).startsWith(
      `${refs.serverUrl}/api/v1/instances/${encodeURIComponent(refs.instanceId)}/`,
    ))).toBe(true);
  }
  vi.useRealTimers();
});

describe('connect-only DrasiClient', () => {
  it('invokes an injected native fetch with the global receiver', async () => {
    const server = new ReadServer();
    const fetcher: typeof fetch = function (this: unknown, input, init) {
      expect(this).toBe(globalThis);
      return server.fetch(input, init);
    };
    const { open } = setup({ fetch: fetcher });
    await open();
    expect(server.fetch).toHaveBeenCalled();
  });

  it('connects with references, scoped DTO reads and an explicit proxy SSE URL', async () => {
    const { client, server, factory, open } = setup();
    await open();
    await client.initialize();
    expect(factory.instances).toHaveLength(1);
    expect(factory.instances[0].url).toBe(refs.reaction.endpoint);
    expect(client.isInitialized()).toBe(true);
    expect(await client.getQueryConfig('stocks')).toMatchObject({ queryLanguage: 'Cypher', sources: [] });
    expect(await client.getQueryResults('stocks')).toEqual([{ id: 'A', value: '10', price: 10 }]);
    expect(client.getServerUiUrl()).toBe(`${refs.serverUrl}/ui?instance=${encodeURIComponent(refs.instanceId)}`);
    expect(server.fetch.mock.calls.length).toBeGreaterThan(3);
  });

  it('overlaps independent validation reads but waits for every query before opening SSE', async () => {
    const { client, server, factory } = setup({ queryIds: ['stocks', 'other'] });
    server.reaction.queries = ['stocks', 'other'];
    const first = deferred<Response>(), second = deferred<Response>();
    const read = server.fetch.getMockImplementation()!;
    server.fetch.mockImplementation((input, init) => {
      if (String(input).includes('/queries/stocks?')) return first.promise;
      if (String(input).includes('/queries/other?')) return second.promise;
      return read(input, init);
    });
    const connected = client.initialize();
    expect(server.fetch).toHaveBeenCalledTimes(2);
    expect(factory.instances).toHaveLength(0);
    second.resolve(json(component('queries', 'other', queryConfig('other'))));
    await Promise.resolve();
    expect(factory.instances).toHaveLength(0);
    first.resolve(json(component('queries', 'stocks', queryConfig('stocks'))));
    await vi.waitFor(() => expect(factory.instances).toHaveLength(1));
    factory.instances[0].open();
    await connected;
  });

  it('drains failed parallel validation before handing off to an app retry', async () => {
    const { client, server } = setup({ queryIds: ['stocks', 'other'] });
    const other = deferred<Response>();
    const read = server.fetch.getMockImplementation()!;
    let otherSignal: AbortSignal | null | undefined;
    server.fetch.mockImplementation((input, init) => {
      if (String(input).includes('/queries/stocks?')) return Promise.resolve(failure(404, 'QUERY_NOT_FOUND'));
      if (String(input).includes('/queries/other?')) {
        otherSignal = init?.signal;
        return other.promise;
      }
      return read(input, init);
    });
    const settled = vi.fn();
    const result = client.initialize().catch((error: unknown) => { settled(); return error; });
    await new Promise(resolve => setTimeout(resolve, 10));
    expect(settled).not.toHaveBeenCalled();
    expect(otherSignal?.aborted).toBe(false);
    other.resolve(failure(404, 'QUERY_NOT_FOUND'));
    expect(await result).toMatchObject({ code: 'QUERY_NOT_FOUND', resourceId: 'stocks' });
    expect(otherSignal?.aborted).toBe(false);
  });

  it.each([
    ['query', 'QUERY_NOT_FOUND', 'stocks'],
    ['reaction', 'REACTION_NOT_FOUND', 'stream'],
    ['instance', 'INSTANCE_NOT_FOUND', refs.instanceId],
  ] as const)('surfaces missing %s without provisioning or retrying', async (kind, code, resourceId) => {
    const { client, server, factory } = setup();
    server.missing = kind;
    const error = await client.initialize().catch(error => error);
    expect(error).toBeInstanceOf(DrasiError);
    expect(error).toMatchObject({ code, instanceId: refs.instanceId, resourceKind: kind, resourceId, retryable: false });
    expect(error.message).not.toContain('Sensitive');
    expect(client.getConnectionStatus().error).toBe(error);
    expect(factory.instances).toHaveLength(0);
    server.missing = null;
    const retry = client.initialize();
    await vi.waitFor(() => expect(factory.instances).toHaveLength(1));
    factory.instances[0].open();
    await retry;
  });

  it.each([
    ['Stopped', 'RESOURCE_STOPPED', false],
    ['Added', 'RESOURCE_STOPPED', false],
    ['Starting', 'RESOURCE_STARTING', true],
    ['Reconfiguring', 'RESOURCE_STARTING', true],
    ['Error', 'RESOURCE_UNAVAILABLE', false],
    ['Stopping', 'RESOURCE_UNAVAILABLE', false],
    ['Removed', 'RESOURCE_UNAVAILABLE', false],
  ] as const)('classifies %s without a start request', async (status, code, retryable) => {
    const { client, server } = setup();
    server.queryStatus = status;
    await expect(client.initialize()).rejects.toMatchObject({ code, resourceStatus: status, retryable });
    server.queryStatus = 'Running';
    server.reactionStatus = status;
    await expect(client.initialize()).rejects.toMatchObject({ code, resourceKind: 'reaction' });
  });

  it.each([401, 403, 400, 500, 503, 408, 429])('classifies HTTP %s and hides raw server errors', async status => {
    const { client, server } = setup();
    server.fetch.mockImplementation(async () => failure(status));
    const error = await client.initialize().catch(error => error);
    expect(error).toBeInstanceOf(DrasiError);
    expect(error.code).toBe(status === 401 ? 'UNAUTHENTICATED' : status === 403 ? 'FORBIDDEN'
      : status === 400 ? 'INCOMPATIBLE_RESOURCE' : 'SERVER_UNAVAILABLE');
    expect(error.status).toBe(status);
    expect(error.message).not.toContain('Sensitive');
  });

  it('classifies network errors without interpreting their text as absence', async () => {
    const { client, server } = setup();
    server.fetch.mockRejectedValue(new TypeError('404 QUERY_NOT_FOUND network secret'));
    await expect(client.initialize()).rejects.toMatchObject({ code: 'SERVER_UNAVAILABLE', retryable: true });
  });

  it.each([
    new Response('<html>Not found</html>', { status: 404 }),
    failure(404),
    new Response('bad json'),
    json({ status: 'Running', config: {} }),
    json({ id: 'stocks', status: 'Mystery', config: {} }),
    json({ id: 'stocks', status: 'Running', config: { id: 'stocks', query: 7, queryLanguage: 'Cypher', sources: [] } }),
    new Response(JSON.stringify({ success: false, data: [] })),
  ])('rejects unsupported DTOs/opaque 404s rather than inventing absence', async response => {
    const { client, server } = setup();
    server.fetch.mockResolvedValue(response);
    await expect(client.initialize()).rejects.toMatchObject({ code: 'INVALID_PAYLOAD', retryable: false });
  });

  it.each(['wrong-kind', 'missing-membership'])('rejects incompatible reaction %s', async mode => {
    const { client, server } = setup();
    if (mode === 'wrong-kind') server.reaction.kind = 'log';
    else server.reaction.queries = [];
    await expect(client.initialize()).rejects.toMatchObject({ code: 'INCOMPATIBLE_RESOURCE', resourceId: 'stream' });
  });

  it('rejects malformed reaction membership before opening a stream', async () => {
    const { client, server, factory } = setup();
    const read = server.fetch.getMockImplementation()!;
    server.fetch.mockImplementation((input, init) => String(input).includes('/reactions/')
      ? Promise.resolve(json({ id: 'stream', status: 'Running', config: { ...server.reaction, queries: [7] } }))
      : read(input, init));
    await expect(client.initialize()).rejects.toMatchObject({ code: 'INVALID_PAYLOAD', resourceId: 'stream' });
    expect(factory.instances).toHaveLength(0);
  });

  it('validates config at construction and never guesses deployment defaults', () => {
    for (const overrides of [
      { serverUrl: 'file:///private' }, { serverUrl: 'https://name:secret@host' },
      { serverUrl: 'https://host?secret=1' }, { serverUrl: 'http://0.0.0.0' },
      { instanceId: '' }, { instanceId: '..' }, { instanceId: '\uD800' },
      { queryIds: ['stocks', 'stocks'] }, { queryIds: [''] }, { queryIds: ['.'] },
      { reaction: { id: 'stream', endpoint: '/relative' } }, { requestTimeoutMs: 0 },
      { reconnect: { maxReconnectAttempts: Infinity } },
    ]) expect(() => new DrasiClient({ ...refs, ...overrides })).toThrow(DrasiError);
  });

  it('rejects traversal-like optional read IDs instead of escaping the selected instance', async () => {
    const { client, server } = setup();
    await expect(client.getQueryConfig('..')).rejects.toMatchObject({ code: 'INVALID_CONFIGURATION' });
    expect(server.fetch).not.toHaveBeenCalled();
  });

  it('rechecks REST after an opaque stream error and stops on confirmed absence', async () => {
    const { client, server, factory, open } = setup({ reconnect: { maxReconnectAttempts: 3 } });
    await open();
    const onError = vi.fn();
    client.subscribe('stocks', vi.fn(), onError);
    server.missing = 'reaction';
    factory.instances[0].fail();
    await vi.waitFor(() => expect(onError).toHaveBeenCalled());
    const error = client.getConnectionStatus().error;
    expect(error).toMatchObject({ code: 'REACTION_NOT_FOUND', retryable: false });
    expect(onError.mock.calls[onError.mock.calls.length - 1][0]).toBe(error);
    expect(client.getConnectionStatus().reconnecting).toBe(false);
    expect(factory.instances).toHaveLength(1);
  });

  it('bounds unreachable and never-opening streams without inferring missing resources', async () => {
    vi.useFakeTimers();
    const { client, factory } = setup({
      reconnect: { maxReconnectAttempts: 2, initialReconnectDelayMs: 10, maxReconnectDelayMs: 20, connectionTimeoutMs: 50 },
    });
    const connected = client.initialize();
    const rejected = expect(connected).rejects.toMatchObject({ code: 'STREAM_UNAVAILABLE', retryable: true });
    await vi.runAllTimersAsync();
    await rejected;
    expect(factory.instances).toHaveLength(3);
    expect(factory.instances.every(source => source.closed)).toBe(true);
    expect(client.getConnectionStatus().reconnecting).toBe(false);
  });

  it('stops in-flight validation on disconnect and ignores stale success', async () => {
    const { client, server, factory } = setup();
    const pending = deferred<Response>();
    server.fetch.mockImplementationOnce(() => pending.promise);
    const connected = client.initialize();
    const rejected = expect(connected).rejects.toMatchObject({ name: 'AbortError' });
    await client.disconnect();
    pending.resolve(json({ id: 'stocks', status: 'Running', config: {} }));
    await rejected;
    expect(factory.instances).toHaveLength(0);
    expect(client.isInitialized()).toBe(false);
  });

  it('bounds hung REST requests and preserves explicit abort identity', async () => {
    vi.useFakeTimers();
    const { client, server } = setup({ requestTimeoutMs: 20 });
    server.fetch.mockImplementation((_input, init) => new Promise((_resolve, reject) => {
      init?.signal?.addEventListener('abort', () => reject(init.signal?.reason), { once: true });
    }));
    const request = client.getQueryConfig('stocks');
    const rejected = expect(request).rejects.toMatchObject({ code: 'SERVER_UNAVAILABLE' });
    await vi.advanceTimersByTimeAsync(20);
    await rejected;
    const controller = new AbortController();
    const aborted = client.getQueryConfig('stocks', controller.signal);
    controller.abort();
    await expect(aborted).rejects.toBe(controller.signal.reason);
  });

  it.each(['network', 'timeout'] as const)(
    'classifies a %s failure while reading the response body as unavailable, not malformed', async mode => {
      vi.useFakeTimers();
      const { client, server } = setup({ requestTimeoutMs: 20 });
      server.fetch.mockImplementation(async (_input, init) => new Response(new ReadableStream<Uint8Array>({
        start(controller) {
          if (mode === 'network') controller.error(new TypeError('Connection reset'));
          else init?.signal?.addEventListener('abort', () => controller.error(init.signal?.reason), { once: true });
        },
      })));
      const pending = client.getQueryConfig('stocks').catch((error: unknown) => error);
      await vi.advanceTimersByTimeAsync(20);
      const error = await pending;
      expect(error).toBeInstanceOf(DrasiError);
      expect(error).toMatchObject({
        code: 'SERVER_UNAVAILABLE', retryable: true, instanceId: refs.instanceId, resourceId: 'stocks',
      });
    },
  );
});

describe('snapshot/live lifecycle', () => {
  it('reports a malformed snapshot even when a direct consumer omitted its error callback', async () => {
    const { client, server, open } = setup();
    await open();
    server.snapshot = async () => json([42]);
    const log = vi.spyOn(console, 'error').mockImplementation(() => {});
    client.subscribe('stocks', vi.fn());
    await vi.waitFor(() => expect(log).toHaveBeenCalledExactlyOnceWith(
      'Drasi query subscription failed:', 'INVALID_PAYLOAD',
    ));
  });

  it.each([10, 12])('refreshes a snapshot valued %s overlapping delta 11 instead of guessing its ordering', async price => {
    vi.useFakeTimers();
    const { client, server, factory, open } = setup({
      reconnect: { maxReconnectAttempts: 1, initialReconnectDelayMs: 10 },
    });
    const snapshot = deferred<Response>();
    server.snapshot = () => snapshot.promise;
    await open();
    const batches = vi.fn(), errors = vi.fn();
    const subscription = client.subscribe('stocks', batches, errors);
    factory.instances[0].message({
      queryId: 'stocks', timestamp: 1, results: [{ type: 'ADD', data: { id: 'A', price: 11 } }],
    });
    expect(batches).not.toHaveBeenCalled();
    snapshot.resolve(json([{ id: 'A', price }]));
    await vi.waitFor(() => expect(errors).toHaveBeenCalledOnce());
    expect(subscription.getState()).toMatchObject({ status: 'resynchronizing', error: { code: 'SNAPSHOT_OVERLAP' } });
    expect(batches).not.toHaveBeenCalled();
    server.snapshot = () => Promise.resolve(json([{ id: 'A', price: 13 }]));
    await vi.advanceTimersByTimeAsync(10);
    expect(batches.mock.calls.map(([batch]) => batch)).toEqual([
      expect.objectContaining({ kind: 'snapshot', rows: [{ id: 'A', price: 13 }] }),
    ]);
    expect(subscription.getState()).toMatchObject({ status: 'live', error: null, stale: false });
  });

  it('reports transient snapshot errors and recovers within capped retries', async () => {
    vi.useFakeTimers();
    const { client, server, factory, open } = setup({ reconnect: { maxReconnectAttempts: 2 } });
    server.snapshot = vi.fn().mockResolvedValueOnce(failure(503)).mockResolvedValue(json([{ id: 'A', price: 12 }]));
    await open();
    const onResult = vi.fn(), onError = vi.fn();
    client.subscribe('stocks', onResult, onError);
    await vi.waitFor(() => expect(onError).toHaveBeenCalledOnce());
    expect(onError.mock.calls[0][0]).toBeInstanceOf(DrasiError);
    factory.instances[0].message({
      queryId: 'stocks', timestamp: 1, results: [{ type: 'ADD', data: { id: 'A', price: 11 } }],
    });
    expect(onResult).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1000);
    expect(onResult.mock.calls[0][0]).toMatchObject({ kind: 'snapshot', rows: [{ id: 'A', price: 12 }] });
  });

  it('refreshes known reconnect overlap without replaying potentially older buffered data', async () => {
    vi.useFakeTimers();
    const { client, server, factory, open } = setup({
      reconnect: { maxReconnectAttempts: 2, initialReconnectDelayMs: 10 },
    });
    await open();
    const onResult = vi.fn(), onError = vi.fn();
    const stop = client.subscribe('stocks', onResult, onError);
    await vi.waitFor(() => expect(onResult).toHaveBeenCalledOnce());
    const snapshot = deferred<Response>();
    server.snapshot = () => snapshot.promise;
    factory.instances[0].fail();
    await vi.advanceTimersByTimeAsync(10);
    expect(factory.instances).toHaveLength(2);
    factory.instances[1].open();
    factory.instances[1].message({
      queryId: 'stocks', timestamp: 1, results: [{ type: 'ADD', data: { id: 'A', price: 13 } }],
    });
    expect(onResult).toHaveBeenCalledOnce();
    snapshot.resolve(json([{ id: 'A', price: 12 }]));
    await vi.waitFor(() => expect(onError).toHaveBeenCalledOnce());
    expect(stop.getState()).toMatchObject({ status: 'resynchronizing', stale: true });
    server.snapshot = () => Promise.resolve(json([{ id: 'A', price: 13 }]));
    await vi.advanceTimersByTimeAsync(10);
    expect(onResult.mock.calls.slice(1).map(([batch]) => batch)).toEqual([
      expect.objectContaining({ kind: 'snapshot', rows: [{ id: 'A', price: 13 }] }),
    ]);
    stop();
  });

  it('terminates snapshots after the finite retry budget rather than loading forever', async () => {
    vi.useFakeTimers();
    const { client, server, factory, open } = setup({
      reconnect: { maxReconnectAttempts: 2, initialReconnectDelayMs: 10, maxReconnectDelayMs: 15 },
    });
    server.snapshot = vi.fn(async () => failure(503));
    await open();
    const onError = vi.fn(), onResult = vi.fn();
    client.subscribe('stocks', onResult, onError);
    await vi.runAllTimersAsync();
    expect(server.snapshot).toHaveBeenCalledTimes(3);
    expect(onError).toHaveBeenCalledTimes(3);
    factory.instances[0].message({
      queryId: 'stocks', timestamp: 1, results: [{ type: 'ADD', data: { id: 'A' } }],
    });
    expect(onResult).not.toHaveBeenCalled();
  });

  it.each(['missing', 'stopped', 'malformed', 'forbidden'] as const)(
    'does not retry a permanent %s snapshot failure', async mode => {
      vi.useFakeTimers();
      const { client, server, open } = setup({ reconnect: { maxReconnectAttempts: 3 } });
      await open();
      if (mode === 'missing') server.missing = 'query';
      if (mode === 'stopped') server.queryStatus = 'Stopped';
      if (mode === 'malformed') server.snapshot = async () => json({ not: 'an array' });
      if (mode === 'forbidden') server.snapshot = async () => failure(403);
      const onError = vi.fn();
      client.subscribe('stocks', vi.fn(), onError);
      await vi.runAllTimersAsync();
      expect(onError).toHaveBeenCalledOnce();
      expect(onError.mock.calls[0][0]).toMatchObject({ retryable: false });
    },
  );

  it('rejects an unconfigured subscription and cancels pending snapshots on unsubscribe', async () => {
    const { client, server, open } = setup();
    expect(() => client.subscribe('not-configured', vi.fn())).toThrow(DrasiError);
    const invalid = vi.fn();
    client.subscribe('not-configured', vi.fn(), invalid)();
    expect(invalid.mock.calls[0][0].code).toBe('INVALID_CONFIGURATION');
    await open();
    const snapshot = deferred<Response>();
    server.snapshot = () => snapshot.promise;
    const result = vi.fn();
    const stop = client.subscribe('stocks', result);
    stop();
    snapshot.resolve(json([{ id: 'A' }]));
    await Promise.resolve();
    expect(result).not.toHaveBeenCalled();
  });
});
