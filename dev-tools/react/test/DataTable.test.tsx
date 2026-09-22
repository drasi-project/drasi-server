// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { StrictMode, createRef, useState } from 'react';
import { act, fireEvent, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, expectTypeOf, it, vi } from 'vitest';
import {
  DataTable, type ColumnDef, type DataTableProps, type DataTableState, type SortConfig,
} from '../src/components';

interface Rack {
  code: string;
  units: number;
  rank: number;
  locked?: boolean;
  pending?: boolean;
}
const rows: readonly Rack[] = Object.freeze([
  Object.freeze({ code: 'b', units: 12, rank: 1 }),
  Object.freeze({ code: 'a', units: 3, rank: 2, locked: true }),
  Object.freeze({ code: 'c', units: 20, rank: 0, pending: true }),
]);
const columns: readonly ColumnDef<Rack>[] = [
  { key: 'code', label: 'Rack', width: '5rem' },
  { key: 'units', label: 'Units', align: 'right' },
];
const props: DataTableProps<Rack> = { rows, columns, rowKey: row => row.code };
const codes = () => screen.getAllByRole('row').slice(1)
  .map(row => within(row).getAllByRole('cell')[0].textContent);

describe('provider-free DataTable', () => {
  it('renders non-Trading readonly rows without a provider, network or tutorial controls', () => {
    const fetcher = vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('No network allowed'));
    const ref = createRef<HTMLDivElement>();
    render(<DataTable {...props} containerRef={ref} title="Warehouse" defaultSort={{ column: 'rank', direction: 'desc' }} />);
    expect(codes()).toEqual(['a', 'b', 'c']);
    expect(rows.map(row => row.code)).toEqual(['b', 'a', 'c']);
    expect(ref.current?.contains(screen.getByRole('table'))).toBe(true);
    expect(screen.getByRole('columnheader', { name: 'Rack' }).style.width).toBe('5rem');
    expect(screen.queryByRole('button')).toBeNull();
    expect(fetcher).not.toHaveBeenCalled();
  });

  it('shares typed computed cells, actions and header composition without a query schema', async () => {
    const inspect = vi.fn<(row: Rack) => void>();
    const onSortChange = vi.fn();
    const action = { icon: '?', label: 'Inspect rack', onClick: inspect, disabled: (row: Rack) => row.locked === true, loading: (row: Rack) => row.pending === true };
    render(<DataTable
      {...props}
      title="Warehouse"
      headerActions={<button>Add rack</button>}
      headerControls={<button>Export</button>}
      headerSlot={<span>Capacity</span>}
      actions={[action]}
      actionsWidth="3rem"
      onSortChange={onSortChange}
      rowClassName={(row, index) => `${row.code}-${index}`}
      columns={[{
        key: 'summary', label: 'Rack', sortable: false, align: 'center',
        format: (value, row) => {
          expectTypeOf(value).toEqualTypeOf<unknown>();
          expectTypeOf(row).toEqualTypeOf<Rack>();
          expect(value).toBeUndefined();
          return `${row.code}: ${row.units}`;
        },
        className: (_value, row) => row.code,
        headerClassName: 'rack-heading',
      }, columns[1]]}
      renderHeader={({ defaultRender, rows: view, setSort, sort }) => (
        <>{defaultRender()}<button onClick={() => setSort(null)}>Clear {sort?.column ?? 'none'} ({view?.length})</button></>
      )}
    />);
    const user = userEvent.setup();
    expect(screen.getByText('b: 12').className).toContain('drasi-align--center');
    expect(screen.getByText('b: 12').closest('tr')?.className).toContain('b-0');
    expect(screen.getByRole('columnheader', { name: 'Rack' }).className).toContain('rack-heading');
    expect(screen.getByText('Capacity').parentElement?.className).toContain('header-slot');
    expect(screen.getByRole('button', { name: 'Export' }).parentElement?.className).toContain('header-controls');
    const buttons = screen.getAllByRole('button', { name: 'Inspect rack' });
    await user.click(buttons[0]);
    await user.click(buttons[1]);
    await user.click(buttons[2]);
    expect(inspect).toHaveBeenCalledExactlyOnceWith(rows[0]);
    expect(buttons[1].hasAttribute('disabled')).toBe(true);
    expect(buttons[2].querySelector('.drasi-spinner--small')).not.toBeNull();
    fireEvent.click(screen.getByRole('columnheader', { name: 'Rack' }));
    fireEvent.keyDown(screen.getByRole('columnheader', { name: 'Rack' }), { key: ' ' });
    fireEvent.keyDown(screen.getByRole('columnheader', { name: 'Units' }), { key: 'ArrowDown' });
    expect(onSortChange).not.toHaveBeenCalled();
    await user.click(screen.getByRole('button', { name: 'Clear none (3)' }));
    expect(onSortChange).toHaveBeenCalledExactlyOnceWith(null);
  });

  it('has controlled precedence and only requests changes until its owner applies them', async () => {
    const onSortChange = vi.fn<(sort: SortConfig | null) => void>();
    const controlled = { ...props, defaultSort: { column: 'rank', direction: 'desc' } as const, onSortChange };
    const clear = ({ setSort }: { setSort: (sort: SortConfig | null) => void }) => <button onClick={() => setSort(null)}>Clear</button>;
    const table = render(<StrictMode><DataTable {...controlled} sort={{ column: 'code', direction: 'asc' }} renderHeader={clear} /></StrictMode>);
    expect(codes()).toEqual(['a', 'b', 'c']);
    fireEvent.click(screen.getByRole('columnheader', { name: 'Units' }));
    expect(onSortChange).toHaveBeenCalledExactlyOnceWith({ column: 'units', direction: 'asc' });
    expect(codes()).toEqual(['a', 'b', 'c']);
    table.rerender(<StrictMode><DataTable {...controlled} sort={{ column: 'units', direction: 'desc' }} renderHeader={clear} /></StrictMode>);
    expect(codes()).toEqual(['c', 'b', 'a']);
    expect(onSortChange).toHaveBeenCalledTimes(1);
    await userEvent.setup().click(screen.getByRole('button', { name: 'Clear' }));
    expect(onSortChange).toHaveBeenLastCalledWith(null);
    expect(codes()).toEqual(['c', 'b', 'a']);
    table.rerender(<StrictMode><DataTable {...controlled} sort={null} renderHeader={clear} /></StrictMode>);
    expect(codes()).toEqual(['b', 'a', 'c']);
    expect(screen.getByRole('columnheader', { name: 'Units' }).hasAttribute('aria-sort')).toBe(false);
    expect(onSortChange).toHaveBeenCalledTimes(2);
  });

  it('uses defaultSort only on mount, toggles by keyboard and can clear uncontrolled sorting', () => {
    const onSortChange = vi.fn();
    const renderHeader: DataTableProps<Rack>['renderHeader'] = ({ setSort }) => <button onClick={() => setSort(null)}>Clear</button>;
    const table = render(<StrictMode><DataTable {...props} defaultSort={{ column: 'units', direction: 'desc' }} onSortChange={onSortChange} renderHeader={renderHeader} /></StrictMode>);
    expect(codes()).toEqual(['c', 'b', 'a']);
    table.rerender(<StrictMode><DataTable {...props} defaultSort={{ column: 'code', direction: 'asc' }} onSortChange={onSortChange} renderHeader={renderHeader} /></StrictMode>);
    expect(codes()).toEqual(['c', 'b', 'a']);
    expect(onSortChange).not.toHaveBeenCalled();
    const header = screen.getByRole('columnheader', { name: 'Rack' });
    fireEvent.click(header);
    fireEvent.keyDown(header, { key: 'Enter' });
    expect(codes()).toEqual(['c', 'b', 'a']);
    fireEvent.keyDown(header, { key: ' ' });
    expect(codes()).toEqual(['a', 'b', 'c']);
    fireEvent.click(screen.getByRole('button', { name: 'Clear' }));
    expect(codes()).toEqual(['b', 'a', 'c']);
    expect(onSortChange.mock.calls).toEqual([
      [{ column: 'code', direction: 'asc' }], [{ column: 'code', direction: 'desc' }],
      [{ column: 'code', direction: 'asc' }], [null],
    ]);
  });

  it.each([
    { values: [12, 3, -1, 3], expected: ['2', '1', '3', '0'] },
    { values: ['b', 'a', 'c', 'a'], expected: ['1', '3', '0', '2'] },
    { values: [null, '10', undefined, 2, '2'], expected: ['3', '1', '4', '0', '2'] },
  ])('preserves numeric/string/null/mixed comparisons and stable ties: $values', ({ values, expected }) => {
    render(<DataTable
      rows={Object.freeze(values.map((value, index) => Object.freeze({ code: String(index), value })))}
      columns={[{ key: 'code', label: 'Code' }, { key: 'value', label: 'Value' }]}
      rowKey={row => row.code}
      defaultSort={{ column: 'value', direction: 'asc' }}
    />);
    expect(codes()).toEqual(expected);
    fireEvent.click(screen.getByRole('columnheader', { name: 'Value' }));
    expect(screen.getByRole('columnheader', { name: 'Value' }).getAttribute('aria-sort')).toBe('descending');
    if (values.includes(null)) expect(codes().slice(0, 2)).toEqual(['0', '2']);
  });

  it('keeps custom row state keyed by identity across reordering and supplied animation', () => {
    function RackRow({ row }: { row: Rack }) {
      const [note, setNote] = useState(row.code);
      return <tr><td>{row.code}<input aria-label={`Note ${row.code}`} value={note} onChange={event => setNote(event.target.value)} /></td></tr>;
    }
    const table = render(<DataTable {...props} renderRow={row => <RackRow row={row} />} />);
    fireEvent.change(screen.getByRole('textbox', { name: 'Note b' }), { target: { value: 'keep me' } });
    table.rerender(<DataTable {...props} rows={[...rows].reverse()} renderRow={row => <RackRow row={row} />} />);
    expect(screen.getByRole('textbox', { name: 'Note b' }).getAttribute('value')).toBe('keep me');
    expect(screen.getAllByRole('row')[1].textContent).toBe('c');
  });

  it('supports defaultRender, external animation precedence and numeric/string change animation', () => {
    vi.useFakeTimers();
    const renderRow: DataTableProps<Rack>['renderRow'] = (row, renderedColumns, animation, defaultRender) => {
      expectTypeOf(row).toEqualTypeOf<Rack>();
      expectTypeOf(renderedColumns).toEqualTypeOf<readonly ColumnDef<Rack>[]>();
      expect([null, 'up', 'down', 'change']).toContain(animation);
      return defaultRender();
    };
    const table = render(<DataTable {...props} rows={[rows[0]]} animateOnChange="units" renderRow={renderRow} rowClassName="custom-row" />);
    table.rerender(<DataTable {...props} rows={[{ ...rows[0], units: 15 }]} animateOnChange="units" renderRow={renderRow} />);
    expect(screen.getAllByRole('row')[1].className).toContain('drasi-row--up');
    table.rerender(<DataTable {...props} rows={[{ ...rows[0], units: 8 }]} animateOnChange="units" renderRow={renderRow} />);
    expect(screen.getAllByRole('row')[1].className).toContain('drasi-row--down');
    table.rerender(<DataTable {...props} rows={[rows[0]]} animateOnChange="units" rowAnimations={new Map([['b', 'change']])} />);
    expect(screen.getAllByRole('row')[1].className).toContain('drasi-row--change');
    table.rerender(<DataTable {...props} rows={[]} animateOnChange="units" />);
    act(() => vi.advanceTimersByTime(501));
    expect(screen.queryByText('b')).toBeNull();
    table.unmount();
    vi.useRealTimers();
  });
});

