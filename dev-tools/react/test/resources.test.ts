// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { describe, expect, it, vi } from 'vitest';
import { DrasiClient } from '../src/client/DrasiClient';
import { DrasiError } from '../src/client/errors';
import { readQuery, readReaction, readResponse, readRows } from '../src/client/resources';
import captured from './fixtures/server-v1/contract.json';
import current from './fixtures/server-v1-0.2.3/contract.json';
import { component, ids, json, queryConfig, ReadServer, refs } from './server';

const queryDetails = { instanceId: refs.instanceId, resourceKind: 'query' as const, resourceId: ids.stocks };
const reactionDetails = { instanceId: refs.instanceId, resourceKind: 'reaction' as const, resourceId: ids.stream };
const queryValue = component('queries', ids.stocks, queryConfig(ids.stocks, { name: 'orders' }));
const reactionValue = component('reactions', ids.stream, new ReadServer().reaction);

describe('UUID component read contract', () => {
  it.each([
    { version: '0.2.1', sseVersion: '0.3.4', record: captured },
    { version: '0.2.3', sseVersion: '0.3.6', record: current },
  ])('rejects captured pre-UUID $version component identities without a compatibility fallback', async ({ version, sseVersion, record }) => {
    const fetcher = vi.fn<typeof fetch>(async input => {
      const url = new URL(String(input));
      expect(url.pathname.startsWith(`/api/v1/instances/${encodeURIComponent(refs.instanceId)}/`)).toBe(true);
      const response = url.pathname.endsWith('/results') ? record.snapshot
        : url.pathname.includes('/reactions/') ? record.reaction : record.query;
      return new Response(JSON.stringify(response.body));
    });
    const client = new DrasiClient({
      ...refs, serverUrl: 'https://fixture.invalid', fetch: fetcher,
    });
    await expect(client.validateResources()).rejects.toMatchObject({ code: 'INVALID_PAYLOAD' });
    await expect(client.getReaction()).rejects.toMatchObject({ code: 'INVALID_PAYLOAD' });
    expect(record.provenance.serverVersion).toBe(version);
    expect(record.provenance.ssePluginVersion).toBe(sseVersion);
    expect(fetcher.mock.calls.every(([, init]) => init?.method === 'GET')).toBe(true);
    await client.disconnect();
  });

  it('reads assigned IDs, names and bound reaction references through the public client', async () => {
    const server = new ReadServer();
    const client = new DrasiClient({ ...refs, fetch: server.fetch });
    await expect(client.validateResources()).resolves.toBeUndefined();
    expect(await client.getQuery(ids.stocks)).toMatchObject({ id: ids.stocks, name: `component-${ids.stocks}` });
    expect((await client.getReaction()).config.queries).toEqual([{ id: ids.stocks }]);
    expect(await client.getQueryResults(ids.stocks)).toEqual([{ id: 'A', value: '10', price: 10 }]);
    await client.disconnect();
  });

  it('keeps name references distinct from explicit UUID references', () => {
    const inputs = [ids.other, { id: ids.other }, { component: { id: ids.other }, port: 'out' }];
    const value = { ...queryValue, config: { ...queryValue.config, inputs } };
    expect(readQuery(value, queryDetails).config.inputs).toEqual(inputs);
    const renamed = { ...value, name: 'renamed', config: { ...value.config, name: 'renamed' } };
    expect(readQuery(renamed, queryDetails)).toMatchObject({ id: ids.stocks, name: 'renamed' });
    expect(readQuery(renamed, queryDetails).config.inputs).toEqual(inputs);
  });

  it('does not interpret a UUID-shaped reaction name reference as membership by ID', async () => {
    const server = new ReadServer();
    server.reaction.queries = [ids.stocks];
    const client = new DrasiClient({ ...refs, fetch: server.fetch });
    await expect(client.validateResources()).rejects.toMatchObject({ code: 'INCOMPATIBLE_RESOURCE' });
    await client.disconnect();
  });

  it('types supported GQL, middleware, optional capacities, dispatch/storage and component errors without creation defaults', () => {
    const config = queryConfig('b20c42ee-59a1-441f-a549-87a72be1f1e9', {
      autoStart: false, queryLanguage: 'GQL', enableBootstrap: false,
      middleware: [{ kind: 'map', name: 'mapper', config: { path: 'value', flags: [true, null, 1] } }],
      inputs: [{ component: 'telemetry', nodes: ['Reading'], relations: ['HAS'], pipeline: ['mapper'] }, 'room-comfort', { component: 'floor-comfort', port: 'out' }],
      result: { label: 'FloorComfort' },
      joins: [{ id: 'HAS', keys: [{ label: 'Reading', property: 'device' }] }],
      priorityQueueCapacity: 42, dispatchBufferCapacity: 99, dispatchMode: 'Channel',
      storageBackend: { kind: 'memory', enable_archive: false },
    });
    const value = { ...component('queries', 'b20c42ee-59a1-441f-a549-87a72be1f1e9', config), errorMessage: 'diagnostic read, never logged' };
    expect(readQuery(value, { instanceId: refs.instanceId, resourceKind: 'query', resourceId: 'b20c42ee-59a1-441f-a549-87a72be1f1e9' })).toEqual(value);
  });

  it('normalizes current literal-ID server links without permitting another instance/resource', () => {
    const value = component('queries', 'b20c42ee-59a1-441f-a549-87a72be1f1e9', queryConfig('b20c42ee-59a1-441f-a549-87a72be1f1e9'));
    const self = `/api/v1/instances/${refs.instanceId}/queries/${ids.stocks}`;
    expect(readQuery({ ...value, links: { self, full: `${self}?view=full` } }, {
      instanceId: refs.instanceId, resourceKind: 'query', resourceId: 'b20c42ee-59a1-441f-a549-87a72be1f1e9',
    })).toEqual(value);
  });

  it.each([
    { id: 'other-query' }, { name: undefined }, { name: ' ' },
    { status: 'unknown' }, { config: null }, { errorMessage: 3 },
    { links: undefined }, { links: [] }, { links: { self: '/api/v1/queries/watchlist-query', full: '/wrong' } },
    { links: { self: 'https://external.invalid', full: 'https://external.invalid?view=full' } },
    { links: { self: queryValue.links.self, full: `/api/v1/instances/other/queries/${ids.stocks}?view=full` } },
  ])('rejects malformed/mismatched full-view component %j', change => {
    expect(() => readQuery({ ...queryValue, ...change }, queryDetails)).toThrow(DrasiError);
  });

  it.each([
    { id: 'other-query' }, { name: undefined }, { name: 'not-the-current-name' },
    { query: 7 }, { queryLanguage: 'SQL' }, { autoStart: 'true' },
    { enableBootstrap: null }, { outboxCapacity: -1 }, { bootstrapTimeoutSecs: 0.5 },
    { outboxCapacity: Number.MAX_SAFE_INTEGER + 1 }, { inputs: null }, { inputs: undefined }, { inputs: [''] }, { inputs: [{ sourceId: 'one' }] }, { inputs: [{ component: 'one', extra: true }] },
    { result: { label: '' } }, { result: 7 },
    { inputs: [{ component: 'one', nodes: 7 }] },
    { inputs: [{ component: 3, nodes: [], pipeline: [], relations: [] }] },
    { inputs: [{ component: 'one', nodes: [7], pipeline: [], relations: [] }] },
    { inputs: [{ component: 'one', nodes: [], pipeline: false, relations: [] }] },
    { inputs: [{ id: 'one' }] }, { inputs: [{ id: ids.other, name: 'one' }] },
    { inputs: [{ component: { id: ids.other, extra: true } }] },
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
      readQuery({ ...queryValue, config: { ...queryValue.config, ...change } }, queryDetails);
      throw new Error('Expected protocol rejection');
    } catch (error) {
      expect(error).toBeInstanceOf(DrasiError);
      expect(error).toMatchObject({ code: 'INVALID_PAYLOAD', retryable: false, ...queryDetails });
    }
  });

  it.each([
    { id: 'wrong' }, { name: undefined }, { kind: null }, { queries: 'one' }, { queries: [''] },
    { queries: [{ id: '..' }] }, { queries: [{ id: ids.stocks, name: 'orders' }] },
    { host: 7 }, { port: 0 }, { port: 65536 }, { port: 1.5 }, { ssePath: 'relative' },
    { heartbeatIntervalMs: -1 }, { autoStart: 'yes' }, { identityProvider: 7 },
  ])('rejects malformed supported SSE properties %j', change => {
    expect(() => readReaction({
      ...reactionValue, config: { ...reactionValue.config, ...change },
    }, reactionDetails)).toThrow(DrasiError);
  });

  it('retains plugin-owned JSON inspection fields and permits unknown kinds only as reads', () => {
    const value = {
      ...reactionValue,
      config: { id: ids.stream, name: reactionValue.name, kind: 'log', queries: [],
        identityProvider: 'identity', autoStart: false, config: { extra: null } },
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
