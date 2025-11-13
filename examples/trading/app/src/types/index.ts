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

export interface SectorPerformance {
  sector: string;
  avgPrice: number;
  avgChangePercent: number;
  stockCount: number;
}

export interface QueryResult {
  queryId: string;
  timestamp: number;
  data: any[];
  error?: string;
}

export interface QuerySubscription {
  queryId: string;
  callback: (result: QueryResult) => void;
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
  parameters?: Record<string, any>;
  source_subscriptions: Array<{ source_id: string; pipeline: string[] }>;
}

export interface ConnectionStatus {
  connected: boolean;
  error?: string;
  reconnecting?: boolean;
  lastConnected?: Date;
}