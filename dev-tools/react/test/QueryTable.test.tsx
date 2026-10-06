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
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const hooks = vi.hoisted(() => ({
  useDrasiQuery: vi.fn(),
  useDrasiQueryDefinition: vi.fn(),
  useDrasiServerUiUrl: vi.fn(),
}));

vi.mock('../src/react/DrasiContext', () => hooks);

import { QueryTable } from '../src/components/QueryTable';

interface Stock {
  symbol: string;
  price: number;
}

const columns = [
  { key: 'symbol' as const, label: 'Symbol', width: '5rem' },
  { key: 'price' as const, label: 'Price' },
];

beforeEach(() => {
  hooks.useDrasiQueryDefinition.mockReturnValue({
    config: {},
    loading: false,
    error: null,
  });
  hooks.useDrasiServerUiUrl.mockReturnValue(null);
});

describe('QueryTable', () => {
  it('uses rowKey for accumulation and supports accessible sorting', () => {
    hooks.useDrasiQuery.mockReturnValue({
      data: [
        { symbol: 'MSFT', price: 12 },
        { symbol: 'AAPL', price: 10 },
      ],
      loading: false,
      error: null,
      lastUpdate: null,
    });
    const rowKey = (row: Stock) => row.symbol;

    render(
      <QueryTable<Stock>
        queryId="stocks"
        columns={columns}
        rowKey={rowKey}
        defaultSort={{ column: 'symbol', direction: 'asc' }}
      />,
    );

    const queryOptions = hooks.useDrasiQuery.mock.calls[0][1];
    expect(queryOptions.getKey({ symbol: 'AAPL', price: 10 })).toBe('AAPL');

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

  it('renders query errors instead of an empty table', () => {
    hooks.useDrasiQuery.mockReturnValue({
      data: null,
      loading: false,
      error: 'snapshot failed',
      lastUpdate: null,
    });

    render(
      <QueryTable<Stock>
        queryId="stocks"
        columns={columns}
        rowKey={(row) => row.symbol}
      />,
    );

    expect(screen.getByText('Error: snapshot failed')).not.toBeNull();
    expect(screen.queryByRole('table')).toBeNull();
  });

  it('restores page scrolling when an expanded table enters an error state', async () => {
    hooks.useDrasiQuery.mockReturnValue({
      data: [{ symbol: 'AAPL', price: 10 }],
      loading: false,
      error: null,
      lastUpdate: null,
    });
    const requestFrame = vi
      .spyOn(window, 'requestAnimationFrame')
      .mockReturnValue(1);
    const cancelFrame = vi
      .spyOn(window, 'cancelAnimationFrame')
      .mockImplementation(() => {});

    const rendered = render(
      <QueryTable<Stock>
        queryId="stocks"
        title="Stocks"
        columns={columns}
        rowKey={(row) => row.symbol}
      />,
    );
    fireEvent.click(screen.getByRole('button', { name: 'Expand table' }));
    expect(document.body.style.overflow).toBe('hidden');

    hooks.useDrasiQuery.mockReturnValue({
      data: null,
      loading: false,
      error: 'snapshot failed',
      lastUpdate: null,
    });
    rendered.rerender(
      <QueryTable<Stock>
        queryId="stocks"
        title="Stocks"
        columns={columns}
        rowKey={(row) => row.symbol}
      />,
    );

    await waitFor(() => expect(document.body.style.overflow).toBe(''));
    requestFrame.mockRestore();
    cancelFrame.mockRestore();
  });
});
