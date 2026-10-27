// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import React from 'react';
import { act, cleanup, render } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { accumulateResult } from '../src/client/accumulation';
import { DrasiError } from '../src/client/errors';
import {
  applyGraphChange, elementKey, graphRow, graphRowIdentity, graphRowKey, nativeGraphResultAdapter,
  readGraphChange, readGraphSnapshot, readGraphSnapshotResponse,
} from '../src/client/graph';
import { readAdaptedResults } from '../src/client/results';
import type { OutputDelta, ResultRow } from '../src/client/types';
import { DrasiProvider, useDrasiClient, useDrasiQuery } from '../src/react/DrasiContext';
import type { UseDrasiQueryResult } from '../src/react/types';
import { fakeEventSourceFactory } from './FakeEventSource';
import { ReadServer, deferred, refs } from './server';

const id = (key: string, namespace = 'db') => ({ namespace, id: key });
const add = (key: string, after: ResultRow, namespace = 'db') =>
  ({ operation: 'ADD', identity: id(key, namespace), after });
const context = { receivedAt: 1 };
const out = (alias: string) => ({ alias, component: alias, port: 'out' });
const delta = (changes: unknown[], alias = 'db'): OutputDelta => {
  const [result] = nativeGraphResultAdapter(
    { output: out(alias), sequence: 1, publishedAt: 1, changes }, context,
  );
  return result;
};
const apply = (rows: ResultRow[], changes: unknown[]) =>
  accumulateResult(rows, delta(changes), graphRowKey);
const snapshot = (...changes: unknown[]) =>
  readGraphSnapshot(changes);

