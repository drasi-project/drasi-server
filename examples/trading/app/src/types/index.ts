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

import type { ResultRow } from '@drasi/react/client';

export interface Stock {
  symbol: string;
  name: string;
  price: number;
  previousClose: number;
  changePercent: number;
  volume?: number;
  sector?: string;
  high?: number;
  low?: number;
}

export interface PortfolioPosition {
  id: number;
  symbol: string;
  name: string;
  quantity: number;
  purchasePrice: number;
  currentPrice: number;
  currentValue: number;
  costBasis: number;
  profitLoss: number;
  profitLossPercent: number;
}

/** Portfolio normalization retains missing fields, nulls, and empty strings. */
export type PortfolioNumber = number | null | '';

/** Also accepts the id-only delete rows delivered by the portfolio query. */
export interface PortfolioRow {
  id?: number | string | null;
  symbol?: string;
  name?: string;
  quantity?: PortfolioNumber;
  purchasePrice?: PortfolioNumber;
  currentPrice?: PortfolioNumber;
  currentValue?: PortfolioNumber;
  costBasis?: PortfolioNumber;
  profitLoss?: PortfolioNumber;
  profitLossPercent?: PortfolioNumber;
  changePercent?: PortfolioNumber;
  _deleted?: boolean;
}

export interface PriceTickerRow {
  symbol: string;
  price: number | string;
  changePercent: number | string;
}

export interface SectorPerformance {
  sector: string;
  stockCount: number;
  avgChangePercent: number;
  totalVolume: number;
  minPrice: number;
  maxPrice: number;
}

export interface PortfolioSummary {
  totalValue: number;
  totalCost: number;
  totalProfitLoss: number;
  totalProfitLossPercent: number;
  positionCount: number;
}

export interface LimitOrderResult {
  id: number;
  symbol: string;
  orderType: string;
  targetPrice: number;
  currentPrice: number;
  quantity: number;
  status: string;
  createdAt: string;
  triggeredAt?: string;
  expiresAt?: string;
  distancePercent: number;
}

export interface OrderAlert {
  id: number;
  symbol: string;
  orderType: string;
  targetPrice: number;
  quantity: number;
  triggeredAt?: string;
  expiresAt?: string;
  alertType: 'STALE' | 'EXPIRED';
  alertMessage: string;
}

/** The application, not the reusable client, owns these query projections. */
export interface TradingQueryRows {
  'watchlist-query': Stock;
  'portfolio-query': PortfolioRow;
  'top-gainers-query': Stock;
  'top-losers-query': Stock;
  'high-volume-query': Stock;
  'price-ticker-query': PriceTickerRow;
  'sector-performance-query': SectorPerformance;
  'portfolio-summary-query': PortfolioSummary;
  'active-orders-query': LimitOrderResult;
  'stale-orders-query': OrderAlert;
  'expiring-orders-query': OrderAlert;
}

export type TradingQueryId = keyof TradingQueryRows;
export type MarketMoverQueryId = 'top-gainers-query' | 'top-losers-query' | 'high-volume-query';

export interface QueryResult<T = ResultRow> {
  queryId: string;
  timestamp: number;
  data: T[];
  error?: string;
}

export interface QuerySubscription<T = ResultRow> {
  queryId: string;
  callback: (result: QueryResult<T>) => void;
  unsubscribe: () => void;
}

export interface ScreenerFilters {
  minPrice?: number;
  maxPrice?: number;
  sector?: string | null;
  minVolume?: number;
  minChangePercent?: number;
  maxChangePercent?: number;
}

export interface DrasiQuery {
  id: string;
  query: string;
  parameters?: Record<string, unknown>;
  source_subscriptions: Array<{ source_id: string; pipeline: string[] }>;
}

export interface ConnectionStatus {
  connected: boolean;
  error?: string;
  reconnecting?: boolean;
  lastConnected?: Date;
}