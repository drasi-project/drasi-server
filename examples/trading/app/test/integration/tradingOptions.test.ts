// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { describe, expect, it, vi } from 'vitest';
import { DrasiError, type ResultRow } from '@drasi/react/client';
import { routeTradingData, tradingResultAdapter, DRASI_SERVER_URL, TRADING_REACTION } from '../../src/drasi/config';
import { portfolioRowKey, tradingQueryOptions } from '../../src/drasi/queryOptions';
import { ALL_QUERIES, HAS_PRICE, ON_WATCHLIST, ORDER_HAS_PRICE, OWNS_STOCK } from '../../src/services/queries';
import type { TradingQueryId } from '../../src/types';
import { formatCompactNumber, formatCurrency, formatPercent, formatPrice, formatVolume } from '../../src/utils/formatters';
import { QUERY_IDS, SyntheticTrading } from '../fixtures/synthetic/trading';

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
    const raw = { id: 7, symbol: 'AAPL', quantity: '4', currentPrice: '12.50', profitLoss: 'bad', purchasePrice: '' };
    const position = options.transform(raw);
    expect(position).toEqual({ id: 7, symbol: 'AAPL', quantity: 4, currentPrice: 12.5, profitLoss: null, purchasePrice: '' });
    expect(options.getKey(raw)).toBe(options.getKey({ id: 7, _deleted: true }));
    expect(raw.quantity).toBe('4');
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

  it('rejects unknown legacy shapes safely rather than logging rows or assigning arbitrary queries', () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const deliver = vi.fn();
    routeTradingData([], deliver);
    expect(() => routeTradingData([{ unrelated: 'private-row-value' }], deliver))
      .toThrowError(new DrasiError('UNROUTABLE_RESULT'));
    expect(deliver).not.toHaveBeenCalled();
    expect(warn).not.toHaveBeenCalled();
  });
});

