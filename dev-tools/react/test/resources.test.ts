// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { describe, expect, it, vi } from 'vitest';
import { DrasiClient } from '../src/client/DrasiClient';
import { DrasiError } from '../src/client/errors';
import { readQuery, readReaction, readResponse, readRows } from '../src/client/resources';
import captured from './fixtures/server-v1/contract.json';
import { component, json, queryConfig, refs } from './server';

const queryDetails = { instanceId: 'trading-server', resourceKind: 'query' as const, resourceId: 'watchlist-query' };
const reactionDetails = { instanceId: 'trading-server', resourceKind: 'reaction' as const, resourceId: 'sse-stream' };

describe('versioned real server read contract', () => {
  it('consumes captured full-view query/reaction and snapshot bodies through the public client', async () => {
    const fetcher = vi.fn<typeof fetch>(async input => {
      const url = new URL(String(input));
      expect(url.pathname.startsWith('/api/v1/instances/trading-server/')).toBe(true);
      const response = url.pathname.endsWith('/results') ? captured.snapshot
        : url.pathname.includes('/reactions/') ? captured.reaction : captured.query;
      return new Response(JSON.stringify(response.body));
    });
    const client = new DrasiClient({
      serverUrl: 'https://fixture.invalid', instanceId: 'trading-server', queryIds: ['watchlist-query'],
      reaction: { id: 'sse-stream', endpoint: 'https://proxy.invalid/events' }, fetch: fetcher,
    });
    await expect(client.validateResources()).resolves.toBeUndefined();
    expect(await client.getQuery('watchlist-query')).toEqual(captured.query.body.data);
    expect(await client.getReaction()).toEqual(captured.reaction.body.data);
    expect(await client.getQueryResults('watchlist-query')).toEqual(captured.snapshot.body.data);
    const config = await client.getQueryConfig('watchlist-query');
    expect(config.sources.map(source => source.sourceId)).toEqual(['postgres-stocks', 'price-feed']);
    expect(config.joins?.map(join => join.id)).toEqual(['ON_WATCHLIST', 'HAS_PRICE']);
    expect(config.middleware).toEqual([]);
    expect(config.bootstrapTimeoutSecs).toBe(300);
    expect(captured.provenance.ssePluginVersion).toBe('0.3.4');
    expect(fetcher.mock.calls.every(([, init]) => init?.method === 'GET')).toBe(true);
    await client.disconnect();
  });

  it('types supported GQL, middleware, optional capacities, dispatch/storage and component errors without creation defaults', () => {
    const config = queryConfig('stocks', {
      autoStart: false, queryLanguage: 'GQL', enableBootstrap: false,
      middleware: [{ kind: 'map', name: 'mapper', config: { path: 'value', flags: [true, null, 1] } }],
      sources: [{ sourceId: 'telemetry', nodes: ['Reading'], relations: ['HAS'], pipeline: ['mapper'] }],
      joins: [{ id: 'HAS', keys: [{ label: 'Reading', property: 'device' }] }],
      priorityQueueCapacity: 42, dispatchBufferCapacity: 99, dispatchMode: 'Channel',
      storageBackend: { kind: 'memory', enable_archive: false },
    });
    const value = { ...component('queries', 'stocks', config), error_message: 'diagnostic read, never logged' };
    expect(readQuery(value, { instanceId: refs.instanceId, resourceKind: 'query', resourceId: 'stocks' })).toEqual(value);
  });

  it('normalizes current literal-ID server links without permitting another instance/resource', () => {
    const value = component('queries', 'stocks', queryConfig('stocks'));
    const self = `/api/v1/instances/${refs.instanceId}/queries/stocks`;
    expect(readQuery({ ...value, links: { self, full: `${self}?view=full` } }, {
      instanceId: refs.instanceId, resourceKind: 'query', resourceId: 'stocks',
    })).toEqual(value);
  });

  it.each([
    { id: 'other-query' }, { status: 'unknown' }, { config: null }, { error_message: 3 },
    { links: undefined }, { links: [] }, { links: { self: '/api/v1/queries/watchlist-query', full: '/wrong' } },
    { links: { self: 'https://external.invalid', full: 'https://external.invalid?view=full' } },
    { links: { self: captured.query.body.data.links.self, full: '/api/v1/instances/other/queries/watchlist-query?view=full' } },
  ])('rejects malformed/mismatched full-view component %j', change => {
    expect(() => readQuery({ ...captured.query.body.data, ...change }, queryDetails)).toThrow(DrasiError);
  });

  it.each([
    { id: 'other-query' }, { query: 7 }, { queryLanguage: 'SQL' }, { autoStart: 'true' },
    { enableBootstrap: null }, { bootstrapBufferSize: -1 }, { bootstrapTimeoutSecs: 0.5 },
    { outboxCapacity: Number.MAX_SAFE_INTEGER + 1 }, { sources: null },
    { sources: [{ sourceId: 'one', nodes: [], pipeline: [] }] },
    { sources: [{ sourceId: 3, nodes: [], pipeline: [], relations: [] }] },
    { sources: [{ sourceId: 'one', nodes: [7], pipeline: [], relations: [] }] },
    { sources: [{ sourceId: 'one', nodes: [], pipeline: false, relations: [] }] },
    { middleware: {} }, { middleware: [{ kind: 'map', name: 7, config: {} }] },
    { middleware: [{ kind: 7, name: 'mapper', config: {} }] },
    { middleware: [{ kind: 'map', name: 'mapper', config: [] }] },
    { middleware: [{ kind: 'map', name: 'mapper', config: { value: undefined } }] },
    { joins: null }, { joins: [{ id: 7, keys: [] }] }, { joins: [{ id: 'join', keys: false }] },
    { joins: [{ id: 'join', keys: [{ label: 7, property: 'key' }] }] },
    { joins: [{ id: 'join', keys: [{ label: 'Node', property: 7 }] }] },
    { priorityQueueCapacity: '1000' }, { dispatchBufferCapacity: -1 }, { dispatchMode: 'Broadcast' },
    { storageBackend: { invalid: () => {} } },
  ])('rejects incompatible query config fields %j', change => {
    try {
      readQuery({ ...captured.query.body.data, config: { ...captured.query.body.data.config, ...change } }, queryDetails);
      throw new Error('Expected protocol rejection');
    } catch (error) {
      expect(error).toBeInstanceOf(DrasiError);
      expect(error).toMatchObject({ code: 'INVALID_PAYLOAD', retryable: false, ...queryDetails });
    }
  });

  it.each([
    { id: 'wrong' }, { kind: null }, { queries: 'one' }, { queries: ['..'] },
    { host: 7 }, { port: 0 }, { port: 65536 }, { port: 1.5 }, { ssePath: 'relative' },
    { heartbeatIntervalMs: -1 }, { autoStart: 'yes' }, { identityProvider: 7 },
  ])('rejects malformed supported SSE properties %j', change => {
    expect(() => readReaction({
      ...captured.reaction.body.data, config: { ...captured.reaction.body.data.config, ...change },
    }, reactionDetails)).toThrow(DrasiError);
  });

  it('retains plugin-owned JSON inspection fields and permits unknown kinds only as reads', () => {
    const value = {
      ...captured.reaction.body.data,
      config: { id: 'sse-stream', kind: 'log', queries: [], identityProvider: 'identity', autoStart: false, config: { extra: null } },
    };
    expect(readReaction(value, reactionDetails)).toEqual(value);
  });

  it.each([null, {}, [null], [42], [['nested-row']], [{ invalid: Infinity }], [{ nested: { value: undefined } }]])(
    'rejects snapshots that are not arrays of JSON objects: %j', value => {
      expect(() => readRows(value, queryDetails)).toThrow(DrasiError);
    },
  );

  it('accepts empty and nested JSON snapshots without asserting an application schema', () => {
    expect(readRows([], queryDetails)).toEqual([]);
    const rows = [{ list: [true, false, null, { value: 'unknown to client' }], number: 0 }];
    expect(readRows(rows, queryDetails)).toBe(rows);
  });

  it('rejects a nominally successful envelope containing an error', async () => {
    await expect(readResponse(new Response(JSON.stringify({ success: true, data: [], error: 'private' })), queryDetails))
      .rejects.toMatchObject({ code: 'INVALID_PAYLOAD' });
    await expect(readResponse(json(null), queryDetails)).resolves.toBeNull();
  });
});
