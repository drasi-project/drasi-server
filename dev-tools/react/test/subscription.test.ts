// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { DrasiClient, type DrasiClientOptions } from '../src/client/DrasiClient';
import { accumulateResult } from '../src/client/accumulation';
import { DrasiError } from '../src/client/errors';
import type { QueryResult, QuerySubscription, ResultRow } from '../src/client/types';
import { fakeEventSourceFactory } from './FakeEventSource';
import { ReadServer, deferred, failure, json, refs } from './server';

const clients: DrasiClient[] = [];
beforeEach(() => vi.useFakeTimers());
afterEach(async () => {
  for (const client of clients.splice(0)) await client.disconnect();
  vi.useRealTimers();
});
const flush = () => vi.advanceTimersByTimeAsync(0);

async function fixture(overrides: Partial<DrasiClientOptions> = {}) {
  const server = new ReadServer(), factory = fakeEventSourceFactory();
  const client = new DrasiClient({
    ...refs, fetch: server.fetch, eventSourceFactory: factory.create,
    reconnect: { maxReconnectAttempts: 1, initialReconnectDelayMs: 10 }, ...overrides,
  });
  clients.push(client);
  const ready = client.initialize();
  await flush();
  factory.instances[0].open();
  await ready;
  const source = factory.instances[0];
  const emit = (results: unknown[], timestamp = 1) => source.message({ queryId: 'stocks', results, timestamp });
  return { client, server, factory, emit, source };
}