describe('standard graph changes from any producer', () => {
  it('validates the real operation, identity and only the images a record carries', () => {
    expect(readGraphChange(add('a', { name: 'one' }))).toEqual({
      kind: 'graph-add', identity: id('a'), after: { name: 'one' },
    });
    expect(readGraphChange({ operation: 'REPLACE', identity: id('a'), after: { n: 1 }, before: { n: 0 } }))
      .toEqual({ kind: 'graph-replace', identity: id('a'), after: { n: 1 }, before: { n: 0 } });
    // An absent before image is unknown: the key is missing, not null.
    expect(readGraphChange({ operation: 'REPLACE', identity: id('a'), after: {} })).not.toHaveProperty('before');
    expect(readGraphChange({
      operation: 'PATCH', identity: id('a'), after: { nick: null }, removedProperties: ['colour'],
    })).toEqual({ kind: 'graph-patch', identity: id('a'), set: { nick: null }, removed: ['colour'] });
    expect(readGraphChange({ operation: 'DELETE', identity: id('a') }))
      .toEqual({ kind: 'graph-delete', identity: id('a') });

    for (const bad of [
      null, [], { operation: 'ADD', identity: id('a') },
      { operation: 'ADD', identity: { namespace: '', id: 'a' }, after: {} },
      { operation: 'ADD', identity: { namespace: 'db' }, after: {} },
      { operation: 'ADD', identity: id('a'), after: null },
      { operation: 'ADD', identity: id('a'), after: {}, removedProperties: [] },
      { operation: 'REPLACE', identity: id('a'), after: {}, before: null },
      { operation: 'PATCH', identity: id('a'), after: {} },
      { operation: 'PATCH', identity: id('a'), after: { x: 1 }, removedProperties: ['x'] },
      { operation: 'PATCH', identity: id('a'), after: {}, removedProperties: [1] },
      { operation: 'DELETE', identity: id('a'), after: {} },
      { operation: 'UPSERT', identity: id('a'), after: {} },
      { operation: 'ADD', identity: id('a'), after: {}, row_signature: 3 },
    ]) {
      expect(() => readGraphChange(bad), JSON.stringify(bad)).toThrow(DrasiError);
    }
  });

  it('keys rows by canonical identity: equal values stay separate and the pair is unambiguous', () => {
    const rows = apply([], [
      add('a', { v: 1 }), add('b', { v: 1 }), add('a', { v: 1 }, 'other'),
      add('c', { v: 1 }, 'ab'), add('bc', { v: 1 }, 'a'),
    ]);
    expect(rows).toHaveLength(5);
    expect(new Set(rows.map(graphRowKey)).size).toBe(5);
    expect(elementKey(id('bc', 'a'))).not.toBe(elementKey(id('c', 'ab')));
    expect(graphRowIdentity(rows[0])).toEqual(id('a'));
    expect(() => graphRowKey({ v: 1 })).toThrow(DrasiError);
  });

  it('add and replace need no base; replace drops absent properties; repeats are idempotent', () => {
    let rows = apply([], [add('a', { x: 1, y: 2 })]);
    expect(apply(rows, [add('a', { x: 1, y: 2 })])).toEqual([{ x: 1, y: 2 }]);
    rows = apply(rows, [{ operation: 'REPLACE', identity: id('a'), after: { x: 5 } }]);
    expect(rows).toEqual([{ x: 5 }]);
    // Replace with no base row is a complete element, so it creates it.
    expect(apply([], [{ operation: 'REPLACE', identity: id('z'), after: { x: 9 } }])).toEqual([{ x: 9 }]);
  });

  it('a patch sets, removes and keeps explicit null apart from a removed property', () => {
    const base = apply([], [add('a', { name: 'one', colour: 'red', age: 4, nick: 'n' })]);
    const next = apply(base, [{
      operation: 'PATCH', identity: id('a'), after: { name: 'two', age: null }, removedProperties: ['colour', 'ghost'],
    }]);
    expect(next).toEqual([{ name: 'two', age: null, nick: 'n' }]);
    expect(next[0]).toHaveProperty('age', null);
    expect(next[0]).not.toHaveProperty('colour');
    expect(graphRowIdentity(next[0])).toEqual(id('a'));
    // The input state was not mutated.
    expect(base).toEqual([{ name: 'one', colour: 'red', age: 4, nick: 'n' }]);
  });

  it('a patch without its base is an explicit retryable error that leaves state untouched', () => {
    const base = apply([], [add('a', { v: 1 })]);
    const run = () => apply(base, [{ operation: 'PATCH', identity: id('b'), after: { v: 2 }, removedProperties: [] }]);
    expect(run).toThrow(expect.objectContaining({ code: 'MISSING_BASE_ROW', retryable: true }));
    expect(base).toEqual([{ v: 1 }]);
    const rows = new Map<string, ResultRow>();
    expect(() => applyGraphChange(rows, { kind: 'graph-patch', identity: id('x'), set: {}, removed: [] }))
      .toThrow(DrasiError);
  });

  it('applies an identity-only delete to existing state; a delete of an absent element is a no-op', () => {
    const base = apply([], [add('a', { v: 1 }), add('b', { v: 1 })]);
    const next = apply(base, [{ operation: 'DELETE', identity: id('a') }]);
    expect(next).toEqual([{ v: 1 }]);
    expect(graphRowIdentity(next[0])).toEqual(id('b'));
    expect(apply(next, [{ operation: 'DELETE', identity: id('gone') }])).toEqual(next);
    expect(apply(base, [{ operation: 'DELETE', identity: id('a'), before: { v: 1 } }])).toHaveLength(1);
  });

  it('reads a snapshot of ADD changes with identities and rejects anything else', () => {
    const rows = snapshot(add('a', { v: 1 }), add('b', { v: 2 }));
    expect(rows.map(graphRowIdentity)).toEqual([id('a'), id('b')]);
    expect(() => readGraphSnapshot({})).toThrow(DrasiError);
    expect(() => readGraphSnapshot([{ v: 1 }])).toThrow(DrasiError);
    expect(() => readGraphSnapshot([{ operation: 'DELETE', identity: id('a') }])).toThrow(DrasiError);
    expect(() => readGraphSnapshot([add('a', {}), add('a', {})])).toThrow(DrasiError);
    // A snapshot then a patch uses the snapshot's identity as its base.
    expect(accumulateResult([], { kind: 'snapshot', outputId: 'db', rows, receivedAt: 0 }, graphRowKey)).toHaveLength(2);
    expect(apply(rows, [{ operation: 'PATCH', identity: id('b'), after: { w: 3 }, removedProperties: ['v'] }])[1])
      .toEqual({ w: 3 });
  });

  it('maps a native frame to a delta keyed by its output alias and rejects query-shaped frames', () => {
    const [result] = nativeGraphResultAdapter({
      output: { alias: 'db', component: 'facilities-db', port: 'errors' },
      sequence: 4, publishedAt: 12, changes: [add('a', { v: 1 })],
    }, context);
    expect(result).toMatchObject({ kind: 'delta', outputId: 'db', sourceTimestamp: 12 });
    expect(nativeGraphResultAdapter({ type: 'heartbeat', ts: 5 }, context)).toEqual([]);
    const frame = { output: out('db'), sequence: 1, publishedAt: 1, changes: [] };
    for (const bad of [
      null, { producer: 'db', results: [], timestamp: 1 },
      { queryId: 'db', sequence: 1, publishedAt: 1, changes: [] },
      { ...frame, queryId: 'db' },
      { ...frame, producer: 'db' },
      { ...frame, timestamp: 1 },
      { output: out('db'), sequence: 1, changes: [] },
      { ...frame, sequence: 1.5 },
      { ...frame, publishedAt: -1 },
      { ...frame, changes: [{ type: 'ADD', data: {} }] },
      { ...frame, output: { alias: 'db', component: 'db' } },
      { ...frame, output: { ...out('db'), extra: 1 } },
      { ...frame, output: out('') },
      { type: 'heartbeat', ts: 5, output: undefined, extra: 1 },
    ]) {
      expect(() => nativeGraphResultAdapter(bad, context), JSON.stringify(bad)).toThrow(DrasiError);
    }
  });

  it('validates graph changes returned by a custom adapter', () => {
    const ok = [{
      kind: 'delta', outputId: 'db', receivedAt: 1,
      changes: [
        { kind: 'graph-add', identity: id('a'), after: { v: 1 } },
        { kind: 'graph-replace', identity: id('a'), after: { v: 2 }, before: { v: 1 } },
        { kind: 'graph-patch', identity: id('a'), set: { v: 3 }, removed: [] },
        { kind: 'graph-delete', identity: id('a') },
      ],
    }];
    expect(readAdaptedResults(ok, context)[0].changes).toHaveLength(4);
    for (const change of [
      { kind: 'graph-add', identity: id('a') },
      { kind: 'graph-patch', identity: id('a'), set: { v: 3 } },
      { kind: 'graph-delete', identity: { namespace: 'db' } },
    ]) {
      expect(() => readAdaptedResults([{ ...ok[0], changes: [change] }], context)).toThrow(DrasiError);
    }
  });

  it('classifies a snapshot endpoint response, naming an absent read capability', async () => {
    const status = (code: number) => readGraphSnapshotResponse(new Response('x', { status: code }), {});
    await expect(status(501)).rejects.toMatchObject({ code: 'UNSUPPORTED_CAPABILITY', status: 501 });
    await expect(status(404)).rejects.toMatchObject({ code: 'INCOMPATIBLE_RESOURCE' });
    await expect(status(503)).rejects.toMatchObject({ code: 'SERVER_UNAVAILABLE' });
    await expect(status(401)).rejects.toMatchObject({ code: 'UNAUTHENTICATED' });
    await expect(status(403)).rejects.toMatchObject({ code: 'FORBIDDEN' });
    await expect(readGraphSnapshotResponse(new Response('not json'), {})).rejects.toMatchObject({ code: 'INVALID_PAYLOAD' });
    await expect(readGraphSnapshotResponse(new Response('[]'), {})).resolves.toEqual([]);
  });
});

