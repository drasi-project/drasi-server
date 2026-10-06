// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { StrictMode } from 'react';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { DrasiClient, DrasiError, useDrasiClient, useDrasiQuery, type EventSourceLike } from '@drasi/react';
import { canPrepareTrading, ensureTradingResources, resolveTradingInstance } from '../../src/drasi/ensureTradingResources';
import { TradingProvider } from '../../src/drasi/TradingProvider';
import { tradingQueryOptions } from '../../src/drasi/queryOptions';
import { DRASI_SERVER_URL, TRADING_QUERIES, TRADING_QUERY_IDS, TRADING_REACTION, TRADING_STREAM } from '../../src/drasi/config';
import { SyntheticTrading } from '../fixtures/synthetic/trading';

const base = '/api/v1/instances/trading-server';
const reconnect = { maxReconnectAttempts: 0 };
const json = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status });
const missing = () => new DrasiError('QUERY_NOT_FOUND', {
  instanceId: 'trading-server', resourceKind: 'query', resourceId: TRADING_QUERY_IDS[0],
});

function setup(present = false, backend = new SyntheticTrading()) {
  if (present) {
    for (const query of TRADING_QUERIES) backend.queries.set(query.id, { ...query, autoStart: true });
    backend.reaction = { ...TRADING_REACTION, queries: TRADING_QUERY_IDS, autoStart: true };
  }
  const fetcher = vi.fn<typeof fetch>(async (input, init) => {
    init?.signal?.throwIfAborted();
    const response = backend.handle({
      method: init?.method ?? 'GET', path: new URL(String(input)).pathname,
      body: typeof init?.body === 'string' ? JSON.parse(init.body) : undefined,
    });
    return json(response.body, response.status);
  });
  const client = new DrasiClient({
    serverUrl: DRASI_SERVER_URL, instanceId: backend.instanceId, queryIds: TRADING_QUERY_IDS,
    reaction: TRADING_STREAM, fetch: fetcher, reconnect,
  });
  const options = { client, serverUrl: DRASI_SERVER_URL, fetch: fetcher };
  const run = (signal = new AbortController().signal, error = missing()) =>
    ensureTradingResources(options, error, signal);
  const writes = () => backend.requests.filter(request => request.method !== 'GET');
  return { backend, fetcher, client, run, writes };
}

afterEach(() => vi.useRealTimers());

