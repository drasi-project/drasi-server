// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import recordingText from './fixtures/server-v1/sse-0.3.4.ndjson?raw';
import currentRecordingText from './fixtures/server-v1-0.2.3/sse-0.3.6.ndjson?raw';
import currentProvenance from './fixtures/server-v1-0.2.3/sse-0.3.6.provenance.json';
import { describe, expect, it, vi } from 'vitest';
import { accumulateResult } from '../src/client/accumulation';
import { DrasiError } from '../src/client/errors';
import { DrasiSSEClient } from '../src/client/DrasiSSEClient';
import { isRecord } from '../src/client/resources';
import { createLegacyResultAdapter, readAdaptedResults, sse034ResultAdapter } from '../src/client/results';
import type { QueryDelta, QueryResult, ResultChange, ResultRow, RowKey } from '../src/client/types';
import { fakeEventSourceFactory } from './FakeEventSource';

const context = { receivedAt: 10, instanceId: 'warehouse' };
const envelope = (results: unknown[], queryId = 'inventory') => ({ queryId, results, timestamp: 1 });
const normalize = (payload: unknown) => sse034ResultAdapter(payload, context);
const legacy = createLegacyResultAdapter();
const key: RowKey = row => {
  if (typeof row.rack !== 'string' || typeof row.slot !== 'string') throw new Error('invalid identity');
  return `${row.rack}/${row.slot}`;
};
const delta = (...changes: ResultChange[]): QueryDelta => ({
  kind: 'delta', queryId: 'inventory', changes, receivedAt: 10,
});
const initial: QueryResult = {
  kind: 'snapshot', queryId: 'inventory', receivedAt: 10,
  rows: [{ rack: 'north', slot: 'one', value: 8 }, { rack: 'south', slot: 'one', value: 8 }],
};

describe('versioned observed SSE evidence', () => {
  it.each([
    { version: '0.3.4', text: recordingText },
    { version: '0.3.6', text: currentRecordingText },
  ])('normalizes every original $version raw recording without inferring identity from unsafe signatures', ({ text }) => {
    const kinds = new Set<string>();
    const queries = new Set<string>();
    let unsafeSignatures = 0;
    for (const line of text.trim().split('\n')) {
      const recording: unknown = JSON.parse(line);
      if (!isRecord(recording) || typeof recording.data !== 'string') throw new Error('Invalid versioned recording');
      const wire: unknown = JSON.parse(recording.data);
      const results = normalize(wire);
      expect(readAdaptedResults(results, context)).toEqual(results);
      if (isRecord(wire) && Array.isArray(wire.results)) {
        for (const result of wire.results) {
          if (isRecord(result) && typeof result.row_signature === 'number' && !Number.isSafeInteger(result.row_signature)) {
            unsafeSignatures++;
          }
        }
      }
      for (const result of results) {
        queries.add(result.queryId);
        expect(result.receivedAt).toBe(10);
        for (const change of result.changes) {
          kinds.add(change.kind);
          expect(change).not.toHaveProperty('row_signature');
          if (change.kind === 'update') {
            expect(change.before).not.toEqual(change.after);
          }
        }
      }
    }
    expect(unsafeSignatures).toBeGreaterThan(10);
    expect(kinds).toEqual(new Set(['upsert', 'delete', 'update']));
    expect(queries.has('portfolio-summary-query')).toBe(true);
    expect(queries.has('sector-performance-query')).toBe(true);
  });

  it('routes actual 0.3.6 default envelopes and preserves aggregation before/after without requiring data', async () => {
    expect(currentProvenance).toMatchObject({
      serverVersion: '0.2.3', libraryVersion: '0.9.1', indexVersion: '0.6.1',
      hostSdkVersion: '0.11.0', pluginSdkVersion: '0.11.1',
      ssePluginVersion: '0.3.6', pluginAbi: '0.13.0', signatureVerified: true,
    });
    const factory = fakeEventSourceFactory();
    const client = new DrasiSSEClient({ eventSourceFactory: factory.create });
    const watchlist = vi.fn(), gainers = vi.fn(), sector = vi.fn(), errors = vi.fn();
    client.subscribe('watchlist-query', watchlist, errors);
    client.subscribe('top-gainers-query', gainers, errors);
    client.subscribe('sector-performance-query', sector, errors);
    const connected = client.connect(
      ['watchlist-query', 'top-gainers-query', 'sector-performance-query'], 'https://stream.invalid/events',
    );
    factory.instances[0].open();
    await connected;
    const lines = currentRecordingText.trim().split('\n');
    expect(lines).toHaveLength(currentProvenance.lines);
    const observed = new Map<string, number>();
    try {
      for (const line of lines) {
        const recording: unknown = JSON.parse(line);
        if (!isRecord(recording) || typeof recording.data !== 'string') throw new Error('Invalid current recording');
        const wire: unknown = JSON.parse(recording.data);
        if (!isRecord(wire) || !Array.isArray(wire.results)) throw new Error('Invalid current envelope');
        expect(Object.keys(wire).sort()).toEqual(['queryId', 'results', 'timestamp']);
        expect(readAdaptedResults(legacy(wire, context), context)).toEqual(normalize(wire));
        for (const change of wire.results) {
          if (!isRecord(change) || typeof change.type !== 'string') throw new Error('Invalid recorded change');
          observed.set(change.type, (observed.get(change.type) ?? 0) + 1);
          if (change.type === 'aggregation') {
            expect(change).not.toHaveProperty('data');
            expect(change).toHaveProperty('before');
            expect(change).toHaveProperty('after');
          }
        }
        factory.instances[0].onmessage?.(new MessageEvent('message', { data: recording.data }));
      }
      expect(Object.fromEntries(observed)).toEqual({ ADD: 3, DELETE: 3, UPDATE: 10, aggregation: 1 });
      expect(watchlist).toHaveBeenCalledTimes(3);
      expect(gainers).toHaveBeenCalledOnce();
      expect(watchlist.mock.calls[2][0]).toMatchObject({ queryId: 'watchlist-query' });
      expect(gainers.mock.calls[0][0]).toMatchObject({ queryId: 'top-gainers-query' });
      expect(sector).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({
        queryId: 'sector-performance-query',
        changes: [{
          kind: 'update',
          before: { avgChangePercent: 3.75, maxPrice: 180, minPrice: 95, sector: 'Technology', stockCount: 4, totalVolume: 60000000 },
          after: { avgChangePercent: 5, maxPrice: 180, minPrice: 95, sector: 'Technology', stockCount: 4, totalVolume: 60000000 },
        }],
      }));
      expect(errors).not.toHaveBeenCalled();
      expect(client.isConnected()).toBe(true);
    } finally {
      await client.disconnect();
    }
  });

  it('handles source-defined nullable aggregation before, noop and validated heartbeats', () => {
    expect(normalize({ type: 'heartbeat', ts: 1 })).toEqual([]);
    expect(normalize(envelope([{ type: 'noop' }]))[0].changes).toEqual([]);
    expect(normalize(envelope([{ type: 'aggregation', before: null, after: { sector: 'x', value: 0 } }]))[0].changes)
      .toEqual([{ kind: 'upsert', after: { sector: 'x', value: 0 } }]);
    expect(normalize(envelope([{ type: 'UPDATE', before: { key: 'old' }, after: { key: 'new' },
      data: { key: 'new' }, grouping_keys: ['key'] }]))[0].changes)
      .toEqual([{ kind: 'update', before: { key: 'old' }, after: { key: 'new' } }]);
  });
});

