// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { expect, expectTypeOf, it, vi } from 'vitest';
import { DrasiError } from '../src/client';
import { DrasiProvider, type UseDrasiQueryOptions } from '../src/react';
import { QueryTable, type QueryTableProps } from '../src/components';
import { fakeEventSourceFactory } from './FakeEventSource';
import { ReadServer, deferred, json, refs } from './server';

interface Reading { code: string; amount: number }
const queryOptions: UseDrasiQueryOptions<Reading> = {
  getKey: row => {
    if (typeof row.id !== 'string') throw new Error('Expected identity');
    return row.id;
  },
  transform: row => {
    if (typeof row.id !== 'string' || typeof row.value !== 'number') throw new Error('Expected reading');
    return { code: `rack-${row.id}`, amount: row.value };
  },
};
const props: QueryTableProps<Reading> = {
  queryId: 'stocks', queryOptions, rowKey: row => row.code,
  columns: [{ key: 'code', label: 'Rack' }, { key: 'amount', label: 'Amount' }],
};

it('exposes the full query state to loading, empty and retained-data slots through the real provider', async () => {
  const initial = deferred<Response>();
  const refresh = deferred<Response>();
  const server = new ReadServer();
  server.snapshot = () => initial.promise;
  const stream = fakeEventSourceFactory();
  const loading = vi.fn<NonNullable<QueryTableProps<Reading>['renderLoading']>>(
    ({ query }) => <span>Starting: {query.status}</span>,
  );
  const stale = vi.fn<NonNullable<QueryTableProps<Reading>['renderStale']>>(
    ({ query, rows, state }) => <span>{query.status}: {rows?.length} retained; {state.retryLabel}</span>,
  );
  render(<DrasiProvider {...refs} fetch={server.fetch} eventSourceFactory={stream.create}>
    <QueryTable {...props}
      renderLoading={loading}
      renderStale={stale}
      renderEmpty={({ query, state }) => <button onClick={state.retry}>No readings ({query.status})</button>}
    />
  </DrasiProvider>);
  expect(screen.getByText('Starting: initial-loading')).not.toBeNull();
  await waitFor(() => expect(stream.instances).toHaveLength(1));
  act(() => stream.instances[0].open());
  await act(async () => initial.resolve(json([])));
  await screen.findByRole('button', { name: 'No readings (empty)' });
  server.snapshot = () => refresh.promise;
  fireEvent.click(screen.getByRole('button', { name: 'No readings (empty)' }));
  await screen.findByText('resynchronizing: 0 retained; Retry query');
  const current = stale.mock.calls[stale.mock.calls.length - 1]?.[0];
  expect(current?.query.stale).toBe(true);
  expect(current?.query.errorScope).toBeNull();
  expect(current?.query.lastUpdate).toBeInstanceOf(Date);
  expect(current?.state.retry).toBe(current?.query.retry);
  expect(stream.instances).toHaveLength(1);
  await act(async () => refresh.resolve(json([{ id: 'a', value: 42 }])));
  await screen.findByText('rack-a');
  expect(screen.queryByRole('button', { name: /No readings/ })).toBeNull();
  expect(screen.queryByText(/retained/)).toBeNull();
});

it('keeps errors typed and query-local retry isolated from another real table subscription', async () => {
  const server = new ReadServer();
  server.reaction.queries = ['stocks', 'other'];
  server.snapshot = async () => json([{ id: 'a', value: 42 }]);
  const stream = fakeEventSourceFactory();
  const errorSlot = vi.fn<NonNullable<QueryTableProps<Reading>['renderError']>>(
    ({ error, query, state, retryConnection, rows }) => {
      expectTypeOf(error).toEqualTypeOf<DrasiError>();
      expect(error).toBe(query.error);
      expect(rows?.[0].code).toBe('rack-a');
      expect(query.errorScope).toBe('query');
      expect(state.retry).toBe(query.retry);
      expect(state.retry).not.toBe(retryConnection);
      return <button onClick={state.retry}>{error.code}: retry one</button>;
    },
  );
  render(<DrasiProvider {...refs} queryIds={['stocks', 'other']} fetch={server.fetch} eventSourceFactory={stream.create}>
    <section aria-label="First"><QueryTable {...props} renderError={errorSlot} /></section>
    <section aria-label="Second"><QueryTable {...props} queryId="other" /></section>
  </DrasiProvider>);
  await waitFor(() => expect(stream.instances).toHaveLength(1));
  act(() => stream.instances[0].open());
  await waitFor(() => expect(screen.getAllByText('42')).toHaveLength(2));
  const snapshots = (id: string) => server.fetch.mock.calls.filter(([input]) => String(input).endsWith(`/queries/${id}/results`)).length;
  const firstBefore = snapshots('stocks');
  const secondBefore = snapshots('other');
  act(() => stream.instances[0].message({ queryId: 'stocks', timestamp: 1, results: [{ type: 'ADD', data: false }] }));
  const slot = errorSlot.mock.calls[errorSlot.mock.calls.length - 1]?.[0];
  expect(slot?.error).toBeInstanceOf(DrasiError);
  expect(slot?.error.resourceId).toBe('stocks');
  expect(slot?.query.status).toBe('terminal-error');
  expect(slot?.query.stale).toBe(true);
  expect(within(screen.getByRole('region', { name: 'Second' })).queryByRole('alert')).toBeNull();
  fireEvent.click(screen.getByRole('button', { name: /retry one/ }));
  await waitFor(() => expect(screen.queryByRole('button', { name: /retry one/ })).toBeNull());
  expect(snapshots('stocks')).toBe(firstBefore + 1);
  expect(snapshots('other')).toBe(secondBefore);
  expect(stream.instances).toHaveLength(1);
  expect(stream.instances[0].closed).toBe(false);
});

