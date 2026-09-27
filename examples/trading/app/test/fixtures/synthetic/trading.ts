// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

// Synthetic, deliberately small fixtures. These are NOT recorded server results.
import type { Stock as ApiStock } from '../../../src/services/TradingApi';

export const FIXED_TIME = '2026-01-15T12:00:00.000Z';

export interface FixtureStock extends ApiStock {
  price: number;
  previousClose: number;
  volume: number;
}

export const STOCKS: FixtureStock[] = [
  { symbol: 'AAPL', name: 'Apple Inc.', sector: 'Technology', industry: 'Consumer Electronics', price: 110, previousClose: 100, volume: 12_000_000 },
  { symbol: 'AMD', name: 'Advanced Micro Devices', sector: 'Technology', industry: 'Semiconductors', price: 95, previousClose: 100, volume: 10_000_000 },
  { symbol: 'AMZN', name: 'Amazon.com', sector: 'Consumer', industry: 'E-commerce', price: 105, previousClose: 100, volume: 20_000_000 },
  { symbol: 'JPM', name: 'JPMorgan Chase', sector: 'Financial', industry: 'Banking', price: 100, previousClose: 100, volume: 1_000_000 },
  { symbol: 'MSFT', name: 'Microsoft Corporation', sector: 'Technology', industry: 'Software', price: 180, previousClose: 200, volume: 30_000_000 },
  { symbol: 'NVDA', name: 'NVIDIA Corporation', sector: 'Technology', industry: 'Semiconductors', price: 120, previousClose: 100, volume: 8_000_000 },
];

export const POSITIONS = [
  { id: 1, symbol: 'AAPL', quantity: 10, purchase_price: 80, purchase_date: '2024-01-15T10:30:00' },
  { id: 2, symbol: 'MSFT', quantity: 5, purchase_price: 200, purchase_date: '2024-01-20T14:15:00' },
];

export const ORDERS = [
  { id: 1, symbol: 'AMZN', order_type: 'sell', target_price: 110, quantity: 3, status: 'filled', created_at: '2024-01-15T10:30:00', expires_at: '2024-01-15T10:31:00', stale_duration: 30, expire_duration: 60 },
  { id: 2, symbol: 'AMD', order_type: 'buy', target_price: 90, quantity: 4, status: 'expired', created_at: '2024-01-16T10:30:00', expires_at: '2024-01-16T10:31:00', stale_duration: 30, expire_duration: 60 },
];

export const QUERY_IDS = [
  'watchlist-query', 'portfolio-query', 'top-gainers-query', 'top-losers-query',
  'high-volume-query', 'price-ticker-query', 'sector-performance-query',
  'portfolio-summary-query', 'active-orders-query', 'stale-orders-query',
  'expiring-orders-query',
] as const;

export type FixtureRow = Record<string, unknown>;
export interface SyntheticBatch {
  query_id: string;
  results: Array<{ op: 'c' | 'u' | 'd'; before?: FixtureRow; after?: FixtureRow }>;
}

export interface FixtureRequest {
  method: string;
  path: string;
  body?: FixtureRow;
}

export interface FixtureResponse {
  status: number;
  body: unknown;
}

function requiredString(body: FixtureRow, key: string): string {
  const value = body[key];
  if (typeof value !== 'string') throw new Error(`Fixture expected string ${key}`);
  return value;
}

function requiredNumber(body: FixtureRow, key: string): number {
  const value = body[key];
  if (typeof value !== 'number') throw new Error(`Fixture expected number ${key}`);
  return value;
}

export class SyntheticTrading {
  readonly requests: FixtureRequest[] = [];
  readonly queries = new Map<string, FixtureRow>();
  readonly queryStatuses = new Map<string, string>();
  instanceId = 'trading-server';
  reactionStatus = 'Running';
  reaction: FixtureRow | null = null;
  stocks = structuredClone(STOCKS);
  watchlist = new Set(['AAPL', 'MSFT']);
  positions = structuredClone(POSITIONS);
  orders = structuredClone(ORDERS);
  onBatch: (batch: SyntheticBatch) => void = () => {};
  failure: { method: string; path: string; status: number; error: string } | null = null;
  private nextPositionId = 3;
  private nextOrderId = 3;

