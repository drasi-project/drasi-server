// Copyright 2025 The Drasi Authors.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

import { describe, expect, it, vi } from 'vitest';
import { DrasiClient } from '../src/client/DrasiClient';
import { fakeEventSourceFactory } from './FakeEventSource';

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });
}

function deferred<T>(): {
  promise: Promise<T>;
  resolve: (value: T) => void;
} {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((currentResolve) => {
    resolve = currentResolve;
  });
  return { promise, resolve };
}

describe('DrasiClient', () => {
  it('does not restart queries or reactions that are already starting', async () => {
    const factory = fakeEventSourceFactory();
    const fetcher = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.endsWith('/health')) return jsonResponse({});
      if (url.endsWith('/api/v1/instances')) return jsonResponse([]);
      if (url.includes('/api/v1/queries/stocks?view=full')) {
        return jsonResponse({ status: 'Starting', config: {} });
      }
      if (url.includes('/api/v1/reactions/stream?view=full')) {
        return jsonResponse({
          status: 'Starting',
          config: { properties: { port: 8281, ssePath: '/events' } },
        });
      }
      throw new Error(`Unexpected request: ${url}`);
    });
    const client = new DrasiClient({
      queries: [
        {
          id: 'stocks',
          query: 'MATCH (n) RETURN n',
          sources: [],
        },
      ],
      reaction: { id: 'stream', port: 8281 },
      fetch: fetcher as typeof fetch,
      eventSourceFactory: factory.create,
    });

    const initialized = client.initialize();
    await vi.waitFor(() => expect(factory.instances).toHaveLength(1));
    factory.instances[0].open();
    await initialized;

    expect(
      fetcher.mock.calls.some(([input]) => String(input).endsWith('/start')),
    ).toBe(false);
    await client.disconnect();
  });

  it('subscribes before snapshot fetch and replays queued deltas afterward', async () => {
    const snapshot = deferred<Response>();
    const factory = fakeEventSourceFactory();
    const fetcher = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.endsWith('/health')) return jsonResponse({});
      if (url.endsWith('/api/v1/instances')) return jsonResponse([]);
      if (url.includes('/api/v1/queries/stocks?view=full')) {
        return jsonResponse({ status: 'running', config: {} });
      }
      if (url.includes('/api/v1/reactions/stream?view=full')) {
        return jsonResponse({
          status: 'running',
          config: {
            properties: {
              host: 'localhost',
              port: 8281,
              ssePath: '/events',
            },
          },
        });
      }
      if (url.endsWith('/api/v1/queries/stocks/results')) {
        return snapshot.promise;
      }
      throw new Error(`Unexpected request: ${url}`);
    });
    const client = new DrasiClient({
      queries: [
        {
          id: 'stocks',
          query: 'MATCH (n) RETURN n',
          sources: [],
        },
      ],
      reaction: { id: 'stream', port: 8281 },
      fetch: fetcher as typeof fetch,
      eventSourceFactory: factory.create,
    });

    const initialized = client.initialize();
    await vi.waitFor(() => expect(factory.instances).toHaveLength(1));
    factory.instances[0].open();
    await initialized;

    const batches: unknown[] = [];
    client.subscribe('stocks', (result) => batches.push(result));
    factory.instances[0].message({
      queryId: 'stocks',
      data: { id: 'A', price: 11 },
    });
    expect(batches).toEqual([]);

    snapshot.resolve(jsonResponse([{ id: 'A', price: 10 }]));
    await vi.waitFor(() => expect(batches).toHaveLength(2));

    expect(batches).toEqual([
      expect.objectContaining({
        snapshot: true,
        data: [{ id: 'A', price: 10 }],
      }),
      expect.objectContaining({
        data: [{ id: 'A', price: 11 }],
      }),
    ]);
  });

  it('reports snapshot HTTP failures to the subscriber', async () => {
    vi.useFakeTimers();
    const factory = fakeEventSourceFactory();
    const failedSnapshot = deferred<Response>();
    const recoveredSnapshot = deferred<Response>();
    let snapshotRequests = 0;
    const fetcher = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.endsWith('/health')) return jsonResponse({});
      if (url.endsWith('/api/v1/instances')) return jsonResponse([]);
      if (url.includes('/api/v1/queries/stocks?view=full')) {
        return jsonResponse({ status: 'running', config: {} });
      }
      if (url.includes('/api/v1/reactions/stream?view=full')) {
        return jsonResponse({ status: 'running', config: {} });
      }
      if (url.endsWith('/api/v1/queries/stocks/results')) {
        snapshotRequests += 1;
        return snapshotRequests === 1
          ? failedSnapshot.promise
          : recoveredSnapshot.promise;
      }
      throw new Error(`Unexpected request: ${url}`);
    });
    const client = new DrasiClient({
      queries: [
        {
          id: 'stocks',
          query: 'MATCH (n) RETURN n',
          sources: [],
        },
      ],
      reaction: {
        id: 'stream',
        port: 8281,
        endpoint: 'http://localhost:8281/events',
      },
      fetch: fetcher as typeof fetch,
      eventSourceFactory: factory.create,
    });

    const initialized = client.initialize();
    await vi.waitFor(() => expect(factory.instances).toHaveLength(1));
    factory.instances[0].open();
    await initialized;
    const onError = vi.fn();
    const onResult = vi.fn();
    const unsubscribe = client.subscribe('stocks', onResult, onError);
    factory.instances[0].message({
      queryId: 'stocks',
      data: { id: 'A', price: 11 },
    });
    failedSnapshot.resolve(jsonResponse({ message: 'broken' }, 500));

    await vi.waitFor(() => expect(onError).toHaveBeenCalledOnce());
    expect(onError.mock.calls[0][0].message).toContain(
      'Failed to get results for query stocks (500)',
    );
    expect(onResult).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(1000);
    factory.instances[0].message({
      queryId: 'stocks',
      data: { id: 'A', price: 13 },
    });
    recoveredSnapshot.resolve(jsonResponse([{ id: 'A', price: 12 }]));
    await vi.waitFor(() => expect(onResult).toHaveBeenCalledTimes(2));

    expect(onResult.mock.calls.map(([result]) => result)).toEqual([
      expect.objectContaining({
        snapshot: true,
        data: [{ id: 'A', price: 12 }],
      }),
      expect.objectContaining({
        data: [{ id: 'A', price: 13 }],
      }),
    ]);
    unsubscribe();
    await client.disconnect();
  });

  it('fetches a fresh snapshot and buffers live deltas after reconnecting', async () => {
    vi.useFakeTimers();
    const factory = fakeEventSourceFactory();
    const reconnectedSnapshot = deferred<Response>();
    let snapshotRequests = 0;
    const fetcher = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.endsWith('/health')) return jsonResponse({});
      if (url.endsWith('/api/v1/instances')) return jsonResponse([]);
      if (url.includes('/api/v1/queries/stocks?view=full')) {
        return jsonResponse({ status: 'running', config: {} });
      }
      if (url.includes('/api/v1/reactions/stream?view=full')) {
        return jsonResponse({ status: 'running', config: {} });
      }
      if (url.endsWith('/api/v1/queries/stocks/results')) {
        snapshotRequests += 1;
        return snapshotRequests === 1
          ? jsonResponse([{ id: 'A', price: 10 }])
          : reconnectedSnapshot.promise;
      }
      throw new Error(`Unexpected request: ${url}`);
    });
    const client = new DrasiClient({
      queries: [
        {
          id: 'stocks',
          query: 'MATCH (n) RETURN n',
          sources: [],
        },
      ],
      reaction: {
        id: 'stream',
        port: 8281,
        endpoint: 'http://localhost:8281/events',
      },
      reconnect: { initialReconnectDelayMs: 10 },
      fetch: fetcher as typeof fetch,
      eventSourceFactory: factory.create,
    });

    const initialized = client.initialize();
    await vi.waitFor(() => expect(factory.instances).toHaveLength(1));
    factory.instances[0].open();
    await initialized;

    const onResult = vi.fn();
    const unsubscribe = client.subscribe('stocks', onResult);
    await vi.waitFor(() => expect(onResult).toHaveBeenCalledOnce());

    factory.instances[0].fail();
    await vi.advanceTimersByTimeAsync(10);
    expect(factory.instances).toHaveLength(2);
    factory.instances[1].open();
    factory.instances[1].message({
      queryId: 'stocks',
      data: { id: 'A', price: 13 },
    });
    expect(onResult).toHaveBeenCalledOnce();

    reconnectedSnapshot.resolve(jsonResponse([{ id: 'A', price: 12 }]));
    await vi.waitFor(() => expect(onResult).toHaveBeenCalledTimes(3));
    expect(onResult.mock.calls.slice(1).map(([result]) => result)).toEqual([
      expect.objectContaining({
        snapshot: true,
        data: [{ id: 'A', price: 12 }],
      }),
      expect.objectContaining({
        data: [{ id: 'A', price: 13 }],
      }),
    ]);

    unsubscribe();
    await client.disconnect();
  });

  it('keeps retrying snapshots at capped backoff until REST recovers', async () => {
    vi.useFakeTimers();
    const factory = fakeEventSourceFactory();
    let snapshotRequests = 0;
    const fetcher = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.endsWith('/health')) return jsonResponse({});
      if (url.endsWith('/api/v1/instances')) return jsonResponse([]);
      if (url.includes('/api/v1/queries/stocks?view=full')) {
        return jsonResponse({ status: 'running', config: {} });
      }
      if (url.includes('/api/v1/reactions/stream?view=full')) {
        return jsonResponse({ status: 'running', config: {} });
      }
      if (url.endsWith('/api/v1/queries/stocks/results')) {
        snapshotRequests += 1;
        return snapshotRequests <= 11
          ? jsonResponse({ message: 'temporarily unavailable' }, 503)
          : jsonResponse([{ id: 'A', price: 10 }]);
      }
      throw new Error(`Unexpected request: ${url}`);
    });
    const client = new DrasiClient({
      queries: [
        {
          id: 'stocks',
          query: 'MATCH (n) RETURN n',
          sources: [],
        },
      ],
      reaction: {
        id: 'stream',
        port: 8281,
        endpoint: 'http://localhost:8281/events',
      },
      fetch: fetcher as typeof fetch,
      eventSourceFactory: factory.create,
    });

    const initialized = client.initialize();
    await vi.waitFor(() => expect(factory.instances).toHaveLength(1));
    factory.instances[0].open();
    await initialized;

    const onResult = vi.fn();
    const onError = vi.fn();
    const unsubscribe = client.subscribe('stocks', onResult, onError);
    for (let failure = 1; failure <= 11; failure += 1) {
      await vi.waitFor(() =>
        expect(onError).toHaveBeenCalledTimes(failure),
      );
      await vi.advanceTimersToNextTimerAsync();
    }

    await vi.waitFor(() => expect(onResult).toHaveBeenCalledOnce());
    expect(snapshotRequests).toBe(12);
    expect(onResult.mock.calls[0][0]).toEqual(
      expect.objectContaining({
        snapshot: true,
        data: [{ id: 'A', price: 10 }],
      }),
    );

    unsubscribe();
    await client.disconnect();
  });
});
