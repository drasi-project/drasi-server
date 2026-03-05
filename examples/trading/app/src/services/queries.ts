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
 * ============================================================================
 * DRASI TRADING DEMO - QUERY DEFINITIONS
 * ============================================================================
 * 
 * This file contains all Cypher queries used in the Trading Demo.
 * Queries demonstrate key Drasi capabilities:
 * 
 * - Multi-source joins (PostgreSQL CDC + HTTP price feed)
 * - Synthetic relationships (HAS_PRICE, OWNS_STOCK, ON_WATCHLIST)
 * - Real-time aggregations (GROUP BY with COUNT, AVG, SUM, MIN, MAX)
 * - Computed fields (profit/loss calculations, percentage changes)
 * - Filtering (gainers vs losers, volume thresholds)
 */

// ============================================================================
// SYNTHETIC JOIN DEFINITIONS
// ============================================================================
// Synthetic joins create relationships between data from different sources
// that don't have explicit foreign keys in the database.

export interface QueryJoin {
  id: string;
  keys: Array<{ label: string; property: string }>;
}

/**
 * HAS_PRICE - Links company data to real-time price data
 * Joins: stocks (PostgreSQL) ↔ stock_prices (HTTP feed)
 * Join key: symbol
 */
export const HAS_PRICE: QueryJoin = {
  id: 'HAS_PRICE',
  keys: [
    { label: 'stocks', property: 'symbol' },
    { label: 'stock_prices', property: 'symbol' }
  ]
};

/**
 * OWNS_STOCK - Links portfolio positions to company data
 * Joins: portfolio (PostgreSQL) ↔ stocks (PostgreSQL)
 * Join key: symbol
 */
export const OWNS_STOCK: QueryJoin = {
  id: 'OWNS_STOCK',
  keys: [
    { label: 'portfolio', property: 'symbol' },
    { label: 'stocks', property: 'symbol' }
  ]
};

/**
 * ON_WATCHLIST - Links watchlist entries to company data
 * Joins: watchlist (PostgreSQL) ↔ stocks (PostgreSQL)
 * Join key: symbol
 */
export const ON_WATCHLIST: QueryJoin = {
  id: 'ON_WATCHLIST',
  keys: [
    { label: 'watchlist', property: 'symbol' },
    { label: 'stocks', property: 'symbol' }
  ]
};

// ============================================================================
// QUERY DEFINITIONS
// ============================================================================

export interface QueryDefinition {
  id: string;
  description: string;
  query: string;
  sources: Array<{ sourceId: string; pipeline: string[] }>;
  joins: QueryJoin[];
}

/**
 * WATCHLIST QUERY
 * 
 * Three-way join: watchlist → stocks → stock_prices
 * Shows stocks the user is watching with real-time prices.
 * 
 * Demonstrates:
 * - Multi-hop synthetic joins
 * - Computed percentage change
 * - Data from PostgreSQL CDC + HTTP source
 */
export const WATCHLIST_QUERY: QueryDefinition = {
  id: 'watchlist-query',
  description: 'Real-time watchlist with prices from three-way join',
  query: `
    MATCH (w:watchlist)-[:ON_WATCHLIST]->(s:stocks)-[:HAS_PRICE]->(sp:stock_prices)
    RETURN s.symbol AS symbol,
           s.name AS name,
           sp.price AS price,
           sp.previous_close AS previous_close,
           ((sp.price - sp.previous_close) / sp.previous_close * 100) AS change_percent
  `,
  sources: [
    { sourceId: 'postgres-stocks', pipeline: [] },
    { sourceId: 'price-feed', pipeline: [] }
  ],
  joins: [ON_WATCHLIST, HAS_PRICE]
};

/**
 * PORTFOLIO QUERY
 * 
 * Joins portfolio positions with company data and real-time prices.
 * Calculates P&L metrics in real-time as prices change.
 * 
 * Demonstrates:
 * - Complex computed fields (value, cost basis, P&L)
 * - OPTIONAL MATCH for prices (position shows even without price)
 * - Multi-source joins
 */
