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
 * Trading-application configuration for the reusable `drasi-react` package.
 *
 * Everything in this file is specific to the Trading example: the set of
 * continuous queries, the SSE reaction that multiplexes them, and the
 * content-based routing used for aggregation change events that arrive without
 * an explicit query id. The reusable components themselves contain none of this
 * — they receive it through `<DrasiProvider>`.
 */

import type { QueryDefinition, ReactionDefinition, RouteUnidentified } from '@drasi/react';
import { ALL_QUERIES } from '@/services/queries';

/** Base URL of the Drasi Server REST API used by the Trading example. */
export const DRASI_SERVER_URL = 'http://localhost:8280';

/** All continuous queries the Trading example multiplexes over one connection. */
export const TRADING_QUERIES: QueryDefinition[] = ALL_QUERIES as QueryDefinition[];

/** The SSE reaction that streams every query over a single connection. */
export const TRADING_REACTION: ReactionDefinition = {
  id: 'sse-stream',
  kind: 'sse',
  host: '0.0.0.0',
  port: 8281,
  ssePath: '/events',
  heartbeatIntervalMs: 15000,
};

/**
 * Route aggregation/change payloads that arrive without a query id to the
 * correct continuous query, based on the shape of the row content. This mirrors
 * the original behavior the Trading example relied on; the reusable library is
 * intentionally agnostic to these application-specific shapes.
 */
export const routeTradingData: RouteUnidentified = (rows, deliver) => {
  const first = rows[0];
  if (!first) {
    return;
  }

  // Portfolio summary (single aggregation row).
  if (first.total_value !== undefined && first.total_cost !== undefined) {
    deliver('portfolio-summary-query', rows);
  }
  // Limit-order data.
  else if (first.order_type !== undefined && first.target_price !== undefined) {
    deliver('active-orders-query', rows);
  }
  // Full portfolio rows, or portfolio delete events that only carry an id.
  else if (
    first.id !== undefined ||
    (first.quantity !== undefined && first.purchase_price !== undefined)
  ) {
    deliver('portfolio-query', rows);
  }
  // Sector performance aggregation.
  else if (
    first.sector !== undefined &&
    (first.stockCount !== undefined || first.avgChangePercent !== undefined)
  ) {
    deliver('sector-performance-query', rows);
  }
  // Watchlist rows.
  else if (first.watchlist_id !== undefined) {
    deliver('watchlist-query', rows);
  }
  // Generic stock price rows feed every price-driven query.
  else if (first.symbol !== undefined && first.price !== undefined) {
    [
      'watchlist-query',
      'top-gainers-query',
      'top-losers-query',
      'high-volume-query',
      'price-ticker-query',
      'price-screener-query',
    ].forEach((queryId) => deliver(queryId, rows));
  } else {
    console.warn('Unable to route data to specific query, data structure:', first);
  }
};
