// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import React from 'react';
import { act, cleanup, render } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { DrasiProvider, useDrasiClient, useDrasiQuery } from '../src/react/DrasiContext';
import type { DrasiContextValue } from '../src/react/DrasiContext';
import type { DrasiClientOptions } from '../src/client/DrasiClient';
import { DrasiError } from '../src/client/errors';
import type { ResultRow } from '../src/client/types';
import type { UseDrasiQueryOptions, UseDrasiQueryResult } from '../src/react/types';
import { fakeEventSourceFactory } from './FakeEventSource';
import { ReadServer, deferred, failure, json, refs } from './server';

interface ViewRow { label: string; value: number }
const options: UseDrasiQueryOptions<ViewRow> = {
  getKey: row => {
    if (typeof row.asset !== 'string') throw new Error('Missing raw identity');
    return row.asset;
  },
  transform: row => {
    if (typeof row.label !== 'string' || typeof row.value !== 'number') throw new Error('Invalid view row');
    return { label: row.label, value: row.value };
  },
};
const first = { asset: 'rack-a', label: 'same visible value', value: 1 };
const second = { asset: 'rack-b', label: 'same visible value', value: 1 };
const flush = () => act(async () => { await vi.advanceTimersByTimeAsync(0); });

beforeEach(() => vi.useFakeTimers());
afterEach(() => { cleanup(); vi.useRealTimers(); });

function fixture(overrides: Partial<DrasiClientOptions> = {}) {
  const server = new ReadServer(), factory = fakeEventSourceFactory();
  server.reaction.queries = ['stocks', 'other'];
  const snapshots = new Map<string, () => Promise<Response>>([
    ['stocks', async () => json([first])], ['other', async () => json([second])],
  ]);
  const read = server.fetch.getMockImplementation()!;
  server.fetch.mockImplementation((input, init) => {
    const url = new URL(String(input));
    if (url.pathname.endsWith('/results')) {
      const segments = url.pathname.split('/');
      const queryId = segments[segments.length - 2];
      return snapshots.get(queryId)!();
    }
    return read(input, init);
  });
  const observed = new Map<string, UseDrasiQueryResult<ViewRow>>();
  let context: DrasiContextValue | undefined;
  function Probe({ name = 'first', queryId = 'stocks', queryOptions = options }: {
    name?: string; queryId?: string; queryOptions?: UseDrasiQueryOptions<ViewRow>;
  }) {
    const result = useDrasiQuery(queryId, queryOptions);
    context = useDrasiClient();
    observed.set(name, result);
    return <output aria-label={name}>{JSON.stringify({ ...result, error: result.error?.code })}</output>;
  }
  const tree = (children: React.ReactNode) => <DrasiProvider
    {...refs} queryIds={['stocks', 'other']} fetch={server.fetch} eventSourceFactory={factory.create}
    reconnect={{ maxReconnectAttempts: 2, initialReconnectDelayMs: 10, maxReconnectDelayMs: 20 }}
    {...overrides}
  >{children}</DrasiProvider>;
  const get = (name = 'first') => observed.get(name)!;
  const source = () => factory.instances[factory.instances.length - 1];
  const open = async () => {
    await flush();
    expect(factory.instances).toHaveLength(1);
    act(() => source().open());
    await flush();
  };
  const emit = (results: unknown[], queryId = 'stocks', timestamp = 1) =>
    act(() => source().message({ queryId, timestamp, results }));
  return { server, factory, snapshots, Probe, tree, get, open, emit, source, context: () => context! };
}