describe('strict wire and custom adapter validation', () => {
  it.each([
    null, [], {}, { type: 'heartbeat' }, { type: 'heartbeat', ts: -1 },
    { type: 'heartbeat', ts: 1, results: [] }, { ...envelope([]), type: 'heartbeat' },
    { query_id: 'inventory', results: [], timestamp: 1 },
    { queryId: '', results: [], timestamp: 1 }, { queryId: 5, results: [], timestamp: 1 },
    { queryId: 'inventory', data: [] },
    { ...envelope([]), addedResults: [] }, { ...envelope([]), data: [] },
    { ...envelope([]), timestamp: '2026-01-01' }, { ...envelope([]), timestamp: Infinity },
    envelope([null]), envelope([{ type: 'future-type', data: {} }]),
    envelope([{ type: 'ADD', data: [] }]), envelope([{ type: 'ADD', data: { v: undefined } }]),
    envelope([{ type: 'ADD', data: {}, row_signature: 'lossless-looking-but-not-supported' }]),
    envelope([{ type: 'ADD', data: {}, row_signature: -1 }]),
    envelope([{ type: 'ADD', data: {}, row_signature: 1.5 }]),
    envelope([{ type: 'UPDATE', before: {}, after: {} }]),
    envelope([{ type: 'UPDATE', before: {}, after: {}, data: {}, grouping_keys: [7] }]),
    envelope([{ type: 'UPDATE', before: {}, after: {}, data: {}, grouping_keys: 'id' }]),
    envelope([{ type: 'aggregation', after: {} }]),
  ])('rejects unsupported/malformed payload %j with no sensitive body in the error', wire => {
    expect(() => normalize(wire)).toThrowError(DrasiError);
    try { normalize(wire); } catch (error) {
      expect(error).toMatchObject({ code: 'INVALID_PAYLOAD', retryable: false });
      expect(String(error)).not.toContain('lossless-looking');
    }
  });

  it.each([
    null, {}, [null], [{ kind: 'snapshot' }], [{ ...delta(), queryId: '' }],
    [{ ...delta(), receivedAt: NaN }], [{ ...delta(), sourceTimestamp: -1 }],
    [{ ...delta(), changes: [null] }], [{ ...delta(), changes: [{ kind: 'unknown' }] }],
    [{ ...delta(), changes: [{ kind: 'upsert', after: null }] }],
  ])('rejects a custom adapter violating its declared output %j', value => {
    expect(() => readAdaptedResults(value, context)).toThrow(DrasiError);
  });

  it('validates all normalized change forms and preserves legitimate empty/unused query batches', () => {
    const value = delta(
      { kind: 'upsert', after: { key: 'one' } },
      { kind: 'update', before: { key: 'one' }, after: { key: 'two' } },
      { kind: 'delete', before: { key: 'two' } },
    );
    expect(readAdaptedResults([value], context)).toEqual([value]);
    expect(normalize(envelope([], 'not-subscribed'))[0]).toMatchObject({ queryId: 'not-subscribed', changes: [] });
  });
});

