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
 * The reusable `drasi-react` library accumulates query results generically; the
 * Trading example supplies how to key rows, normalize them, and sort/filter the
 * accumulated set through these options. This keeps all domain knowledge in the
 * application while the components stay generic.
 */

import type { UseDrasiQueryOptions } from '@drasi/react';

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
];

/** Compute the unique key used to accumulate a row for a given query. */
function getItemKey(item: any, queryId: string): string | null {
  // Portfolio items use id as the primary key so delete events (which only
  // carry an id) can be matched.
  if (queryId === 'portfolio-query') {
    if (item.id !== undefined) {
      return `portfolio-id-${item.id}`;
    }
    if (item.symbol) {
      return `portfolio-${item.symbol}`;
    }
  }
  // Portfolio summary is a single aggregation row.
  if (queryId === 'portfolio-summary-query') {
    return 'portfolio-summary';
  }
  // Most rows are uniquely identified by their symbol.
  if (item.symbol) {
    return item.symbol;
  }
  // Sector performance is keyed by sector.
  if (queryId === 'sector-performance-query') {
    if (item.sector) {
      return `sector-${item.sector}`;
    }
    return `sector-${JSON.stringify(item)}`;
  }
  if (item.id) {
    return item.id;
  }
  return JSON.stringify(item);
}

/** Normalize portfolio numeric string fields to numbers (or null). */
function transformPortfolioRow(item: any): any {
  const transformed: any = { ...item };
  for (const field of PORTFOLIO_NUMERIC_FIELDS) {
    if (transformed[field] != null && transformed[field] !== '') {
      const parsed = parseFloat(String(transformed[field]));
      transformed[field] = isNaN(parsed) ? null : parsed;
    }
  }
  return transformed;
}

/** Query-specific sorting/filtering applied to the accumulated result set. */
function postProcess(queryId: string, rows: any[]): any[] {
  switch (queryId) {
    case 'top-gainers-query':
      return rows
        .filter((item: any) => item.changePercent > 0)
        .sort((a: any, b: any) => b.changePercent - a.changePercent)
        .slice(0, 10);
    case 'top-losers-query':
      return rows
        .filter((item: any) => item.changePercent < 0)
        .sort((a: any, b: any) => a.changePercent - b.changePercent)
        .slice(0, 10);
    case 'high-volume-query':
      return rows
        .sort((a: any, b: any) => (b.volume || 0) - (a.volume || 0))
        .slice(0, 10);
    case 'watchlist-query':
      return rows.sort((a: any, b: any) =>
        (a.symbol || '').localeCompare(b.symbol || ''),
      );
    case 'portfolio-query':
      return rows.sort((a: any, b: any) => (b.currentValue || 0) - (a.currentValue || 0));
    default:
      return rows;
  }
}

/**
 * Build the `useDrasiQuery` options for a Trading-example query, replicating the
 * keying, transform and sort/filter behavior the example depends on.
 */
export function tradingQueryOptions<T = any>(queryId: string): UseDrasiQueryOptions<T> {
  return {
    getKey: (row: any) => getItemKey(row, queryId),
    transform:
      queryId === 'portfolio-query'
        ? (row: any) => transformPortfolioRow(row) as T
        : undefined,
    postProcess: (rows: T[]) => postProcess(queryId, rows as any[]) as T[],
  };
}
