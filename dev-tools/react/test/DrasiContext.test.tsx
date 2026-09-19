// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import React from 'react';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import {
  DrasiProvider, DrasiClientProvider, useDrasiClient, useDrasiConnectionStatus,
  useDrasiQuery, useDrasiQueryDefinition, useDrasiServerUiUrl,
} from '../src/react/DrasiContext';
import { DrasiError } from '../src/client/errors';
import { fakeEventSourceFactory } from './FakeEventSource';
import { ReadServer, failure, refs } from './server';

function Probe({ observe = () => {} }: { observe?: (errors: (DrasiError | null | undefined)[]) => void }) {
  const context = useDrasiClient();
  const connection = useDrasiConnectionStatus();
  const definition = useDrasiQueryDefinition('stocks');
  const ui = useDrasiServerUiUrl();
  const query = useDrasiQuery<{ id: string; value: number }>('stocks', {
    getKey: row => {
      if (typeof row.id !== 'string') throw new Error('Expected raw row ID');
      return row.id;
    },
    transform: row => {
      if (typeof row.id !== 'string') throw new Error('Expected row ID');
      return { id: row.id, value: Number(row.value) };
    },
  });
  observe([context.error, connection.error, query.error, definition.error]);
  return <div>
    <span data-testid="loading">{String(query.loading)}</span>
    <span data-testid="error">{query.error?.message ?? ''}</span>
    <span data-testid="data">{JSON.stringify(query.data)}</span>
    <span data-testid="ui">{ui}</span>
    <button onClick={context.retry}>Retry</button>
  </div>;
}

