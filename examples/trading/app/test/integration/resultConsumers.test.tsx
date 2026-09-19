// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { act, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { tradingQueryOptions } from '../../src/drasi/queryOptions';
import { SyntheticTrading } from '../fixtures/synthetic/trading';
import { panel, renderTrading, symbols } from './renderTrading';

beforeEach(() => {
  vi.spyOn(window, 'requestAnimationFrame').mockReturnValue(1);
  vi.spyOn(window, 'cancelAnimationFrame').mockImplementation(() => {});
});

describe('Trading consumers of the built result contract', () => {
  it('removes an id-only portfolio delete before a transform that needs the full projected row', async () => {
    const options = tradingQueryOptions('portfolio-query');
    const project = options.transform;
    const transform = vi.spyOn(options, 'transform').mockImplementation(raw => {
      if (raw.symbol === undefined) throw new Error('A delete must not need projection fields');
      return project(raw);
    });
    const { backend } = await renderTrading();
    expect(symbols('Portfolio')).toEqual(['AAPL', 'MSFT']);
    transform.mockClear();

    act(() => backend.onBatch({
      queryId: 'portfolio-query', timestamp: Date.now(),
      results: [{ type: 'DELETE', data: { id: 1 } }],
    }));

    expect(symbols('Portfolio')).toEqual(['MSFT']);
    expect(transform).toHaveBeenCalled();
    expect(transform.mock.calls.every(([raw]) => raw.symbol === 'MSFT')).toBe(true);
    expect(within(panel('Portfolio')).queryByRole('alert')).toBeNull();
  });

  it('keeps duplicate-symbol portfolio render identities aligned with their raw position IDs', async () => {
    const backend = new SyntheticTrading();
    backend.positions.push({
      id: 3, symbol: 'AAPL', quantity: 2, purchase_price: 90, purchase_date: '2024-01-22T10:00:00',
    });
    await renderTrading(backend);
    expect(symbols('Portfolio')).toEqual(['AAPL', 'AAPL', 'MSFT']);
    expect(within(panel('Portfolio')).getAllByRole('row', { name: /^AAPL / })).toHaveLength(2);

    act(() => backend.onBatch({
      queryId: 'portfolio-query', timestamp: Date.now(),
      results: [{ type: 'DELETE', data: { id: 1 } }],
    }));

    expect(symbols('Portfolio')).toEqual(['AAPL', 'MSFT']);
    const survivingPosition = within(panel('Portfolio')).getByRole('row', { name: /^AAPL / });
    expect(within(survivingPosition).getAllByRole('cell')[2].textContent).toBe('2');
    expect(survivingPosition.textContent).toContain('$220.00');
  });

  it('uses symbol-only deletes without trying to validate a full stock projection', async () => {
    const options = tradingQueryOptions('watchlist-query');
    const transform = vi.spyOn(options, 'transform');
    const { backend } = await renderTrading();
    transform.mockClear();
    act(() => backend.onBatch({
      queryId: 'watchlist-query', timestamp: Date.now(),
      results: [{ type: 'DELETE', data: { symbol: 'AAPL' } }],
    }));
    expect(symbols('Watchlist')).toEqual(['MSFT']);
    expect(transform.mock.calls.every(([raw]) => raw.symbol === 'MSFT' && raw.name !== undefined)).toBe(true);
    expect(within(panel('Watchlist')).queryByRole('alert')).toBeNull();
  });
});
