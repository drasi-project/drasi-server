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

import React, { useState, useEffect } from 'react';
import { useQuery } from '@/hooks/useDrasi';
import { Stock } from '@/types';
import { tradingApi, Stock as ApiStock } from '@/services/TradingApi';
import clsx from 'clsx';

export const Watchlist: React.FC = () => {
  const { data, loading, error, lastUpdate } = useQuery<Stock>('watchlist-query');
  const [prevPrices, setPrevPrices] = useState<Map<string, number>>(new Map());
  const [priceChanges, setPriceChanges] = useState<Map<string, 'up' | 'down' | null>>(new Map());
  const [showAddModal, setShowAddModal] = useState(false);
  const [availableStocks, setAvailableStocks] = useState<ApiStock[]>([]);
  const [selectedSymbol, setSelectedSymbol] = useState('');
  const [isAdding, setIsAdding] = useState(false);
  const [isRemoving, setIsRemoving] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);

  // Track price changes for highlight effect
  useEffect(() => {
    if (data) {
      const newPriceChanges = new Map<string, 'up' | 'down' | null>();
      
      data.forEach(stock => {
        const prevPrice = prevPrices.get(stock.symbol);
        if (prevPrice !== undefined && prevPrice !== stock.price) {
          newPriceChanges.set(stock.symbol, stock.price > prevPrice ? 'up' : 'down');
          setTimeout(() => {
            setPriceChanges(prev => {
              const updated = new Map(prev);
              updated.set(stock.symbol, null);
              return updated;
            });
          }, 500);
        }
      });

      setPriceChanges(newPriceChanges);
      setPrevPrices(new Map(data.map(s => [s.symbol, s.price])));
    }
  }, [data]);

  // Load available stocks when modal opens
  useEffect(() => {
    if (showAddModal) {
      loadAvailableStocks();
    }
  }, [showAddModal]);

  const loadAvailableStocks = async () => {
    try {
      const stocks = await tradingApi.getStocks();
      // Filter out stocks already in watchlist
      const watchlistSymbols = new Set(data?.map(s => s.symbol) || []);
      const available = stocks.filter(s => !watchlistSymbols.has(s.symbol));
      setAvailableStocks(available);
      if (available.length > 0) {
        setSelectedSymbol(available[0].symbol);
      }
    } catch (err) {
      console.error('Failed to load stocks:', err);
      setActionError('Failed to load available stocks');
    }
  };

  const handleAddToWatchlist = async () => {
    if (!selectedSymbol) return;
    
    setIsAdding(true);
    setActionError(null);
    try {
      await tradingApi.addToWatchlist(selectedSymbol);
      setShowAddModal(false);
      setSelectedSymbol('');
      // The UI will update automatically via Drasi SSE
    } catch (err) {
      setActionError(err instanceof Error ? err.message : 'Failed to add to watchlist');
    } finally {
      setIsAdding(false);
    }
  };

  const handleRemoveFromWatchlist = async (symbol: string) => {
    setIsRemoving(symbol);
    setActionError(null);
    try {
      await tradingApi.removeFromWatchlist(symbol);
      // The UI will update automatically via Drasi SSE
    } catch (err) {
      setActionError(err instanceof Error ? err.message : 'Failed to remove from watchlist');
    } finally {
      setIsRemoving(null);
    }
  };

  const formatPrice = (price: number) => {
    return new Intl.NumberFormat('en-US', {
      style: 'currency',
      currency: 'USD',
      minimumFractionDigits: 2,
      maximumFractionDigits: 2,
    }).format(price);
  };

  const formatPercent = (percent: number) => {
    const formatted = Math.abs(percent).toFixed(2);
    return `${percent >= 0 ? '+' : '-'}${formatted}%`;
  };

  if (loading && !data) {
    return (
      <div className="bg-trading-card rounded-lg p-6 border border-trading-border h-[400px] flex flex-col">
        <h2 className="text-xl font-bold mb-4">Watchlist</h2>
        <div className="flex items-center justify-center flex-1">
          <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-trading-blue"></div>
        </div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="bg-trading-card rounded-lg p-6 border border-trading-border h-[400px]">
        <h2 className="text-xl font-bold mb-4">Watchlist</h2>
        <div className="text-trading-red">Error: {error}</div>
      </div>
    );
  }

  return (
    <div className="bg-trading-card rounded-lg border border-trading-border h-[400px] flex flex-col">
      <div className="flex justify-between items-center p-6 pb-4 flex-shrink-0">
        <div className="flex items-center gap-3">
          <h2 className="text-xl font-bold">Watchlist</h2>
          <button
            onClick={() => setShowAddModal(true)}
            className="p-1 rounded hover:bg-trading-border/50 transition-colors text-trading-blue"
            title="Add to watchlist"
          >
            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
            </svg>
          </button>
        </div>
        {lastUpdate && (
          <span className="text-xs text-gray-500">
            Updated: {lastUpdate.toLocaleTimeString()}
          </span>
        )}
      </div>

      {actionError && (
        <div className="mx-6 mb-2 p-2 bg-red-900/30 border border-red-500/50 rounded text-sm text-red-400">
          {actionError}
        </div>
      )}
      
      <div className="overflow-auto flex-1 px-6 pb-6">
        <table className="w-full">
          <thead className="sticky top-0 bg-trading-card z-10">
            <tr className="border-b border-trading-border">
              <th className="text-left py-2 px-2 text-sm font-medium text-gray-400">Symbol</th>
              <th className="text-left py-2 px-2 text-sm font-medium text-gray-400">Name</th>
              <th className="text-right py-2 px-2 text-sm font-medium text-gray-400">Price</th>
              <th className="text-right py-2 px-2 text-sm font-medium text-gray-400">Change</th>
              <th className="w-10"></th>
            </tr>
          </thead>
          <tbody>
            {data?.map((stock) => {
              const change = priceChanges.get(stock.symbol);
              return (
                <tr 
                  key={stock.symbol} 
                  className={clsx(
                    "border-b border-trading-border/50 hover:bg-trading-border/20 transition-colors",
                    change === 'up' && 'price-up',
                    change === 'down' && 'price-down'
                  )}
                >
                  <td className="py-3 px-2 font-medium">{stock.symbol}</td>
                  <td className="py-3 px-2 text-sm text-gray-300">{stock.name}</td>
                  <td className="py-3 px-2 text-right font-mono">
                    {formatPrice(stock.price)}
                  </td>
                  <td className={clsx(
                    "py-3 px-2 text-right font-mono text-sm",
                    stock.changePercent >= 0 ? "text-trading-green" : "text-trading-red"
                  )}>
                    <span className="inline-flex items-center gap-1">
                      {stock.changePercent >= 0 ? (
                        <svg className="w-3 h-3" fill="currentColor" viewBox="0 0 20 20">
                          <path d="M10 5l5 7H5l5-7z"/>
                        </svg>
                      ) : (
                        <svg className="w-3 h-3" fill="currentColor" viewBox="0 0 20 20">
                          <path d="M10 15l-5-7h10l-5 7z"/>
                        </svg>
                      )}
                      {formatPercent(stock.changePercent)}
                    </span>
                  </td>
                  <td className="py-3 px-2">
                    <button
                      onClick={() => handleRemoveFromWatchlist(stock.symbol)}
                      disabled={isRemoving === stock.symbol}
                      className="p-1 rounded hover:bg-red-900/30 transition-colors text-gray-500 hover:text-red-400 disabled:opacity-50"
                      title="Remove from watchlist"
                    >
                      {isRemoving === stock.symbol ? (
                        <div className="w-4 h-4 animate-spin rounded-full border-2 border-gray-500 border-t-transparent"></div>
                      ) : (
                        <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                        </svg>
                      )}
                    </button>
                  </td>
                </tr>
              );
            })}
            {(!data || data.length === 0) && (
              <tr>
                <td colSpan={5} className="py-8 text-center text-gray-500">
                  No stocks in watchlist. Click + to add.
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>

      {/* Add to Watchlist Modal */}
      {showAddModal && (
        <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
          <div className="bg-trading-card border border-trading-border rounded-lg p-6 w-96 max-w-[90vw]">
            <h3 className="text-lg font-bold mb-4">Add to Watchlist</h3>
            
            {availableStocks.length === 0 ? (
              <p className="text-gray-400 mb-4">No more stocks available to add.</p>
            ) : (
              <>
                <label className="block text-sm text-gray-400 mb-2">Select Stock</label>
                <select
                  value={selectedSymbol}
                  onChange={(e) => setSelectedSymbol(e.target.value)}
                  className="w-full bg-trading-bg border border-trading-border rounded p-2 mb-4 text-white"
                >
                  {availableStocks.map(stock => (
                    <option key={stock.symbol} value={stock.symbol}>
                      {stock.symbol} - {stock.name}
                    </option>
                  ))}
                </select>
              </>
            )}

            <div className="flex gap-3 justify-end">
              <button
                onClick={() => setShowAddModal(false)}
                className="px-4 py-2 rounded border border-trading-border hover:bg-trading-border/30 transition-colors"
              >
                Cancel
              </button>
              <button
                onClick={handleAddToWatchlist}
                disabled={isAdding || !selectedSymbol || availableStocks.length === 0}
                className="px-4 py-2 rounded bg-trading-blue hover:bg-trading-blue/80 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
              >
                {isAdding ? 'Adding...' : 'Add'}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};
