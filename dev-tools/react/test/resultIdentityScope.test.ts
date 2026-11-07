// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { afterEach, describe, expect, it, vi } from 'vitest';
import { DrasiClient } from '../src/client/DrasiClient';
import { accumulateResult } from '../src/client/accumulation';
import { DrasiError, type DrasiErrorDetails } from '../src/client/errors';
import { createLegacyResultAdapter, sse034ResultAdapter } from '../src/client/results';
import type { OutputResult, ResultAdapter, ResultRow } from '../src/client/types';
import { fakeEventSourceFactory } from './FakeEventSource';
import { ReadServer, refs } from './server';

const context = {
  instanceId: refs.instanceId, resourceKind: 'reaction' as const, resourceId: 'ee84d7f5-adf8-4f1f-95e7-b67a67f39a0d', receivedAt: 1,
};
const adapters = [
  { name: 'canonical', adapter: sse034ResultAdapter },
  { name: 'explicit legacy', adapter: createLegacyResultAdapter() },
];
const wire = (identity: ResultRow) => ({
  ...identity, timestamp: 1, results: [{ type: 'ADD', data: { id: 'A', value: 99 } }],
});
const queryDetails = (resourceId: string) => ({
  code: 'INVALID_PAYLOAD', instanceId: refs.instanceId, resourceKind: 'query', resourceId, retryable: false,
});
const connectionDetails = {
  code: 'INVALID_PAYLOAD', instanceId: refs.instanceId, resourceKind: 'reaction', resourceId: 'ee84d7f5-adf8-4f1f-95e7-b67a67f39a0d', retryable: false,
};

function rejected(adapter: ResultAdapter, payload: unknown): unknown {
  try {
    adapter(payload, context);
  } catch (error) {
    expect(error).toBeInstanceOf(DrasiError);
    return error;
  }
  throw new Error('Expected invalid envelope identity to be rejected');
}

describe.each(adapters)('$name adapter identity error details', ({ adapter }) => {
  it.each([7, null, '', '..', { private: 'not a query ID' }, 'e4ba7cee-4b9b-4b24-ad4e-a58f0e04d481'])(
    'keeps a valid canonical query when query_id is invalid or conflicting: %j', query_id => {
      const error = rejected(adapter, wire({ queryId: 'b20c42ee-59a1-441f-a549-87a72be1f1e9', query_id }));
      expect(error).toMatchObject(queryDetails('b20c42ee-59a1-441f-a549-87a72be1f1e9'));
      expect(String(error)).not.toContain('private');
    },
  );

  it.each([
    { queryId: 7, query_id: false },
    { queryId: '', query_id: 'b20c42ee-59a1-441f-a549-87a72be1f1e9' },
    { queryId: '..', query_id: 'b20c42ee-59a1-441f-a549-87a72be1f1e9' },
    { queryId: '\uD800', query_id: 'b20c42ee-59a1-441f-a549-87a72be1f1e9' },
  ])('does not guess a query from an invalid higher-priority identity: %j', identity => {
    expect(rejected(adapter, wire(identity))).toMatchObject(connectionDetails);
  });

  it('accepts canonical identity and preserves the result unchanged', () => {
    expect(adapter(wire({ queryId: 'b20c42ee-59a1-441f-a549-87a72be1f1e9' }), context)).toEqual([{
      kind: 'delta', outputId: 'b20c42ee-59a1-441f-a549-87a72be1f1e9', receivedAt: 1, sourceTimestamp: 1,
      changes: [{ kind: 'upsert', after: { id: 'A', value: 99 } }],
    }]);
  });

  it('keeps a valid but unsubscribed canonical ID instead of blaming a subscribed alias', () => {
    expect(rejected(adapter, wire({ queryId: 'unsubscribed', query_id: 'e4ba7cee-4b9b-4b24-ad4e-a58f0e04d481' })))
      .toMatchObject(queryDetails('unsubscribed'));
  });
});