const graphSnapshots = { url: (producer: string) => `https://events.invalid/snapshot/${encodeURIComponent(producer)}` };

describe('graph row state through the real provider, client and stream', () => {
  const flush = () => act(async () => { await vi.advanceTimersByTimeAsync(0); });
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => { cleanup(); vi.useRealTimers(); });

  function fixture(initial: () => Promise<Response>) {
    const server = new ReadServer(), factory = fakeEventSourceFactory();
    let snapshotCalls = 0;
    const read = server.fetch.getMockImplementation()!;
    let snapshot = initial;
    server.fetch.mockImplementation((input, init) => {
      const url = new URL(String(input));
      if (url.origin === 'https://events.invalid' && url.pathname.startsWith('/snapshot/')) {
        snapshotCalls += 1;
        return snapshot();
      }
      return read(input, init);
    });
    let observed: UseDrasiQueryResult<ResultRow> | undefined;
    let client: ReturnType<typeof useDrasiClient>['client'] = null;
    function Probe() {
      observed = useDrasiQuery<ResultRow>('stocks', { getKey: graphRowKey, transform: row => ({ ...row }) });
      client = useDrasiClient().client;
      return null;
    }
    const tree = <DrasiProvider
      {...refs} fetch={server.fetch} eventSourceFactory={factory.create}
      resultAdapter={nativeGraphResultAdapter}
      graphSnapshots={graphSnapshots}
      reconnect={{ maxReconnectAttempts: 2, initialReconnectDelayMs: 10, maxReconnectDelayMs: 20 }}
    ><Probe /></DrasiProvider>;
    const source = () => factory.instances[factory.instances.length - 1];
    const emit = (changes: unknown[]) =>
      act(() => source().message({ output: out('stocks'), sequence: 1, publishedAt: 1, changes }));
    return {
      tree, emit, source, get: () => observed!, client: () => client, calls: () => snapshotCalls,
      setSnapshot: (next: () => Promise<Response>) => { snapshot = next; },
    };
  }
  const rows = (...changes: unknown[]) => Promise.resolve(new Response(JSON.stringify(changes)));

  it('loads identity-bearing rows and applies patches, null values and identity deletes', async () => {
    const f = fixture(() => rows(add('a', { name: 'one', colour: 'red' }), add('b', { name: 'one', colour: 'red' })));
    render(f.tree);
    await flush();
    act(() => f.source().open());
    await flush();
    expect(f.get().status).toBe('live');
    expect(f.get().data).toHaveLength(2);

    f.emit([{ operation: 'PATCH', identity: id('a'), after: { name: 'two', nick: null }, removedProperties: ['colour'] }]);
    expect(f.get().data).toEqual([{ name: 'two', nick: null }, { name: 'one', colour: 'red' }]);
    f.emit([{ operation: 'DELETE', identity: id('b') }]);
    expect(f.get().data).toEqual([{ name: 'two', nick: null }]);
    f.emit([{ operation: 'ADD', identity: id('c'), after: { name: 'three' } }]);
    expect(f.get().data).toHaveLength(2);
  });

  it('resynchronizes from a fresh snapshot when a patch has no base', async () => {
    const f = fixture(() => rows(add('a', { name: 'one' })));
    render(f.tree);
    await flush();
    act(() => f.source().open());
    await flush();
    expect(f.calls()).toBe(1);
    f.setSnapshot(() => rows(add('a', { name: 'one' }), add('z', { name: 'fresh' })));
    f.emit([{ operation: 'PATCH', identity: id('missing'), after: { name: 'x' }, removedProperties: [] }]);
    await flush();
    await act(async () => { await vi.advanceTimersByTimeAsync(50); });
    expect(f.calls()).toBeGreaterThan(1);
    expect(f.get().data).toEqual([{ name: 'one' }, { name: 'fresh' }]);
    expect(f.get().status).toBe('live');
  });

  it('reports an absent snapshot capability explicitly instead of an empty table', async () => {
    const f = fixture(() => Promise.resolve(new Response('no provider', { status: 501 })));
    render(f.tree);
    await flush();
    act(() => f.source().open());
    await flush();
    expect(f.get().status).toBe('terminal-error');
    expect(f.get().error?.code).toBe('UNSUPPORTED_CAPABILITY');
    expect(f.get().data).toBeNull();
  });

  it('exposes row identity for rows the application built', () => {
    const row = graphRow(id('x'), { v: 1 });
    expect(graphRowIdentity(row)).toEqual(id('x'));
    expect(row).toEqual({ v: 1 });
    expect(deferred<number>().promise).toBeInstanceOf(Promise);
    expect(React).toBeDefined();
  });
});