describe('explicit compatibility selection and routing', () => {
  it('honors envelope IDs before any content router for identical-shaped aggregate/add/update/delete batches', () => {
    const router = vi.fn(), adapter = createLegacyResultAdapter({ routeUnidentified: router });
    for (const queryId of ['first', 'second']) {
      const result = adapter({
        queryId, addedResults: [{ value: 1 }], updatedResults: [{ before: { value: 1 }, after: { value: 2 } }],
        deletedResults: [{ before: { value: 3 } }],
      }, context);
      expect(result[0]).toMatchObject({ queryId, changes: [
        { kind: 'upsert', after: { value: 1 } },
        { kind: 'update', before: { value: 1 }, after: { value: 2 } },
        { kind: 'delete', before: { value: 3 } },
      ] });
    }
    expect(router).not.toHaveBeenCalled();
  });

  it('normalizes legacy sparse deletes without exposing marker-based deletion in the canonical contract', () => {
    expect(legacy({ query_id: 'q', data: [{ rack: 'one', _deleted: true }, { rack: 'two', _deleted: false }] }, context)[0].changes)
      .toEqual([{ kind: 'delete', before: { rack: 'one' } }, { kind: 'upsert', after: { rack: 'two' } }]);
    const official = normalize(envelope([{ type: 'ADD', data: { _deleted: true } }]))[0];
    expect(official.changes).toEqual([{ kind: 'upsert', after: { _deleted: true } }]);
    for (const type of ['delete', 'DELETE']) {
      expect(legacy({ queryId: 'q', type, data: { rack: 'one' } }, context)[0].changes)
        .toEqual([{ kind: 'delete', before: { rack: 'one' } }]);
    }
    expect(legacy({ type: 'heartbeat', ts: 0 }, context)).toEqual([]);
  });

  it('retains before/after in identified op updates and lowercase updates', () => {
    for (const marker of [{ op: 'u' }, { type: 'update' }]) {
      expect(legacy({ queryId: 'q', results: [{ ...marker, before: { k: 1 }, after: { k: 2 } }] }, context)[0].changes)
        .toEqual([{ kind: 'update', before: { k: 1 }, after: { k: 2 } }]);
    }
    expect(legacy({ queryId: 'q', results: [{ type: 'noop' }] }, context)[0].changes).toEqual([]);
  });

  it('makes legacy unidentified update limits explicit as delete+upsert and allows deliberate fanout', () => {
    const adapter = createLegacyResultAdapter({ routeUnidentified: (rows, deliver) => {
      expect(rows).toEqual([{ key: 'old', _deleted: true }, { key: 'new' }]);
      deliver('q', rows); deliver('deliberately-unused', rows);
    } });
    const changes = adapter({ updatedResults: [{ before: { key: 'old' }, after: { key: 'new' } }] }, context);
    expect(changes.map(item => item.queryId)).toEqual(['q', 'deliberately-unused']);
    expect(changes[0].changes).toEqual([
      { kind: 'delete', before: { key: 'old' } }, { kind: 'upsert', after: { key: 'new' } },
    ]);
  });

  it.each([
    { queryId: 'one', query_id: 'two', data: [] }, { queryId: null, data: [] },
    { query_id: false, data: [] }, { queryId: 'q' }, { queryId: 'q', results: false },
    { queryId: 'q', results: [], data: [] }, { queryId: 'q', addedResults: [], results: [] },
    { queryId: 'q', addedResults: [], data: [] }, { addedResults: false },
    { queryId: 'q', results: [{ op: 'future', after: {} }] },
    { queryId: 'q', results: [{ type: 'future', after: {} }] },
    { queryId: 'q', results: [{ data: null }] }, { updatedResults: [{ after: null }] },
    { deletedResults: [{ before: null }] },
    { queryId: 'q', data: { _deleted: 1 } },
    { queryId: 'q', type: 'future', data: {} },
    { type: 'heartbeat', ts: 0, data: [] }, { type: 'heartbeat', ts: 'not numeric' },
  ])('rejects malformed compatibility input rather than guessing %j', wire => {
    expect(() => legacy(wire, context)).toThrow(DrasiError);
  });

  it('surfaces missing/partial/invalid app routing safely, never merely logging rows', () => {
    const log = vi.spyOn(console, 'error');
    const wire = { addedResults: [{ secretRow: 1 }, { secretRow: 2 }] };
    for (const adapter of [
      legacy, createLegacyResultAdapter({ routeUnidentified: () => {} }),
      createLegacyResultAdapter({ routeUnidentified: (rows, deliver) => deliver('q', [rows[0]]) }),
      createLegacyResultAdapter({ routeUnidentified: (rows, deliver) => deliver('', rows) }),
      createLegacyResultAdapter({ routeUnidentified: (rows, deliver) => deliver('q', rows.map(row => ({ ...row }))) }),
      createLegacyResultAdapter({ routeUnidentified: (rows, deliver) => {
        rows.splice(0, 1); deliver('q', rows);
      } }),
    ]) {
      try { adapter(wire, context); expect.fail('Expected routing error'); } catch (error) {
        expect(error).toMatchObject({ code: 'UNROUTABLE_RESULT' });
        expect(String(error)).not.toContain('secretRow');
      }
    }
    expect(log).not.toHaveBeenCalled();
  });

  it('classifies generic app router failures safely and preserves intentional typed error identity', () => {
    const details = { ...context, resourceKind: 'reaction' as const, resourceId: 'events' };
    const adapter = createLegacyResultAdapter({ routeUnidentified: () => { throw new Error('private row content'); } });
    try { adapter({ device: 'one' }, details); expect.fail('Expected routing failure'); } catch (error) {
      expect(error).toMatchObject({ code: 'UNROUTABLE_RESULT', resourceKind: 'reaction', resourceId: 'events' });
      expect(String(error)).not.toContain('private row content');
    }
    const error = new DrasiError('UNROUTABLE_RESULT', details);
    const typed = createLegacyResultAdapter({ routeUnidentified: () => { throw error; } });
    expect(() => typed({ device: 'one' }, details)).toThrow(error);
  });
});