describe('observable result state on the real provider/client transport', () => {
  it('does not equate an open socket with data readiness and names an empty snapshot', async () => {
    const f = fixture(), pending = deferred<Response>();
    f.snapshots.set('stocks', () => pending.promise);
    render(f.tree(<f.Probe />));
    expect(f.get()).toMatchObject({ status: 'initial-loading', data: null, loading: true, stale: false });
    await f.open();
    expect(f.context().client?.getConnectionStatus().connected).toBe(true);
    expect(f.get()).toMatchObject({ status: 'initial-loading', data: null, loading: true });
    pending.resolve(json([]));
    await flush();
    expect(f.get()).toMatchObject({ status: 'empty', data: [], loading: false, error: null, stale: false });
  });

  it('uses raw identities even when transforms drop them, keeping equal-valued groups and sparse deletes distinct', async () => {
    const f = fixture();
    f.snapshots.set('stocks', async () => json([first, second]));
    const transform = vi.fn(options.transform);
    render(f.tree(<f.Probe queryOptions={{ ...options, transform }} />));
    await f.open();
    expect(f.get().data).toEqual([{ label: first.label, value: 1 }, { label: first.label, value: 1 }]);
    const moved = { ...first, asset: 'rack-c', value: 2 };
    f.emit([{ type: 'UPDATE', before: { asset: 'rack-a' }, after: moved, data: moved }]);
    f.emit([{ type: 'ADD', data: moved }, { type: 'ADD', data: moved }]);
    expect(f.get().data).toEqual([{ label: first.label, value: 1 }, { label: first.label, value: 2 }]);
    f.emit([{ type: 'DELETE', data: { asset: 'rack-c' } }]);
    expect(f.get().data).toEqual([{ label: first.label, value: 1 }]);
    expect(transform.mock.calls.every(([row]) => typeof row.value === 'number')).toBe(true);
    f.emit([{ type: 'DELETE', data: { asset: 'rack-b' } }]);
    expect(f.get()).toMatchObject({ status: 'empty', data: [], stale: false });
  });

  it('makes transform-null and derived filtering reactive without orphaning hidden raw identities', async () => {
    const f = fixture();
    let minimum = 0;
    const viewOptions = (): UseDrasiQueryOptions<ViewRow> => ({
      ...options,
      transform: row => row.hidden ? null : options.transform(row),
      postProcess: rows => rows.filter(row => row.value >= minimum),
    });
    const view = render(f.tree(<f.Probe queryOptions={viewOptions()} />));
    await f.open();
    const reads = f.server.fetch.mock.calls.length;
    minimum = 3;
    view.rerender(f.tree(<f.Probe queryOptions={viewOptions()} />));
    expect(f.get()).toMatchObject({ status: 'empty', data: [] });
    minimum = 0;
    view.rerender(f.tree(<f.Probe queryOptions={viewOptions()} />));
    expect(f.get().data).toHaveLength(1);
    f.emit([{ type: 'ADD', data: { ...first, hidden: true } }]);
    expect(f.get().data).toEqual([]);
    f.emit([{ type: 'DELETE', data: { asset: first.asset } }]);
    view.rerender(f.tree(<f.Probe queryOptions={options} />));
    expect(f.get().data).toEqual([]);
    f.emit([{ type: 'ADD', data: { ...first, value: 2 } }]);
    expect(f.get().data).toEqual([{ label: first.label, value: 2 }]);
    expect(f.server.fetch).toHaveBeenCalledTimes(reads);
    expect(f.factory.instances).toHaveLength(1);
  });

  it('rekeys retained raw rows on option changes without reopening the stream', async () => {
    const f = fixture();
    f.snapshots.set('stocks', async () => json([{ ...first, renamedId: 'alias' }]));
    const view = render(f.tree(<f.Probe />));
    await f.open();
    const reads = f.server.fetch.mock.calls.length;
    view.rerender(f.tree(<f.Probe queryOptions={{
      ...options, getKey: row => typeof row.renamedId === 'string' ? row.renamedId : '',
    }} />));
    f.emit([{ type: 'DELETE', data: { renamedId: 'alias' } }]);
    expect(f.get().data).toEqual([]);
    expect(f.server.fetch).toHaveBeenCalledTimes(reads);
    expect(f.factory.instances).toHaveLength(1);
  });

  it('preserves last-good data through reconnect and a new baseline, including offline deletion', async () => {
    const f = fixture(), snapshot = deferred<Response>();
    render(f.tree(<f.Probe />));
    await f.open();
    f.snapshots.set('stocks', () => snapshot.promise);
    act(() => f.source().fail());
    expect(f.get()).toMatchObject({ status: 'reconnecting', stale: true, errorScope: 'connection' });
    expect(f.get().data).toHaveLength(1);
    await act(async () => { await vi.advanceTimersByTimeAsync(10); });
    expect(f.factory.instances).toHaveLength(2);
    act(() => f.source().open());
    expect(f.get()).toMatchObject({ status: 'resynchronizing', stale: true });
    snapshot.resolve(json([]));
    await flush();
    expect(f.get()).toMatchObject({ status: 'empty', data: [], stale: false, error: null });
  });

  it('names a shared explicit retry resynchronizing when useful data already exists', async () => {
    const f = fixture(), pending = deferred<Response>();
    render(f.tree(<f.Probe />));
    await f.open();
    f.snapshots.set('stocks', () => pending.promise);
    act(() => f.context().retry());
    expect(f.get()).toMatchObject({
      status: 'resynchronizing', stale: true, loading: false, data: [{ label: first.label, value: 1 }],
    });
    await flush();
    expect(f.factory.instances).toHaveLength(2);
    expect(f.factory.instances[0].closed).toBe(true);
    act(() => f.source().open());
    await flush();
    expect(f.get().status).toBe('resynchronizing');
    pending.resolve(json([{ ...first, value: 4 }]));
    await flush();
    expect(f.get()).toMatchObject({ status: 'live', stale: false, data: [{ label: first.label, value: 4 }] });
  });

  it('keeps transient query failure and explicit retry local while another query remains live', async () => {
    const f = fixture();
    render(f.tree(<><f.Probe /><f.Probe name="second" queryId="other" /></>));
    await f.open();
    f.snapshots.set('stocks', async () => failure(503));
    act(() => f.get().retry());
    await flush();
    expect(f.get()).toMatchObject({
      status: 'stale-last-good-data', stale: true, data: [{ label: first.label, value: 1 }],
      error: { code: 'SERVER_UNAVAILABLE' }, errorScope: 'query',
    });
    const pending = deferred<Response>();
    f.snapshots.set('stocks', () => pending.promise);
    act(() => f.get().retry());
    expect(f.get()).toMatchObject({ status: 'resynchronizing', stale: true });
    f.emit([{ type: 'ADD', data: { ...second, value: 5 } }], 'other');
    expect(f.get('second')).toMatchObject({ status: 'live', data: [{ label: second.label, value: 5 }], stale: false });
    pending.resolve(json([{ ...first, value: 9 }]));
    await flush();
    await act(async () => { await vi.advanceTimersByTimeAsync(100); });
    expect(f.get()).toMatchObject({ status: 'live', data: [{ label: first.label, value: 9 }], error: null });
    expect(f.factory.instances).toHaveLength(1);
  });

  it('isolates terminal query protocol errors and retries the selected query without recreating resources or sockets', async () => {
    const f = fixture();
    render(f.tree(<><f.Probe /><f.Probe name="second" queryId="other" /></>));
    await f.open();
    f.emit([{ type: 'UPDATE', after: {} }]);
    expect(f.get()).toMatchObject({
      status: 'terminal-error', stale: true, error: { code: 'INVALID_PAYLOAD', resourceId: 'stocks' }, errorScope: 'query',
    });
    expect(f.get('second').status).toBe('live');
    expect(f.context().error).toBeNull();
    expect(f.source().closed).toBe(false);
    f.emit([{ type: 'ADD', data: { ...first, value: 8 } }]);
    expect(f.get().data?.[0].value).toBe(1);
    f.snapshots.set('stocks', async () => json([{ ...first, value: 8 }]));
    act(() => f.get().retry());
    await flush();
    expect(f.get()).toMatchObject({ status: 'live', data: [{ label: first.label, value: 8 }], error: null });
    expect(f.factory.instances).toHaveLength(1);
    expect(f.server.fetch.mock.calls.every(([, init]) => init?.method === 'GET')).toBe(true);
  });

  it('reports missing raw identity transactionally and retains useful last-good data', async () => {
    const f = fixture();
    render(f.tree(<f.Probe />));
    await f.open();
    f.emit([{ type: 'DELETE', data: { asset: 'rack-a' } }, { type: 'ADD', data: { label: 'bad', value: 3 } }]);
    expect(f.get()).toMatchObject({
      status: 'terminal-error', stale: true, error: { code: 'INVALID_ROW_KEY' },
      data: [{ label: first.label, value: 1 }],
    });
    expect(f.source().closed).toBe(false);
  });

  it('keeps projection errors safe and reactive, retaining last-good values and the original typed error', async () => {
    const f = fixture(), error = new DrasiError('RESULT_PROCESSING_FAILED', { resourceKind: 'query', resourceId: 'stocks' });
    const view = render(f.tree(<f.Probe />));
    await f.open();
    const lastUpdate = f.get().lastUpdate, reads = f.server.fetch.mock.calls.length;
    view.rerender(f.tree(<f.Probe queryOptions={{ ...options, transform: () => { throw error; } }} />));
    expect(f.get()).toMatchObject({ status: 'terminal-error', stale: true, data: [{ label: first.label, value: 1 }] });
    expect(f.get().error).toBe(error);
    f.emit([{ type: 'ADD', data: { ...first, value: 2 } }]);
    expect(f.get().lastUpdate).toBe(lastUpdate);
    view.rerender(f.tree(<f.Probe queryOptions={{
      ...options, postProcess: () => { throw new Error('private derived row content'); },
    }} />));
    expect(f.get().error?.code).toBe('RESULT_PROCESSING_FAILED');
    expect(f.get().error?.message).not.toContain('private');
    view.rerender(f.tree(<f.Probe />));
    expect(f.get()).toMatchObject({ status: 'live', stale: false, data: [{ label: first.label, value: 2 }], error: null });
    expect(f.server.fetch).toHaveBeenCalledTimes(reads);
  });

  it('shows overflow/resynchronization, aborts stale reads, and does not cap the actual result set', async () => {
    const f = fixture({ reconciliation: { maxPendingChanges: 2 } }), pending = deferred<Response>();
    const many: ResultRow[] = Array.from({ length: 25 }, (_, i) => ({ asset: `rack-${i}`, label: 'row', value: i }));
    f.snapshots.set('stocks', async () => json(many));
    render(f.tree(<f.Probe />));
    await f.open();
    expect(f.get().data).toHaveLength(25);
    f.snapshots.set('stocks', () => pending.promise);
    act(() => f.get().retry());
    await flush();
    const calls = f.server.fetch.mock.calls.filter(([url]) => String(url).endsWith('/stocks/results'));
    const pendingSignal = calls[calls.length - 1][1]?.signal;
    f.emit([1, 2, 3].map(value => ({ type: 'ADD', data: { ...first, value } })));
    expect(pendingSignal?.aborted).toBe(true);
    expect(f.get()).toMatchObject({
      status: 'resynchronizing', stale: true, error: { code: 'RESULT_BUFFER_OVERFLOW' }, errorScope: 'query',
    });
    expect(f.get().data).toHaveLength(25);
    pending.resolve(json([{ ...first, value: -100 }]));
    f.snapshots.set('stocks', async () => json([...many, { ...first, value: 50 }]));
    await act(async () => { await vi.advanceTimersByTimeAsync(10); });
    expect(f.get()).toMatchObject({ status: 'live', error: null, stale: false });
    expect(f.get().data).toHaveLength(26);
    expect(f.get().data?.some(row => row.value === -100)).toBe(false);
    expect(f.factory.instances).toHaveLength(1);
  });

  it('supports multiple independent subscribers, unsubscribe/resubscribe and StrictMode with one connection', async () => {
    const f = fixture();
    const children = (show: boolean) => <React.StrictMode><f.Probe />
      {show && <f.Probe name="second" />}
    </React.StrictMode>;
    const view = render(f.tree(children(true)));
    await f.open();
    expect(f.get().data).toHaveLength(1);
    expect(f.get('second').data).toHaveLength(1);
    const staleRetry = f.get('second').retry;
    view.rerender(f.tree(children(false)));
    const reads = f.server.fetch.mock.calls.length;
    act(() => staleRetry());
    expect(f.server.fetch).toHaveBeenCalledTimes(reads);
    f.emit([{ type: 'DELETE', data: { asset: first.asset } }]);
    expect(f.get().data).toEqual([]);
    f.snapshots.set('stocks', async () => json([]));
    view.rerender(f.tree(children(true)));
    await flush();
    expect(f.get('second').data).toEqual([]);
    expect(f.source().closed).toBe(false);
    expect(f.factory.instances).toHaveLength(1);
    view.unmount();
    expect(f.source().closed).toBe(true);
  });
});
