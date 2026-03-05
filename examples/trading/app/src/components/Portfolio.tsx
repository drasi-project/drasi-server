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

import React, { useMemo, useState, useEffect } from 'react';
import { useQuery } from '@/hooks/useDrasi';
import { PortfolioPosition } from '@/types';
import { tradingApi, Stock as ApiStock } from '@/services/TradingApi';
import { PositionDialog, PositionFormData } from './PositionDialog';
import clsx from 'clsx';

interface DeletingPosition {
  id: number;
  symbol: string;
  name: string;
  quantity: number;
  purchasePrice: number;
  purchaseDate: string;
  currentPrice?: number;
  currentValue?: number;
  profitLoss?: number;
}

export const Portfolio: React.FC = () => {
  const { data, loading, error, lastUpdate } = useQuery<PortfolioPosition>('portfolio-query');
  const [dialogMode, setDialogMode] = useState<'add' | 'edit' | null>(null);
  const [deletingPosition, setDeletingPosition] = useState<DeletingPosition | null>(null);
  const [availableStocks, setAvailableStocks] = useState<ApiStock[]>([]);
  const [editingPosition, setEditingPosition] = useState<PositionFormData | null>(null);
  
  // Loading states
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [isDeleting, setIsDeleting] = useState<number | null>(null);

  // Track price changes for highlight effect
  const [prevPrices, setPrevPrices] = useState<Map<string, number>>(new Map());
  const [priceChanges, setPriceChanges] = useState<Map<string, 'up' | 'down' | null>>(new Map());

  useEffect(() => {
    if (data) {
      const newPriceChanges = new Map<string, 'up' | 'down' | null>();
      
      data.forEach(pos => {
        const prevPrice = prevPrices.get(pos.symbol);
        if (prevPrice !== undefined && prevPrice !== pos.currentPrice) {
          newPriceChanges.set(pos.symbol, (pos.currentPrice || 0) > prevPrice ? 'up' : 'down');
          setTimeout(() => {
            setPriceChanges(prev => {
              const updated = new Map(prev);
              updated.set(pos.symbol, null);
              return updated;
            });
          }, 500);
        }
      });

      setPriceChanges(newPriceChanges);
      setPrevPrices(new Map(data.map(p => [p.symbol, p.currentPrice || 0])));
    }
  }, [data]);

  // Load available stocks when dialog opens in add mode
  useEffect(() => {
    if (dialogMode === 'add') {
      loadAvailableStocks();
    }
  }, [dialogMode]);

  const loadAvailableStocks = async () => {
    try {
      const stocks = await tradingApi.getStocks();
      // Filter out stocks already in portfolio
      const portfolioSymbols = new Set(data?.map(p => p.symbol) || []);
      const available = stocks.filter(s => !portfolioSymbols.has(s.symbol));
      setAvailableStocks(available);
    } catch (err) {
      console.error('Failed to load stocks:', err);
    }
  };

  const handlePositionSubmit = async (formData: PositionFormData) => {
    setIsSubmitting(true);
    try {
      if (dialogMode === 'add') {
        await tradingApi.addPosition(formData.symbol, formData.quantity, formData.purchasePrice, formData.purchaseDate);
      } else if (dialogMode === 'edit' && formData.id) {
        await tradingApi.updatePosition(formData.id, {
          quantity: formData.quantity,
          purchasePrice: formData.purchasePrice,
          purchaseDate: formData.purchaseDate
        });
      }
      setDialogMode(null);
      setEditingPosition(null);
      // The UI will update automatically via Drasi SSE
    } finally {
      setIsSubmitting(false);
    }
  };

  const handleDeletePosition = async () => {
    if (!deletingPosition) return;
    
    setIsDeleting(deletingPosition.id);
    try {
      await tradingApi.deletePosition(deletingPosition.id);
      setDeletingPosition(null);
      // The UI will update automatically via Drasi SSE
    } finally {
      setIsDeleting(null);
    }
  };

  const openEditDialog = (position: PortfolioPosition) => {
    // We need to look up the portfolio ID and purchase date from the API
    tradingApi.getPortfolio().then(portfolio => {
      const found = portfolio.find(p => p.symbol === position.symbol);
      if (found) {
        setEditingPosition({
          id: found.id,
          symbol: position.symbol,
          name: position.name,
          quantity: position.quantity,
          purchasePrice: position.purchasePrice || 0,
          purchaseDate: found.purchase_date ? found.purchase_date.split('T')[0] : new Date().toISOString().split('T')[0]
        });
        setDialogMode('edit');
      }
    }).catch((err) => {
      console.error('Failed to load position details:', err);
    });
  };

  const openDeleteConfirm = (position: PortfolioPosition) => {
    tradingApi.getPortfolio().then(portfolio => {
      const found = portfolio.find(p => p.symbol === position.symbol);
      if (found) {
        setDeletingPosition({
          id: found.id,
          symbol: position.symbol,
          name: position.name,
          quantity: position.quantity,
          purchasePrice: position.purchasePrice || 0,
          purchaseDate: found.purchase_date ? found.purchase_date.split('T')[0] : '',
          currentPrice: position.currentPrice,
          currentValue: position.currentValue,
          profitLoss: position.profitLoss
        });
      }
    }).catch((err) => {
      console.error('Failed to load position details:', err);
    });
  };

  const portfolioStats = useMemo(() => {
    if (!data || data.length === 0) {
      return {
        totalValue: 0,
        totalCost: 0,
        totalProfitLoss: 0,
        totalProfitLossPercent: 0,
        positions: 0
      };
    }

    const totalValue = data.reduce((sum, pos) => {
      const value = Number(pos.currentValue) || 0;
      return sum + (isNaN(value) ? 0 : value);
    }, 0);
    
    const totalCost = data.reduce((sum, pos) => {
      let cost = Number(pos.costBasis) || 0;
      if (cost === 0 && pos.purchasePrice && pos.quantity) {
        cost = Number(pos.purchasePrice) * Number(pos.quantity);
      }
      return sum + (isNaN(cost) ? 0 : cost);
    }, 0);
    
    const totalProfitLoss = totalValue - totalCost;
    const totalProfitLossPercent = totalCost > 0 ? (totalProfitLoss / totalCost) * 100 : 0;

    return {
      totalValue,
      totalCost,
      totalProfitLoss,
      totalProfitLossPercent,
      positions: data.length
    };
  }, [data]);

  const formatCurrency = (value: number) => {
    return new Intl.NumberFormat('en-US', {
      style: 'currency',
      currency: 'USD',
      minimumFractionDigits: 2,
      maximumFractionDigits: 2,
    }).format(value);
  };

  const formatPercent = (percent: number) => {
    const formatted = Math.abs(percent).toFixed(2);
    return `${percent >= 0 ? '+' : '-'}${formatted}%`;
  };

  if (loading && !data) {
    return (
      <div className="bg-trading-card rounded-lg p-6 border border-trading-border h-[400px] flex flex-col">
        <h2 className="text-xl font-bold mb-4">Portfolio</h2>
        <div className="flex items-center justify-center flex-1">
          <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-trading-blue"></div>
        </div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="bg-trading-card rounded-lg p-6 border border-trading-border h-[400px]">
        <h2 className="text-xl font-bold mb-4">Portfolio</h2>
        <div className="text-trading-red">Error: {error}</div>
      </div>
    );
  }

  return (
    <div className="bg-trading-card rounded-lg border border-trading-border h-[400px] flex flex-col">
      <div className="flex justify-between items-center p-6 pb-4 flex-shrink-0">
        <div className="flex items-center gap-3">
          <h2 className="text-xl font-bold">Portfolio</h2>
          <button
            onClick={() => setDialogMode('add')}
            className="p-1 rounded hover:bg-trading-border/50 transition-colors text-trading-blue"
            title="Add position"
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

      {/* Portfolio Summary */}
      <div className="grid grid-cols-2 md:grid-cols-4 gap-4 px-6 pb-4 flex-shrink-0">
        <div className="bg-trading-bg rounded p-3">
          <div className="text-xs text-gray-400 mb-1">Total Value</div>
          <div className="text-lg font-bold">{formatCurrency(portfolioStats.totalValue)}</div>
        </div>
        <div className="bg-trading-bg rounded p-3">
          <div className="text-xs text-gray-400 mb-1">Total Cost</div>
          <div className="text-lg font-bold">{formatCurrency(portfolioStats.totalCost)}</div>
        </div>
        <div className="bg-trading-bg rounded p-3">
          <div className="text-xs text-gray-400 mb-1">Total P/L</div>
          <div className={clsx(
            "text-lg font-bold",
            portfolioStats.totalProfitLoss >= 0 ? "text-trading-green" : "text-trading-red"
          )}>
            {formatCurrency(portfolioStats.totalProfitLoss)}
          </div>
        </div>
        <div className="bg-trading-bg rounded p-3">
          <div className="text-xs text-gray-400 mb-1">Total Return</div>
          <div className={clsx(
            "text-lg font-bold",
            portfolioStats.totalProfitLossPercent >= 0 ? "text-trading-green" : "text-trading-red"
          )}>
            {formatPercent(portfolioStats.totalProfitLossPercent)}
          </div>
        </div>
      </div>

      {/* Positions Table */}
      <div className="overflow-auto flex-1 px-6 pb-6">
        <table className="w-full">
          <thead className="sticky top-0 bg-trading-card z-10">
            <tr className="border-b border-trading-border">
              <th className="text-left py-2 px-2 text-sm font-medium text-gray-400">Symbol</th>
              <th className="text-left py-2 px-2 text-sm font-medium text-gray-400">Name</th>
              <th className="text-right py-2 px-2 text-sm font-medium text-gray-400">Qty</th>
              <th className="text-right py-2 px-2 text-sm font-medium text-gray-400">Avg Cost</th>
              <th className="text-right py-2 px-2 text-sm font-medium text-gray-400">Current</th>
              <th className="text-right py-2 px-2 text-sm font-medium text-gray-400">Value</th>
              <th className="text-right py-2 px-2 text-sm font-medium text-gray-400">P/L</th>
              <th className="text-right py-2 px-2 text-sm font-medium text-gray-400">P/L %</th>
              <th className="w-20"></th>
            </tr>
          </thead>
          <tbody>
            {data?.map((position) => {
              const change = priceChanges.get(position.symbol);
              return (
                <tr 
                  key={position.symbol} 
                  className={clsx(
                    "border-b border-trading-border/50 hover:bg-trading-border/20 transition-colors",
                    change === 'up' && 'price-up',
                    change === 'down' && 'price-down'
                  )}
                >
                  <td className="py-3 px-2 font-medium">{position.symbol}</td>
                  <td className="py-3 px-2 text-sm text-gray-300">{position.name}</td>
                  <td className="py-3 px-2 text-right">{position.quantity}</td>
                  <td className="py-3 px-2 text-right font-mono text-sm">
                    {formatCurrency(position.purchasePrice || 0)}
                  </td>
                  <td className="py-3 px-2 text-right font-mono text-sm">
                    {position.currentPrice ? formatCurrency(position.currentPrice) : '-'}
                  </td>
                  <td className="py-3 px-2 text-right font-mono">
                    {position.currentValue ? formatCurrency(position.currentValue) : '-'}
                  </td>
                  <td className={clsx(
                    "py-3 px-2 text-right font-mono text-sm",
                    !position.profitLoss ? "" : position.profitLoss >= 0 ? "text-trading-green" : "text-trading-red"
                  )}>
                    {position.profitLoss != null ? formatCurrency(position.profitLoss) : '-'}
                  </td>
                  <td className={clsx(
                    "py-3 px-2 text-right font-mono text-sm",
                    !position.profitLossPercent ? "" : position.profitLossPercent >= 0 ? "text-trading-green" : "text-trading-red"
                  )}>
                    {position.profitLossPercent != null ? (
                      <span className="inline-flex items-center gap-1">
                        {position.profitLossPercent >= 0 ? (
                          <svg className="w-3 h-3" fill="currentColor" viewBox="0 0 20 20">
                            <path d="M10 5l5 7H5l5-7z"/>
                          </svg>
                        ) : (
                          <svg className="w-3 h-3" fill="currentColor" viewBox="0 0 20 20">
                            <path d="M10 15l-5-7h10l-5 7z"/>
                          </svg>
                        )}
                        {formatPercent(position.profitLossPercent)}
                      </span>
                    ) : '-'}
                  </td>
                  <td className="py-3 px-2">
                    <div className="flex gap-1 justify-end">
                      <button
                        onClick={() => openEditDialog(position)}
                        className="p-1 rounded hover:bg-trading-border/50 transition-colors text-gray-500 hover:text-trading-blue"
                        title="Edit position"
                      >
                        <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
                        </svg>
                      </button>
                      <button
                        onClick={() => openDeleteConfirm(position)}
                        className="p-1 rounded hover:bg-red-900/30 transition-colors text-gray-500 hover:text-red-400"
                        title="Delete position"
                      >
                        <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
                        </svg>
                      </button>
                    </div>
                  </td>
                </tr>
              );
            })}
            {(!data || data.length === 0) && (
              <tr>
                <td colSpan={9} className="py-8 text-center text-gray-500">
                  No positions in portfolio. Click + to add.
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>

      {/* Position Dialog (Add/Edit) */}
      <PositionDialog
        isOpen={dialogMode !== null}
        mode={dialogMode || 'add'}
        position={editingPosition || undefined}
        availableStocks={availableStocks}
        isSubmitting={isSubmitting}
        onSubmit={handlePositionSubmit}
        onCancel={() => {
          setDialogMode(null);
          setEditingPosition(null);
        }}
      />

      {/* Delete Confirmation Modal */}
      {deletingPosition && (
        <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
          <div className="bg-trading-card border border-trading-border rounded-lg p-6 w-96 max-w-[90vw]">
            <h3 className="text-lg font-bold mb-4 text-red-400">Delete Position</h3>
            
            {/* Position details */}
            <div className="bg-trading-bg rounded p-4 mb-4 space-y-2">
              <div className="flex justify-between">
                <span className="text-gray-400">Stock:</span>
                <span className="font-bold">{deletingPosition.symbol} - {deletingPosition.name}</span>
              </div>
              <div className="flex justify-between">
                <span className="text-gray-400">Quantity:</span>
                <span>{deletingPosition.quantity} shares</span>
              </div>
              <div className="flex justify-between">
                <span className="text-gray-400">Purchase Price:</span>
                <span>{formatCurrency(deletingPosition.purchasePrice)}</span>
              </div>
              <div className="flex justify-between">
                <span className="text-gray-400">Purchase Date:</span>
                <span>{deletingPosition.purchaseDate || 'N/A'}</span>
              </div>
              {deletingPosition.currentPrice && (
                <div className="flex justify-between">
                  <span className="text-gray-400">Current Price:</span>
                  <span>{formatCurrency(deletingPosition.currentPrice)}</span>
                </div>
              )}
              {deletingPosition.currentValue && (
                <div className="flex justify-between">
                  <span className="text-gray-400">Current Value:</span>
                  <span className="font-bold">{formatCurrency(deletingPosition.currentValue)}</span>
                </div>
              )}
              {deletingPosition.profitLoss != null && (
                <div className="flex justify-between">
                  <span className="text-gray-400">P/L:</span>
                  <span className={clsx(
                    "font-bold",
                    deletingPosition.profitLoss >= 0 ? "text-trading-green" : "text-trading-red"
                  )}>
                    {formatCurrency(deletingPosition.profitLoss)}
                  </span>
                </div>
              )}
            </div>
            
            <p className="text-gray-400 text-sm mb-4">This action cannot be undone.</p>
            
            <div className="flex gap-3 justify-end">
              <button
                onClick={() => setDeletingPosition(null)}
                className="px-4 py-2 rounded border border-trading-border hover:bg-trading-border/30 transition-colors"
              >
                Cancel
              </button>
              <button
                onClick={handleDeletePosition}
                disabled={isDeleting !== null}
                className="px-4 py-2 rounded bg-red-600 hover:bg-red-700 transition-colors disabled:opacity-50"
              >
                {isDeleting !== null ? 'Deleting...' : 'Delete'}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};