export const PORTFOLIO_QUERY: QueryDefinition = {
  id: 'portfolio-query',
  description: 'Portfolio positions with real-time P&L calculations',
  query: `
    MATCH (p:portfolio)-[:OWNS_STOCK]->(s:stocks)-[:HAS_PRICE]->(sp:stock_prices)
    WITH p, 
         s.name AS name, 
         sp.price AS current_price,
         (sp.price * p.quantity) AS current_value,
         (p.purchase_price * p.quantity) AS cost_basis,
         ((sp.price - p.purchase_price) * p.quantity) AS profit_loss,
         ((sp.price - p.purchase_price) / p.purchase_price * 100) AS profit_loss_percent
    RETURN p.id AS id,
           p.symbol AS symbol,
           p.quantity AS quantity,
           p.purchase_price AS purchase_price,
           name,
           current_price,
           current_value,
           cost_basis,
           profit_loss,
           profit_loss_percent
  `,
  sources: [
    { sourceId: 'postgres-stocks', pipeline: [] },
    { sourceId: 'price-feed', pipeline: [] }
  ],
  joins: [OWNS_STOCK, HAS_PRICE]
};

/**
 * TOP GAINERS QUERY
 * 
 * Filters to stocks where current price > previous close.
 * UI sorts by change_percent descending to show biggest gainers.
 * 
 * Demonstrates:
 * - WHERE clause filtering
 * - Real-time filtering as prices change
 */
export const TOP_GAINERS_QUERY: QueryDefinition = {
  id: 'top-gainers-query',
  description: 'Stocks with positive price change',
  query: `
    MATCH (s:stocks)-[:HAS_PRICE]->(sp:stock_prices)
    WHERE sp.price > sp.previous_close
    RETURN s.symbol AS symbol,
           s.name AS name,
           sp.price AS price,
           sp.previous_close AS previous_close,
           ((sp.price - sp.previous_close) / sp.previous_close * 100) AS change_percent
  `,
  sources: [
    { sourceId: 'postgres-stocks', pipeline: [] },
    { sourceId: 'price-feed', pipeline: [] }
  ],
  joins: [HAS_PRICE]
};

/**
 * TOP LOSERS QUERY
 * 
 * Filters to stocks where current price < previous close.
 * UI sorts by change_percent ascending to show biggest losers.
 * 
 * Demonstrates:
 * - WHERE clause filtering (opposite of gainers)
 * - Stocks move between gainers/losers as prices change
 */
export const TOP_LOSERS_QUERY: QueryDefinition = {
  id: 'top-losers-query',
  description: 'Stocks with negative price change',
  query: `
    MATCH (s:stocks)-[:HAS_PRICE]->(sp:stock_prices)
    WHERE sp.price < sp.previous_close
    RETURN s.symbol AS symbol,
           s.name AS name,
           sp.price AS price,
           sp.previous_close AS previous_close,
           ((sp.price - sp.previous_close) / sp.previous_close * 100) AS change_percent
  `,
  sources: [
    { sourceId: 'postgres-stocks', pipeline: [] },
    { sourceId: 'price-feed', pipeline: [] }
  ],
  joins: [HAS_PRICE]
};

/**
 * HIGH VOLUME QUERY
 * 
 * Filters to stocks with trading volume above threshold.
 * 
 * Demonstrates:
 * - Numeric threshold filtering
 * - Volume-based analysis
 */
export const HIGH_VOLUME_QUERY: QueryDefinition = {
  id: 'high-volume-query',
  description: 'Stocks with high trading volume',
  query: `
    MATCH (s:stocks)-[:HAS_PRICE]->(sp:stock_prices)
    WHERE sp.volume > 10000000
    RETURN s.symbol AS symbol,
           s.name AS name,
           sp.price AS price,
           sp.volume AS volume,
           ((sp.price - sp.previous_close) / sp.previous_close * 100) AS change_percent
  `,
  sources: [
    { sourceId: 'postgres-stocks', pipeline: [] },
    { sourceId: 'price-feed', pipeline: [] }
  ],
  joins: [HAS_PRICE]
};

