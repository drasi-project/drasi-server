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

  // ============================================================================
  // Stocks API
  // ============================================================================

  /**
   * Get all available stocks (for dropdowns)
   */
  async getStocks(): Promise<Stock[]> {
    const response = await fetch(`${this.baseUrl}/api/stocks`);
    const data: ApiResponse<Stock[]> = await response.json();
    if (!data.success) {
      throw new Error(data.error || 'Failed to fetch stocks');
    }
    return data.data || [];
  }

  // ============================================================================
  // Watchlist API
  // ============================================================================

  /**
   * Get current watchlist items
   */
  async getWatchlist(): Promise<WatchlistItem[]> {
    const response = await fetch(`${this.baseUrl}/api/watchlist`);
    const data: ApiResponse<WatchlistItem[]> = await response.json();
    if (!data.success) {
      throw new Error(data.error || 'Failed to fetch watchlist');
    }
    return data.data || [];
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
    const data: ApiResponse<WatchlistItem> = await response.json();
    if (!data.success) {
      throw new Error(data.error || 'Failed to add to watchlist');
    }
    return data.data!;
  }

  /**
   * Remove a stock from the watchlist
   */
  async removeFromWatchlist(symbol: string): Promise<void> {
    const response = await fetch(`${this.baseUrl}/api/watchlist/${symbol}`, {
      method: 'DELETE'
    });
    const data: ApiResponse<void> = await response.json();
    if (!data.success) {
      throw new Error(data.error || 'Failed to remove from watchlist');
    }
  }

  // ============================================================================
  // Portfolio API
  // ============================================================================

  /**
   * Get current portfolio positions
   */
  async getPortfolio(): Promise<PortfolioPosition[]> {
    const response = await fetch(`${this.baseUrl}/api/portfolio`);
    const data: ApiResponse<PortfolioPosition[]> = await response.json();
    if (!data.success) {
      throw new Error(data.error || 'Failed to fetch portfolio');
    }
    return data.data || [];
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
    const data: ApiResponse<PortfolioPosition> = await response.json();
    if (!data.success) {
      throw new Error(data.error || 'Failed to add position');
    }
    return data.data!;
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
    const data: ApiResponse<PortfolioPosition> = await response.json();
    if (!data.success) {
      throw new Error(data.error || 'Failed to update position');
    }
    return data.data!;
  }

  /**
   * Delete a portfolio position
   */
  async deletePosition(id: number): Promise<void> {
    const response = await fetch(`${this.baseUrl}/api/portfolio/${id}`, {
      method: 'DELETE'
    });
    const data: ApiResponse<void> = await response.json();
    if (!data.success) {
      throw new Error(data.error || 'Failed to delete position');
    }
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
