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
 * Trading-application configuration for the reusable `@drasi/react` package.
 *
 * Everything in this file is specific to the Trading example: the set of
 * continuous queries, the SSE reaction that multiplexes them, and the
 * content-based routing used for aggregation change events that arrive without
 * an explicit query id. The reusable components themselves contain none of this
 * — the package receives only references; setup stays in this application.
 */

import {
  createLegacyResultAdapter, DrasiError,
  type QueryConfig, type QuerySource, type ResultRow, type RouteUnidentified,
} from '@drasi/react/client';
import { ALL_QUERIES } from '@/services/queries';

/** Base URL of the Drasi Server REST API used by the Trading example. */
export const DRASI_SERVER_URL = 'http://localhost:8280';

/** App-owned creation/comparison fields, not the complete server read DTO. */
export type TradingQueryDefinition =
  Pick<QueryConfig, 'id' | 'query' | 'queryLanguage' | 'joins'> & {
    readonly sources: readonly (
      Pick<QuerySource, 'sourceId'> & Partial<Omit<QuerySource, 'sourceId'>>
    )[];
  };

/** All continuous queries the Trading example multiplexes over one connection. */
export const TRADING_QUERIES: TradingQueryDefinition[] = ALL_QUERIES.map(({ id, query, sources, joins }) => ({
  id, query, sources, joins, queryLanguage: 'Cypher',
}));
export const TRADING_QUERY_IDS = TRADING_QUERIES.map(query => query.id);

/** The SSE reaction that streams every query over a single connection. */
export const TRADING_REACTION = {
  id: 'sse-stream',
  kind: 'sse',
  host: '0.0.0.0',
  port: 8281,
  ssePath: '/events',
  heartbeatIntervalMs: 15000,
};

/** The browser URL is not the reaction's server bind address. */
export const TRADING_STREAM = {
  id: TRADING_REACTION.id,
  endpoint: 'http://localhost:8281/events',
};

/**
 * Route aggregation/change payloads that arrive without a query id to the
 * correct continuous query, based on the shape of the row content. This mirrors
 * the original behavior the Trading example relied on; the reusable library is
 * intentionally agnostic to these application-specific shapes.
 */
function legacyQueryIds(row: ResultRow): string[] {
  // Portfolio summary (single aggregation row).
  if (row.total_value !== undefined && row.total_cost !== undefined) {
    return ['portfolio-summary-query'];
  }
  // Limit-order data.
  else if (row.order_type !== undefined && row.target_price !== undefined) {
    return ['active-orders-query'];
  }
  // Full portfolio rows, or portfolio delete events that only carry an id.
  else if (
    row.id !== undefined ||
    (row.quantity !== undefined && row.purchase_price !== undefined)
  ) {
    return ['portfolio-query'];
  }
  // Sector performance aggregation.
  else if (
    row.sector !== undefined &&
    (row.stockCount !== undefined || row.avgChangePercent !== undefined)
  ) {
    return ['sector-performance-query'];
  }
  // Watchlist rows.
  else if (row.watchlist_id !== undefined) {
    return ['watchlist-query'];
  }
  // Generic stock price rows feed every price-driven query.
  else if (row.symbol !== undefined && row.price !== undefined) {
    return [
      'watchlist-query',
      'top-gainers-query',
      'top-losers-query',
      'high-volume-query',
      'price-ticker-query',
      'price-screener-query',
    ];
  }
  throw new DrasiError('UNROUTABLE_RESULT');
}

export const routeTradingData: RouteUnidentified = (rows, deliver) => {
  const routed = new Map<string, ResultRow[]>();
  // Classify every row before delivery so a mixed batch cannot silently route
  // an unknown row using its neighbour's identity. Never log arbitrary payloads.
  for (const row of rows) {
    for (const queryId of legacyQueryIds(row)) {
      const selected = routed.get(queryId) ?? [];
      selected.push(row);
      routed.set(queryId, selected);
    }
  }
  for (const [queryId, selected] of routed) deliver(queryId, selected);
};

/** Stable callable identity; explicit wire query IDs always bypass app routing. */
export const tradingResultAdapter = createLegacyResultAdapter({ routeUnidentified: routeTradingData });