describe('app-owned setup using the built package', () => {
  it('creates the exact known bundle in order, omitting app-only description metadata', async () => {
    const { run, backend, writes } = setup();
    await run();
    expect(writes().map(request => request.body?.id)).toEqual([...TRADING_QUERY_IDS, TRADING_REACTION.id]);
    expect(writes().every(request => request.path.startsWith(base))).toBe(true);
    expect(writes().slice(0, 11).map(request => request.body)).toEqual(
      TRADING_QUERIES.map(query => ({ ...query, autoStart: true })),
    );
    expect(writes().every(request => !('description' in request.body!))).toBe(true);
    expect([...backend.queries.keys()]).toEqual(TRADING_QUERY_IDS);
  });

  it('makes no redundant mutations when another consumer already prepared everything', async () => {
    const { run, writes, fetcher } = setup(true);
    await run();
    expect(writes()).toEqual([]);
    expect(fetcher).toHaveBeenCalledTimes(12);
  });

  it('repairs partial setup, starting each stopped resource only once', async () => {
    const { run, backend, writes } = setup(true);
    backend.queries.delete('portfolio-query');
    backend.queryStatuses.set('watchlist-query', 'Stopped');
    backend.reactionStatus = 'Stopped';
    await run();
    expect(writes().map(request => request.path)).toEqual([
      `${base}/queries/watchlist-query/start`, `${base}/queries`, `${base}/reactions/sse-stream/start`,
    ]);
  });

  it('waits for already-starting resources without duplicate start/create requests', async () => {
    const { run, backend, fetcher, writes } = setup(true);
    backend.queryStatuses.set('watchlist-query', 'Starting');
    backend.reactionStatus = 'Reconfiguring';
    const original = fetcher.getMockImplementation()!;
    let queryReads = 0, reactionReads = 0;
    fetcher.mockImplementation((input, init) => {
      const url = String(input);
      if (url.includes('/queries/watchlist-query?') && ++queryReads === 3) backend.queryStatuses.set('watchlist-query', 'Running');
      if (url.includes('/reactions/sse-stream?') && ++reactionReads === 3) backend.reactionStatus = 'Running';
      return original(input, init);
    });
    await run(new AbortController().signal, new DrasiError('RESOURCE_STARTING', {
      instanceId: 'trading-server', resourceKind: 'query', resourceId: 'watchlist-query',
    }));
    expect(writes()).toEqual([]);
    expect(queryReads).toBe(3);
  });

  it('shares one bounded setup under concurrent consumers', async () => {
    const { run, writes } = setup();
    await Promise.all([run(), run(), run()]);
    expect(writes()).toHaveLength(12);
  });

  it('converges after concurrent-tab 409 responses by reading the winning definitions', async () => {
    const first = setup();
    const second = setup(false, first.backend);
    const statuses: number[] = [];
    const original = second.fetcher.getMockImplementation()!;
    second.fetcher.mockImplementation(async (input, init) => {
      const response = await original(input, init);
      if (init?.method === 'POST') statuses.push(response.status);
      return response;
    });
    await Promise.all([first.run(), second.run()]);
    expect(statuses).toContain(409);
    expect(first.backend.queries.size).toBe(11);
    expect(first.writes().length).toBeLessThanOrEqual(24);
    expect(first.writes().some(request => request.path.endsWith('/start'))).toBe(false);
  });

  it('serializes tabs with Web Locks when supported', async () => {
    const requestLock = vi.fn(async (_name: string, _options: unknown, run: () => Promise<void>) => run());
    const descriptor = Object.getOwnPropertyDescriptor(navigator, 'locks');
    Object.defineProperty(navigator, 'locks', { configurable: true, value: { request: requestLock } });
    try {
      const { run } = setup();
      await run();
      expect(requestLock).toHaveBeenCalledOnce();
      expect(requestLock.mock.calls[0][0]).toContain('trading-server');
    } finally {
      if (descriptor) Object.defineProperty(navigator, 'locks', descriptor);
      else Reflect.deleteProperty(navigator, 'locks');
    }
  });

  it('rejects a conflicting winner after 409 without overwrite or retry storms', async () => {
    const { run, backend, fetcher, writes } = setup();
    const original = fetcher.getMockImplementation()!;
    fetcher.mockImplementation(async (input, init) => {
      if (init?.method === 'POST') {
        const query = TRADING_QUERIES[0];
        backend.queries.set(query.id, { ...query, queryLanguage: 'GQL' });
        return json({ code: 'DUPLICATE_RESOURCE' }, 409);
      }
      return original(input, init);
    });
    await expect(run()).rejects.toMatchObject({ code: 'INCOMPATIBLE_RESOURCE' });
    expect(writes()).toEqual([]);
    expect(fetcher.mock.calls.filter(([, init]) => init?.method === 'POST')).toHaveLength(1);
  });

  it.each(['text', 'language', 'source-order', 'join', 'reaction-port', 'unusable'] as const)(
    'preflights %s conflicts before writing any partial resources', async kind => {
      const { run, backend, writes } = setup(true);
      backend.queries.delete('watchlist-query');
      const query = backend.queries.get('portfolio-query')!;
      if (kind === 'text') query.query = 'MATCH (x) RETURN x';
      if (kind === 'language') query.queryLanguage = 'GQL';
      if (kind === 'source-order') query.sources = [...TRADING_QUERIES[1].sources].reverse();
      if (kind === 'join') query.joins = [];
      if (kind === 'reaction-port') backend.reaction!.port = 9999;
      if (kind === 'unusable') backend.queryStatuses.set('portfolio-query', 'Error');
      await expect(run()).rejects.toMatchObject({
        code: kind === 'unusable' ? 'RESOURCE_UNAVAILABLE' : 'INCOMPATIBLE_RESOURCE',
      });
      expect(writes()).toEqual([]);
    },
  );

  it.each([401, 403, 503, 'network', 'malformed'] as const)(
    'does not provision around %s errors in the preflight', async mode => {
      const { run, fetcher, writes } = setup();
      if (typeof mode === 'number') fetcher.mockResolvedValue(json({ code: 'PRIVATE_DETAIL' }, mode));
      if (mode === 'network') fetcher.mockRejectedValue(new TypeError('private network information'));
      if (mode === 'malformed') fetcher.mockResolvedValue(json({ success: true, data: {} }));
      await expect(run()).rejects.toBeInstanceOf(DrasiError);
      expect(writes()).toEqual([]);
    },
  );

  it('rejects unrelated/unauthorized/unknown-instance errors without even a setup read', async () => {
    const { run, fetcher } = setup();
    for (const error of [
      new Error('QUERY_NOT_FOUND'), new DrasiError('STREAM_UNAVAILABLE'),
      new DrasiError('INSTANCE_NOT_FOUND', { instanceId: 'trading-server', resourceKind: 'instance' }),
      new DrasiError('QUERY_NOT_FOUND', { instanceId: 'other', resourceKind: 'query', resourceId: 'watchlist-query' }),
      new DrasiError('QUERY_NOT_FOUND', { instanceId: 'trading-server', resourceKind: 'query', resourceId: 'not-owned' }),
    ]) expect(canPrepareTrading(error, 'trading-server')).toBe(false);
    const error = new DrasiError('FORBIDDEN');
    await expect(run(new AbortController().signal, error)).rejects.toBe(error);
    expect(fetcher).not.toHaveBeenCalled();
  });

  it('does not select or mutate an identically named query in the wrong instance', async () => {
    const { client, backend, run, writes } = setup(true);
    backend.instanceId = 'another-instance';
    const error = await client.initialize().catch(error => error);
    expect(error).toMatchObject({ code: 'INSTANCE_NOT_FOUND', instanceId: 'trading-server' });
    await expect(run(new AbortController().signal, error)).rejects.toBe(error);
    expect(writes()).toEqual([]);
  });

  it('cancels one waiter without aborting shared work needed by another', async () => {
    const { run, fetcher, writes } = setup();
    let resume!: () => void;
    let taskSignal: AbortSignal | null | undefined;
    const original = fetcher.getMockImplementation()!;
    fetcher.mockImplementationOnce((input, init) => new Promise(resolve => {
      taskSignal = init?.signal;
      resume = () => { void original(input, init).then(resolve); };
    }));
    const controller = new AbortController();
    const first = run(controller.signal), second = run();
    controller.abort();
    await expect(first).rejects.toMatchObject({ name: 'AbortError' });
    expect(taskSignal?.aborted).toBe(false);
    resume();
    await second;
    expect(writes()).toHaveLength(12);
  });

  it('aborts underlying work when the last consumer cancels', async () => {
    const { run, fetcher, writes } = setup();
    let taskSignal: AbortSignal | null | undefined;
    fetcher.mockImplementationOnce((_input, init) => new Promise((_resolve, reject) => {
      taskSignal = init?.signal;
      init?.signal?.addEventListener('abort', () => reject(init.signal?.reason), { once: true });
    }));
    const controller = new AbortController();
    const pending = run(controller.signal);
    controller.abort();
    await expect(pending).rejects.toMatchObject({ name: 'AbortError' });
    expect(taskSignal?.aborted).toBe(true);
    expect(writes()).toEqual([]);
    const aborted = new AbortController();
    aborted.abort();
    expect(() => run(aborted.signal)).toThrow();
  });

  it('retains partial success and resumes without recreating a committed query after cancellation', async () => {
    const { run, fetcher, backend, writes } = setup();
    const controller = new AbortController(), original = fetcher.getMockImplementation()!;
    fetcher.mockImplementation(async (input, init) => {
      const response = await original(input, init);
      if (init?.method === 'POST' && backend.queries.size === 1) controller.abort();
      return response;
    });
    await expect(run(controller.signal)).rejects.toMatchObject({ name: 'AbortError' });
    expect(writes()).toHaveLength(1);
    fetcher.mockImplementation(original);
    await run();
    expect(writes()).toHaveLength(12);
  });

  it('bounds readiness polling to 60 seconds with no duplicate starts', async () => {
    vi.useFakeTimers();
    const { run, backend, writes } = setup(true);
    backend.queryStatuses.set('watchlist-query', 'Starting');
    const pending = run();
    const rejected = expect(pending).rejects.toMatchObject({ code: 'SERVER_UNAVAILABLE' });
    await vi.advanceTimersByTimeAsync(60000);
    await rejected;
    expect(writes()).toEqual([]);
    expect(vi.getTimerCount()).toBe(0);
  });

  it.each([401, 403, 500, 'network', 'malformed', 'false-success'] as const)(
    'surfaces %s during creation without repeating writes', async mode => {
      const { run, fetcher } = setup();
      const original = fetcher.getMockImplementation()!;
      fetcher.mockImplementation((input, init) => {
        if (init?.method !== 'POST') return original(input, init);
        if (mode === 'network') return Promise.reject(new TypeError('offline'));
        if (mode === 'malformed') return Promise.resolve(new Response('bad JSON'));
        if (mode === 'false-success') return Promise.resolve(json({ success: false }));
        return Promise.resolve(json({ code: 'FAILED' }, mode));
      });
      await expect(run()).rejects.toBeInstanceOf(DrasiError);
      expect(fetcher.mock.calls.filter(([, init]) => init?.method === 'POST')).toHaveLength(1);
    },
  );

  it('accepts a racing failed start only after a compatible active read', async () => {
    const { run, backend, fetcher } = setup(true);
    backend.queryStatuses.set('watchlist-query', 'Stopped');
    const original = fetcher.getMockImplementation()!;
    fetcher.mockImplementation((input, init) => {
      if (String(input).endsWith('/start')) {
        backend.queryStatuses.set('watchlist-query', 'Running');
        return Promise.resolve(json({ code: 'QUERY_START_FAILED' }, 500));
      }
      return original(input, init);
    });
    await run();
    expect(fetcher.mock.calls.filter(([url]) => String(url).endsWith('/start'))).toHaveLength(1);
  });

  describe.each([
    ['query', 'create', 'watchlist-query'],
    ['query', 'start', 'watchlist-query'],
    ['reaction', 'create', 'sse-stream'],
    ['reaction', 'start', 'sse-stream'],
  ] as const)('%s %s error identity', (kind, operation, id) => {
    it.each([
      [401, 'UNAUTHENTICATED'],
      [403, 'FORBIDDEN'],
      [503, 'SERVER_UNAVAILABLE'],
      ['malformed', 'INVALID_PAYLOAD'],
      ['body-read', 'SERVER_UNAVAILABLE'],
      ['timeout', 'SERVER_UNAVAILABLE'],
      ['network', 'SERVER_UNAVAILABLE'],
      ['transport-abort', 'SERVER_UNAVAILABLE'],
    ] as const)('preserves the known resource through %s failure', async (mode, code) => {
      vi.useFakeTimers();
      const { run, backend, fetcher } = setup(true);
      if (kind === 'query') {
        if (operation === 'create') backend.queries.delete(id);
        else backend.queryStatuses.set(id, 'Stopped');
      } else if (operation === 'create') backend.reaction = null;
      else backend.reactionStatus = 'Stopped';
      const original = fetcher.getMockImplementation()!;
      fetcher.mockImplementation(async (input, init) => {
        if (init?.method !== 'POST') return original(input, init);
        if (typeof mode === 'number') return json({ code: 'PRIVATE_SERVER_DETAIL' }, mode);
        if (mode === 'network') throw new TypeError('private network detail');
        if (mode === 'transport-abort') throw new DOMException('private transport cancellation', 'AbortError');
        if (mode === 'malformed') return new Response('private invalid JSON', { status: 201 });
        return new Response(new ReadableStream<Uint8Array>({
          start(controller) {
            if (mode === 'body-read') controller.error(new TypeError('private body failure'));
            else init?.signal?.addEventListener('abort', () => controller.error(init.signal?.reason), { once: true });
          },
        }), { status: 201 });
      });
      const pending = run().catch((error: unknown) => error);
      await vi.advanceTimersByTimeAsync(10000);
      const error = await pending;
      expect(error).toBeInstanceOf(DrasiError);
      expect(error).toMatchObject({
        code, instanceId: 'trading-server', resourceKind: kind, resourceId: id,
        status: typeof mode === 'number' ? mode : ['network', 'transport-abort'].includes(mode) ? undefined : 201,
        retryable: code === 'SERVER_UNAVAILABLE',
      });
      expect((error as DrasiError).message).not.toContain('private');
      const writes = fetcher.mock.calls.filter(([, init]) => init?.method === 'POST');
      expect(writes).toHaveLength(1);
      expect(new URL(String(writes[0][0])).pathname).toBe(
        `${base}/${kind === 'query' ? 'queries' : 'reactions'}${operation === 'start' ? `/${id}/start` : ''}`,
      );
      expect(vi.getTimerCount()).toBe(0);
    });

    it('preserves explicit caller cancellation without more mutations', async () => {
      vi.useFakeTimers();
      const { run, backend, fetcher } = setup(true);
      if (kind === 'query') {
        if (operation === 'create') backend.queries.delete(id);
        else backend.queryStatuses.set(id, 'Stopped');
      } else if (operation === 'create') backend.reaction = null;
      else backend.reactionStatus = 'Stopped';
      const original = fetcher.getMockImplementation()!;
      let pendingSignal: AbortSignal | null | undefined;
      fetcher.mockImplementation((input, init) => {
        if (init?.method !== 'POST') return original(input, init);
        pendingSignal = init.signal;
        return new Promise((_resolve, reject) => {
          init.signal?.addEventListener('abort', () => reject(init.signal?.reason), { once: true });
        });
      });
      const controller = new AbortController();
      const result = run(controller.signal).catch((error: unknown) => error);
      await vi.advanceTimersByTimeAsync(0);
      expect(pendingSignal).toBeDefined();
      controller.abort();
      expect(await result).toBe(controller.signal.reason);
      expect(pendingSignal?.aborted).toBe(true);
      await vi.advanceTimersByTimeAsync(60000);
      expect(fetcher.mock.calls.filter(([, init]) => init?.method === 'POST')).toHaveLength(1);
      expect(vi.getTimerCount()).toBe(0);
    });
  });

  it.each(['query', 'reaction'] as const)(
    'identifies the active %s when the shared setup deadline cancels a mutation body', async kind => {
      vi.useFakeTimers();
      const descriptor = Object.getOwnPropertyDescriptor(navigator, 'locks');
      Object.defineProperty(navigator, 'locks', { configurable: true, value: {
        request: async (_name: string, _options: unknown, start: () => Promise<void>) => {
          await new Promise(resolve => setTimeout(resolve, 55000));
          await start();
        },
      } });
      try {
        const { run, backend, fetcher } = setup(true);
        const id = kind === 'query' ? 'watchlist-query' : 'sse-stream';
        if (kind === 'query') backend.queries.delete(id);
        else backend.reaction = null;
        const original = fetcher.getMockImplementation()!;
        fetcher.mockImplementation((input, init) => {
          if (init?.method !== 'POST') return original(input, init);
          return Promise.resolve(new Response(new ReadableStream<Uint8Array>({
            start(controller) {
              init.signal?.addEventListener('abort', () => controller.error(init.signal?.reason), { once: true });
            },
          }), { status: 201 }));
        });
        const pending = run().catch((error: unknown) => error);
        await vi.advanceTimersByTimeAsync(60000);
        expect(await pending).toMatchObject({
          code: 'SERVER_UNAVAILABLE', instanceId: 'trading-server',
          resourceKind: kind, resourceId: id, status: 201,
        });
        expect(fetcher.mock.calls.filter(([, init]) => init?.method === 'POST')).toHaveLength(1);
        expect(vi.getTimerCount()).toBe(0);
      } finally {
        if (descriptor) Object.defineProperty(navigator, 'locks', descriptor);
        else Reflect.deleteProperty(navigator, 'locks');
      }
    },
  );

  it('does not accept a malformed successful start response just because the resource became active', async () => {
    const { run, backend, fetcher } = setup(true);
    backend.queryStatuses.set('watchlist-query', 'Stopped');
    const original = fetcher.getMockImplementation()!;
    fetcher.mockImplementation((input, init) => {
      if (String(input).endsWith('/start')) {
        backend.queryStatuses.set('watchlist-query', 'Running');
        return Promise.resolve(new Response('private malformed body'));
      }
      return original(input, init);
    });
    await expect(run()).rejects.toMatchObject({
      code: 'INVALID_PAYLOAD', status: 200, resourceKind: 'query', resourceId: 'watchlist-query',
    });
  });
});

