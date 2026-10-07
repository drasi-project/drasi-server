// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { afterEach, describe, expect, it, vi } from 'vitest';
import { DrasiSSEClient } from '../src/client/DrasiSSEClient';
import { DrasiError } from '../src/client/errors';
import { createLegacyResultAdapter } from '../src/client/results';
import type { QueryDelta, RouteUnidentified } from '../src/client/types';
import { fakeEventSourceFactory } from './FakeEventSource';

const clients: DrasiSSEClient[] = [];
afterEach(async () => {
  await Promise.all(clients.splice(0).map(client => client.disconnect()));
});

async function open(routeUnidentified?: RouteUnidentified) {
  const factory = fakeEventSourceFactory();
  const client = new DrasiSSEClient({
    eventSourceFactory: factory.create, resultAdapter: createLegacyResultAdapter({ routeUnidentified }),
  });
  clients.push(client);
  const results = vi.fn<(value: QueryDelta) => void>(), errors = vi.fn();
  client.subscribe('telemetry', results, errors);
  const connected = client.connect(['telemetry'], 'https://stream.invalid/events');
  factory.instances[0].open();
  await connected;
  return { client, results, errors, source: factory.instances[0] };
}

describe('explicit legacy SSE adapter boundary', () => {
  it.each([
    [{ type: 'aggregation', after: { device: 'one' } }, { kind: 'upsert', after: { device: 'one' } }],
    [{ op: 'd', before: { device: 'one' } }, { kind: 'delete', before: { device: 'one' } }],
    [{ op: 'u', before: { device: 'one' } }, { kind: 'delete', before: { device: 'one' } }],
    [{ op: 'c', after: { device: 'one' } }, { kind: 'upsert', after: { device: 'one' } }],
    [{ op: 'r', after: { device: 'one' } }, { kind: 'upsert', after: { device: 'one' } }],
    [{ op: 'u', after: { device: 'one' } }, { kind: 'upsert', after: { device: 'one' } }],
    [{ type: 'delete', before: { device: 'one' } }, { kind: 'delete', before: { device: 'one' } }],
    [{ type: 'DELETE', data: { device: 'one' } }, { kind: 'delete', before: { device: 'one' } }],
    [{ type: 'add', data: { device: 'one' } }, { kind: 'upsert', after: { device: 'one' } }],
    [{ type: 'ADD', data: { device: 'one' } }, { kind: 'upsert', after: { device: 'one' } }],
    [{ type: 'update', after: { device: 'one' } }, { kind: 'upsert', after: { device: 'one' } }],
    [{ type: 'UPDATE', after: { device: 'one' } }, { kind: 'upsert', after: { device: 'one' } }],
    [{ data: { device: 'one' } }, { kind: 'upsert', after: { device: 'one' } }],
    [{ device: 'one' }, { kind: 'upsert', after: { device: 'one' } }],
  ])('normalizes the explicitly selected historical result alternative %j', async (wire, change) => {
    const { results, source } = await open();
    source.namedMessage('query-result', {
      query_id: 'telemetry', results: [wire], timestamp: '2026-01-15T12:00:00Z',
    });
    expect(results).toHaveBeenCalledExactlyOnceWith({
      kind: 'delta', queryId: 'telemetry', changes: [change], receivedAt: expect.any(Number),
      sourceTimestamp: Date.parse('2026-01-15T12:00:00Z'),
    });
  });

  it('preserves explicit routing and normalized deletes for unidentified legacy batches', async () => {
    const route = vi.fn<RouteUnidentified>((rows, deliver) => deliver('telemetry', rows));
    const { source, results } = await open(route);
    source.message({ addedResults: [{ after: { key: 1 } }, { key: 2 }],
      updatedResults: [{ after: { key: 3 } }, { key: 4 }],
      deletedResults: [{ before: { key: 5 } }, { key: 6 }] });
    expect(results.mock.calls[0][0].changes).toEqual([
      ...[1, 2, 3, 4].map(key => ({ kind: 'upsert', after: { key } })),
      ...[5, 6].map(key => ({ kind: 'delete', before: { key } })),
    ]);
    source.message({ device: 'plain-row' });
    expect(results.mock.calls[1][0].changes).toEqual([{ kind: 'upsert', after: { device: 'plain-row' } }]);
    source.message({ addedResults: [], updatedResults: [], deletedResults: [] });
    source.message({ type: 'heartbeat' });
    source.namedMessage('heartbeat', { privateData: 'not logged' });
    expect(route).toHaveBeenCalledTimes(2);
  });

  it.each([
    { queryId: 'telemetry', type: 'update', data: { device: 'one' }, timestamp: 1234 },
    { queryId: 'telemetry', data: [{ device: 'one' }], timestamp: 1234 },
  ])('normalizes keyed object/array legacy batches %j', async wire => {
    const { source, results } = await open();
    source.message(wire);
    expect(results.mock.calls[0][0]).toEqual({
      kind: 'delta', queryId: 'telemetry', changes: [{ kind: 'upsert', after: { device: 'one' } }],
      receivedAt: expect.any(Number), sourceTimestamp: 1234,
    });
  });

  it.each([
    { queryId: 'telemetry', results: [7] },
    { queryId: 'telemetry', results: [null] },
    { queryId: 'telemetry', data: false },
    { queryId: 'telemetry', results: [{ op: 'd', before: 7 }] },
    { queryId: 'telemetry', data: {}, timestamp: 'not a timestamp' },
    { queryId: 'telemetry', data: {}, timestamp: {} },
  ])('isolates an identifiable malformed payload to its query: %j', async wire => {
    const { client, source, results, errors } = await open();
    source.message(wire);
    expect(errors).toHaveBeenCalledExactlyOnceWith(client.getQueryError('telemetry'));
    expect(client.getQueryError('telemetry')).toMatchObject({ code: 'INVALID_PAYLOAD', retryable: false });
    expect(client.getConnectionStatus().connected).toBe(true);
    expect(source.closed).toBe(false);
    expect(results).not.toHaveBeenCalled();
  });

  it.each([{ query_id: 7 }, { addedResults: [false] }, { updatedResults: [{ after: 7 }] },
    { deletedResults: [{ before: 7 }] }])('terminates unidentifiable malformed payload %j', async wire => {
    const { client, source } = await open();
    source.message(wire);
    expect(client.getConnectionStatus().error?.code).toBe('INVALID_PAYLOAD');
    expect(source.closed).toBe(true);
  });

  it('ignores valid unused/empty batches and exposes sanitized subscriber failures without disrupting peers', async () => {
    const { client, source, results } = await open();
    const log = vi.spyOn(console, 'error').mockImplementation(() => {});
    const onError = vi.fn();
    client.subscribe('telemetry', () => { throw new Error('private callback details'); }, onError);
    const other = vi.fn(), stop = client.subscribe('telemetry', other);
    const stopStatus = client.onConnectionStatusChange(status => {
      if (!status.connected) throw new Error('private listener details');
    });
    source.message({ queryId: 'unknown-query', data: {} });
    source.message({ queryId: 'telemetry', results: [] });
    expect(results).not.toHaveBeenCalled();
    source.message({ queryId: 'telemetry', data: {} });
    expect(other).toHaveBeenCalledOnce();
    expect(onError.mock.calls[0][0]).toMatchObject({ code: 'RESULT_PROCESSING_FAILED', resourceId: 'telemetry' });
    stop();
    source.message({ queryId: 'telemetry', data: {} });
    expect(other).toHaveBeenCalledOnce();
    source.message({});
    expect(client.getConnectionStatus().error).toBeInstanceOf(DrasiError);
    expect(log.mock.calls).toEqual([['Drasi connection status listener failed.']]);
    expect(JSON.stringify(onError.mock.calls)).not.toContain('private callback details');
    stopStatus();
  });

  it('rejects non-string custom-factory messages without logging the payload', async () => {
    const { source, client } = await open();
    source.onmessage?.(new MessageEvent('message', { data: { privateData: 'fixture' } }));
    expect(client.getConnectionStatus().error?.code).toBe('INVALID_PAYLOAD');
  });
});
