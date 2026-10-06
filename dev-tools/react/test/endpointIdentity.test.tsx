// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { act, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { DrasiClient } from '../src/client/DrasiClient';
import type { EventSourceFactory } from '../src/client/DrasiSSEClient';
import { DrasiError } from '../src/client/errors';
import type { DrasiHeadersProvider } from '../src/client/types';
import { DrasiProvider, useDrasiClient, useDrasiQuery, type DrasiContextValue } from '../src/react/DrasiContext';
import { configurationKey } from '../src/react/configuration';
import { fakeEventSourceFactory } from './FakeEventSource';
import { ReadServer, refs } from './server';

const clients: DrasiClient[] = [];
afterEach(async () => {
  await Promise.all(clients.splice(0).map(client => client.disconnect()));
});

const endpoints = [
  'https://events.invalid/proxy/events/',
  'https://events.invalid/proxy/events?opaque=part/',
  'https://events.invalid/proxy/events/?opaque=part%2F/',
];

function Probe({ observe }: { observe: (context: DrasiContextValue) => void }) {
  const context = useDrasiClient();
  const query = useDrasiQuery('stocks', {
    getKey: row => typeof row.id === 'string' ? row.id : '',
    transform: row => row,
  });
  observe(context);
  return <>
    <output data-testid="ready">{String(context.initialized)}</output>
    <output data-testid="rows">{JSON.stringify(query.data)}</output>
  </>;
}