describe('Trading raw identity and validated projections', () => {
  const keys: Array<{ queryId: TradingQueryId; row: ResultRow; key: string }> = [
    { queryId: 'portfolio-query', row: { id: 7, symbol: 'AAPL' }, key: 'portfolio-id-7' },
    { queryId: 'portfolio-query', row: { id: 7 }, key: 'portfolio-id-7' },
    { queryId: 'portfolio-query', row: { symbol: 'AAPL' }, key: 'portfolio-AAPL' },
    { queryId: 'portfolio-summary-query', row: { totalValue: 2000 }, key: 'portfolio-summary' },
    { queryId: 'watchlist-query', row: { symbol: 'AAPL' }, key: 'AAPL' },
    { queryId: 'top-gainers-query', row: { symbol: 'AAPL' }, key: 'AAPL' },
    { queryId: 'top-losers-query', row: { symbol: 'MSFT' }, key: 'MSFT' },
    { queryId: 'high-volume-query', row: { symbol: 'AMZN' }, key: 'AMZN' },
    { queryId: 'price-ticker-query', row: { symbol: 'AAPL' }, key: 'AAPL' },
    { queryId: 'sector-performance-query', row: { sector: 'Technology' }, key: 'sector-Technology' },
    { queryId: 'active-orders-query', row: { id: 7, symbol: 'AAPL' }, key: 'AAPL' },
    { queryId: 'stale-orders-query', row: { id: 7, symbol: 'AAPL' }, key: 'AAPL' },
    { queryId: 'expiring-orders-query', row: { id: 7 }, key: '7' },
  ];

  it.each(keys)('retains the raw $queryId domain key $key', ({ queryId, row, key }) => {
    const options = tradingQueryOptions(queryId);
    expect(options.getKey(row)).toBe(key);
    expect(tradingQueryOptions(queryId)).toBe(options);
    if (queryId === 'portfolio-query') expect(portfolioRowKey(row)).toBe(key);
  });

  it('rejects missing/invalid identities instead of serializing financial values', () => {
    for (const queryId of QUERY_IDS) {
      if (queryId === 'portfolio-summary-query') continue;
      const options = tradingQueryOptions(queryId);
      for (const row of [{}, { currentValue: 100 }, { symbol: '', id: {} }]) {
        expect(() => options.getKey(row)).toThrowError(new DrasiError('INVALID_ROW_KEY', {
          resourceKind: 'query', resourceId: queryId,
        }));
      }
    }
    expect(portfolioRowKey({ id: 1, symbol: 'AAPL' })).not.toBe(portfolioRowKey({ id: 2, symbol: 'AAPL' }));
    expect(portfolioRowKey({ id: 1, symbol: 'OLD' })).toBe(portfolioRowKey({ id: 1, symbol: 'NEW' }));
  });

  it('preserves the portfolio parseFloat, empty, missing, null and invalid-number semantics without mutation', () => {
    const raw = {
      id: 7, symbol: 'AAPL', quantity: '4 shares', purchasePrice: '', currentPrice: null,
      currentValue: ' 12.50 ', costBasis: '0', profitLoss: 'bad', profitLossPercent: undefined,
      changePercent: '-2.1e2 percent',
    };
    const original = { ...raw };
    expect(tradingQueryOptions('portfolio-query').transform(raw)).toEqual({
      id: 7, symbol: 'AAPL', quantity: 4, purchasePrice: '', currentPrice: null,
      currentValue: 12.5, costBasis: 0, profitLoss: null, profitLossPercent: undefined,
      changePercent: -210,
    });
    expect(raw).toEqual(original);
  });

  it('validates actual per-query projections without filling in fields absent from the query', () => {
    const backend = new SyntheticTrading();
    for (const queryId of QUERY_IDS) {
      for (const raw of backend.snapshot(queryId)) {
        expect(tradingQueryOptions(queryId).transform(raw)).toEqual(raw);
      }
    }
    const volume = { symbol: 'AAPL', name: 'Apple Inc.', price: 110, volume: 12_000_000, changePercent: 10 };
    expect(tradingQueryOptions('high-volume-query').transform(volume)).toBe(volume);
    expect(volume).not.toHaveProperty('previousClose');
    expect(() => tradingQueryOptions('watchlist-query').transform(volume))
      .toThrowError(new DrasiError('RESULT_PROCESSING_FAILED', {
        resourceKind: 'query', resourceId: 'watchlist-query',
      }));

    const ticker = { symbol: 'AAPL', price: '110.00', changePercent: '10.0' };
    expect(tradingQueryOptions('price-ticker-query').transform(ticker)).toBe(ticker);
    for (const alertType of ['STALE', 'EXPIRED']) {
      const alert = {
        id: 7, symbol: 'AAPL', orderType: 'buy', targetPrice: 100, quantity: 4,
        expiresAt: null, alertType, alertMessage: 'Order state changed',
      };
      expect(tradingQueryOptions('stale-orders-query').transform(alert)).toBe(alert);
      expect(tradingQueryOptions('expiring-orders-query').transform(alert)).toBe(alert);
    }
    const order = { ...backend.snapshot('active-orders-query')[0], expiresAt: null };
    expect(tradingQueryOptions('active-orders-query').transform(order)).toBe(order);
  });

  it('does not claim a typed projection for malformed fields or sparse delete shapes', () => {
    const stock = { symbol: 'AAPL', name: 'Apple Inc.', price: 110, previousClose: 100, changePercent: 10 };
    for (const row of [{ ...stock, price: '110' }, { ...stock, name: {} }, { symbol: 'AAPL' }]) {
      expect(() => tradingQueryOptions('watchlist-query').transform(row)).toThrowError(DrasiError);
    }
    const summary = {
      totalValue: 2000, totalCost: 1800, totalProfitLoss: 200,
      totalProfitLossPercent: 200 / 1800 * 100, positionCount: 2,
    };
    expect(tradingQueryOptions('portfolio-summary-query').transform(summary)).toBe(summary);
    expect(() => tradingQueryOptions('portfolio-summary-query').transform({ ...summary, totalValue: '2000' }))
      .toThrowError(DrasiError);
  });

  it('derives existing business ordering and filters without mutating the input rows', () => {
    const backend = new SyntheticTrading();
    const stocks = backend.snapshot('watchlist-query').map(raw => {
      const row = tradingQueryOptions('watchlist-query').transform(raw);
      if (!row) throw new Error('Expected a visible stock');
      return row;
    }).reverse();
    expect(tradingQueryOptions('watchlist-query').postProcess?.(stocks).map(row => row.symbol)).toEqual(['AAPL', 'MSFT']);
    expect(stocks.map(row => row.symbol)).toEqual(['MSFT', 'AAPL']);
    const positions = [{ id: 1, currentValue: 10 }, { id: 2, currentValue: 20 }];
    expect(tradingQueryOptions('portfolio-query').postProcess?.(positions)).toEqual([...positions].reverse());
    expect(positions.map(row => row.id)).toEqual([1, 2]);
    const movers = [
      { symbol: 'LOW', name: 'Low', price: 100, previousClose: 100, changePercent: -1, volume: 12 },
      { symbol: 'HIGH', name: 'High', price: 100, previousClose: 100, changePercent: 2, volume: 30 },
      { symbol: 'ZERO', name: 'Zero', price: 100, previousClose: 100, changePercent: 0, volume: 20 },
    ];
    expect(tradingQueryOptions('high-volume-query').postProcess?.(movers).map(row => row.symbol)).toEqual(['HIGH', 'ZERO', 'LOW']);
    expect(tradingQueryOptions('top-gainers-query').postProcess?.(movers).map(row => row.symbol)).toEqual(['HIGH']);
    expect(tradingQueryOptions('top-losers-query').postProcess?.(movers).map(row => row.symbol)).toEqual(['LOW']);
    expect(movers.map(row => row.symbol)).toEqual(['LOW', 'HIGH', 'ZERO']);
  });
});

