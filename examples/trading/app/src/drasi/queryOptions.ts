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

import { DrasiError, type ResultRow } from '@drasi/react/client';
import type { UseDrasiQueryOptions } from '@drasi/react/react';
import type {
  HighVolumeStock, LimitOrderResult, OrderAlert, PortfolioRow, PortfolioSummary,
  PriceTickerRow, SectorPerformance, Stock, TradingQueryId, TradingQueryRows,
} from '@/types';

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

function isKey(value: unknown): value is string | number {
  return (typeof value === 'string' && value.trim().length > 0) ||
    (typeof value === 'number' && Number.isFinite(value));
}

function invalidKey(queryId: TradingQueryId): never {
  throw new DrasiError('INVALID_ROW_KEY', { resourceKind: 'query', resourceId: queryId });
}

/** Shared raw/render identity, including distinct positions in the same symbol. */
export function portfolioRowKey(row: { readonly id?: unknown; readonly symbol?: unknown }): string {
  if (row.id !== undefined && row.id !== null) {
    if (!isKey(row.id)) return invalidKey('portfolio-query');
    return `portfolio-id-${row.id}`;
  }
  if (typeof row.symbol === 'string' && row.symbol.trim()) return `portfolio-${row.symbol}`;
  return invalidKey('portfolio-query');
}

/** Compute the unique key used to accumulate a row for a given query. */
function getItemKey(item: Readonly<ResultRow>, queryId: TradingQueryId): string {
  const { id, symbol } = item;
  // Portfolio items use id as the primary key so delete events (which only
  // carry an id) can be matched.
  if (queryId === 'portfolio-query') {
    return portfolioRowKey(item);
  }
  // Portfolio summary is a single aggregation row.
  if (queryId === 'portfolio-summary-query') {
    return 'portfolio-summary';
  }
  // Most rows are uniquely identified by their symbol.
  if (typeof symbol === 'string' && symbol.trim()) {
    return symbol;
  }
  // Sector performance is keyed by sector.
  if (queryId === 'sector-performance-query') {
    const { sector } = item;
    if (typeof sector === 'string' && sector.trim()) {
      return `sector-${sector}`;
    }
    return invalidKey(queryId);
  }
  if (isKey(id)) {
    return String(id);
  }
  return invalidKey(queryId);
}