describe('explicit identity compatibility and unidentifiable controls', () => {
  it('does not recognize a legacy-only identity unless that adapter is selected', () => {
    expect(rejected(sse034ResultAdapter, wire({ query_id: 'b20c42ee-59a1-441f-a549-87a72be1f1e9' }))).toMatchObject(connectionDetails);
    expect(createLegacyResultAdapter()(wire({ query_id: 'b20c42ee-59a1-441f-a549-87a72be1f1e9' }), context)[0].outputId).toBe('b20c42ee-59a1-441f-a549-87a72be1f1e9');
  });

  it('accepts matching aliases only in legacy mode while retaining canonical error scope in strict mode', () => {
    const payload = wire({ queryId: 'b20c42ee-59a1-441f-a549-87a72be1f1e9', query_id: 'b20c42ee-59a1-441f-a549-87a72be1f1e9' });
    expect(rejected(sse034ResultAdapter, payload)).toMatchObject(queryDetails('b20c42ee-59a1-441f-a549-87a72be1f1e9'));
    expect(createLegacyResultAdapter()(payload, context)[0].outputId).toBe('b20c42ee-59a1-441f-a549-87a72be1f1e9');
  });

  it('preserves nullish selection of an explicitly enabled alias without accepting malformed canonical IDs', () => {
    const payload = wire({ queryId: null, query_id: 'b20c42ee-59a1-441f-a549-87a72be1f1e9' });
    expect(rejected(sse034ResultAdapter, payload)).toMatchObject(connectionDetails);
    expect(rejected(createLegacyResultAdapter(), payload)).toMatchObject(queryDetails('b20c42ee-59a1-441f-a549-87a72be1f1e9'));
  });

  it('keeps both missing or invalid identities connection-scoped, without content routing', () => {
    const route = vi.fn();
    const legacy = createLegacyResultAdapter({ routeUnidentified: route });
    for (const adapter of [sse034ResultAdapter, legacy]) {
      expect(rejected(adapter, {})).toMatchObject(connectionDetails);
      expect(rejected(adapter, wire({ queryId: null, query_id: 7 }))).toMatchObject(connectionDetails);
    }
    expect(route).not.toHaveBeenCalled();
  });

  it('scopes malformed legacy content to the explicitly recognized legacy query', () => {
    expect(rejected(createLegacyResultAdapter(), { query_id: 'b20c42ee-59a1-441f-a549-87a72be1f1e9', results: [null] }))
      .toMatchObject(queryDetails('b20c42ee-59a1-441f-a549-87a72be1f1e9'));
  });
});

const clients: DrasiClient[] = [];
afterEach(async () => {
  for (const client of clients.splice(0)) await client.disconnect();
  vi.useRealTimers();
});

async function connectedClient(adapter: ResultAdapter) {
  vi.useFakeTimers();
  const server = new ReadServer(), factory = fakeEventSourceFactory();
  server.reaction.queries = [{ id: 'b20c42ee-59a1-441f-a549-87a72be1f1e9' }, { id: 'e4ba7cee-4b9b-4b24-ad4e-a58f0e04d481' }];
  const client = new DrasiClient({
    ...refs, outputIds: ['b20c42ee-59a1-441f-a549-87a72be1f1e9', 'e4ba7cee-4b9b-4b24-ad4e-a58f0e04d481'], fetch: server.fetch, eventSourceFactory: factory.create,
    resultAdapter: adapter, reconnect: { maxReconnectAttempts: 2, initialReconnectDelayMs: 10 },
  });
  clients.push(client);
  const connected = client.initialize();
  await vi.advanceTimersByTimeAsync(0);
  expect(factory.instances).toHaveLength(1);
  const source = factory.instances[0];
  source.open();
  await connected;
  const subscribe = vi.spyOn(client, 'subscribe'), states = vi.fn();
  client.onConnectionStatusChange(states);
  function listener(queryId: string) {
    let rows: ResultRow[] = [];
    const results = vi.fn((result: OutputResult) => {
      rows = accumulateResult(rows, result, row => typeof row.id === 'string' ? row.id : '');
    });
    const errors = vi.fn();
    const subscription = client.subscribe(queryId, results, errors);
    return { results, errors, subscription, rows: () => rows };
  }
  const first = listener('b20c42ee-59a1-441f-a549-87a72be1f1e9'), other = listener('e4ba7cee-4b9b-4b24-ad4e-a58f0e04d481');
  await vi.advanceTimersByTimeAsync(0);
  expect(first.subscription.getState().status).toBe('live');
  expect(other.subscription.getState().status).toBe('live');
  return { client, server, factory, source, subscribe, states, first, other };
}

