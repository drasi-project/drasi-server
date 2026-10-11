// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import React from 'react';
import { act, fireEvent, render, renderHook, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { DrasiClient, type DrasiClientOptions } from '../src/client/DrasiClient';
import { createLegacyResultAdapter } from '../src/client/results';
import {
  DrasiClientProvider, DrasiProvider, useDrasiClient, useDrasiConnectionStatus,
  useDrasiQuery, useDrasiQueryDefinition, type DrasiContextValue,
} from '../src/react/DrasiContext';
import { fakeEventSourceFactory } from './FakeEventSource';
import { component, deferred, failure, json, queryConfig, refs } from './server';

afterEach(() => vi.useRealTimers());

function fixture() {
  const factory = fakeEventSourceFactory();
  const fetcher = vi.fn<typeof fetch>(async (input, init) => {
    init?.signal?.throwIfAborted();
    const url = new URL(String(input)), segments = url.pathname.split('/');
    const instance = decodeURIComponent(segments[4]), kind = segments[5], id = decodeURIComponent(segments[6]);
    if (url.pathname.endsWith('/results')) return json([{ device: instance, query: id, value: 1 }]);
    if (kind === 'queries') return json(component('queries', id, queryConfig(id), 'Running', instance));
    return json(component('reactions', id, {
      id, kind: 'sse', queries: ['stocks', 'other'], host: '0.0.0.0', port: 9999,
      ssePath: '/events', heartbeatIntervalMs: 15000,
    }, 'Running', instance));
  });
  return { factory, fetcher };
}

function Probe({ observe, queryId = 'stocks' }: {
  queryId?: string; observe?: (value: DrasiContextValue) => void;
}) {
  const context = useDrasiClient(), query = useDrasiQuery(queryId, {
    getKey: row => String(row.device), transform: row => row,
  }), definition = useDrasiQueryDefinition(queryId);
  const status = useDrasiConnectionStatus();
  observe?.(context);
  return <div>
    <output data-testid="data">{JSON.stringify(query.data)}</output>
    <output data-testid="definition">{definition.config?.id ?? ''}</output>
    <output data-testid="status">{status.connected ? 'connected' : 'disconnected'}</output>
    <output data-testid="error">{query.error?.code ?? context.error?.code ?? ''}</output>
    <button onClick={context.retry}>retry</button>
  </div>;
}

describe('real provider configuration and ownership', () => {
  it('surfaces invalid data configuration without throwing during render or running a transport', async () => {
    const { factory, fetcher } = fixture(), observed = vi.fn<(context: DrasiContextValue) => void>();
    const view = render(<DrasiProvider {...refs} serverUrl="not a URL"
      fetch={fetcher} eventSourceFactory={factory.create}><Probe observe={observed} /></DrasiProvider>);
    await waitFor(() => expect(screen.getByTestId('error').textContent).toBe('INVALID_CONFIGURATION'));
    expect(fetcher).not.toHaveBeenCalled();
    expect(factory.instances).toHaveLength(0);
    const retry = observed.mock.calls[observed.mock.calls.length - 1][0].retry;
    view.unmount();
    act(() => retry());
    expect(fetcher).not.toHaveBeenCalled();
  });

  it('gives an actionable missing-provider error instead of implicit global state', () => {
    vi.spyOn(console, 'error').mockImplementation(() => {});
    expect(() => renderHook(() => useDrasiClient())).toThrow('useDrasiClient must be used within a <DrasiProvider>');
  });

  it('preserves a single client/stream for equivalent inline objects, headers and query sets', async () => {
    const { factory, fetcher } = fixture(), observed = vi.fn<(context: DrasiContextValue) => void>();
    const tree = (reverse: boolean) => <DrasiProvider
      {...refs} queryIds={reverse ? ['other', 'stocks'] : ['stocks', 'other']}
      reaction={{ ...refs.reaction }} reconnect={{ ...refs.reconnect }}
      headers={reverse ? [['x-scope', 'test'], ['accept', 'application/json']] : { Accept: 'application/json', 'X-Scope': 'test' }}
      fetch={fetcher} eventSourceFactory={factory.create}
    ><Probe observe={observed} /></DrasiProvider>;
    const view = render(tree(false));
    await waitFor(() => expect(factory.instances).toHaveLength(1));
    act(() => factory.instances[0].open());
    await waitFor(() => expect(screen.getByTestId('data').textContent).toContain(refs.instanceId));
    await waitFor(() => expect(screen.getByTestId('definition').textContent).toBe('stocks'));
    const client = observed.mock.calls[observed.mock.calls.length - 1][0].client, reads = fetcher.mock.calls.length;
    for (let index = 0; index < 6; index++) view.rerender(tree(index % 2 === 0));
    expect(observed.mock.calls[observed.mock.calls.length - 1][0].client).toBe(client);
    expect(factory.instances).toHaveLength(1);
    expect(factory.instances[0].closed).toBe(false);
    expect(fetcher).toHaveBeenCalledTimes(reads);
    view.unmount();
    expect(factory.instances[0].closed).toBe(true);
  });

  it.each<[string, Partial<DrasiClientOptions>]>([
    ['server', { serverUrl: 'https://other.invalid' }],
    ['instance', { instanceId: 'other / tenant' }],
    ['reaction', { reaction: { ...refs.reaction, id: 'other-stream' } }],
    ['endpoint', { reaction: { ...refs.reaction, endpoint: 'https://events.invalid/other' } }],
    ['query set', { queryIds: ['stocks', 'other'] }],
    ['credentials', { credentials: 'include' }],
    ['headers', { headers: { Authorization: 'fixture-only' } }],
    ['timeout', { requestTimeoutMs: 20000 }],
    ['retry policy', { reconnect: { maxReconnectAttempts: 1 } }],
    ['adapter identity', { resultAdapter: createLegacyResultAdapter() }],
    ['reconciliation policy', { reconciliation: { maxPendingChanges: 50 } }],
    ['auth identity', { headers: () => ({}) }],
  ])(
    'disposes old work for a material %s change and ignores its retry/events', async (_label, change) => {
      const { factory, fetcher } = fixture(), observed = vi.fn<(context: DrasiContextValue) => void>();
      const tree = (options: Partial<DrasiClientOptions>) => <DrasiProvider
        {...refs} fetch={fetcher} eventSourceFactory={factory.create} {...options}
      ><Probe observe={observed} /></DrasiProvider>;
      const view = render(tree({}));
      await waitFor(() => expect(factory.instances).toHaveLength(1));
      act(() => factory.instances[0].open());
      await waitFor(() => expect(screen.getByTestId('data').textContent).toContain(refs.instanceId));
      const first = observed.mock.calls[observed.mock.calls.length - 1][0];
      view.rerender(tree(change));
      expect(screen.getByTestId('data').textContent).toBe('null');
      expect(screen.getByTestId('definition').textContent).toBe('');
      expect(factory.instances[0].closed).toBe(true);
      await waitFor(() => expect(factory.instances).toHaveLength(2));
      act(() => factory.instances[1].open());
      await waitFor(() => expect(screen.getByTestId('data').textContent).toContain(change.instanceId ?? refs.instanceId));
      const reads = fetcher.mock.calls.length;
      act(() => {
        first.retry();
        factory.instances[0].message({ queryId: 'stocks', data: [{ device: 'stale', value: 99 }] });
        factory.instances[0].fail();
        factory.instances[0].open();
      });
      expect(screen.getByTestId('data').textContent).not.toContain('stale');
      expect(factory.instances).toHaveLength(2);
      expect(fetcher).toHaveBeenCalledTimes(reads);
      expect(observed.mock.calls[observed.mock.calls.length - 1][0].client).not.toBe(first.client);
      view.unmount();
      expect(factory.instances.every(source => source.closed)).toBe(true);
    },
  );

  it('treats fetch and factory identities as material even when their behavior is equivalent', async () => {
    const { factory, fetcher } = fixture();
    const otherFetch: typeof fetch = (input, init) => fetcher(input, init);
    const otherFactory = (url: string) => factory.create(url);
    const tree = (fetch: typeof globalThis.fetch, eventSourceFactory = factory.create) =>
      <DrasiProvider {...refs} fetch={fetch} eventSourceFactory={eventSourceFactory}><Probe /></DrasiProvider>;
    const view = render(tree(fetcher));
    await waitFor(() => expect(factory.instances).toHaveLength(1));
    act(() => factory.instances[0].open());
    view.rerender(tree(otherFetch));
    await waitFor(() => expect(factory.instances).toHaveLength(2));
    expect(factory.instances[0].closed).toBe(true);
    act(() => factory.instances[1].open());
    view.rerender(tree(otherFetch, otherFactory));
    await waitFor(() => expect(factory.instances).toHaveLength(3));
    expect(factory.instances[1].closed).toBe(true);
    view.unmount();
  });

  it('cancels an old instance snapshot and suppresses late fulfillment after reconfiguration', async () => {
    const { factory, fetcher } = fixture(), snapshot = deferred<Response>();
    let oldSignal: AbortSignal | null | undefined;
    const read = fetcher.getMockImplementation()!;
    fetcher.mockImplementation((input, init) => {
      if (String(input).includes(encodeURIComponent(refs.instanceId)) && String(input).endsWith('/results')) {
        oldSignal = init?.signal;
        return snapshot.promise;
      }
      return read(input, init);
    });
    const tree = (instanceId: string) => <DrasiProvider
      {...refs} instanceId={instanceId} fetch={fetcher} eventSourceFactory={factory.create}
    ><Probe /></DrasiProvider>;
    const view = render(tree(refs.instanceId));
    await waitFor(() => expect(factory.instances).toHaveLength(1));
    act(() => factory.instances[0].open());
    await waitFor(() => expect(oldSignal).toBeDefined());
    view.rerender(tree('new-instance'));
    expect(oldSignal?.aborted).toBe(true);
    await waitFor(() => expect(factory.instances).toHaveLength(2));
    act(() => factory.instances[1].open());
    await waitFor(() => expect(screen.getByTestId('data').textContent).toContain('new-instance'));
    await act(async () => { snapshot.resolve(json([{ device: 'late-old-instance' }])); });
    expect(screen.getByTestId('data').textContent).not.toContain('late-old-instance');
    view.unmount();
  });

  it('aborts StrictMode/unmount authentication work and never opens a late stream', async () => {
    const { factory, fetcher } = fixture(), pending = deferred<HeadersInit>();
    const signals: AbortSignal[] = [];
    const headers = ({ signal }: { signal: AbortSignal }) => { signals.push(signal); return pending.promise; };
    const view = render(<React.StrictMode>
      <DrasiProvider {...refs} fetch={fetcher} headers={headers} eventSourceFactory={factory.create}><Probe /></DrasiProvider>
    </React.StrictMode>);
    expect(signals.length).toBeGreaterThan(0);
    view.unmount();
    expect(signals.every(signal => signal.aborted)).toBe(true);
    await act(async () => { pending.resolve({}); });
    expect(factory.instances).toHaveLength(0);
    expect(fetcher).not.toHaveBeenCalled();
  });

  it('scopes query/definition changes inside a shared provider without bouncing its stream', async () => {
    const { factory, fetcher } = fixture();
    const tree = (queryId: string) => <DrasiProvider {...refs} queryIds={['stocks', 'other']}
      fetch={fetcher} eventSourceFactory={factory.create}><Probe queryId={queryId} /></DrasiProvider>;
    const view = render(tree('stocks'));
    await waitFor(() => expect(factory.instances).toHaveLength(1));
    act(() => factory.instances[0].open());
    await waitFor(() => expect(screen.getByTestId('data').textContent).toContain('"query":"stocks"'));
    view.rerender(tree('other'));
    expect(screen.getByTestId('data').textContent).toBe('null');
    await waitFor(() => expect(screen.getByTestId('definition').textContent).toBe('other'));
    await waitFor(() => expect(screen.getByTestId('data').textContent).toContain('"query":"other"'));
    act(() => factory.instances[0].message({ queryId: 'stocks', data: [{ device: 'old-query' }] }));
    expect(screen.getByTestId('data').textContent).not.toContain('old-query');
    expect(factory.instances).toHaveLength(1);
    view.unmount();
  });

  it('surfaces a definition-only authorization failure without converting it to a connection failure', async () => {
    const { factory, fetcher } = fixture(), read = fetcher.getMockImplementation()!;
    fetcher.mockImplementation((input, init) => String(input).endsWith('/inspection?view=full')
      ? Promise.resolve(failure(403)) : read(input, init));
    function Inspection() {
      const result = useDrasiQueryDefinition('inspection'), status = useDrasiConnectionStatus();
      return <output>{result.error?.code}:{result.error?.resourceId}:{String(status.connected)}</output>;
    }
    const view = render(<DrasiProvider {...refs} fetch={fetcher} eventSourceFactory={factory.create}>
      <Inspection />
    </DrasiProvider>);
    await waitFor(() => expect(factory.instances).toHaveLength(1));
    act(() => factory.instances[0].open());
    await waitFor(() => expect(screen.getByRole('status').textContent).toBe('FORBIDDEN:inspection:true'));
    expect(factory.instances).toHaveLength(1);
    view.unmount();
  });

  it('reports validating-transform failures safely and applies option changes to later batches without a resubscribe', async () => {
    const { factory, fetcher } = fixture();
    function Transform({ reject = false, scale = 1 }: { reject?: boolean; scale?: number }) {
      const query = useDrasiQuery<{ device: string; value: number }>('stocks', {
        getKey: row => String(row.device),
        transform: row => {
          if (reject || typeof row.device !== 'string' || typeof row.value !== 'number') {
            throw new Error('private user payload details');
          }
          return { device: row.device, value: row.value * scale };
        },
        postProcess: rows => rows.slice().sort((a, b) => a.value - b.value),
      });
      return <output>{query.error?.code ?? JSON.stringify(query.data)}</output>;
    }
    const tree = (reject: boolean, scale: number) => <DrasiProvider {...refs}
      fetch={fetcher} eventSourceFactory={factory.create}><Transform reject={reject} scale={scale} /></DrasiProvider>;
    const view = render(tree(true, 1));
    await waitFor(() => expect(factory.instances).toHaveLength(1));
    act(() => factory.instances[0].open());
    await waitFor(() => expect(screen.getByRole('status').textContent).toBe('RESULT_PROCESSING_FAILED'));
    const reads = fetcher.mock.calls.length;
    view.rerender(tree(false, 10));
    expect(screen.getByRole('status').textContent).toBe(`[{"device":"${refs.instanceId}","value":10}]`);
    act(() => factory.instances[0].message({
      queryId: 'stocks', timestamp: 1, results: [{ type: 'ADD', data: { device: 'valid', value: 2 } }],
    }));
    expect(screen.getByRole('status').textContent)
      .toBe(`[{"device":"${refs.instanceId}","value":10},{"device":"valid","value":20}]`);
    expect(fetcher).toHaveBeenCalledTimes(reads);
    expect(factory.instances).toHaveLength(1);
    view.unmount();
  });

  it('retries the shared provider explicitly, with no resource writes or second concurrent stream', async () => {
    const { factory, fetcher } = fixture();
    const view = render(<DrasiProvider {...refs} fetch={fetcher} eventSourceFactory={factory.create}><Probe /></DrasiProvider>);
    await waitFor(() => expect(factory.instances).toHaveLength(1));
    act(() => factory.instances[0].open());
    await waitFor(() => expect(screen.getByTestId('status').textContent).toBe('connected'));
    fireEvent.click(screen.getByText('retry'));
    expect(factory.instances[0].closed).toBe(true);
    await waitFor(() => expect(factory.instances).toHaveLength(2));
    act(() => factory.instances[1].open());
    await waitFor(() => expect(screen.getByTestId('status').textContent).toBe('connected'));
    expect(factory.instances.filter(source => !source.closed)).toHaveLength(1);
    expect(fetcher.mock.calls.every(([, init]) => init?.method === 'GET')).toBe(true);
    view.unmount();
  });

  it('keeps a transient snapshot retry query-local while another query and stream stay live', async () => {
    vi.useFakeTimers();
    const { factory, fetcher } = fixture(), read = fetcher.getMockImplementation()!;
    let failed = false;
    fetcher.mockImplementation((input, init) => {
      if (String(input).endsWith('/stocks/results') && !failed) { failed = true; return Promise.resolve(failure(503)); }
      return read(input, init);
    });
    const client = new DrasiClient({
      ...refs, queryIds: ['stocks', 'other'], fetch: fetcher, eventSourceFactory: factory.create,
      reconnect: { maxReconnectAttempts: 1, initialReconnectDelayMs: 10 },
    });
    const connected = client.initialize();
    await vi.waitFor(() => expect(factory.instances).toHaveLength(1));
    factory.instances[0].open();
    await connected;
    const first = vi.fn(), second = vi.fn(), errors = vi.fn();
    client.subscribe('stocks', first, errors);
    client.subscribe('other', second);
    await vi.waitFor(() => expect(second).toHaveBeenCalledOnce());
    await vi.runAllTimersAsync();
    expect(errors).toHaveBeenCalledOnce();
    expect(first).toHaveBeenCalledOnce();
    expect(second).toHaveBeenCalledOnce();
    expect(factory.instances).toHaveLength(1);
    expect(client.getConnectionStatus().connected).toBe(true);
    await client.disconnect();
  });

  it('leaves an app-owned client alive when a controlled binding mounts, rerenders and unmounts', async () => {
    const { factory, fetcher } = fixture();
    const client = new DrasiClient({ ...refs, fetch: fetcher, eventSourceFactory: factory.create });
    const value = { client, initialized: false, error: null, retry: vi.fn() };
    const view = render(<DrasiClientProvider value={value}><Probe /></DrasiClientProvider>);
    expect(fetcher).not.toHaveBeenCalled();
    expect(factory.instances).toHaveLength(0);
    const connected = client.initialize();
    await waitFor(() => expect(factory.instances).toHaveLength(1));
    act(() => factory.instances[0].open());
    await connected;
    view.rerender(<DrasiClientProvider value={{ ...value, initialized: true }}><Probe /></DrasiClientProvider>);
    await waitFor(() => expect(screen.getByTestId('data').textContent).toContain(refs.instanceId));
    view.unmount();
    expect(client.isInitialized()).toBe(true);
    expect(factory.instances[0].closed).toBe(false);
    await client.disconnect();
    expect(factory.instances[0].closed).toBe(true);
  });
});
