// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { afterEach, describe, expect, it, vi } from 'vitest';
import { DrasiClient } from '../src/client/DrasiClient';
import { DrasiSSEClient, type EventSourceFactory } from '../src/client/DrasiSSEClient';
import { DrasiError } from '../src/client/errors';
import type { DrasiHeadersProvider, DrasiRequestContext } from '../src/client/types';
import { FakeEventSource, fakeEventSourceFactory } from './FakeEventSource';
import { component, deferred, failure, json, queryConfig, ReadServer, refs } from './server';

const clients: Array<DrasiClient | DrasiSSEClient> = [];
afterEach(async () => {
  await Promise.all(clients.splice(0).map(client => client.disconnect()));
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

function setup(options: Partial<ConstructorParameters<typeof DrasiClient>[0]> = {}) {
  const server = new ReadServer(), factory = fakeEventSourceFactory();
  const create = vi.fn<EventSourceFactory>((url, _options) => factory.create(url));
  const client = new DrasiClient({ ...refs, fetch: server.fetch, eventSourceFactory: create, ...options });
  clients.push(client);
  return { client, server, factory, create };
}

describe('shared read-only transport/auth policy', () => {
  it.each(['omit', 'same-origin', 'include'] as const)(
    'uses %s credentials and isolated headers for every read and custom stream opening', async credentials => {
      const headers = new Headers({ Authorization: 'Bearer fixture-only', 'X-Tenant': 'one' });
      const { client, server, create, factory } = setup({ credentials, headers });
      headers.set('X-Tenant', 'changed-after-construction');
      const connected = client.initialize();
      await vi.waitFor(() => expect(factory.instances).toHaveLength(1));
      const streamOptions = create.mock.calls[0][1];
      expect(streamOptions.credentials).toBe(credentials);
      expect(streamOptions.headers.get('Authorization')).toBe('Bearer fixture-only');
      expect(streamOptions.headers.get('Accept')).toBe('text/event-stream');
      expect(streamOptions.headers.get('X-Tenant')).toBe('one');
      streamOptions.headers.set('X-Tenant', 'factory-mutation');
      factory.instances[0].open();
      await connected;
      await client.getQuery('stocks');
      await client.getQueryConfig('stocks');
      await client.getReaction();
      await client.getQueryResults('stocks');
      for (const [url, init] of server.fetch.mock.calls) {
        expect(String(url)).toContain(`/instances/${encodeURIComponent(refs.instanceId)}/`);
        expect(init).toMatchObject({ method: 'GET', credentials, redirect: 'manual' });
        expect(init?.signal).toBeInstanceOf(AbortSignal);
        const requestHeaders = new Headers(init?.headers);
        expect(requestHeaders.get('Authorization')).toBe('Bearer fixture-only');
        expect(requestHeaders.get('Accept')).toBe('application/json');
        expect(requestHeaders.get('X-Tenant')).toBe('one');
      }
      expect(new Set(server.fetch.mock.calls.map(([, init]) => init?.headers)).size).toBe(server.fetch.mock.calls.length);
      await client.disconnect();
      expect(streamOptions.signal.aborted).toBe(true);
    },
  );

  it('refreshes callable auth for reads, snapshots and each stream retry without a new client', async () => {
    vi.useFakeTimers();
    let token = 'first';
    const contexts: DrasiRequestContext[] = [];
    const headers: DrasiHeadersProvider = async context => {
      contexts.push(context);
      return { Authorization: `Bearer fixture-${token}` };
    };
    const { client, server, create, factory } = setup({
      headers, reconnect: { maxReconnectAttempts: 1, initialReconnectDelayMs: 10 },
    });
    const connected = client.initialize();
    await vi.waitFor(() => expect(factory.instances).toHaveLength(1));
    factory.instances[0].open();
    await connected;
    token = 'second';
    const received = vi.fn();
    client.subscribe('stocks', received);
    await vi.waitFor(() => expect(received).toHaveBeenCalledOnce());
    expect(new Headers(server.fetch.mock.calls[server.fetch.mock.calls.length - 1][1]?.headers)
      .get('Authorization')).toBe('Bearer fixture-second');
    token = 'third';
    factory.instances[0].fail();
    await vi.advanceTimersByTimeAsync(10);
    expect(factory.instances).toHaveLength(2);
    expect(create.mock.calls[1][1].headers.get('Authorization')).toBe('Bearer fixture-third');
    expect(create.mock.calls[0][1].signal.aborted).toBe(true);
    expect(contexts.every(context => context.instanceId === refs.instanceId)).toBe(true);
    expect(contexts.filter(context => context.transport === 'sse').map(context => context.url))
      .toEqual([refs.reaction.endpoint, refs.reaction.endpoint]);
  });

  it.each([401, 403])('preserves HTTP %s across all read endpoints and stops initialization', async status => {
    const { client, server, factory } = setup({ credentials: 'include', headers: { Authorization: 'fixture' } });
    server.fetch.mockImplementation(async () => failure(status));
    for (const read of [
      () => client.getQuery('stocks'), () => client.getQueryConfig('stocks'),
      () => client.getQueryResults('stocks'), () => client.getReaction(), () => client.initialize(),
    ]) {
      await expect(read()).rejects.toMatchObject({
        code: status === 401 ? 'UNAUTHENTICATED' : 'FORBIDDEN', status, retryable: false,
      });
    }
    expect(factory.instances).toHaveLength(0);
    expect(server.fetch.mock.calls.every(([, init]) => init?.method === 'GET')).toBe(true);
  });

  it('retains typed auth-provider errors without exposing credentials or opening a stream', async () => {
    const error = new DrasiError('UNAUTHENTICATED', { instanceId: refs.instanceId });
    const { client, server, factory } = setup({ headers: async () => { throw error; } });
    await expect(client.initialize()).rejects.toBe(error);
    expect(client.getConnectionStatus().error).toBe(error);
    expect(server.fetch).not.toHaveBeenCalled();
    expect(factory.instances).toHaveLength(0);
  });

  it('rejects an invalid JavaScript auth-provider result instead of silently dropping configured auth', async () => {
    const { client, server } = setup({
      // @ts-expect-error JavaScript callers can violate the required HeadersInit return.
      headers: async () => undefined,
    });
    await expect(client.initialize()).rejects.toMatchObject({ code: 'INVALID_CONFIGURATION', retryable: false });
    expect(server.fetch).not.toHaveBeenCalled();
  });

  it('bounds uncooperative auth/fetch/body promises, including an overlapping validation batch', async () => {
    vi.useFakeTimers();
    for (const mode of ['auth', 'fetch', 'body'] as const) {
      const { client, server } = setup({
        queryIds: ['stocks', 'other'], requestTimeoutMs: 25,
        ...(mode === 'auth' ? { headers: () => new Promise<HeadersInit>(() => {}) } : {}),
      });
      if (mode === 'fetch') server.fetch.mockImplementation(() => new Promise<Response>(() => {}));
      if (mode === 'body') server.fetch.mockImplementation(async () => new Response(new ReadableStream()));
      const request = client.initialize().catch((error: unknown) => error);
      await vi.advanceTimersByTimeAsync(25);
      expect(await request).toMatchObject({ code: 'SERVER_UNAVAILABLE', resourceId: 'stocks', retryable: true });
      expect(client.getConnectionStatus().reconnecting).toBe(false);
    }
  });

  it.each(['query', 'config', 'results', 'reaction'] as const)(
    'preserves the exact %s read abort reason and prevents late auth from issuing a request', async endpoint => {
      const auth = deferred<HeadersInit>();
      const { client, server } = setup({ headers: () => auth.promise });
      const controller = new AbortController(), reason = new DOMException('Fixture cancellation', 'AbortError');
      const pending = endpoint === 'reaction' ? client.getReaction(controller.signal)
        : endpoint === 'query' ? client.getQuery('stocks', controller.signal)
          : endpoint === 'config' ? client.getQueryConfig('stocks', controller.signal)
            : client.getQueryResults('stocks', controller.signal);
      controller.abort(reason);
      await expect(pending).rejects.toBe(reason);
      auth.resolve({ Authorization: 'must-not-be-sent' });
      await Promise.resolve();
      expect(server.fetch).not.toHaveBeenCalled();
    },
  );

  it('does not follow a redirect into another instance or accept a mismatched full-view link', async () => {
    const { client, server } = setup();
    server.fetch.mockResolvedValueOnce(new Response(null, {
      status: 302, headers: { Location: 'https://drasi.invalid/api/v1/instances/other/queries/stocks' },
    }));
    await expect(client.getQuery('stocks')).rejects.toMatchObject({ code: 'INCOMPATIBLE_RESOURCE', retryable: false });
    expect(server.fetch).toHaveBeenCalledOnce();
    server.fetch.mockResolvedValueOnce(json(component('queries', 'stocks', queryConfig('stocks'), 'Running', 'other')));
    await expect(client.getQuery('stocks')).rejects.toMatchObject({ code: 'INVALID_PAYLOAD', instanceId: refs.instanceId });
  });

  it('keeps separate clients isolated even when query and reaction IDs are identical', async () => {
    const fetcher = vi.fn<typeof fetch>(async input => {
      const url = new URL(String(input)), segments = url.pathname.split('/');
      const instanceId = decodeURIComponent(segments[4]), id = decodeURIComponent(segments[6]);
      return json(component('queries', id, queryConfig(id, { query: `RETURN '${instanceId}'` }), 'Running', instanceId));
    });
    const first = setup({ fetch: fetcher, instanceId: 'tenant/one' }).client;
    const second = setup({ fetch: fetcher, instanceId: 'tenant two' }).client;
    expect((await first.getQueryConfig('stocks')).query).toBe("RETURN 'tenant/one'");
    expect((await second.getQueryConfig('stocks')).query).toBe("RETURN 'tenant two'");
    expect(first.getServerUiUrl()).toContain('tenant%2Fone');
    expect(second.getServerUiUrl()).toContain('tenant%20two');
  });
});

describe('native EventSource authentication and execution boundary', () => {
  it.each(['same-origin', 'include'] as const)('uses native withCredentials for %s', async credentials => {
    const sources: FakeEventSource[] = [];
    const native = vi.fn(function (url: string, init?: EventSourceInit) {
      expect(init?.withCredentials).toBe(credentials === 'include');
      const source = new FakeEventSource(url);
      sources.push(source);
      return source;
    });
    vi.stubGlobal('EventSource', native);
    const client = new DrasiSSEClient({ credentials });
    clients.push(client);
    expect(native).not.toHaveBeenCalled();
    const connected = client.connect(['stocks'], refs.reaction.endpoint);
    sources[0].open();
    await connected;
  });

  it('fails closed on unsupported native custom headers/omit at execution and rejects malformed options', async () => {
    for (const options of [
      { headers: { Authorization: 'fixture' } },
      { headers: () => ({ Authorization: 'fixture' }) },
      { credentials: 'omit' as const },
    ]) {
      const client = new DrasiSSEClient(options);
      clients.push(client);
      await expect(client.connect(['stocks'], refs.reaction.endpoint)).rejects.toMatchObject({
        code: 'INVALID_CONFIGURATION', retryable: false,
      });
    }
    expect(() => new DrasiClient({ ...refs, headers: { 'Bad\nHeader': 'fixture' } })).toThrow(DrasiError);
    // Deliberately exercise invalid JavaScript callers, not just TypeScript.
    expect(() => new DrasiSSEClient({ credentials: JSON.parse('"invalid"') })).toThrow(DrasiError);
    vi.stubGlobal('fetch', undefined);
    expect(() => new DrasiClient(refs)).toThrow(DrasiError);
  });

  it('allows authenticated Node REST-only inspection without requiring a stream factory', async () => {
    vi.stubGlobal('EventSource', undefined);
    const { client, server } = setup({
      eventSourceFactory: undefined, credentials: 'omit', headers: { Authorization: 'fixture-only' },
    });
    expect((await client.getQueryConfig('stocks')).id).toBe('stocks');
    expect(new Headers(server.fetch.mock.calls[0][1]?.headers).get('Authorization')).toBe('fixture-only');
    await expect(client.initialize()).rejects.toMatchObject({ code: 'INVALID_CONFIGURATION' });
  });

  it('reports unavailable native execution as invalid configuration, not missing resources or infinite retry', async () => {
    vi.stubGlobal('EventSource', undefined);
    const client = new DrasiSSEClient();
    clients.push(client);
    await expect(client.connect(['stocks'], refs.reaction.endpoint)).rejects.toMatchObject({
      code: 'INVALID_CONFIGURATION', retryable: false,
    });
  });

  it('bounds asynchronous SSE auth and preserves abort before a factory is called', async () => {
    vi.useFakeTimers();
    const pending = deferred<HeadersInit>(), create = vi.fn<EventSourceFactory>(url => new FakeEventSource(url));
    const client = new DrasiSSEClient({
      eventSourceFactory: create, headers: () => pending.promise, connectionTimeoutMs: 20, maxReconnectAttempts: 0,
    });
    clients.push(client);
    const first = client.connect(['stocks'], refs.reaction.endpoint).catch((error: unknown) => error);
    await vi.advanceTimersByTimeAsync(20);
    expect(await first).toMatchObject({ code: 'STREAM_UNAVAILABLE' });
    const controller = new AbortController(), reason = new DOMException('cancel fixture', 'AbortError');
    const second = client.connect(['stocks'], refs.reaction.endpoint, controller.signal);
    controller.abort(reason);
    await expect(second).rejects.toBe(reason);
    pending.resolve({});
    await Promise.resolve();
    expect(create).not.toHaveBeenCalled();
  });
});