describe('explicit presentation states', () => {
  class InventoryError extends Error { readonly code = 'WAREHOUSE_OFFLINE'; }

  it('renders a default state card with a named loading indicator only before rows arrive', () => {
    const table = render(<DataTable {...props} rows={null} title="Warehouse" state={{ loading: true }} />);
    expect(screen.getByRole('status', { name: 'Loading' })).not.toBeNull();
    expect(screen.queryByRole('table')).toBeNull();
    expect(screen.getByRole('heading', { name: 'Warehouse' }).className).toContain('state-title');
    table.rerender(<DataTable {...props} state={{ loading: true }} />);
    expect(screen.getByRole('table')).not.toBeNull();
    expect(screen.getByRole('status', { name: 'Loading' })).not.toBeNull();
  });

  it('prioritizes a typed error over loading, exposes retry and escapes supplied text', () => {
    const error = new InventoryError('<script>not HTML</script>');
    const retry = vi.fn();
    const state = { error, loading: true, retry, retryLabel: 'Reconnect inventory' };
    const table = render(<DataTable<Rack, InventoryError> {...props} rows={null} state={state} />);
    expect(screen.getByRole('alert').textContent).toContain(error.message);
    expect(screen.queryByRole('status')).toBeNull();
    expect(document.querySelector('script')).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: 'Reconnect inventory' }));
    expect(retry).toHaveBeenCalledTimes(1);
    table.rerender(<DataTable<Rack, InventoryError>
      {...props} rows={null} state={state}
      renderError={context => {
        expectTypeOf(context.error).toEqualTypeOf<InventoryError>();
        expect(context.error).toBe(error);
        return <button onClick={context.state.retry}>{context.error.code}</button>;
      }}
    />);
    fireEvent.click(screen.getByRole('button', { name: 'WAREHOUSE_OFFLINE' }));
    expect(retry).toHaveBeenCalledTimes(2);
  });

  it('retains useful or empty rows with error, stale and loading precedence', () => {
    const loading = vi.fn(() => <span>Refreshing inventory</span>);
    const stale = vi.fn(() => <span>Retained inventory</span>);
    const error = new Error('Unavailable');
    const table = render(<DataTable {...props} state={{ loading: true }} renderLoading={loading} renderStale={stale} />);
    expect(codes()).toEqual(['b', 'a', 'c']);
    expect(screen.getByText('Refreshing inventory')).not.toBeNull();
    table.rerender(<DataTable {...props} state={{ loading: true, stale: true }} renderLoading={loading} renderStale={stale} />);
    expect(screen.getByText('Retained inventory')).not.toBeNull();
    expect(screen.queryByText('Refreshing inventory')).toBeNull();
    table.rerender(<DataTable {...props} state={{ loading: true, stale: true, error }} renderLoading={loading} renderStale={stale} />);
    expect(screen.getByRole('alert').textContent).toContain('Showing last known data.');
    expect(screen.queryByText('Retained inventory')).toBeNull();
    expect(codes()).toEqual(['b', 'a', 'c']);
    table.rerender(<DataTable {...props} rows={[]} state={{ error, stale: true }} />);
    expect(screen.getByRole('alert')).not.toBeNull();
    expect(screen.getByText('No data available')).not.toBeNull();
    expect(screen.queryByRole('button', { name: 'Retry' })).toBeNull();
  });

  it('lets empty/header/state renderers see the typed sorted view and intentional null slots', () => {
    const table = render(<DataTable
      {...props} rows={[]} actions={[{ icon: '?', label: 'Inspect', onClick: () => {} }]}
      emptyMessage="unused"
      renderEmpty={({ rows: view, state, sort }) => {
        expect(view).toEqual([]);
        expect(state).toEqual({});
        expect(sort).toBeNull();
        return <strong>Nothing in inventory</strong>;
      }}
    />);
    expect(screen.getByText('Nothing in inventory').closest('td')?.colSpan).toBe(3);
    expect(screen.queryByText('unused')).toBeNull();
    table.rerender(<DataTable {...props} rows={null} renderEmpty={() => null} />);
    expect(screen.queryByText('No data available')).toBeNull();
    table.rerender(<DataTable {...props} rows={null} state={{ loading: true }} renderLoading={() => null} />);
    expect(screen.queryByRole('status')).toBeNull();
    table.rerender(<DataTable {...props} rows={null} state={{ error: new Error('hidden') }} renderError={() => null} />);
    expect(screen.queryByRole('alert')).toBeNull();
    table.rerender(<DataTable {...props} state={{ stale: true }} renderStale={() => null} />);
    expect(screen.queryByRole('status')).toBeNull();
    expect(screen.getByRole('table')).not.toBeNull();
  });

  it('supplies generic fallback retry/empty/stale labels without query knowledge', () => {
    const state: DataTableState = { stale: true };
    const table = render(<DataTable {...props} state={state} />);
    expect(screen.getByRole('status').textContent).toBe('Showing last known data.');
    table.rerender(<DataTable {...props} state={{ ...state, staleMessage: 'Waiting for inventory sync' }} />);
    expect(screen.getByRole('status').textContent).toBe('Waiting for inventory sync');
    const retry = vi.fn();
    table.rerender(<DataTable {...props} state={{ error: new Error('Offline'), retry }} />);
    fireEvent.click(screen.getByRole('button', { name: 'Retry' }));
    expect(retry).toHaveBeenCalledTimes(1);
    table.rerender(<DataTable {...props} rows={null} emptyMessage="No racks" />);
    expect(screen.getByText('No racks')).not.toBeNull();
  });
});