describe('raw domain identity and transactional accumulation', () => {
  it('keeps equal-valued groups separate, idempotently upserts, and removes old keys in key-changing updates', () => {
    let rows = accumulateResult([], initial, key);
    expect(rows).toHaveLength(2);
    const after = { rack: 'north', slot: 'two', value: 9 };
    rows = accumulateResult(rows, delta({ kind: 'update', before: { rack: 'north', slot: 'one' }, after }), key);
    rows = accumulateResult(rows, delta({ kind: 'upsert', after }), key);
    expect(rows).toEqual([{ rack: 'south', slot: 'one', value: 8 }, after]);
    rows = accumulateResult(rows, delta({ kind: 'delete', before: { rack: 'north', slot: 'two' } }), key);
    rows = accumulateResult(rows, delta({ kind: 'delete', before: { rack: 'missing', slot: 'one' } }), key);
    expect(rows).toEqual([{ rack: 'south', slot: 'one', value: 8 }]);
    expect(accumulateResult(rows, { ...initial, rows: [] }, key)).toEqual([]);
  });

  it('keeps stable-key updates in insertion order and never mutates last-good rows on an invalid batch', () => {
    const rows = accumulateResult([], initial, key);
    const after = { rack: 'north', slot: 'one', value: 10 };
    expect(accumulateResult(rows, delta({ kind: 'update', before: initial.rows[0], after }), key))
      .toEqual([after, initial.rows[1]]);
    expect(() => accumulateResult(rows, delta(
      { kind: 'delete', before: initial.rows[0] }, { kind: 'upsert', after: { noKey: true } },
    ), key)).toThrowError(DrasiError);
    expect(rows).toEqual(initial.rows);
  });

  it('makes skip/empty/missing key strategies invalid and preserves intentional typed callback failures', () => {
    const details = { resourceKind: 'query' as const, resourceId: 'inventory' };
    expect(() => accumulateResult([], initial, () => '', details)).toThrowError(
      expect.objectContaining({ code: 'INVALID_ROW_KEY', resourceId: 'inventory' }),
    );
    const error = new DrasiError('INVALID_ROW_KEY', details);
    expect(() => accumulateResult([], initial, () => { throw error; })).toThrow(error);
    expect(() => accumulateResult([], { ...initial, rows: [{}] }, key)).toThrowError(DrasiError);
    const noFallback: ResultRow[] = [{ symbol: 'not-an-implicit-key' }];
    expect(() => accumulateResult([], { ...initial, rows: noFallback }, key)).toThrowError(DrasiError);
  });
});
