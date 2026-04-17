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
 * Trading API Service
 * Handles CRUD operations for watchlist and portfolio via the Trading API server.
 * Changes made through this API flow through PostgreSQL CDC to Drasi and back to the UI.
 */

const TRADING_API_URL = 'http://localhost:9200';

export interface Stock {
  symbol: string;
  name: string;
  sector: string;
  industry: string;
}

export interface WatchlistItem {
  id: number;
  symbol: string;
  name: string;
  sector: string;
  added_at: string;
}

export interface PortfolioPosition {
  id: number;
  symbol: string;
  name: string;
  sector: string;
  quantity: number;
  purchase_price: number;
  purchase_date: string;
}

export interface LimitOrder {
  id: number;
  symbol: string;
  name?: string;
  sector?: string;
  order_type: 'buy' | 'sell';
  target_price: number;
  quantity: number;
  status: 'pending' | 'stale' | 'filled' | 'expired' | 'cancelled';
  created_at: string;
  filled_at?: string;
  expires_at?: string;
}

interface ApiResponse<T> {
  success: boolean;
  data?: T;
  error?: string;
  message?: string;
}

class TradingApiService {
  private baseUrl: string;

  constructor(baseUrl: string = TRADING_API_URL) {
    this.baseUrl = baseUrl;
  }

  /**
   * Check response status and parse JSON, throwing a structured error on failure.
   */
  private async parseResponse<T>(response: Response, context: string): Promise<T> {
    if (!response.ok) {
      let errorMessage = `${context}: ${response.status} ${response.statusText}`;
      try {
        const body = await response.json();
        if (body.error) errorMessage = `${context}: ${body.error}`;
      } catch {
        // Response body wasn't JSON, use the status text
      }
      throw new Error(errorMessage);
    }
    const data: ApiResponse<T> = await response.json();
    if (!data.success) {
      throw new Error(data.error || context);
    }
    return data.data as T;
  }

  // ============================================================================
  // Stocks API
  // ============================================================================

  /**
   * Get all available stocks (for dropdowns)
   */
  async getStocks(): Promise<Stock[]> {
    const response = await fetch(`${this.baseUrl}/api/stocks`);
    return await this.parseResponse<Stock[]>(response, 'Failed to fetch stocks');
  }

  // ============================================================================
  // Watchlist API
  // ============================================================================

  /**
   * Get current watchlist items
   */
  async getWatchlist(): Promise<WatchlistItem[]> {
    const response = await fetch(`${this.baseUrl}/api/watchlist`);
    return await this.parseResponse<WatchlistItem[]>(response, 'Failed to fetch watchlist');
  }

  /**
   * Add a stock to the watchlist
   */
  async addToWatchlist(symbol: string): Promise<WatchlistItem> {
    const response = await fetch(`${this.baseUrl}/api/watchlist`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ symbol })
    });
    const data = await this.parseResponse<WatchlistItem>(response, 'Failed to add to watchlist');
    return data;
  }

  /**
   * Remove a stock from the watchlist
   */
  async removeFromWatchlist(symbol: string): Promise<void> {
    const response = await fetch(`${this.baseUrl}/api/watchlist/${symbol}`, {
      method: 'DELETE'
    });
    await this.parseResponse<void>(response, 'Failed to remove from watchlist');
  }

  // ============================================================================
  // Portfolio API
  // ============================================================================

  /**
   * Get current portfolio positions
   */
  async getPortfolio(): Promise<PortfolioPosition[]> {
    const response = await fetch(`${this.baseUrl}/api/portfolio`);
    return await this.parseResponse<PortfolioPosition[]>(response, 'Failed to fetch portfolio');
  }

  /**
   * Add a new portfolio position
   */
  async addPosition(symbol: string, quantity: number, purchasePrice: number, purchaseDate?: string): Promise<PortfolioPosition> {
    const response = await fetch(`${this.baseUrl}/api/portfolio`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ symbol, quantity, purchasePrice, purchaseDate })
    });
    return await this.parseResponse<PortfolioPosition>(response, 'Failed to add position');
  }

  /**
   * Update an existing portfolio position
   */
  async updatePosition(id: number, updates: { quantity?: number; purchasePrice?: number; purchaseDate?: string }): Promise<PortfolioPosition> {
    const response = await fetch(`${this.baseUrl}/api/portfolio/${id}`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(updates)
    });
    return await this.parseResponse<PortfolioPosition>(response, 'Failed to update position');
  }

  /**
   * Delete a portfolio position
   */
  async deletePosition(id: number): Promise<void> {
    const response = await fetch(`${this.baseUrl}/api/portfolio/${id}`, {
      method: 'DELETE'
    });
    await this.parseResponse<void>(response, 'Failed to delete position');
  }

  // ============================================================================
  // Limit Orders API
  // ============================================================================

  /**
   * Get all limit orders (optionally filtered by status)
   */
  async getOrders(status?: string): Promise<LimitOrder[]> {
    const url = status 
      ? `${this.baseUrl}/api/orders?status=${status}`
      : `${this.baseUrl}/api/orders`;
    const response = await fetch(url);
    return await this.parseResponse<LimitOrder[]>(response, 'Failed to fetch orders');
  }

  /**
   * Create a new limit order
   */
  async createOrder(
    symbol: string, 
    orderType: 'buy' | 'sell', 
    targetPrice: number, 
    quantity: number,
    expiresAt?: string,
    staleDuration?: number,
    expireDuration?: number
  ): Promise<LimitOrder> {
    const response = await fetch(`${this.baseUrl}/api/orders`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ symbol, orderType, targetPrice, quantity, expiresAt, staleDuration, expireDuration })
    });
    return await this.parseResponse<LimitOrder>(response, 'Failed to create order');
  }

  /**
   * Update a limit order's status
   */
  async updateOrderStatus(id: number, status: LimitOrder['status']): Promise<LimitOrder> {
    const response = await fetch(`${this.baseUrl}/api/orders/${id}`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ status })
    });
    return await this.parseResponse<LimitOrder>(response, 'Failed to update order');
  }

  /**
   * Cancel (delete) a limit order
   */
  async cancelOrder(id: number): Promise<void> {
    const response = await fetch(`${this.baseUrl}/api/orders/${id}`, {
      method: 'DELETE'
    });
    await this.parseResponse<void>(response, 'Failed to cancel order');
  }

  // ============================================================================
  // Health Check
  // ============================================================================

  /**
   * Check if the Trading API is available
   */
  async isHealthy(): Promise<boolean> {
    try {
      const response = await fetch(`${this.baseUrl}/health`);
      const data = await response.json();
      return data.status === 'healthy';
    } catch {
      return false;
    }
  }
}

// Export singleton instance
export const tradingApi = new TradingApiService();
export default tradingApi;
