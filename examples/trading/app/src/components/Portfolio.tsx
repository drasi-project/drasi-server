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
import clsx from 'clsx';

interface EditingPosition {
  id: number;
  quantity: number;
  purchasePrice: number;
}

export const Portfolio: React.FC = () => {
  const { data, loading, error, lastUpdate } = useQuery<PortfolioPosition>('portfolio-query');
  const [showAddModal, setShowAddModal] = useState(false);
  const [showEditModal, setShowEditModal] = useState(false);
  const [showDeleteConfirm, setShowDeleteConfirm] = useState<number | null>(null);
  const [availableStocks, setAvailableStocks] = useState<ApiStock[]>([]);
  const [editingPosition, setEditingPosition] = useState<EditingPosition | null>(null);
  
  // Form state for adding position
  const [newSymbol, setNewSymbol] = useState('');
  const [newQuantity, setNewQuantity] = useState('');
  const [newPrice, setNewPrice] = useState('');
  
  // Loading and error states
  const [isAdding, setIsAdding] = useState(false);
  const [isEditing, setIsEditing] = useState(false);
  const [isDeleting, setIsDeleting] = useState<number | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);

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

  // Load available stocks when add modal opens
  useEffect(() => {
    if (showAddModal) {
      loadAvailableStocks();
    }
  }, [showAddModal]);

  const loadAvailableStocks = async () => {
    try {
      const stocks = await tradingApi.getStocks();
      // Filter out stocks already in portfolio
      const portfolioSymbols = new Set(data?.map(p => p.symbol) || []);
      const available = stocks.filter(s => !portfolioSymbols.has(s.symbol));
      setAvailableStocks(available);
      if (available.length > 0) {
        setNewSymbol(available[0].symbol);
      }
    } catch (err) {
      console.error('Failed to load stocks:', err);
      setActionError('Failed to load available stocks');
    }
  };

  const handleAddPosition = async () => {
    if (!newSymbol || !newQuantity || !newPrice) return;
    
    setIsAdding(true);
    setActionError(null);
    try {
      await tradingApi.addPosition(newSymbol, parseInt(newQuantity), parseFloat(newPrice));
      setShowAddModal(false);
      resetAddForm();
      // The UI will update automatically via Drasi SSE
    } catch (err) {
      setActionError(err instanceof Error ? err.message : 'Failed to add position');
    } finally {
      setIsAdding(false);
    }
  };

  const handleEditPosition = async () => {
    if (!editingPosition) return;
    
    setIsEditing(true);
    setActionError(null);
    try {
      await tradingApi.updatePosition(editingPosition.id, {
        quantity: editingPosition.quantity,
        purchasePrice: editingPosition.purchasePrice
      });
      setShowEditModal(false);
      setEditingPosition(null);
      // The UI will update automatically via Drasi SSE
    } catch (err) {
      setActionError(err instanceof Error ? err.message : 'Failed to update position');
    } finally {
      setIsEditing(false);
    }
  };

  const handleDeletePosition = async (id: number) => {
    setIsDeleting(id);
    setActionError(null);
    try {
      await tradingApi.deletePosition(id);
      setShowDeleteConfirm(null);
      // The UI will update automatically via Drasi SSE
    } catch (err) {
      setActionError(err instanceof Error ? err.message : 'Failed to delete position');
    } finally {
      setIsDeleting(null);
    }
  };

  const openEditModal = (position: PortfolioPosition) => {
    // We need to look up the portfolio ID from the API since our Drasi data doesn't include it
    // For now, we'll use symbol as a workaround - the API will need the ID
    tradingApi.getPortfolio().then(portfolio => {
      const found = portfolio.find(p => p.symbol === position.symbol);
      if (found) {
        setEditingPosition({
          id: found.id,
          quantity: position.quantity,
          purchasePrice: position.purchasePrice || 0
        });
        setShowEditModal(true);
      }
    }).catch(() => {
      setActionError('Failed to load position details');
    });
  };

  const openDeleteConfirm = (position: PortfolioPosition) => {
    tradingApi.getPortfolio().then(portfolio => {
      const found = portfolio.find(p => p.symbol === position.symbol);
      if (found) {
        setShowDeleteConfirm(found.id);
      }
    }).catch(() => {
      setActionError('Failed to load position details');
    });
  };

  const resetAddForm = () => {
    setNewSymbol('');
    setNewQuantity('');
    setNewPrice('');
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
            onClick={() => setShowAddModal(true)}
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

      {actionError && (
        <div className="mx-6 mb-2 p-2 bg-red-900/30 border border-red-500/50 rounded text-sm text-red-400">
          {actionError}
        </div>
      )}

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
                        onClick={() => openEditModal(position)}
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

      {/* Add Position Modal */}
      {showAddModal && (
        <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
          <div className="bg-trading-card border border-trading-border rounded-lg p-6 w-96 max-w-[90vw]">
            <h3 className="text-lg font-bold mb-4">Add Position</h3>
            
            {availableStocks.length === 0 ? (
              <p className="text-gray-400 mb-4">No more stocks available to add.</p>
            ) : (
              <>
                <label className="block text-sm text-gray-400 mb-2">Stock</label>
                <select
                  value={newSymbol}
                  onChange={(e) => setNewSymbol(e.target.value)}
                  className="w-full bg-trading-bg border border-trading-border rounded p-2 mb-4 text-white"
                >
                  {availableStocks.map(stock => (
                    <option key={stock.symbol} value={stock.symbol}>
                      {stock.symbol} - {stock.name}
                    </option>
                  ))}
                </select>

                <label className="block text-sm text-gray-400 mb-2">Quantity</label>
                <input
                  type="number"
                  value={newQuantity}
                  onChange={(e) => setNewQuantity(e.target.value)}
                  placeholder="e.g., 100"
                  className="w-full bg-trading-bg border border-trading-border rounded p-2 mb-4 text-white"
                />

                <label className="block text-sm text-gray-400 mb-2">Purchase Price ($)</label>
                <input
                  type="number"
                  step="0.01"
                  value={newPrice}
                  onChange={(e) => setNewPrice(e.target.value)}
                  placeholder="e.g., 150.00"
                  className="w-full bg-trading-bg border border-trading-border rounded p-2 mb-4 text-white"
                />
              </>
            )}

            <div className="flex gap-3 justify-end">
              <button
                onClick={() => { setShowAddModal(false); resetAddForm(); }}
                className="px-4 py-2 rounded border border-trading-border hover:bg-trading-border/30 transition-colors"
              >
                Cancel
              </button>
              <button
                onClick={handleAddPosition}
                disabled={isAdding || !newSymbol || !newQuantity || !newPrice || availableStocks.length === 0}
                className="px-4 py-2 rounded bg-trading-blue hover:bg-trading-blue/80 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
              >
                {isAdding ? 'Adding...' : 'Add'}
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Edit Position Modal */}
      {showEditModal && editingPosition && (
        <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
          <div className="bg-trading-card border border-trading-border rounded-lg p-6 w-96 max-w-[90vw]">
            <h3 className="text-lg font-bold mb-4">Edit Position</h3>
            
            <label className="block text-sm text-gray-400 mb-2">Quantity</label>
            <input
              type="number"
              value={editingPosition.quantity}
              onChange={(e) => setEditingPosition({...editingPosition, quantity: parseInt(e.target.value) || 0})}
              className="w-full bg-trading-bg border border-trading-border rounded p-2 mb-4 text-white"
            />

            <label className="block text-sm text-gray-400 mb-2">Purchase Price ($)</label>
            <input
              type="number"
              step="0.01"
              value={editingPosition.purchasePrice}
              onChange={(e) => setEditingPosition({...editingPosition, purchasePrice: parseFloat(e.target.value) || 0})}
              className="w-full bg-trading-bg border border-trading-border rounded p-2 mb-4 text-white"
            />

            <div className="flex gap-3 justify-end">
              <button
                onClick={() => { setShowEditModal(false); setEditingPosition(null); }}
                className="px-4 py-2 rounded border border-trading-border hover:bg-trading-border/30 transition-colors"
              >
                Cancel
              </button>
              <button
                onClick={handleEditPosition}
                disabled={isEditing}
                className="px-4 py-2 rounded bg-trading-blue hover:bg-trading-blue/80 transition-colors disabled:opacity-50"
              >
                {isEditing ? 'Saving...' : 'Save'}
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Delete Confirmation Modal */}
      {showDeleteConfirm !== null && (
        <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
          <div className="bg-trading-card border border-trading-border rounded-lg p-6 w-96 max-w-[90vw]">
            <h3 className="text-lg font-bold mb-4">Delete Position</h3>
            <p className="text-gray-300 mb-6">Are you sure you want to delete this position? This action cannot be undone.</p>
            
            <div className="flex gap-3 justify-end">
              <button
                onClick={() => setShowDeleteConfirm(null)}
                className="px-4 py-2 rounded border border-trading-border hover:bg-trading-border/30 transition-colors"
              >
                Cancel
              </button>
              <button
                onClick={() => handleDeletePosition(showDeleteConfirm)}
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