  snapshot(queryId: string): FixtureRow[] {
    const prices = this.stocks.map(stock => ({
      symbol: stock.symbol,
      name: stock.name,
      price: stock.price,
      previousClose: stock.previousClose,
      changePercent: (stock.price - stock.previousClose) / stock.previousClose * 100,
      volume: stock.volume,
    }));
    const positions = this.positions.map(position => {
      const stock = this.stock(position.symbol);
      return {
        id: position.id, symbol: position.symbol, name: stock.name,
        quantity: position.quantity, purchasePrice: position.purchase_price,
        currentPrice: stock.price, currentValue: stock.price * position.quantity,
        costBasis: position.purchase_price * position.quantity,
        profitLoss: (stock.price - position.purchase_price) * position.quantity,
        profitLossPercent: (stock.price - position.purchase_price) / position.purchase_price * 100,
      };
    });
    switch (queryId) {
      case 'watchlist-query': return prices.filter(row => this.watchlist.has(row.symbol));
      case 'price-ticker-query': return prices;
      case 'top-gainers-query': return prices.filter(row => row.changePercent > 0);
      case 'top-losers-query': return prices.filter(row => row.changePercent < 0);
      case 'high-volume-query': return prices.filter(row => row.volume > 10_000_000);
      case 'portfolio-query': return positions;
      case 'portfolio-summary-query': {
        const totalValue = positions.reduce((sum, row) => sum + row.currentValue, 0);
        const totalCost = positions.reduce((sum, row) => sum + row.costBasis, 0);
        return [{
          totalValue, totalCost, totalProfitLoss: totalValue - totalCost,
          totalProfitLossPercent: totalCost ? (totalValue - totalCost) / totalCost * 100 : 0,
          positionCount: positions.length,
        }];
      }
      case 'sector-performance-query':
        return [...new Set(this.stocks.map(row => row.sector))].map(sector => {
          const stocks = this.stocks.filter(row => row.sector === sector);
          return {
            sector, stockCount: stocks.length,
            avgChangePercent: stocks.reduce((sum, row) => sum + (row.price - row.previousClose) / row.previousClose * 100, 0) / stocks.length,
            totalVolume: stocks.reduce((sum, row) => sum + row.volume, 0),
            minPrice: Math.min(...stocks.map(row => row.price)),
            maxPrice: Math.max(...stocks.map(row => row.price)),
          };
        });
      case 'active-orders-query':
        return this.orders.map(order => ({
          id: order.id, symbol: order.symbol, orderType: order.order_type,
          targetPrice: order.target_price, currentPrice: this.stock(order.symbol).price,
          quantity: order.quantity, status: order.status, createdAt: order.created_at,
          expiresAt: order.expires_at,
          distancePercent: (order.target_price - this.stock(order.symbol).price) / this.stock(order.symbol).price * 100,
        }));
      case 'stale-orders-query':
      case 'expiring-orders-query': return [];
      default: throw new Error(`Unknown synthetic query ${queryId}`);
    }
  }

  private stock(symbol: string): FixtureStock {
    const stock = this.stocks.find(row => row.symbol === symbol);
    if (!stock) throw new Error(`Unknown fixture symbol ${symbol}`);
    return stock;
  }

  private mutate(change: () => void): void {
    const previous = new Map(QUERY_IDS.map(id => [id, this.snapshot(id)]));
    change();
    for (const id of QUERY_IDS) {
      const key = (row: FixtureRow) => String(row.id ?? row.symbol ?? row.sector ?? 'summary');
      const before = new Map(previous.get(id)!.map(row => [key(row), row]));
      const after = new Map(this.snapshot(id).map(row => [key(row), row]));
      const results: SyntheticBatch['results'] = [];
      for (const [rowKey, row] of after) {
        if (!before.has(rowKey)) results.push({ op: 'c', after: row });
        else if (JSON.stringify(before.get(rowKey)) !== JSON.stringify(row)) {
          results.push({ op: 'u', before: before.get(rowKey), after: row });
        }
      }
      for (const [rowKey, row] of before) {
        if (!after.has(rowKey)) results.push({ op: 'd', before: row });
      }
      if (results.length) this.onBatch({ query_id: id, results });
    }
  }

  changePrice(symbol: string, price: number, volume?: number): void {
    this.mutate(() => {
      this.stock(symbol).price = price;
      if (volume !== undefined) this.stock(symbol).volume = volume;
    });
  }