it('lets shared-error slots recover initialization using the provider retry, not query retry', async () => {
  const server = new ReadServer();
  server.missing = 'query';
  server.snapshot = async () => json([{ id: 'a', value: 12 }]);
  const stream = fakeEventSourceFactory();
  const renderError = vi.fn<NonNullable<QueryTableProps<Reading>['renderError']>>(
    context => {
      expect(context.query.errorScope).toBe('connection');
      expect(context.state.retry).toBe(context.retryConnection);
      expect(context.state.retry).not.toBe(context.query.retry);
      return <button onClick={context.state.retry}>Retry {context.error.code}</button>;
    },
  );
  render(<DrasiProvider {...refs} fetch={server.fetch} eventSourceFactory={stream.create}>
    <QueryTable {...props} renderError={renderError} />
  </DrasiProvider>);
  await screen.findByRole('button', { name: 'Retry QUERY_NOT_FOUND' });
  expect(stream.instances).toHaveLength(0);
  server.missing = null;
  fireEvent.click(screen.getByRole('button', { name: 'Retry QUERY_NOT_FOUND' }));
  await waitFor(() => expect(stream.instances).toHaveLength(1));
  act(() => stream.instances[0].open());
  await screen.findByText('12');
  expect(screen.queryByRole('button', { name: /Retry/ })).toBeNull();
});

it('forwards controlled sort and header composition without conflating raw and rendered identity', async () => {
  const server = new ReadServer();
  server.snapshot = async () => json([{ id: 'a', value: 12 }, { id: 'b', value: 3 }]);
  const stream = fakeEventSourceFactory();
  const changed = vi.fn();
  const table = (sort: QueryTableProps<Reading>['sort']) => (
    <DrasiProvider {...refs} fetch={server.fetch} eventSourceFactory={stream.create}>
      <QueryTable {...props} sort={sort} onSortChange={changed}
        renderHeader={({ rows, setSort }) => <button onClick={() => setSort(null)}>Clear ({rows?.length})</button>}
      />
    </DrasiProvider>
  );
  const rendered = render(table({ column: 'amount', direction: 'asc' }));
  await waitFor(() => expect(stream.instances).toHaveLength(1));
  act(() => stream.instances[0].open());
  await screen.findByText('rack-b');
  expect(screen.getAllByRole('row')[1].textContent).toContain('rack-b');
  fireEvent.click(screen.getByRole('button', { name: 'Clear (2)' }));
  expect(changed).toHaveBeenCalledExactlyOnceWith(null);
  rendered.rerender(table(null));
  expect(screen.getAllByRole('row')[1].textContent).toContain('rack-a');
  act(() => stream.instances[0].message({ queryId: 'stocks', timestamp: 1, results: [{ type: 'DELETE', data: { id: 'a' } }] }));
  expect(screen.queryByText('rack-a')).toBeNull();
  expect(stream.instances).toHaveLength(1);
});

it('labels retained rows through shared reconnect and resynchronization, then accepts an empty baseline', async () => {
  const server = new ReadServer();
  server.snapshot = async () => json([{ id: 'a', value: 42 }]);
  const stream = fakeEventSourceFactory();
  render(<DrasiProvider {...refs}
    reconnect={{ maxReconnectAttempts: 1, initialReconnectDelayMs: 10, maxReconnectDelayMs: 20 }}
    fetch={server.fetch} eventSourceFactory={stream.create}
  >
    <QueryTable {...props} renderError={({ query, state }) => (
      <div role="alert" data-phase={query.status}>
        {state.staleMessage}
        <button onClick={state.retry}>{state.retryLabel}</button>
      </div>
    )} />
  </DrasiProvider>);
  await waitFor(() => expect(stream.instances).toHaveLength(1));
  act(() => stream.instances[0].open());
  await screen.findByText('42');
  const refreshed = deferred<Response>();
  server.snapshot = () => refreshed.promise;
  act(() => stream.instances[0].onerror?.(new Event('error')));
  expect(screen.getByRole('alert').getAttribute('data-phase')).toBe('reconnecting');
  expect(screen.getByRole('alert').textContent).toContain('Showing last known data.');
  expect(screen.getByRole('button', { name: 'Retry connection' })).not.toBeNull();
  expect(screen.getByText('42')).not.toBeNull();
  await waitFor(() => expect(stream.instances).toHaveLength(2));
  act(() => stream.instances[1].open());
  await waitFor(() => expect(screen.getByRole('alert').getAttribute('data-phase')).toBe('resynchronizing'));
  expect(screen.getByRole('alert').textContent).toContain('Resynchronizing. Showing last known data.');
  expect(screen.getByText('42')).not.toBeNull();
  await act(async () => refreshed.resolve(json([])));
  await screen.findByText('No data available');
  expect(screen.queryByText('42')).toBeNull();
  expect(screen.queryByRole('status')).toBeNull();
  expect(screen.queryByRole('alert')).toBeNull();
});
