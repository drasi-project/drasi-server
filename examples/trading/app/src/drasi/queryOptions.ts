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

/**
 * Trading-specific options for `useDrasiQuery`/`QueryTable`.
 *
 * The reusable `@drasi/react` library accumulates query results generically; the
 * Trading example supplies how to key rows, normalize them, and sort/filter the
 * accumulated set through these options. This keeps all domain knowledge in the
 * application while the components stay generic.
 */

import type { ResultRow } from '@drasi/react/client';
import type { UseDrasiQueryOptions } from '@drasi/react/react';
import type { PortfolioRow, TradingQueryId, TradingQueryRows } from '@/types';

/** Numeric fields that arrive as strings from the portfolio query. */
const PORTFOLIO_NUMERIC_FIELDS = [
  'quantity',
  'purchasePrice',
  'currentPrice',
  'currentValue',
  'costBasis',
  'profitLoss',
  'profitLossPercent',
  'changePercent',
] as const;

function field(row: object, key: string): unknown {
  return Reflect.get(row, key);
}

/** Compute the unique key used to accumulate a row for a given query. */
function getItemKey(item: object, queryId: string): string | null {
  const id = field(item, 'id');
  const symbol = field(item, 'symbol');
  // Portfolio items use id as the primary key so delete events (which only
  // carry an id) can be matched.
  if (queryId === 'portfolio-query') {
    if (id !== undefined) {
      return `portfolio-id-${id}`;
    }
    if (symbol) {
      return `portfolio-${symbol}`;
    }
  }
  // Portfolio summary is a single aggregation row.
  if (queryId === 'portfolio-summary-query') {
    return 'portfolio-summary';
  }
  // Most rows are uniquely identified by their symbol.
  if (symbol) {
    return String(symbol);
  }
  // Sector performance is keyed by sector.
  if (queryId === 'sector-performance-query') {
    const sector = field(item, 'sector');
    if (sector) {
      return `sector-${sector}`;
    }
    return `sector-${JSON.stringify(item)}`;
  }
  if (id) {
    return String(id);
  }
  return JSON.stringify(item);
}

/** Normalize portfolio numeric string fields to numbers (or null). */
function transformPortfolioRow(item: ResultRow): PortfolioRow {
  const transformed: ResultRow = { ...item };
  for (const field of PORTFOLIO_NUMERIC_FIELDS) {
    if (transformed[field] != null && transformed[field] !== '') {
      const parsed = parseFloat(String(transformed[field]));
      transformed[field] = isNaN(parsed) ? null : parsed;
    }
  }
  if (!isPortfolioRow(transformed)) {
    throw new TypeError('Invalid portfolio row');
  }
  return transformed;
}

function isPortfolioRow(row: ResultRow): row is ResultRow & PortfolioRow {
  return (
    (row.id == null || typeof row.id === 'number' || typeof row.id === 'string') &&
    (row.symbol === undefined || typeof row.symbol === 'string') &&
    (row.name === undefined || typeof row.name === 'string') &&
    (row._deleted === undefined || typeof row._deleted === 'boolean') &&
    PORTFOLIO_NUMERIC_FIELDS.every(key =>
      row[key] == null || row[key] === '' || typeof row[key] === 'number',
    )
  );
}

/** Query-specific sorting/filtering applied to the accumulated result set. */
function postProcess<T extends object>(queryId: string, rows: T[]): T[] {
  switch (queryId) {
    case 'top-gainers-query':
      return rows
        .filter(item => Number(field(item, 'changePercent')) > 0)
        .sort((a, b) => Number(field(b, 'changePercent')) - Number(field(a, 'changePercent')))
        .slice(0, 10);
    case 'top-losers-query':
      return rows
        .filter(item => Number(field(item, 'changePercent')) < 0)
        .sort((a, b) => Number(field(a, 'changePercent')) - Number(field(b, 'changePercent')))
        .slice(0, 10);
    case 'high-volume-query':
      return rows
        .sort((a, b) => Number(field(b, 'volume') || 0) - Number(field(a, 'volume') || 0))
        .slice(0, 10);
    case 'watchlist-query':
      return rows.sort((a, b) =>
        String(field(a, 'symbol') || '').localeCompare(String(field(b, 'symbol') || '')),
      );
    case 'portfolio-query':
      return rows.sort((a, b) => Number(field(b, 'currentValue') || 0) - Number(field(a, 'currentValue') || 0));
    default:
      return rows;
  }
}

function queryOptions<T extends object>(queryId: TradingQueryId): UseDrasiQueryOptions<T> {
  return {
    getKey: row => getItemKey(row, queryId),
    postProcess: rows => postProcess(queryId, rows),
  };
}

const optionsForQuery: { [Id in TradingQueryId]: () => UseDrasiQueryOptions<TradingQueryRows[Id]> } = {
  'watchlist-query': () => queryOptions('watchlist-query'),
  'portfolio-query': () => ({
    ...queryOptions<PortfolioRow>('portfolio-query'),
    transform: transformPortfolioRow,
  }),
  'top-gainers-query': () => queryOptions('top-gainers-query'),
  'top-losers-query': () => queryOptions('top-losers-query'),
  'high-volume-query': () => queryOptions('high-volume-query'),
  'price-ticker-query': () => queryOptions('price-ticker-query'),
  'sector-performance-query': () => queryOptions('sector-performance-query'),
  'portfolio-summary-query': () => queryOptions('portfolio-summary-query'),
  'active-orders-query': () => queryOptions('active-orders-query'),
  'stale-orders-query': () => queryOptions('stale-orders-query'),
  'expiring-orders-query': () => queryOptions('expiring-orders-query'),
};

/**
 * Select the query's declared projection instead of asserting an arbitrary T.
 * Portfolio normalization also accepts sparse delete rows before key extraction.
 */
export function tradingQueryOptions<K extends TradingQueryId>(
  queryId: K,
): UseDrasiQueryOptions<TradingQueryRows[K]> {
  return optionsForQuery[queryId]();
}