describe('Trading instance and provider lifecycle', () => {
  it('resolves only the one-instance demo unless explicitly configured', async () => {
    const signal = new AbortController().signal;
    const { fetcher } = setup();
    expect(await resolveTradingInstance(DRASI_SERVER_URL, fetcher, signal, 'selected')).toBe('selected');
    expect(fetcher).not.toHaveBeenCalled();
    expect(await resolveTradingInstance(DRASI_SERVER_URL, fetcher, signal)).toBe('trading-server');
    await expect(resolveTradingInstance(DRASI_SERVER_URL, fetcher, signal, '')).rejects.toMatchObject({ code: 'INVALID_CONFIGURATION' });
    for (const [data, code] of [
      [[], 'INSTANCE_NOT_FOUND'], [[{ id: 'a' }, { id: 'b' }], 'INVALID_CONFIGURATION'],
      [{ id: 'a' }, 'INVALID_PAYLOAD'], [[{ id: 7 }], 'INVALID_PAYLOAD'],
    ]) {
      fetcher.mockResolvedValue(json({ success: true, data }));
      await expect(resolveTradingInstance(DRASI_SERVER_URL, fetcher, signal)).rejects.toMatchObject({ code });
    }
  });

  function Probe() {
    const state = useDrasiClient(), query = useDrasiQuery('watchlist-query', tradingQueryOptions('watchlist-query'));
    return <div>
      <span>{state.initialized ? 'Ready' : 'Pending'}</span>
      <output>{query.error ? `${query.error instanceof DrasiError}:${query.error.code}` : ''}</output>
      <button onClick={state.retry}>Retry probe</button>
    </div>;
  }

  it.each(['query', 'reaction'] as const)(
    'hands a persistently Starting %s to bounded setup with default reconnect settings', async kind => {
      vi.useFakeTimers();
      const { backend, fetcher, writes } = setup(true);
      if (kind === 'query') backend.queryStatuses.set('watchlist-query', 'Starting');
      else backend.reactionStatus = 'Starting';
      const factory = vi.fn();
      const rendered = render(<TradingProvider fetch={fetcher} eventSourceFactory={factory}><Probe /></TradingProvider>);
      try {
        await act(async () => { await vi.advanceTimersByTimeAsync(1000); });
        expect(fetcher.mock.calls.filter(([url]) => String(url).includes('/reactions/')).length).toBeGreaterThan(0);
        expect(screen.queryByRole('alert')).toBeNull();
        await act(async () => { await vi.advanceTimersByTimeAsync(60000); });
        expect(screen.queryByRole('alert')?.textContent).toContain('unavailable');
        expect(screen.queryByRole('button', { name: 'Retry connection' })).not.toBeNull();
        expect(screen.getByText('true:SERVER_UNAVAILABLE')).not.toBeNull();
        expect(writes()).toEqual([]);
        expect(factory).not.toHaveBeenCalled();
        expect(vi.getTimerCount()).toBe(0);
      } finally {
        rendered.unmount();
      }
    },
  );

  it('cancels default initial handoff and setup waits on unmount', async () => {
    vi.useFakeTimers();
    const { backend, fetcher, writes } = setup(true);
    backend.queryStatuses.set('watchlist-query', 'Starting');
    const factory = vi.fn();
    const rendered = render(<TradingProvider fetch={fetcher} eventSourceFactory={factory}><Probe /></TradingProvider>);
    await act(async () => { await vi.advanceTimersByTimeAsync(500); });
    const readCount = fetcher.mock.calls.length;
    rendered.unmount();
    await act(async () => { await vi.advanceTimersByTimeAsync(240000); });
    expect(fetcher).toHaveBeenCalledTimes(readCount);
    expect(writes()).toEqual([]);
    expect(factory).not.toHaveBeenCalled();
    expect(vi.getTimerCount()).toBe(0);
  });

  it.each([true, false])('allows one pre-ready stream recovery (recovers=%s) without provisioning', async recovers => {
    vi.useFakeTimers();
    const { fetcher, writes } = setup(true);
    const sources: EventSourceLike[] = [];
    const factory = () => {
      const source: EventSourceLike = { onopen: null, onmessage: null, onerror: null, addEventListener: vi.fn(), close: vi.fn() };
      sources.push(source);
      return source;
    };
    const rendered = render(<TradingProvider fetch={fetcher} eventSourceFactory={factory}><Probe /></TradingProvider>);
    try {
      await act(async () => { await vi.advanceTimersByTimeAsync(0); });
      expect(sources).toHaveLength(1);
      await act(async () => {
        sources[0].onerror?.(new Event('error'));
        await vi.advanceTimersByTimeAsync(999);
      });
      expect(sources).toHaveLength(1);
      expect(screen.queryByRole('alert')).toBeNull();
      await act(async () => { await vi.advanceTimersByTimeAsync(1); });
      expect(sources).toHaveLength(2);
      await act(async () => {
        if (recovers) sources[1].onopen?.(new Event('open'));
        else sources[1].onerror?.(new Event('error'));
        await vi.advanceTimersByTimeAsync(0);
      });
      if (recovers) {
        expect(screen.getByText('Ready')).not.toBeNull();
        expect(screen.queryByRole('alert')).toBeNull();
      } else {
        expect(screen.getByText('true:STREAM_UNAVAILABLE')).not.toBeNull();
        expect(screen.getByRole('button', { name: 'Retry connection' })).not.toBeNull();
      }
      expect(writes()).toEqual([]);
      expect(sources).toHaveLength(2);
      expect(vi.getTimerCount()).toBe(0);
    } finally {
      rendered.unmount();
    }
  });

  it('keeps all ten default reconnect attempts after ready without provisioning', async () => {
    vi.useFakeTimers();
    const { fetcher, writes } = setup(true);
    const sources: EventSourceLike[] = [];
    const factory = () => {
      const source: EventSourceLike = { onopen: null, onmessage: null, onerror: null, addEventListener: vi.fn(), close: vi.fn() };
      sources.push(source);
      return source;
    };
    const rendered = render(<TradingProvider fetch={fetcher} eventSourceFactory={factory}><Probe /></TradingProvider>);
    try {
      await act(async () => { await vi.advanceTimersByTimeAsync(0); });
      expect(sources).toHaveLength(1);
      await act(async () => { sources[0].onopen?.(new Event('open')); });
      expect(screen.getByText('Ready')).not.toBeNull();
      for (let retry = 0; retry < 10; retry += 1) {
        await act(async () => {
          sources[retry].onerror?.(new Event('error'));
          await vi.advanceTimersByTimeAsync(Math.min(1000 * 2 ** retry, 30000));
        });
        expect(sources).toHaveLength(retry + 2);
        expect(screen.queryByRole('alert')).toBeNull();
      }
      await act(async () => {
        sources[10].onerror?.(new Event('error'));
        await vi.advanceTimersByTimeAsync(0);
      });
      expect(screen.getByText('true:STREAM_UNAVAILABLE')).not.toBeNull();
      expect(screen.getByRole('button', { name: 'Retry connection' })).not.toBeNull();
      expect(writes()).toEqual([]);
      expect(sources).toHaveLength(11);
      expect(vi.getTimerCount()).toBe(0);
    } finally {
      rendered.unmount();
    }
  });

  it('shares the single real package connection under StrictMode, then cleans it up', async () => {
    const { fetcher, writes } = setup();
    const sources: EventSourceLike[] = [];
    const factory = () => {
      const source: EventSourceLike = {
        onopen: null, onerror: null, onmessage: null,
        addEventListener: vi.fn(), close: vi.fn(),
      };
      sources.push(source);
      queueMicrotask(() => source.onopen?.(new Event('open')));
      return source;
    };
    const rendered = render(<StrictMode><TradingProvider fetch={fetcher} eventSourceFactory={factory} reconnect={reconnect}>
      <Probe />
    </TradingProvider></StrictMode>);
    await screen.findByText('Ready');
    expect(writes()).toHaveLength(12);
    expect(sources).toHaveLength(1);
    fetcher.mockImplementation(async () => json({ code: 'FORBIDDEN' }, 403));
    act(() => sources[0].onerror?.(new Event('error')));
    await screen.findByText('true:FORBIDDEN');
    expect(writes()).toHaveLength(12);
    rendered.unmount();
    expect(sources[0].close).toHaveBeenCalledOnce();
  });

  it('surfaces typed auth failures through hooks and keeps retry read-only', async () => {
    const { fetcher } = setup();
    fetcher.mockImplementation(async () => json({ code: 'UNAUTHORIZED' }, 401));
    render(<TradingProvider fetch={fetcher} reconnect={reconnect}><Probe /></TradingProvider>);
    await screen.findByText('true:UNAUTHENTICATED');
    expect(screen.getByRole('alert').textContent).toContain('Authentication');
    fireEvent.click(screen.getByText('Retry probe'));
    await waitFor(() => expect(fetcher).toHaveBeenCalledTimes(2));
    expect(fetcher.mock.calls.every(([, init]) => !init?.method || init.method === 'GET')).toBe(true);
  });
});
