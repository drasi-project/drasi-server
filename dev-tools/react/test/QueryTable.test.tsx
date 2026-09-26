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

import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from '@testing-library/react';
import { describe, expect, expectTypeOf, it, vi } from 'vitest';
import {
  QueryTable,
  type ColumnDef,
  type QueryTableProps,
  type RowAction,
  type SortConfig,
} from '../src/components';
import { DrasiProvider } from '../src/react/DrasiContext';
import type { ResultRow } from '../src/client/types';
import type { UseDrasiQueryOptions } from '../src/react/types';
import type { AnimationDirection } from '../src/react/useRowAnimation';
import { fakeEventSourceFactory } from './FakeEventSource';
import { ReadServer, json, refs } from './server';

interface Stock {
  symbol: string;
  price: number;
}

const columns: ColumnDef<Stock>[] = [
  { key: 'symbol', label: 'Symbol', width: '5rem' },
  { key: 'price', label: 'Price' },
];

const stockOptions: UseDrasiQueryOptions<Stock> = {
  getKey: row => String(row.symbol),
  transform: row => {
    if (typeof row.symbol !== 'string' || typeof row.price !== 'number') {
      throw new TypeError('Invalid synthetic stock row');
    }
    return { symbol: row.symbol, price: row.price };
  },
};

function renderTable<T extends object>(
  props: QueryTableProps<T>,
  rows: ResultRow[],
  server = new ReadServer(),
) {
  server.snapshot = () => Promise.resolve(json(rows));
  const factory = fakeEventSourceFactory();
  const rendered = render(
    <DrasiProvider {...refs} fetch={server.fetch} eventSourceFactory={factory.create}>
      <QueryTable {...props} />
    </DrasiProvider>,
  );
  return {
    ...rendered,
    factory,
    async connect() {
      await waitFor(() => expect(factory.instances).toHaveLength(1));
      act(() => factory.instances[0].open());
      await screen.findByRole('table');
    },
  };
}

