// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { afterEach, describe, expect, it, vi } from 'vitest';
import { DrasiSSEClient } from '../src/client/DrasiSSEClient';
import { DrasiError } from '../src/client/errors';
import type { QueryResult, RouteUnidentified } from '../src/client/types';
import { fakeEventSourceFactory } from './FakeEventSource';

const clients: DrasiSSEClient[] = [];
afterEach(async () => {
  await Promise.all(clients.splice(0).map(client => client.disconnect()));
  vi.useRealTimers();
});

async function open(routeUnidentified?: RouteUnidentified) {
  const factory = fakeEventSourceFactory();
  const client = new DrasiSSEClient({ eventSourceFactory: factory.create, routeUnidentified });
  clients.push(client);
  const results = vi.fn<(value: QueryResult) => void>();
  client.subscribe('telemetry', results);
  const connected = client.connect(['telemetry'], 'https://stream.invalid/events');
  factory.instances[0].open();
  await connected;
  return { client, results, source: factory.instances[0] };
}

// These characterize the existing compatibility boundary, not a new canonical
// adapter/identity contract. P4 owns replacement of these legacy alternatives.
describe('typed legacy SSE object boundary', () => {
  it.each([
    [{ type: 'aggregation', after: { device: 'one' } }, { device: 'one' }],
    [{ op: 'd', before: { device: 'one' } }, { device: 'one', _deleted: true }],
    [{ op: 'u', before: { device: 'one' } }, { device: 'one', _deleted: true }],
    [{ op: 'c', after: { device: 'one' } }, { device: 'one' }],
    [{ op: 'r', after: { device: 'one' } }, { device: 'one' }],
    [{ op: 'u', after: { device: 'one' } }, { device: 'one' }],
    [{ type: 'delete', before: { device: 'one' } }, { device: 'one', _deleted: true }],
    [{ type: 'DELETE', data: { device: 'one' } }, { device: 'one', _deleted: true }],
    [{ type: 'add', data: { device: 'one' } }, { device: 'one' }],
    [{ type: 'ADD', data: { device: 'one' } }, { device: 'one' }],
    [{ type: 'update', after: { device: 'one' } }, { device: 'one' }],
    [{ type: 'UPDATE', after: { device: 'one' } }, { device: 'one' }],
    [{ data: { device: 'one' } }, { device: 'one' }],
    [{ device: 'one' }, { device: 'one' }],
  ])('preserves a previously supported result alternative %j', async (wire, row) => {
    const { results, source } = await open();
    source.namedMessage('query-result', {
      query_id: 'telemetry', results: [null, wire], timestamp: '2026-01-15T12:00:00Z',
    });
    expect(results).toHaveBeenCalledExactlyOnceWith({
      queryId: 'telemetry', data: [row], timestamp: Date.parse('2026-01-15T12:00:00Z'),
    });
  });

  it('preserves explicit caller-owned routing and deletion flags for unidentified legacy batches', async () => {
    const route = vi.fn<RouteUnidentified>((rows, deliver) => deliver('telemetry', rows));
    const { source, results } = await open(route);
    source.message({ addedResults: [{ after: { key: 1 } }, { key: 2 }],
      updatedResults: [{ after: { key: 3 } }, { key: 4 }],
      deletedResults: [{ before: { key: 5 } }, { key: 6 }] });
    expect(results.mock.calls[0][0].data).toEqual([
      { key: 1 }, { key: 2 }, { key: 3 }, { key: 4 },
      { key: 5, _deleted: true }, { key: 6, _deleted: true },
    ]);
    source.message({ device: 'plain-row' });
    expect(results.mock.calls[1][0].data).toEqual([{ device: 'plain-row' }]);
    source.message({ addedResults: [], updatedResults: [], deletedResults: [] });
    source.message({ type: 'heartbeat' });
    source.namedMessage('heartbeat', { privateData: 'not logged' });
    expect(route).toHaveBeenCalledTimes(2);
  });

  it.each([
    { queryId: 'telemetry', type: 'update', data: { device: 'one' }, timestamp: 1234 },
    { queryId: 'telemetry', data: [{ device: 'one' }], timestamp: 1234 },
  ])('retains keyed object/array batches and numeric timestamps %j', async wire => {
    const { source, results } = await open();
    source.message(wire);
    expect(results.mock.calls[0][0]).toEqual({ queryId: 'telemetry', data: [{ device: 'one' }], timestamp: 1234 });
  });

  it.each([
    { query_id: 7 },
    { queryId: 'telemetry', results: [7] },
    { queryId: 'telemetry', data: false },
    { queryId: 'telemetry', results: [{ op: 'd', before: 7 }] },
    { queryId: 'telemetry', data: {}, timestamp: 'not a timestamp' },
    { queryId: 'telemetry', data: {}, timestamp: {} },
    { addedResults: [false] },
    { updatedResults: [{ after: 7 }] },
    { deletedResults: [{ before: 7 }] },
  ])('rejects malformed object boundaries visibly instead of passing ambient any %j', async wire => {
    const { client, source, results } = await open();
    source.message(wire);
    expect(client.getConnectionStatus().error).toMatchObject({ code: 'INVALID_PAYLOAD', retryable: false });
    expect(source.closed).toBe(true);
    expect(results).not.toHaveBeenCalled();
  });

  it('ignores unsubscribed/empty result lists and sanitizes exceptions from independent subscribers/listeners', async () => {
    const { client, source, results } = await open();
    const log = vi.spyOn(console, 'error').mockImplementation(() => {});
    client.subscribe('telemetry', () => { throw new Error('credential-like fixture must never be logged'); });
    const other = vi.fn();
    const stop = client.subscribe('telemetry', other);
    const stopStatus = client.onConnectionStatusChange(status => {
      if (!status.connected) throw new Error('private listener details');
    });
    source.message({ queryId: 'unknown-query', data: {} });
    source.message({ queryId: 'telemetry', results: [] });
    expect(results).not.toHaveBeenCalled();
    source.message({ queryId: 'telemetry', data: {} });
    expect(other).toHaveBeenCalledOnce();
    stop();
    source.message({ queryId: 'telemetry', data: {} });
    expect(other).toHaveBeenCalledOnce();
    source.message({}); // terminal typed protocol failure
    expect(client.getConnectionStatus().error).toBeInstanceOf(DrasiError);
    expect(log.mock.calls).toEqual([
      ['Drasi result subscriber failed.'], ['Drasi result subscriber failed.'],
      ['Drasi connection status listener failed.'],
    ]);
    stopStatus();
  });

  it('rejects non-string custom-factory messages, without logging the payload', async () => {
    const { source, client } = await open();
    // A JavaScript/polyfill caller may violate the declared MessageEvent<string>.
    source.onmessage?.(new MessageEvent('message', { data: { privateData: 'fixture' } }));
    expect(client.getConnectionStatus().error?.code).toBe('INVALID_PAYLOAD');
  });
});
