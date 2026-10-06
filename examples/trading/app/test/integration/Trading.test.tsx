// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { act, fireEvent, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ALL_QUERIES } from '../../src/services/queries';
import { FIXED_TIME, QUERY_IDS } from '../fixtures/synthetic/trading';
import { panel, renderTrading, row, summary, symbols } from './renderTrading';

beforeEach(() => {
  vi.useFakeTimers({ toFake: ['Date'] });
  vi.setSystemTime(new Date(FIXED_TIME));
  // DOM tests do not model layout/frames. Real frames and CSS are tested in Playwright.
  vi.spyOn(window, 'requestAnimationFrame').mockReturnValue(1);
  vi.spyOn(window, 'cancelAnimationFrame').mockImplementation(() => {});
});

afterEach(() => vi.useRealTimers());

describe('Trading with the built @drasi/react dependency and synthetic transport', () => {
  it('automatically provisions all 11 queries and one reaction, then reloads existing resources', async () => {
    const first = await renderTrading();
    expect(screen.getAllByRole('table')).toHaveLength(7);
    expect([...first.backend.queries.keys()]).toEqual(QUERY_IDS);
    for (const query of ALL_QUERIES) {
      expect(first.backend.queries.get(query.id)).toEqual({
        id: query.id, query: query.query, sources: query.sources, joins: query.joins,
        queryLanguage: 'Cypher', autoStart: true,
      });
    }
    expect(first.backend.reaction).toEqual({
      id: 'sse-stream', kind: 'sse', queries: QUERY_IDS, autoStart: true,
      host: '0.0.0.0', port: 8281, ssePath: '/events', heartbeatIntervalMs: 15000,
    });
    first.unmount();
    expect(first.sources.every(source => source.closed)).toBe(true);
    first.backend.requests.length = 0;

    await renderTrading(first.backend);
    expect(first.backend.requests.every(request => request.method === 'GET')).toBe(true);
    expect(symbols('Watchlist')).toEqual(['AAPL', 'MSFT']);
    expect(summary('Total Value')).toBe('$2,000.00');
  });

  it('adds/removes watchlist rows through the Trading API and streamed changes, not optimistic mutation', async () => {
    const { backend } = await renderTrading();
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: 'Add to watchlist' }));
    await user.selectOptions(await screen.findByRole('combobox'), 'NVDA');
    await user.click(screen.getByRole('button', { name: 'Add' }));
    await waitFor(() => expect(symbols('Watchlist')).toEqual(['AAPL', 'MSFT', 'NVDA']));
    expect(backend.requests).toContainEqual({
      method: 'POST', path: '/api/watchlist', body: { symbol: 'NVDA' },
    });
    await user.click(within(row('Watchlist', 'NVDA')).getByRole('button', { name: 'Remove from watchlist' }));
    await waitFor(() => expect(symbols('Watchlist')).toEqual(['AAPL', 'MSFT']));

    backend.failure = { method: 'DELETE', path: '/api/watchlist/AAPL', status: 500, error: 'Write rejected' };
    await user.click(within(row('Watchlist', 'AAPL')).getByRole('button', { name: 'Remove from watchlist' }));
    expect(await screen.findByText('Failed to remove from watchlist: Write rejected')).not.toBeNull();
    expect(symbols('Watchlist')).toEqual(['AAPL', 'MSFT']);
  });

  it('reports duplicate-watchlist errors without hiding the dialog or changing rows', async () => {
    await renderTrading();
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: 'Add to watchlist' }));
    await screen.findByRole('combobox');
    await user.click(screen.getByRole('button', { name: 'Add' }));
    expect(await screen.findByText('Failed to add to watchlist: AAPL already in watchlist')).not.toBeNull();
    expect(screen.getByRole('heading', { name: 'Add to Watchlist' })).not.toBeNull();
    expect(symbols('Watchlist')).toEqual(['AAPL', 'MSFT']);
    await user.click(screen.getByRole('button', { name: 'Cancel' }));
  });

  it('validates, adds, edits and deletes portfolio positions and recomputes all four summaries', async () => {
    const { backend } = await renderTrading();
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: 'Add position' }));
    await screen.findByRole('combobox');
    await user.click(screen.getByRole('button', { name: 'Add' }));
    expect(screen.getByText('Quantity is required')).not.toBeNull();
    expect(screen.getByText('Purchase price is required')).not.toBeNull();
    expect(backend.positions).toHaveLength(2);

    await user.selectOptions(screen.getByRole('combobox'), 'NVDA');
    await user.type(screen.getByPlaceholderText('e.g., 100'), '2');
    await user.type(screen.getByPlaceholderText('e.g., 150.00'), '100');
    await user.click(screen.getByRole('button', { name: 'Add' }));
    await waitFor(() => expect(symbols('Portfolio')).toEqual(['AAPL', 'MSFT', 'NVDA']));
    expect(summary('Total Value')).toBe('$2,240.00');
    expect(summary('Total Cost')).toBe('$2,000.00');
    expect(summary('Total P/L')).toBe('$240.00');
    expect(summary('Total Return')).toBe('+12.00%');

    await user.click(within(row('Portfolio', 'NVDA')).getByRole('button', { name: 'Edit position' }));
    const quantity = await screen.findByPlaceholderText('e.g., 100');
    await user.clear(quantity);
    await user.type(quantity, '3');
    const cost = screen.getByPlaceholderText('e.g., 150.00');
    await user.clear(cost);
    await user.type(cost, '110');
    await user.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(summary('Total Value')).toBe('$2,360.00'));
    expect(summary('Total Cost')).toBe('$2,130.00');
    expect(summary('Total P/L')).toBe('$230.00');
    expect(summary('Total Return')).toBe('+10.80%');
    expect(backend.requests).toContainEqual({
      method: 'PUT', path: '/api/portfolio/3',
      body: { quantity: 3, purchasePrice: 110, purchaseDate: '2026-01-15' },
    });

    await user.click(within(row('Portfolio', 'NVDA')).getByRole('button', { name: 'Delete position' }));
    expect(await screen.findByText('3 shares')).not.toBeNull();
    await user.click(screen.getByRole('button', { name: 'Delete' }));
    await waitFor(() => expect(symbols('Portfolio')).toEqual(['AAPL', 'MSFT']));
    expect(summary('Total Value')).toBe('$2,000.00');
    expect(summary('Total Cost')).toBe('$1,800.00');
  });

  it('creates and cancels limit orders, preserving expiry and buy/sell payloads', async () => {
    const { backend } = await renderTrading();
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: 'New limit order' }));
    await user.selectOptions(await screen.findByRole('combobox'), 'NVDA');
    await user.click(screen.getByRole('button', { name: 'Sell' }));
    await user.type(screen.getByPlaceholderText('e.g., 150.00'), '125');
    await user.type(screen.getByPlaceholderText('e.g., 100'), '2');
    await user.click(screen.getByRole('button', { name: 'Create Order' }));
    await waitFor(() => expect(symbols('Limit Orders')[0]).toBe('NVDA'));
    expect(row('Limit Orders', 'NVDA').textContent).toContain('$125.00');
    expect(row('Limit Orders', 'NVDA').textContent).toContain('+4.17%');
    expect(backend.requests).toContainEqual({
      method: 'POST', path: '/api/orders',
      body: {
        symbol: 'NVDA', orderType: 'sell', targetPrice: 125, quantity: 2,
        expiresAt: '2026-01-15T12:01:00.000Z', staleDuration: 30, expireDuration: 60,
      },
    });
    await user.click(within(row('Limit Orders', 'NVDA')).getByRole('button', { name: 'Cancel order' }));
    expect(screen.getByText('2 shares')).not.toBeNull();
    await user.click(screen.getByRole('button', { name: 'Cancel Order' }));
    await waitFor(() => expect(symbols('Limit Orders')).toEqual(['AMD', 'AMZN']));
  });

  it('updates prices, financial summaries, sectors, movers, ticker and value-change classes without refresh', async () => {
    const { backend } = await renderTrading();
    expect(summary('Total P/L')).toBe('$200.00');
    expect(row('Sector Performance', 'Technology').textContent).toContain('60.0M');
    act(() => {
      vi.setSystemTime(new Date(new Date(FIXED_TIME).getTime() + 600));
      backend.changePrice('AAPL', 90, 9_000_000);
    });
    await waitFor(() => expect(row('Watchlist', 'AAPL').textContent).toContain('$90.00'));
    expect(row('Watchlist', 'AAPL').classList.contains('drasi-row--down')).toBe(true);
    expect(summary('Total Value')).toBe('$1,800.00');
    expect(summary('Total P/L')).toBe('$0.00');
    expect(symbols('Top Gainers')).not.toContain('AAPL');
    expect(symbols('Top Losers')).toContain('AAPL');
    expect(symbols('High Volume')).not.toContain('AAPL');
    expect(row('Sector Performance', 'Technology').textContent).toContain('57.0M');
    expect(document.querySelector('.stock-ticker')?.textContent).toContain('$90.00');
    act(() => backend.changePrice('AAPL', 115));
    expect(row('Watchlist', 'AAPL').classList.contains('drasi-row--up')).toBe(true);
  });

  it('recovers changed and deleted rows after a transport interruption without remounting', async () => {
    const { backend, sources } = await renderTrading();
    act(() => {
      sources[0].fail();
      backend.changePrice('AAPL', 125);
      backend.handle({ method: 'DELETE', path: '/api/watchlist/MSFT' });
    });
    expect(screen.getByText('Reconnecting...')).not.toBeNull();
    expect(symbols('Watchlist')).toEqual(['AAPL', 'MSFT']);
    await waitFor(() => expect(sources).toHaveLength(2));
    await screen.findByText('Connected');
    await waitFor(() => expect(symbols('Watchlist')).toEqual(['AAPL']));
    expect(row('Watchlist', 'AAPL').textContent).toContain('$125.00');
    expect(backend.requests.filter(request => request.path === '/api/v1/instances/trading-server/queries/watchlist-query/results')).toHaveLength(2);
  });

  it('characterizes the known pre-extraction default-sort discrepancy, then honors explicit user sorting', async () => {
    await renderTrading();
    expect(symbols('Top Gainers')).toEqual(['NVDA', 'AAPL', 'AMZN']);
    // Compatibility, not endorsement: KB-01 in the versioned behavior inventory.
    expect(symbols('Top Losers')).toEqual(['AMD', 'MSFT']);
    expect(symbols('High Volume')).toEqual(['AAPL', 'AMZN', 'MSFT']);
    const losers = within(panel('Top Losers')).getByRole('columnheader', { name: 'Change' });
    fireEvent.click(losers);
    expect(symbols('Top Losers')).toEqual(['MSFT', 'AMD']);
    expect(losers.getAttribute('aria-sort')).toBe('ascending');
    const volume = within(panel('High Volume')).getByRole('columnheader', { name: 'Volume' });
    fireEvent.click(volume);
    fireEvent.keyDown(volume, { key: 'Enter' });
    expect(symbols('High Volume')).toEqual(['MSFT', 'AMZN', 'AAPL']);
    expect(volume.getAttribute('aria-sort')).toBe('descending');
    expect(symbols('High Volume')).not.toContain('AMD');
  });
});