describe('explicit endpoint identity', () => {
  it.each(endpoints)('preserves %s for the stream factory and its authentication context', async endpoint => {
    const server = new ReadServer(), factory = fakeEventSourceFactory();
    const headers = vi.fn<DrasiHeadersProvider>(() => ({ 'X-Fixture': 'endpoint-identity' }));
    const client = new DrasiClient({
      ...refs, reaction: { ...refs.reaction, endpoint }, headers,
      fetch: server.fetch, eventSourceFactory: factory.create,
    });
    clients.push(client);
    expect(server.fetch).not.toHaveBeenCalled();
    expect(headers).not.toHaveBeenCalled();
    expect(factory.instances).toHaveLength(0);
    const ready = client.initialize();
    await waitFor(() => expect(factory.instances).toHaveLength(1));
    factory.instances[0].open();
    await ready;
    expect(factory.instances[0].url).toBe(endpoint);
    expect(headers.mock.calls.filter(([context]) => context.transport === 'sse').map(([context]) => context.url))
      .toEqual([endpoint]);
    expect(server.fetch.mock.calls.every(([, init]) => init?.method === 'GET')).toBe(true);
  });

  it.each([
    ['https://drasi.invalid', 'https://drasi.invalid/'],
    ['https://drasi.invalid', 'HTTPS://DRASI.INVALID:443/'],
    ['https://drasi.invalid/proxy', 'https://drasi.invalid/proxy/'],
  ])('normalizes equivalent server bases %s and %s without changing the endpoint', async (first, second) => {
    const server = new ReadServer(), factory = fakeEventSourceFactory();
    const endpoint = endpoints[0];
    const options = { ...refs, reaction: { ...refs.reaction, endpoint } };
    expect(configurationKey({ ...options, serverUrl: first })).toBe(configurationKey({ ...options, serverUrl: second }));
    const fetcher: typeof fetch = (input, init) => {
      const url = new URL(String(input));
      const prefix = first.endsWith('/proxy') ? '/proxy' : '';
      expect(url.pathname.startsWith(`${prefix}/api/v1/instances/`)).toBe(true);
      url.pathname = url.pathname.slice(prefix.length);
      return server.fetch(url, init);
    };
    for (const serverUrl of [first, second]) {
      const client = new DrasiClient({ ...options, serverUrl, fetch: fetcher, eventSourceFactory: factory.create });
      clients.push(client);
      expect((await client.getQueryConfig('stocks')).id).toBe('stocks');
      expect(client.getServerUiUrl()).toBe(`${first}/ui?instance=${encodeURIComponent(refs.instanceId)}`);
    }
  });

  it('keeps one provider client and stream across equivalent server-base rerenders', async () => {
    const server = new ReadServer(), factory = fakeEventSourceFactory();
    const observe = vi.fn<(context: DrasiContextValue) => void>();
    const tree = (serverUrl: string) => <DrasiProvider
      {...refs} serverUrl={serverUrl} queryIds={[...refs.queryIds]}
      reaction={{ ...refs.reaction }} fetch={server.fetch} eventSourceFactory={factory.create}
    ><Probe observe={observe} /></DrasiProvider>;
    const view = render(tree(refs.serverUrl));
    await waitFor(() => expect(factory.instances).toHaveLength(1));
    act(() => factory.instances[0].open());
    await waitFor(() => expect(screen.getByTestId('rows').textContent).toContain('"id":"A"'));
    const client = observe.mock.calls[observe.mock.calls.length - 1][0].client;
    const reads = server.fetch.mock.calls.length;
    view.rerender(tree(`${refs.serverUrl}/`));
    view.rerender(tree('HTTPS://DRASI.INVALID:443/'));
    expect(observe.mock.calls[observe.mock.calls.length - 1][0].client).toBe(client);
    expect(factory.instances).toHaveLength(1);
    expect(factory.instances[0].closed).toBe(false);
    expect(server.fetch).toHaveBeenCalledTimes(reads);
    view.unmount();
    expect(factory.instances[0].closed).toBe(true);
  });

  it.each(endpoints)('reconfigures the real provider when a significant trailing slash changes to %s', async endpoint => {
    const server = new ReadServer(), factory = fakeEventSourceFactory();
    const create = vi.fn<EventSourceFactory>((url, _options) => factory.create(url));
    const observe = vi.fn<(context: DrasiContextValue) => void>();
    const before = endpoint.slice(0, -1);
    const options = (url: string) => ({ ...refs, reaction: { ...refs.reaction, endpoint: url } });
    const tree = (url: string) => <DrasiProvider {...options(url)} fetch={server.fetch} eventSourceFactory={create}>
      <Probe observe={observe} />
    </DrasiProvider>;
    const view = render(tree(before));
    await waitFor(() => expect(factory.instances).toHaveLength(1));
    act(() => factory.instances[0].open());
    await waitFor(() => expect(screen.getByTestId('rows').textContent).toContain('"id":"A"'));
    const first = observe.mock.calls[observe.mock.calls.length - 1][0].client;
    view.rerender(tree(endpoint));
    expect(factory.instances[0].closed).toBe(true);
    expect(configurationKey(options(before))).not.toBe(configurationKey(options(endpoint)));
    expect(create.mock.calls[0][1].signal.aborted).toBe(true);
    await waitFor(() => expect(factory.instances).toHaveLength(2));
    expect(factory.instances[1].url).toBe(endpoint);
    act(() => factory.instances[1].open());
    await waitFor(() => expect(screen.getByTestId('ready').textContent).toBe('true'));
    await waitFor(() => expect(screen.getByTestId('rows').textContent).toContain('"id":"A"'));
    expect(observe.mock.calls[observe.mock.calls.length - 1][0].client).not.toBe(first);
    act(() => factory.instances[0].message({ queryId: 'stocks', data: [{ id: 'stale-endpoint' }] }));
    expect(screen.getByTestId('rows').textContent).not.toContain('stale-endpoint');
    const reads = server.fetch.mock.calls.length;
    view.rerender(tree(endpoint));
    expect(factory.instances).toHaveLength(2);
    expect(server.fetch).toHaveBeenCalledTimes(reads);
    expect(server.fetch.mock.calls.every(([, init]) => init?.method === 'GET')).toBe(true);
    view.unmount();
    expect(factory.instances.every(source => source.closed)).toBe(true);
  });

  it.each([
    '/relative/', 'file:///events/', 'https://user:password@events.invalid/events/',
    'https://events.invalid/events/#fragment', 'http://0.0.0.0/events/', 'http://[::]/events/',
  ])('retains endpoint safety rejection for %s without networking', endpoint => {
    const server = new ReadServer(), factory = fakeEventSourceFactory();
    expect(() => new DrasiClient({
      ...refs, reaction: { ...refs.reaction, endpoint }, fetch: server.fetch, eventSourceFactory: factory.create,
    })).toThrow(DrasiError);
    expect(server.fetch).not.toHaveBeenCalled();
    expect(factory.instances).toHaveLength(0);
  });

  it('still rejects a server-base query even if its value ends in a slash', () => {
    expect(() => new DrasiClient({ ...refs, serverUrl: 'https://drasi.invalid?opaque=part/' }))
      .toThrow(DrasiError);
  });
});