/** Normalize portfolio numeric string fields to numbers (or null). */
function transformPortfolioRow(item: Readonly<ResultRow>): PortfolioRow {
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

function isPortfolioRow(row: Readonly<ResultRow>): row is Readonly<ResultRow> & PortfolioRow {
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

function isNumber(value: unknown): value is number {
  return typeof value === 'number' && Number.isFinite(value);
}

function isOptionalDate(value: unknown): value is string | null | undefined {
  return value == null || typeof value === 'string';
}

function isStockFields(row: Readonly<ResultRow>): boolean {
  return typeof row.symbol === 'string' && typeof row.name === 'string' &&
    isNumber(row.price) && isNumber(row.changePercent) &&
    (row.volume === undefined || isNumber(row.volume)) &&
    (row.sector === undefined || typeof row.sector === 'string') &&
    (row.high === undefined || isNumber(row.high)) &&
    (row.low === undefined || isNumber(row.low));
}

function isStock(row: Readonly<ResultRow>): row is Readonly<ResultRow> & Stock {
  return isStockFields(row) && isNumber(row.previousClose);
}

function isHighVolumeStock(row: Readonly<ResultRow>): row is Readonly<ResultRow> & HighVolumeStock {
  return isStockFields(row) && isNumber(row.volume);
}

function isPriceTickerRow(row: Readonly<ResultRow>): row is Readonly<ResultRow> & PriceTickerRow {
  return typeof row.symbol === 'string' &&
    (isNumber(row.price) || typeof row.price === 'string') &&
    (isNumber(row.changePercent) || typeof row.changePercent === 'string');
}

function isSectorPerformance(row: Readonly<ResultRow>): row is Readonly<ResultRow> & SectorPerformance {
  return typeof row.sector === 'string' && isNumber(row.stockCount) &&
    isNumber(row.avgChangePercent) && isNumber(row.totalVolume) &&
    isNumber(row.minPrice) && isNumber(row.maxPrice);
}

function isPortfolioSummary(row: Readonly<ResultRow>): row is Readonly<ResultRow> & PortfolioSummary {
  return isNumber(row.totalValue) && isNumber(row.totalCost) && isNumber(row.totalProfitLoss) &&
    isNumber(row.totalProfitLossPercent) && isNumber(row.positionCount);
}

function isOrderFields(row: Readonly<ResultRow>): boolean {
  return isNumber(row.id) && typeof row.symbol === 'string' && typeof row.orderType === 'string' &&
    isNumber(row.targetPrice) && isNumber(row.quantity) &&
    isOptionalDate(row.triggeredAt) && isOptionalDate(row.expiresAt);
}

function isLimitOrder(row: Readonly<ResultRow>): row is Readonly<ResultRow> & LimitOrderResult {
  return isOrderFields(row) && isNumber(row.currentPrice) && isNumber(row.distancePercent) &&
    typeof row.status === 'string' && typeof row.createdAt === 'string';
}

function isOrderAlert(row: Readonly<ResultRow>): row is Readonly<ResultRow> & OrderAlert {
  return isOrderFields(row) && isOptionalDate(row.createdAt) &&
    (row.alertType === 'STALE' || row.alertType === 'EXPIRED') && typeof row.alertMessage === 'string';
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
      return [...rows]
        .sort((a, b) => Number(field(b, 'volume') || 0) - Number(field(a, 'volume') || 0))
        .slice(0, 10);
    case 'watchlist-query':
      return [...rows].sort((a, b) =>
        String(field(a, 'symbol') || '').localeCompare(String(field(b, 'symbol') || '')),
      );
    case 'portfolio-query':
      return [...rows].sort((a, b) => Number(field(b, 'currentValue') || 0) - Number(field(a, 'currentValue') || 0));
    default:
      return rows;
  }
}

function queryOptions<T extends object>(
  queryId: TradingQueryId, transform: (row: Readonly<ResultRow>) => T,
): UseDrasiQueryOptions<T> {
  return {
    getKey: row => getItemKey(row, queryId),
    transform,
    postProcess: rows => postProcess(queryId, rows),
  };
}

function validatedOptions<T extends object>(
  queryId: TradingQueryId,
  guard: (row: Readonly<ResultRow>) => row is Readonly<ResultRow> & T,
): UseDrasiQueryOptions<T> {
  return queryOptions<T>(queryId, row => {
    if (!guard(row)) {
      throw new DrasiError('RESULT_PROCESSING_FAILED', { resourceKind: 'query', resourceId: queryId });
    }
    return row;
  });
}

const optionsForQuery: { [Id in TradingQueryId]: UseDrasiQueryOptions<TradingQueryRows[Id]> } = {
  'watchlist-query': validatedOptions('watchlist-query', isStock),
  'portfolio-query': queryOptions('portfolio-query', transformPortfolioRow),
  'top-gainers-query': validatedOptions('top-gainers-query', isStock),
  'top-losers-query': validatedOptions('top-losers-query', isStock),
  'high-volume-query': validatedOptions('high-volume-query', isHighVolumeStock),
  'price-ticker-query': validatedOptions('price-ticker-query', isPriceTickerRow),
  'sector-performance-query': validatedOptions('sector-performance-query', isSectorPerformance),
  'portfolio-summary-query': validatedOptions('portfolio-summary-query', isPortfolioSummary),
  'active-orders-query': validatedOptions('active-orders-query', isLimitOrder),
  'stale-orders-query': validatedOptions('stale-orders-query', isOrderAlert),
  'expiring-orders-query': validatedOptions('expiring-orders-query', isOrderAlert),
};

/**
 * Select the query's declared projection instead of asserting an arbitrary T.
 * Raw keys are extracted before normalization, including for sparse deletes.
 */
export function tradingQueryOptions<K extends TradingQueryId>(
  queryId: K,
): UseDrasiQueryOptions<TradingQueryRows[K]> {
  return optionsForQuery[queryId];
}