describe('typed failures through all React bindings', () => {
  it.each([
    'QUERY_NOT_FOUND', 'REACTION_NOT_FOUND', 'INSTANCE_NOT_FOUND', 'RESOURCE_STOPPED',
    'RESOURCE_STARTING', 'SERVER_UNAVAILABLE', 'UNAUTHENTICATED', 'FORBIDDEN', 'INVALID_PAYLOAD',
    'INVALID_CONFIGURATION', 'INCOMPATIBLE_RESOURCE', 'RESOURCE_UNAVAILABLE',
  ] as const)('preserves the exact %s object through context/status/query/definition hooks', async code => {
    const server = new ReadServer();
    if (code === 'QUERY_NOT_FOUND') server.missing = 'query';
    if (code === 'REACTION_NOT_FOUND') server.missing = 'reaction';
    if (code === 'INSTANCE_NOT_FOUND') server.missing = 'instance';
    if (code === 'RESOURCE_STOPPED') server.queryStatus = 'Stopped';
    if (code === 'RESOURCE_STARTING') server.queryStatus = 'Starting';
    if (code === 'RESOURCE_UNAVAILABLE') server.queryStatus = 'Error';
    if (code === 'SERVER_UNAVAILABLE') server.fetch.mockRejectedValue(new TypeError('secret network details'));
    if (code === 'UNAUTHENTICATED') server.fetch.mockResolvedValue(failure(401));
    if (code === 'FORBIDDEN') server.fetch.mockResolvedValue(failure(403));
    if (code === 'INVALID_PAYLOAD') server.fetch.mockResolvedValue(new Response('broken'));
    if (code === 'INCOMPATIBLE_RESOURCE') server.reaction.kind = 'log';
    const options = code === 'INVALID_CONFIGURATION' ? { ...refs, instanceId: '' } : refs;
    const observe = vi.fn();
    const rendered = render(<DrasiProvider {...options} fetch={server.fetch}><Probe observe={observe} /></DrasiProvider>);
    await waitFor(() => expect(screen.getByTestId('loading').textContent).toBe('false'));
    await waitFor(() => {
      const errors = observe.mock.calls[observe.mock.calls.length - 1][0];
      expect(errors[0]).toBeInstanceOf(DrasiError);
      expect(errors[0].code).toBe(code);
      expect(errors.every((error: unknown) => error === errors[0])).toBe(true);
    });
    expect(screen.getByTestId('error').textContent).not.toContain('secret');
    expect(server.fetch.mock.calls.every(([, init]) => init?.method === 'GET')).toBe(true);
    rendered.unmount();
  });

  it('preserves a terminal stream error and performs only reads on explicit retry', async () => {
    const server = new ReadServer(), factory = fakeEventSourceFactory(), observe = vi.fn();
    const rendered = render(
      <DrasiProvider {...refs} fetch={server.fetch} eventSourceFactory={factory.create}>
        <Probe observe={observe} />
      </DrasiProvider>,
    );
    await waitFor(() => expect(factory.instances).toHaveLength(1));
    act(() => factory.instances[0].fail());
    await waitFor(() => expect(screen.getByTestId('error').textContent).toContain('SSE endpoint'));
    const errors = observe.mock.calls[observe.mock.calls.length - 1][0];
    expect(errors[0]).toBeInstanceOf(DrasiError);
    expect(errors[0].code).toBe('STREAM_UNAVAILABLE');
    expect(errors.every((error: unknown) => error === errors[0])).toBe(true);
    fireEvent.click(screen.getByText('Retry'));
    await waitFor(() => expect(factory.instances).toHaveLength(2));
    act(() => factory.instances[1].open());
    await waitFor(() => expect(screen.getByTestId('data').textContent).toBe('[{"id":"A","value":10}]'));
    expect(screen.getByTestId('error').textContent).toBe('');
    expect(errors[0].code).toBe('STREAM_UNAVAILABLE');
    expect(server.fetch.mock.calls.every(([, init]) => init?.method === 'GET')).toBe(true);
    rendered.unmount();
    expect(factory.instances.every(source => source.closed)).toBe(true);
  });

  it('applies sparse deletes without transforms and surfaces malformed live payloads', async () => {
    const server = new ReadServer(), factory = fakeEventSourceFactory(), observe = vi.fn();
    render(<DrasiProvider {...refs} fetch={server.fetch} eventSourceFactory={factory.create}>
      <Probe observe={observe} />
    </DrasiProvider>);
    await waitFor(() => expect(factory.instances).toHaveLength(1));
    act(() => factory.instances[0].open());
    await waitFor(() => expect(screen.getByTestId('data').textContent).toBe('[{"id":"A","value":10}]'));
    expect(screen.getByTestId('ui').textContent).toContain(encodeURIComponent(refs.instanceId));
    act(() => factory.instances[0].message({
      queryId: 'stocks', timestamp: 1, results: [{ type: 'DELETE', data: { id: 'A' } }],
    }));
    await waitFor(() => expect(screen.getByTestId('data').textContent).toBe('[]'));
    act(() => factory.instances[0].onmessage?.(new MessageEvent('message', { data: 'broken json' })));
    await waitFor(() => expect(screen.getByTestId('error').textContent).toContain('malformed'));
    const errors = observe.mock.calls[observe.mock.calls.length - 1][0];
    expect(errors.every((error: unknown) => error === errors[0])).toBe(true);
    expect(errors[0]).toMatchObject({
      code: 'INVALID_PAYLOAD', instanceId: refs.instanceId, resourceKind: 'reaction', resourceId: 'stream',
    });
    expect(factory.instances[0].closed).toBe(true);
  });

  it('closes the sole EventSource under StrictMode/unmount and ignores obsolete effects', async () => {
    const server = new ReadServer(), factory = fakeEventSourceFactory();
    const rendered = render(<React.StrictMode>
      <DrasiProvider {...refs} fetch={server.fetch} eventSourceFactory={factory.create}><Probe /></DrasiProvider>
    </React.StrictMode>);
    await waitFor(() => expect(factory.instances).toHaveLength(1));
    act(() => factory.instances[0].open());
    await waitFor(() => expect(screen.getByTestId('loading').textContent).toBe('false'));
    rendered.unmount();
    expect(factory.instances.every(source => source.closed)).toBe(true);
  });

  it('binds an app-owned lifecycle without starting another client or converting its error', async () => {
    const error = new DrasiError('FORBIDDEN', { instanceId: refs.instanceId });
    const observe = vi.fn(), retry = vi.fn();
    render(<DrasiClientProvider value={{ client: null, initialized: false, error, retry }}>
      <Probe observe={observe} />
    </DrasiClientProvider>);
    await waitFor(() => expect(observe.mock.calls[observe.mock.calls.length - 1][0]).toEqual([error, error, error, error]));
    fireEvent.click(screen.getByText('Retry'));
    expect(retry).toHaveBeenCalledOnce();
  });
});