describe('QueryTable', () => {
  it('uses explicit raw identity independently of transformed render keys and sorting', async () => {
    const table = renderTable<Stock>({
      queryId: 'stocks',
      columns,
      rowKey: row => row.symbol,
      queryOptions: stockOptions,
      defaultSort: { column: 'symbol', direction: 'asc' },
    }, [
      { symbol: 'MSFT', price: 12 },
      { symbol: 'AAPL', price: 10 },
    ]);
    await table.connect();

    act(() => table.factory.instances[0].message({
      queryId: 'stocks', timestamp: 1, results: [{ type: 'ADD', data: { symbol: 'AAPL', price: 11 } }],
    }));
    expect(screen.getAllByRole('row')).toHaveLength(3);
    expect(screen.queryByText('10')).toBeNull();
    expect(screen.getByText('11')).not.toBeNull();

    const symbolHeader = screen.getByRole('columnheader', { name: /symbol/i });
    expect(symbolHeader.getAttribute('aria-sort')).toBe('ascending');
    expect(symbolHeader.style.width).toBe('5rem');
    expect(
      within(screen.getAllByRole('row')[1]).getByText('AAPL'),
    ).not.toBeNull();

    fireEvent.keyDown(symbolHeader, { key: 'Enter' });
    expect(symbolHeader.getAttribute('aria-sort')).toBe('descending');
    expect(
      within(screen.getAllByRole('row')[1]).getByText('MSFT'),
    ).not.toBeNull();
  });

  it('renders real query lookup errors instead of an empty table', async () => {
    const server = new ReadServer();
    server.missing = 'query';
    const table = renderTable<Stock>({
      queryId: 'stocks', columns, rowKey: row => row.symbol, queryOptions: stockOptions,
    }, [], server);

    expect(await screen.findByText('Error: The referenced query does not exist.')).not.toBeNull();
    expect(screen.queryByRole('table')).toBeNull();
    expect(table.factory.instances).toHaveLength(0);
  });

  it('retains rows on a shared failure and routes recovery to the connection owner', async () => {
    const table = renderTable<Stock>({
      queryId: 'stocks', title: 'Stocks', columns,
      rowKey: row => row.symbol, queryOptions: stockOptions,
    }, [{ symbol: 'AAPL', price: 10 }]);
    await table.connect();
    expect(screen.queryByRole('button', { name: 'Expand table' })).toBeNull();

    act(() => table.factory.instances[0].onmessage?.(
      new MessageEvent('message', { data: 'broken JSON' }),
    ));

    await screen.findByText(/Error: .*malformed/);
    expect(screen.getByRole('table')).not.toBeNull();
    expect(screen.getByText('10')).not.toBeNull();
    fireEvent.click(screen.getByRole('button', { name: 'Retry connection' }));
    await waitFor(() => expect(table.factory.instances).toHaveLength(2));
    expect(table.factory.instances[0].closed).toBe(true);
    act(() => table.factory.instances[1].open());
    await waitFor(() => expect(screen.queryByRole('alert')).toBeNull());
  });

  it('retains useful rows on a query-only fault and retries without opening another stream', async () => {
    const table = renderTable<Stock>({
      queryId: 'stocks', columns, rowKey: row => row.symbol, queryOptions: stockOptions,
    }, [{ symbol: 'AAPL', price: 10 }]);
    await table.connect();
    act(() => table.factory.instances[0].message({
      queryId: 'stocks', timestamp: 1, results: [{ type: 'ADD', data: false }],
    }));
    expect(screen.getByRole('alert').textContent).toContain('Showing last known data');
    expect(screen.getByText('10')).not.toBeNull();
    fireEvent.click(screen.getByRole('button', { name: 'Retry query' }));
    await waitFor(() => expect(screen.queryByRole('alert')).toBeNull());
    expect(table.factory.instances).toHaveLength(1);
    expect(table.factory.instances[0].closed).toBe(false);
  });

  it('supports computed columns, typed actions and non-visible default sorts for non-indexed row interfaces', async () => {
    interface Device {
      identity: { code: string };
      rank: number;
      units: number;
      locked: boolean;
      pending: boolean;
    }

    expectTypeOf<Parameters<NonNullable<ColumnDef<Device>['format']>>>()
      .toEqualTypeOf<[unknown, Device]>();
    expectTypeOf<Parameters<Exclude<ColumnDef<Device>['className'], string | undefined>>>()
      .toEqualTypeOf<[unknown, Device]>();
    const onInspect = vi.fn<(row: Device) => void>();
    const onSort = vi.fn<(sort: SortConfig | null) => void>();
    const format = vi.fn((value: unknown, row: Device) => {
      expect(value).toBeUndefined();
      return `${row.identity.code}: ${row.units} units`;
    });
    const deviceColumns: ColumnDef<Device>[] = [{
      key: 'computed-summary',
      label: 'Device',
      format,
      className: (value, row) => {
        expectTypeOf(value).toEqualTypeOf<unknown>();
        return row.locked ? 'locked-device' : 'available-device';
      },
      sortable: false,
    }, { key: 'units', label: 'Units' }];
    const actions: RowAction<Device>[] = [{
      icon: '?', label: 'Inspect', onClick: onInspect,
      disabled: row => row.locked, loading: row => row.pending,
    }];
    const table = renderTable<Device>({
      queryId: 'stocks',
      columns: deviceColumns,
      actions,
      rowKey: row => row.identity.code,
      queryOptions: {
        getKey: row => String(row.code),
        transform: row => ({
          identity: { code: String(row.code) },
          rank: Number(row.rank),
          units: Number(row.units),
          locked: row.locked === true,
          pending: row.pending === true,
        }),
      },
      defaultSort: { column: 'rank', direction: 'desc' },
      onSortChange: onSort,
      renderRow: (row, renderedColumns, animation, defaultRender) => {
        expectTypeOf(row).toEqualTypeOf<Device>();
        expectTypeOf(renderedColumns).toEqualTypeOf<readonly ColumnDef<Device>[]>();
        expectTypeOf(animation).toEqualTypeOf<AnimationDirection>();
        return defaultRender();
      },
    }, [
      { code: 'rack-a', rank: 2, units: 10 },
      { code: 'rack-b', rank: 3, units: 2, locked: true },
      { code: 'rack-c', rank: 1, units: 30, pending: true },
    ]);
    await table.connect();
    expect(screen.getAllByRole('row').slice(1).map(row => row.textContent))
      .toEqual(['rack-b: 2 units2?', 'rack-a: 10 units10?', 'rack-c: 30 units30']);
    expect(screen.getByText('rack-b: 2 units').classList.contains('locked-device')).toBe(true);

    const buttons = screen.getAllByRole('button', { name: 'Inspect' });
    fireEvent.click(buttons[0]);
    fireEvent.click(buttons[2]);
    expect(onInspect).not.toHaveBeenCalled();
    fireEvent.click(buttons[1]);
    expect(onInspect).toHaveBeenCalledWith({
      identity: { code: 'rack-a' }, rank: 2, units: 10, locked: false, pending: false,
    });

    fireEvent.click(screen.getByRole('columnheader', { name: 'Device' }));
    expect(onSort).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('columnheader', { name: 'Units' }));
    expect(onSort).toHaveBeenLastCalledWith({ column: 'units', direction: 'asc' });
    expect(format).toHaveBeenCalled();
  });

  it('orders numbers before text and preserves null ordering without coercing raw cells', async () => {
    interface Row { code: string; value: unknown }
    const table = renderTable<Row>({
      queryId: 'stocks',
      columns: [{ key: 'code', label: 'Code' }, { key: 'value', label: 'Value' }],
      rowKey: row => row.code,
      queryOptions: { getKey: row => String(row.code), transform: row => ({ code: String(row.code), value: row.value }) },
      defaultSort: { column: 'value', direction: 'asc' },
    }, [
      { code: 'two', value: '2' },
      { code: 'ten', value: '10' },
      { code: 'numeric', value: 3 },
      { code: 'null', value: null },
      { code: 'missing' },
    ]);
    await table.connect();
    const codes = () => screen.getAllByRole('row').slice(1)
      .map(row => within(row).getAllByRole('cell')[0].textContent);
    expect(codes()).toEqual(['numeric', 'ten', 'two', 'null', 'missing']);
    expect(screen.getAllByText('-')).toHaveLength(2);
    fireEvent.click(screen.getByRole('columnheader', { name: 'Value' }));
    expect(codes()).toEqual(['null', 'missing', 'two', 'ten', 'numeric']);
  });
});