describe.each(adapters)('$name shared client identity failure isolation', ({ adapter }) => {
  it.each([7, 'e4ba7cee-4b9b-4b24-ad4e-a58f0e04d481'])('fails only the recognized query for query_id=%j without interrupting healthy rows', async query_id => {
    const f = await connectedClient(adapter), reads = f.server.fetch.mock.calls.length;
    const original = f.first.rows(), status = f.client.getConnectionStatus();
    f.source.message(wire({ queryId: 'b20c42ee-59a1-441f-a549-87a72be1f1e9', query_id }));
    const error = f.first.errors.mock.calls[0]?.[0];
    expect(error).toBeInstanceOf(DrasiError);
    expect(error).toMatchObject(queryDetails('b20c42ee-59a1-441f-a549-87a72be1f1e9'));
    expect(f.first.subscription.getState()).toMatchObject({
      status: 'terminal-error', errorScope: 'query', stale: true, error,
    });
    expect(f.first.subscription.getState().error).toBe(error);
    expect(f.other.errors).not.toHaveBeenCalled();
    expect(f.other.subscription.getState()).toMatchObject({ status: 'live', error: null, stale: false });
    expect(f.first.rows()).toBe(original);
    expect(f.first.results).toHaveBeenCalledOnce();
    expect(f.other.results).toHaveBeenCalledOnce();
    expect(f.client.getConnectionStatus()).toEqual(status);
    expect(f.states).toHaveBeenCalledOnce();
    expect(f.source.closed).toBe(false);

    f.source.message({
      queryId: 'e4ba7cee-4b9b-4b24-ad4e-a58f0e04d481', timestamp: 2, results: [{ type: 'ADD', data: { id: 'B', value: 20 } }],
    });
    f.source.message({
      queryId: 'e4ba7cee-4b9b-4b24-ad4e-a58f0e04d481', timestamp: 3, results: [{ type: 'DELETE', data: { id: 'A' } }],
    });
    await vi.advanceTimersByTimeAsync(100);
    expect(f.other.rows()).toEqual([{ id: 'B', value: 20 }]);
    expect(f.other.results).toHaveBeenCalledTimes(3);
    expect(f.first.rows()).toBe(original);
    expect(f.subscribe).toHaveBeenCalledTimes(2);
    expect(f.server.fetch).toHaveBeenCalledTimes(reads);
    expect(f.factory.instances).toHaveLength(1);
    expect(f.states).toHaveBeenCalledOnce();
    expect(f.source.closed).toBe(false);
    expect(f.server.fetch.mock.calls.every(([, init]) => init?.method === 'GET')).toBe(true);
  });

  it('does not assign an unsubscribed canonical query fault to either healthy subscriber', async () => {
    const f = await connectedClient(adapter), reads = f.server.fetch.mock.calls.length;
    f.source.message(wire({ queryId: 'unsubscribed', query_id: 'e4ba7cee-4b9b-4b24-ad4e-a58f0e04d481' }));
    await vi.advanceTimersByTimeAsync(100);
    expect(f.first.errors).not.toHaveBeenCalled();
    expect(f.other.errors).not.toHaveBeenCalled();
    expect(f.first.rows()).toEqual(f.other.rows());
    expect(f.first.results).toHaveBeenCalledOnce();
    expect(f.other.results).toHaveBeenCalledOnce();
    expect(f.first.subscription.getState().status).toBe('live');
    expect(f.other.subscription.getState().status).toBe('live');
    expect(f.states).toHaveBeenCalledOnce();
    expect(f.subscribe).toHaveBeenCalledTimes(2);
    expect(f.server.fetch).toHaveBeenCalledTimes(reads);
    expect(f.factory.instances).toHaveLength(1);
    expect(f.source.closed).toBe(false);
  });

  it('terminates the shared connection when neither identity is recognizable', async () => {
    const f = await connectedClient(adapter), reads = f.server.fetch.mock.calls.length;
    f.source.message(wire({ queryId: 7, query_id: false }));
    const error = f.client.getConnectionStatus().error;
    expect(error).toMatchObject(connectionDetails);
    expect(f.first.errors).toHaveBeenCalledExactlyOnceWith(error);
    expect(f.other.errors).toHaveBeenCalledExactlyOnceWith(error);
    expect(f.first.subscription.getState()).toMatchObject({ status: 'terminal-error', errorScope: 'connection', error });
    expect(f.other.subscription.getState()).toMatchObject({ status: 'terminal-error', errorScope: 'connection', error });
    expect(f.client.getConnectionStatus()).toMatchObject({ connected: false, reconnecting: false, error });
    expect(f.first.rows()).toEqual(f.other.rows());
    expect(f.first.results).toHaveBeenCalledOnce();
    expect(f.other.results).toHaveBeenCalledOnce();
    expect(f.source.closed).toBe(true);
    await vi.advanceTimersByTimeAsync(100);
    expect(f.server.fetch).toHaveBeenCalledTimes(reads);
    expect(f.factory.instances).toHaveLength(1);
    expect(f.subscribe).toHaveBeenCalledTimes(2);
  });
});