  handle(request: FixtureRequest): FixtureResponse {
    this.requests.push(structuredClone(request));
    const { method, body = {} } = request;
    let { path } = request;
    if (this.failure?.method === method && this.failure.path === path) {
      const failure = this.failure;
      this.failure = null;
      return { status: failure.status, body: { success: false, error: failure.error } };
    }
    const ok = (data: unknown, status = 200) => ({ status, body: { success: true, data } });
    if (path === '/health') return { status: 200, body: { status: 'healthy' } };
    if (path === '/api/v1/instances') return ok([{ id: this.instanceId }]);
    const instance = path.match(/^\/api\/v1\/instances\/([^/]+)(\/.*)$/);
    if (instance) {
      if (decodeURIComponent(instance[1]) !== this.instanceId) {
        return { status: 404, body: { code: 'INSTANCE_NOT_FOUND', message: 'Instance not found' } };
      }
      path = `/api/v1${instance[2]}`;
    } else if (path.startsWith('/api/v1/')) {
      throw new Error(`Unscoped resource request: ${path}`);
    }
    if (path === '/api/v1/queries' && method === 'POST') {
      const fields = ['id', 'query', 'sources', 'joins', 'queryLanguage', 'autoStart'];
      if (Object.keys(body).some(key => !fields.includes(key))) {
        return { status: 400, body: { code: 'INVALID_REQUEST', message: 'Unknown query field' } };
      }
      const id = requiredString(body, 'id');
      if (this.queries.has(id)) return { status: 409, body: { message: 'Already exists' } };
      this.queries.set(id, body);
      return ok(body, 201);
    }
    const query = path.match(/^\/api\/v1\/queries\/([^/?]+)(\/results|\/start)?$/);
    if (query?.[2] === '/start' && method === 'POST') {
      if (!this.queries.has(query[1])) return { status: 404, body: { code: 'QUERY_NOT_FOUND' } };
      this.queryStatuses.set(query[1], 'Running');
      return ok({ message: 'Started' });
    }
    if (query && method === 'GET') {
      if (!this.queries.has(query[1])) return { status: 404, body: { code: 'QUERY_NOT_FOUND', message: 'Not found' } };
      return query[2]
        ? ok(this.snapshot(query[1]))
        : ok({ id: query[1], status: this.queryStatuses.get(query[1]) ?? 'Running', config: this.queries.get(query[1]) });
    }
    if (path === '/api/v1/reactions' && method === 'POST') {
      if (this.reaction) return { status: 409, body: { code: 'DUPLICATE_RESOURCE' } };
      this.reaction = body;
      return ok(body, 201);
    }
    if (path === '/api/v1/reactions/sse-stream/start' && method === 'POST') {
      this.reactionStatus = 'Running';
      return ok({ message: 'Started' });
    }
    if (path === '/api/v1/reactions/sse-stream' && method === 'GET') {
      return this.reaction
        ? ok({ id: 'sse-stream', status: this.reactionStatus, config: this.reaction })
        : { status: 404, body: { code: 'REACTION_NOT_FOUND', message: 'Not found' } };
    }
    if (path === '/api/stocks' && method === 'GET') return ok(this.stocks);
    if (path === '/api/watchlist' && method === 'GET') {
      return ok([...this.watchlist].map(symbol => ({ symbol })));
    }
    if (path === '/api/watchlist' && method === 'POST') {
      const symbol = requiredString(body, 'symbol');
      this.stock(symbol);
      if (this.watchlist.has(symbol)) return { status: 409, body: { error: `${symbol} already in watchlist` } };
      this.mutate(() => { this.watchlist.add(symbol); });
      return ok({ symbol }, 201);
    }
    if (path.startsWith('/api/watchlist/') && method === 'DELETE') {
      this.mutate(() => { this.watchlist.delete(path.split('/').at(-1)!); });
      return ok(null);
    }
    if (path === '/api/portfolio' && method === 'GET') return ok(this.positions);
    if (path === '/api/portfolio' && method === 'POST') {
      const position = {
        id: this.nextPositionId++, symbol: requiredString(body, 'symbol'),
        quantity: requiredNumber(body, 'quantity'),
        purchase_price: requiredNumber(body, 'purchasePrice'),
        purchase_date: requiredString(body, 'purchaseDate'),
      };
      this.stock(position.symbol);
      this.mutate(() => { this.positions.push(position); });
      return ok(position, 201);
    }
    if (path.startsWith('/api/portfolio/')) {
      const id = Number(path.split('/').at(-1));
      if (method === 'DELETE') {
        this.mutate(() => { this.positions = this.positions.filter(row => row.id !== id); });
        return ok(null);
      }
      if (method === 'PUT') {
        const position = this.positions.find(row => row.id === id);
        if (!position) throw new Error(`Unknown fixture position ${id}`);
        this.mutate(() => {
          position.quantity = requiredNumber(body, 'quantity');
          position.purchase_price = requiredNumber(body, 'purchasePrice');
          position.purchase_date = requiredString(body, 'purchaseDate');
        });
        return ok(position);
      }
    }
    if (path === '/api/orders' && method === 'GET') return ok(this.orders);
    if (path === '/api/orders' && method === 'POST') {
      const order = {
        id: this.nextOrderId++, symbol: requiredString(body, 'symbol'),
        order_type: requiredString(body, 'orderType'),
        target_price: requiredNumber(body, 'targetPrice'),
        quantity: requiredNumber(body, 'quantity'), status: 'pending',
        created_at: FIXED_TIME, expires_at: requiredString(body, 'expiresAt'),
        stale_duration: requiredNumber(body, 'staleDuration'),
        expire_duration: requiredNumber(body, 'expireDuration'),
      };
      this.stock(order.symbol);
      this.mutate(() => { this.orders.push(order); });
      return ok(order, 201);
    }
    if (path.startsWith('/api/orders/')) {
      const id = Number(path.split('/').at(-1));
      if (method === 'DELETE') {
        this.mutate(() => { this.orders = this.orders.filter(row => row.id !== id); });
        return ok(null);
      }
      if (method === 'PUT') {
        this.mutate(() => {
          const order = this.orders.find(row => row.id === id);
          if (!order) throw new Error(`Unknown fixture order ${id}`);
          order.status = requiredString(body, 'status');
        });
        return ok(this.orders.find(row => row.id === id));
      }
    }
    throw new Error(`Unimplemented synthetic request: ${method} ${path}`);
  }
}
