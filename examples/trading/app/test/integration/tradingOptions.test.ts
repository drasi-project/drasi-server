// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { describe, expect, it, vi } from 'vitest';
import { routeTradingData, DRASI_SERVER_URL, TRADING_REACTION } from '../../src/drasi/config';
import { tradingQueryOptions } from '../../src/drasi/queryOptions';
import { ALL_QUERIES, HAS_PRICE, ON_WATCHLIST, ORDER_HAS_PRICE, OWNS_STOCK } from '../../src/services/queries';
import { formatCompactNumber, formatCurrency, formatPercent, formatPrice, formatVolume } from '../../src/utils/formatters';
import { QUERY_IDS } from '../fixtures/synthetic/trading';

describe('Trading query and deployment contract', () => {
  it('retains all query IDs, synthetic joins, source identities, thresholds and financial expressions', () => {
    expect(ALL_QUERIES.map(query => query.id)).toEqual(QUERY_IDS);
    expect(DRASI_SERVER_URL).toBe('http://localhost:8280');
    expect(TRADING_REACTION).toEqual({
      id: 'sse-stream', kind: 'sse', host: '0.0.0.0', port: 8281,
      ssePath: '/events', heartbeatIntervalMs: 15000,
    });
    const definitions = new Map(ALL_QUERIES.map(query => [query.id, query]));
    expect(definitions.get('watchlist-query')?.joins).toEqual([ON_WATCHLIST, HAS_PRICE]);
    expect(definitions.get('portfolio-query')?.joins).toEqual([OWNS_STOCK, HAS_PRICE]);
    expect(definitions.get('active-orders-query')?.joins).toEqual([ORDER_HAS_PRICE]);
    expect(HAS_PRICE.keys).toEqual([{ label: 'stocks', property: 'symbol' }, { label: 'stock_prices', property: 'symbol' }]);
    expect(definitions.get('price-ticker-query')?.sources).toEqual([{ sourceId: 'price-feed', pipeline: [] }]);
    expect(definitions.get('portfolio-query')?.sources).toEqual([
      { sourceId: 'postgres-stocks', pipeline: [] }, { sourceId: 'price-feed', pipeline: [] },
    ]);
    expect(definitions.get('active-orders-query')?.sources).toEqual([
      { sourceId: 'postgres-broker', pipeline: [] }, { sourceId: 'price-feed', pipeline: [] },
    ]);
    expect(definitions.get('high-volume-query')?.query).toContain('sp.volume > 10000000');
    expect(definitions.get('top-gainers-query')?.query).toContain('sp.price > sp.previous_close');
    expect(definitions.get('top-losers-query')?.query).toContain('sp.price < sp.previous_close');
    expect(definitions.get('portfolio-query')?.query).toContain('(sp.price - toFloat(p.purchase_price)) * p.quantity');
    expect(definitions.get('portfolio-summary-query')?.query).toContain('sum(toFloat(p.purchase_price) * p.quantity)');
    expect(definitions.get('stale-orders-query')?.query).toContain('drasi.trueFor');
    expect(definitions.get('expiring-orders-query')?.query).toContain('o.expire_duration - o.stale_duration');
  });

  it('normalizes portfolio numbers and retains identity for id-only deletes', () => {
    const options = tradingQueryOptions('portfolio-query');
    const position = options.transform!({ id: 7, symbol: 'AAPL', quantity: '4', currentPrice: '12.50', profitLoss: 'bad', purchasePrice: '' });
    expect(position).toEqual({ id: 7, symbol: 'AAPL', quantity: 4, currentPrice: 12.5, profitLoss: null, purchasePrice: '' });
    expect(options.getKey!(position)).toBe(options.getKey!({ id: 7, _deleted: true }));
  });

  it('filters signs, caps movers at ten and applies the intended data-level ordering', () => {
    const rows = Array.from({ length: 25 }, (_, index) => ({
      symbol: `S${index}`, changePercent: index - 12, volume: index * 1_000_000,
      name: `Stock ${index}`, price: 100, previousClose: 100,
    }));
    const gainers = tradingQueryOptions('top-gainers-query').postProcess!([...rows]);
    const losers = tradingQueryOptions('top-losers-query').postProcess!([...rows]);
    const volume = tradingQueryOptions('high-volume-query').postProcess!([...rows]);
    expect(gainers.map(row => row.changePercent)).toEqual([12, 11, 10, 9, 8, 7, 6, 5, 4, 3]);
    expect(losers.map(row => row.changePercent)).toEqual([-12, -11, -10, -9, -8, -7, -6, -5, -4, -3]);
    expect(volume.map(row => row.volume)).toEqual([24, 23, 22, 21, 20, 19, 18, 17, 16, 15].map(value => value * 1_000_000));
  });

  it.each([
    [{ total_value: 2, total_cost: 1 }, ['portfolio-summary-query']],
    [{ order_type: 'buy', target_price: 10 }, ['active-orders-query']],
    [{ id: 7, _deleted: true }, ['portfolio-query']],
    [{ sector: 'Technology', stockCount: 2 }, ['sector-performance-query']],
    [{ watchlist_id: 1 }, ['watchlist-query']],
    [{ symbol: 'AAPL', price: 110 }, ['watchlist-query', 'top-gainers-query', 'top-losers-query', 'high-volume-query', 'price-ticker-query', 'price-screener-query']],
  ])('characterizes the app-owned unidentified-data adapter for %j', (input, ids) => {
    const deliver = vi.fn();
    routeTradingData([input], deliver);
    expect(deliver.mock.calls.map(call => call[0])).toEqual(ids);
    expect(deliver.mock.calls.every(call => call[1][0] === input)).toBe(true);
  });

  it('warns on unknown legacy shapes rather than assigning arbitrary queries', () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const deliver = vi.fn();
    routeTradingData([], deliver);
    routeTradingData([{ unrelated: true }], deliver);
    expect(deliver).not.toHaveBeenCalled();
    expect(warn).toHaveBeenCalledOnce();
  });
});

describe('Trading number presentation', () => {
  it.each([null, undefined, NaN])('shows a dash for absent/invalid values (%s)', value => {
    for (const format of [formatCurrency, formatPercent, formatVolume, formatCompactNumber, formatPrice]) {
      expect(format(value)).toBe('-');
    }
  });
  it('retains USD precision, signed percentages and compact volume conventions', () => {
    expect(formatCurrency(1234.5)).toBe('$1,234.50');
    expect(formatCurrency(-20)).toBe('-$20.00');
    expect(formatPrice(0)).toBe('$0.00');
    expect(formatPercent(-1.234)).toBe('-1.23%');
    expect(formatPercent(1.234, false)).toBe('1.23%');
    expect([999, 1200, 12_500_000, 1_250_000_000].map(formatVolume)).toEqual(['999', '1.20K', '12.50M', '1.25B']);
    expect([999, 1200, 12_500_000, 1_250_000_000].map(formatCompactNumber)).toEqual(['999', '1.2K', '12.5M', '1.3B']);
  });
});