describe('bounded best-effort reconciliation', () => {
  it.each(['empty', 'newer', 'older', 'duplicate'])(
    'does not guess an ambiguous %s snapshot/deletion cut and replaces it with a refresh', async mode => {
      const f = await fixture(), pending = deferred<Response>();
      f.server.snapshot = () => pending.promise;
      const delivered = vi.fn(), errors = vi.fn();
      const subscription = f.client.subscribe('stocks', delivered, errors);
      await flush();
      f.emit(mode === 'duplicate'
        ? [{ type: 'ADD', data: { id: 'A', value: 1 } }, { type: 'ADD', data: { id: 'A', value: 1 } }]
        : [{ type: 'DELETE', data: { id: 'A' } }], mode === 'newer' ? 9000 : 0);
      pending.resolve(json(mode === 'empty' ? [] : [{ id: 'A', value: mode === 'older' ? 0 : 1 }]));
      await flush();
      expect(delivered).not.toHaveBeenCalled();
      expect(subscription.getState()).toMatchObject({
        status: 'resynchronizing', error: { code: 'SNAPSHOT_OVERLAP' }, errorScope: 'query',
      });
      f.server.snapshot = async () => json([]);
      await vi.advanceTimersByTimeAsync(10);
      expect(delivered).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({ kind: 'snapshot', rows: [] }));
      expect(subscription.getState()).toMatchObject({ status: 'live', stale: false, error: null });
      expect(f.factory.instances).toHaveLength(1);
    },
  );

  it('makes continuous known overlap terminal after the configured budget, not silently live or infinitely retrying', async () => {
    const f = await fixture();
    const pending: ReturnType<typeof deferred<Response>>[] = [];
    f.server.snapshot = () => {
      const request = deferred<Response>();
      pending.push(request);
      return request.promise;
    };
    const delivered = vi.fn(), errors = vi.fn();
    const subscription = f.client.subscribe('stocks', delivered, errors);
    await flush();
    for (let attempt = 0; attempt < 2; attempt++) {
      f.emit([{ type: 'ADD', data: { id: 'A', value: attempt } }]);
      pending[attempt].resolve(json([]));
      await flush();
      if (attempt === 0) await vi.advanceTimersByTimeAsync(10);
    }
    expect(subscription.getState()).toMatchObject({
      status: 'terminal-error', error: { code: 'SNAPSHOT_OVERLAP', retryable: true },
    });
    await vi.advanceTimersByTimeAsync(10000);
    expect(pending).toHaveLength(2);
    expect(delivered).not.toHaveBeenCalled();
    f.server.snapshot = async () => json([{ id: 'A', value: 5 }]);
    subscription.retry();
    await flush();
    expect(subscription.getState().status).toBe('live');
    expect(delivered).toHaveBeenCalledOnce();
  });

  it('does not pretend a no-known-overlap read rules out a delayed older stream event; explicit refresh reconciles it', async () => {
    const f = await fixture();
    f.server.snapshot = async () => json([{ id: 'A', value: 10 }]);
    let rows: ResultRow[] = [];
    const subscription = f.client.subscribe('stocks', result => {
      rows = accumulateResult(rows, result, row => String(row.id));
    });
    await flush();
    f.emit([{ type: 'UPDATE', before: { id: 'A', value: 8 }, after: { id: 'A', value: 9 },
      data: { id: 'A', value: 9 } }], 0);
    expect(rows).toEqual([{ id: 'A', value: 9 }]);
    expect(subscription.getState().status).toBe('live'); // Best effort, not a causal-order certificate.
    subscription.retry();
    await flush();
    expect(rows).toEqual([{ id: 'A', value: 10 }]);
  });

  it('does not mark validated heartbeat/no-op/empty/unused batches as snapshot overlap', async () => {
    const f = await fixture(), pending = deferred<Response>();
    f.server.snapshot = () => pending.promise;
    const results = vi.fn();
    const subscription = f.client.subscribe('stocks', results);
    f.source.message({ type: 'heartbeat', ts: 0 });
    f.emit([{ type: 'noop' }]);
    f.emit([]);
    f.source.message({ queryId: 'unused', timestamp: 1, results: [{ type: 'ADD', data: { id: 'B' } }] });
    pending.resolve(json([]));
    await flush();
    expect(results).toHaveBeenCalledOnce();
    expect(subscription.getState()).toMatchObject({ status: 'live', error: null });
  });

  it('cancels replaced/late replies and subscriber work, including retries after unsubscribe', async () => {
    const f = await fixture(), old = deferred<Response>(), current = deferred<Response>();
    f.server.snapshot = vi.fn().mockImplementationOnce(() => old.promise).mockImplementation(() => current.promise);
    const delivered = vi.fn(), subscription = f.client.subscribe('stocks', delivered);
    await flush();
    const oldCall = f.server.fetch.mock.calls.filter(([url]) => String(url).endsWith('/results'))[0];
    subscription.retry();
    await flush();
    expect(oldCall[1]?.signal?.aborted).toBe(true);
    old.resolve(json([{ id: 'stale' }]));
    current.resolve(json([{ id: 'current' }]));
    await flush();
    expect(delivered).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({ rows: [{ id: 'current' }] }));
    subscription();
    const reads = f.server.fetch.mock.calls.length;
    subscription.retry();
    f.emit([{ type: 'ADD', data: { id: 'late' } }]);
    await vi.advanceTimersByTimeAsync(1000);
    expect(f.server.fetch).toHaveBeenCalledTimes(reads);
    expect(delivered).toHaveBeenCalledOnce();
  });

  it('keeps snapshot callback faults subscription-local and exposes safe state even if error listeners throw', async () => {
    const f = await fixture();
    const log = vi.spyOn(console, 'error').mockImplementation(() => {});
    const other = vi.fn();
    const subscription = f.client.subscribe('stocks', () => { throw new Error('private row body'); },
      () => { throw new Error('private error listener body'); },
      () => { throw new Error('private status listener body'); });
    f.client.subscribe('stocks', other);
    await flush();
    expect(subscription.getState()).toMatchObject({
      status: 'terminal-error', error: { code: 'RESULT_PROCESSING_FAILED', resourceId: 'stocks' },
    });
    f.emit([{ type: 'ADD', data: { id: 'B' } }]);
    expect(other).toHaveBeenCalledTimes(2);
    expect(f.source.closed).toBe(false);
    expect(JSON.stringify(log.mock.calls)).not.toContain('private');
  });

  it('allows synchronous scoped retry from an error callback without scheduling an obsolete duplicate attempt', async () => {
    const f = await fixture();
    f.server.snapshot = vi.fn().mockResolvedValueOnce(failure(503)).mockImplementation(async () => json([]));
    const results = vi.fn();
    let subscription: QuerySubscription;
    subscription = f.client.subscribe('stocks', results, () => subscription.retry());
    await flush();
    expect(subscription.getState().status).toBe('live');
    await vi.advanceTimersByTimeAsync(1000);
    expect(f.server.snapshot).toHaveBeenCalledTimes(2);
    expect(results).toHaveBeenCalledOnce();
  });

  it('supports unsubscribe from result/state callbacks without late live state or extra snapshot reads', async () => {
    const f = await fixture();
    let subscription: QuerySubscription;
    const result = vi.fn<(result: QueryResult) => void>(() => subscription());
    subscription = f.client.subscribe('stocks', result);
    await flush();
    expect(result).toHaveBeenCalledOnce();
    expect(subscription.getState().status).toBe('initial-loading');
    const second = f.client.subscribe('stocks', vi.fn(), undefined, state => {
      if (state.status === 'resynchronizing') second();
    });
    await flush();
    const reads = f.server.fetch.mock.calls.length;
    second.retry();
    await flush();
    expect(f.server.fetch).toHaveBeenCalledTimes(reads);
  });

  it('keeps explicit query retry disconnected until the shared connection is repaired', async () => {
    const f = await fixture({ reconnect: { maxReconnectAttempts: 0 } });
    const errors = vi.fn(), subscription = f.client.subscribe('stocks', vi.fn(), errors);
    await flush();
    f.source.fail();
    await flush();
    const error = f.client.getConnectionStatus().error;
    expect(subscription.getState()).toMatchObject({ status: 'terminal-error', errorScope: 'connection', error });
    const reads = f.server.fetch.mock.calls.length;
    subscription.retry();
    await flush();
    expect(subscription.getState().error).toBe(error);
    expect(f.server.fetch).toHaveBeenCalledTimes(reads);
  });

  it('rejects invalid reconciliation limits and exposes unconfigured-query retry state', async () => {
    for (const maxPendingChanges of [0, -1, NaN, Infinity, 1.5]) {
      expect(() => new DrasiClient({ ...refs, reconciliation: { maxPendingChanges } })).toThrow(DrasiError);
    }
    const f = await fixture(), errors = vi.fn(), states = vi.fn();
    const invalid = f.client.subscribe('not-configured', vi.fn(), errors, states);
    invalid.retry();
    expect(errors).toHaveBeenCalledTimes(2);
    expect(states).toHaveBeenCalledOnce();
    expect(invalid.getState()).toMatchObject({ status: 'terminal-error', error: { code: 'INVALID_CONFIGURATION' } });
    invalid();
  });
});