const incompleteQueryScopes: { name: string; details: DrasiErrorDetails }[] = [
  { name: 'unscoped', details: {} },
  { name: 'reaction-scoped', details: { resourceKind: 'reaction', resourceId: 'adapter-stream' } },
  { name: 'query without an ID', details: { resourceKind: 'query' } },
  { name: 'query with an empty ID', details: { resourceKind: 'query', resourceId: '' } },
];

describe.each(incompleteQueryScopes)('$name typed adapter failures', ({ details }) => {
  it.each([
    { queryId: 'b20c42ee-59a1-441f-a549-87a72be1f1e9' },
    { queryId: 'b20c42ee-59a1-441f-a549-87a72be1f1e9', query_id: 'e4ba7cee-4b9b-4b24-ad4e-a58f0e04d481' },
    { query_id: 'b20c42ee-59a1-441f-a549-87a72be1f1e9' },
    { queryId: null, query_id: 'b20c42ee-59a1-441f-a549-87a72be1f1e9' },
  ])('applies recognized raw query identity %j without interrupting healthy queries', async identity => {
    const failure = new DrasiError('FORBIDDEN', {
      instanceId: 'adapter-instance', ...details, status: 403, resourceStatus: 'Denied',
    });
    const adapter = vi.fn<ResultAdapter>(sse034ResultAdapter)
      .mockImplementationOnce(() => { throw failure; });
    const f = await connectedClient(adapter), reads = f.server.fetch.mock.calls.length;
    const original = f.first.rows(), status = f.client.getConnectionStatus();
    f.source.message(wire(identity));
    const error = f.first.errors.mock.calls[0]?.[0];
    expect(error).toBeInstanceOf(DrasiError);
    expect(error).toMatchObject({
      code: 'FORBIDDEN', instanceId: refs.instanceId, resourceKind: 'query', resourceId: 'b20c42ee-59a1-441f-a549-87a72be1f1e9',
      status: 403, resourceStatus: 'Denied', retryable: false,
    });
    expect(error).not.toBe(failure);
    expect(f.first.errors).toHaveBeenCalledExactlyOnceWith(error);
    expect(f.first.subscription.getState()).toMatchObject({
      status: 'terminal-error', errorScope: 'query', stale: true, error,
    });
    expect(f.first.subscription.getState().error).toBe(error);
    expect(f.other.errors).not.toHaveBeenCalled();
    expect(f.other.subscription.getState()).toMatchObject({ status: 'live', error: null, stale: false });

    f.source.message({
      queryId: 'e4ba7cee-4b9b-4b24-ad4e-a58f0e04d481', timestamp: 2, results: [{ type: 'ADD', data: { id: 'B', value: 20 } }],
    });
    f.source.message({
      queryId: 'e4ba7cee-4b9b-4b24-ad4e-a58f0e04d481', timestamp: 3, results: [{ type: 'DELETE', data: { id: 'A' } }],
    });
    await vi.advanceTimersByTimeAsync(100);
    expect(f.other.rows()).toEqual([{ id: 'B', value: 20 }]);
    expect(f.other.results).toHaveBeenCalledTimes(3);
    expect(f.first.rows()).toBe(original);
    expect(f.first.results).toHaveBeenCalledOnce();
    expect(f.client.getConnectionStatus()).toEqual(status);
    expect(f.states).toHaveBeenCalledOnce();
    expect(f.factory.instances).toHaveLength(1);
    expect(f.source.closed).toBe(false);
    expect(f.subscribe).toHaveBeenCalledTimes(2);
    expect(f.server.fetch).toHaveBeenCalledTimes(reads);
    expect(failure).toMatchObject({ instanceId: 'adapter-instance', ...details });
  });

  it.each([
    {},
    { queryId: 7, query_id: 'b20c42ee-59a1-441f-a549-87a72be1f1e9' },
    { queryId: '', query_id: 'b20c42ee-59a1-441f-a549-87a72be1f1e9' },
    { queryId: '..', query_id: 'b20c42ee-59a1-441f-a549-87a72be1f1e9' },
    { query_id: 7 },
  ])('retains connection-scoped failure for an unrecognized raw identity %j', async identity => {
    const failure = new DrasiError('INVALID_PAYLOAD', {
      instanceId: 'adapter-instance', ...details, status: 422, resourceStatus: 'Rejected',
    });
    const adapter = vi.fn<ResultAdapter>(() => { throw failure; });
    const f = await connectedClient(adapter), reads = f.server.fetch.mock.calls.length;
    const original = f.first.rows(), otherRows = f.other.rows();
    f.source.message(wire(identity));
    expect(f.client.getConnectionStatus().error).toBe(failure);
    expect(f.client.getConnectionStatus()).toMatchObject({ connected: false, reconnecting: false });
    for (const subscriber of [f.first, f.other]) {
      expect(subscriber.errors).toHaveBeenCalledExactlyOnceWith(failure);
      expect(subscriber.subscription.getState()).toMatchObject({
        status: 'terminal-error', errorScope: 'connection', stale: true,
      });
      expect(subscriber.subscription.getState().error).toBe(failure);
      expect(subscriber.results).toHaveBeenCalledOnce();
    }
    await vi.advanceTimersByTimeAsync(100);
    expect(f.first.rows()).toBe(original);
    expect(f.other.rows()).toBe(otherRows);
    expect(f.source.closed).toBe(true);
    expect(f.factory.instances).toHaveLength(1);
    expect(f.server.fetch).toHaveBeenCalledTimes(reads);
    expect(f.subscribe).toHaveBeenCalledTimes(2);
  });
});