/**
 * PRICE TICKER QUERY
 * 
 * Simple single-source query for the scrolling ticker.
 * Only uses the HTTP price feed, no joins needed.
 * 
 * Demonstrates:
 * - Single-source queries
 * - Minimal query for high-frequency updates
 */
export const PRICE_TICKER_QUERY: QueryDefinition = {
  id: 'price-ticker-query',
  description: 'Simple price feed for scrolling ticker',
  query: `
    MATCH (sp:stock_prices)
    RETURN sp.symbol AS symbol,
           sp.price AS price,
           sp.previous_close AS previous_close,
           ((sp.price - sp.previous_close) / sp.previous_close * 100) AS change_percent
  `,
  sources: [
    { sourceId: 'price-feed', pipeline: [] }
  ],
  joins: []
};

/**
 * SECTOR PERFORMANCE QUERY
 * 
 * Aggregates stock data by sector with real-time statistics.
 * 
 * Demonstrates:
 * - GROUP BY aggregation
 * - Multiple aggregate functions (COUNT, AVG, SUM, MIN, MAX)
 * - Real-time aggregation updates as prices change
 */
export const SECTOR_PERFORMANCE_QUERY: QueryDefinition = {
  id: 'sector-performance-query',
  description: 'Real-time sector aggregations',
  query: `
    MATCH (s:stocks)-[:HAS_PRICE]->(sp:stock_prices)
    RETURN s.sector AS sector,
           count(s) AS stock_count,
           avg((sp.price - sp.previous_close) / sp.previous_close * 100) AS avg_change_percent,
           sum(sp.volume) AS total_volume,
           min(sp.price) AS min_price,
           max(sp.price) AS max_price
  `,
  sources: [
    { sourceId: 'postgres-stocks', pipeline: [] },
    { sourceId: 'price-feed', pipeline: [] }
  ],
  joins: [HAS_PRICE]
};

/**
 * PORTFOLIO SUMMARY QUERY
 * 
 * Aggregates portfolio positions into summary statistics.
 * Computes total value, cost, profit/loss in real-time.
 * 
 * Demonstrates:
 * - Aggregation across joined data
 * - Real-time summary updates as prices change
 * - Single-row result set
 */
export const PORTFOLIO_SUMMARY_QUERY: QueryDefinition = {
  id: 'portfolio-summary-query',
  description: 'Real-time portfolio summary statistics',
  query: `
    MATCH (p:portfolio)-[:OWNS_STOCK]->(s:stocks)-[:HAS_PRICE]->(sp:stock_prices)
    WITH sum(sp.price * p.quantity) AS total_value,
         sum(p.purchase_price * p.quantity) AS total_cost,
         count(p) AS position_count
    RETURN total_value,
           total_cost,
           (total_value - total_cost) AS total_profit_loss,
           CASE WHEN total_cost > 0 
                THEN ((total_value - total_cost) / total_cost * 100) 
                ELSE 0 
           END AS total_profit_loss_percent,
           position_count
  `,
  sources: [
    { sourceId: 'postgres-stocks', pipeline: [] },
    { sourceId: 'price-feed', pipeline: [] }
  ],
  joins: [OWNS_STOCK, HAS_PRICE]
};

// ============================================================================
// ALL QUERIES - For easy iteration
// ============================================================================

export const ALL_QUERIES: QueryDefinition[] = [
  WATCHLIST_QUERY,
  PORTFOLIO_QUERY,
  TOP_GAINERS_QUERY,
  TOP_LOSERS_QUERY,
  HIGH_VOLUME_QUERY,
  PRICE_TICKER_QUERY,
  SECTOR_PERFORMANCE_QUERY,
  PORTFOLIO_SUMMARY_QUERY
];

// Query lookup by ID
export const QUERIES_BY_ID: Map<string, QueryDefinition> = new Map(
  ALL_QUERIES.map(q => [q.id, q])
);