describe('Trading explicit legacy adapter', () => {
  const context = { receivedAt: 10, instanceId: 'trading-server' };

  it('keeps official envelope IDs and before/after instead of routing by financial shape', () => {
    const before = { id: 1, symbol: 'OLD' }, after = { id: 2, symbol: 'NEW' };
    const deltas = tradingResultAdapter({
      queryId: 'active-orders-query', timestamp: 5,
      results: [{ type: 'UPDATE', data: after, before, after }],
    }, context);
    expect(deltas).toEqual([{
      kind: 'delta', queryId: 'active-orders-query', receivedAt: 10, sourceTimestamp: 5,
      changes: [{ kind: 'update', before, after }],
    }]);
    expect(tradingResultAdapter({
      query_id: 'active-orders-query', results: [{ op: 'u', before, after }],
    }, context)[0].changes).toEqual([{ kind: 'update', before, after }]);
  });

  it('routes each original unidentified row synchronously and preserves price fan-out', () => {
    const portfolio = { id: 7 }, price = { symbol: 'AAPL', price: 110 };
    const deliver = vi.fn();
    routeTradingData([portfolio, price], deliver);
    expect(deliver).toHaveBeenCalledTimes(7);
    expect(deliver.mock.calls[0]).toEqual(['portfolio-query', [portfolio]]);
    expect(deliver.mock.calls[0][1][0]).toBe(portfolio);
    for (const call of deliver.mock.calls.slice(1)) expect(call[1][0]).toBe(price);
  });

  it('rejects unknown and mixed unidentified rows without delivering or leaking payloads', () => {
    const deliver = vi.fn();
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const log = vi.spyOn(console, 'log').mockImplementation(() => {});
    const error = vi.spyOn(console, 'error').mockImplementation(() => {});
    const privateRow = { unrelated: 'private-row-value' };
    expect(() => routeTradingData([{ id: 7 }, privateRow], deliver)).toThrowError(new DrasiError('UNROUTABLE_RESULT'));
    expect(() => tradingResultAdapter(privateRow, context)).toThrowError(new DrasiError('UNROUTABLE_RESULT'));
    expect(deliver).not.toHaveBeenCalled();
    expect(warn).not.toHaveBeenCalled();
    expect(log).not.toHaveBeenCalled();
    expect(error).not.toHaveBeenCalled();
  });

  it('names the unidentified legacy update limitation as delete then upsert', () => {
    const before = { id: 1, symbol: 'OLD' }, after = { id: 2, symbol: 'NEW' };
    expect(tradingResultAdapter({ results: [{ op: 'u', before, after }] }, context)).toEqual([{
      kind: 'delta', queryId: 'portfolio-query', receivedAt: 10, sourceTimestamp: undefined,
      changes: [{ kind: 'delete', before }, { kind: 'upsert', after }],
    }]);
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