describe('already query-scoped typed adapter failures', () => {
  it.each([
    { queryId: 'e4ba7cee-4b9b-4b24-ad4e-a58f0e04d481' },
    { queryId: 'b20c42ee-59a1-441f-a549-87a72be1f1e9', query_id: 'e4ba7cee-4b9b-4b24-ad4e-a58f0e04d481' },
    { query_id: 'b20c42ee-59a1-441f-a549-87a72be1f1e9' },
    {},
    { queryId: 7 },
  ])('preserves the exact error and its query instead of raw identity %j', async identity => {
    const failure = new DrasiError('RESOURCE_STOPPED', {
      instanceId: 'adapter-instance', resourceKind: 'query', resourceId: 'e4ba7cee-4b9b-4b24-ad4e-a58f0e04d481',
      status: 409, resourceStatus: 'Stopped',
    });
    const adapter = vi.fn<ResultAdapter>(sse034ResultAdapter)
      .mockImplementationOnce(() => { throw failure; });
    const f = await connectedClient(adapter), reads = f.server.fetch.mock.calls.length;
    const original = f.other.rows(), status = f.client.getConnectionStatus();
    f.source.message(wire(identity));
    expect(f.other.errors).toHaveBeenCalledExactlyOnceWith(failure);
    expect(f.other.subscription.getState().error).toBe(failure);
    expect(f.other.subscription.getState()).toMatchObject({
      status: 'terminal-error', errorScope: 'query', stale: true,
    });
    expect(f.first.errors).not.toHaveBeenCalled();
    f.source.message(wire({ queryId: 'b20c42ee-59a1-441f-a549-87a72be1f1e9' }));
    await vi.advanceTimersByTimeAsync(100);
    expect(f.first.rows()).toEqual([{ id: 'A', value: 99 }]);
    expect(f.first.results).toHaveBeenCalledTimes(2);
    expect(f.first.subscription.getState()).toMatchObject({ status: 'live', error: null, stale: false });
    expect(f.other.rows()).toBe(original);
    expect(f.other.results).toHaveBeenCalledOnce();
    expect(f.client.getConnectionStatus()).toEqual(status);
    expect(f.states).toHaveBeenCalledOnce();
    expect(f.factory.instances).toHaveLength(1);
    expect(f.source.closed).toBe(false);
    expect(f.subscribe).toHaveBeenCalledTimes(2);
    expect(f.server.fetch).toHaveBeenCalledTimes(reads);
  });
});
