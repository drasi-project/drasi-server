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
import { QueryTable, ColumnDef, RowAction } from './QueryTable';
import { Stock } from '@/types';
import { tradingApi, Stock as ApiStock } from '@/services/TradingApi';
import { formatCurrency, formatPercent } from '@/utils/formatters';
import clsx from 'clsx';

// Change indicator with arrow icon
const ChangeIndicator: React.FC<{ value: number }> = ({ value }) => (
  <span className="inline-flex items-center gap-1">
    {value >= 0 ? (
      <svg className="w-3 h-3" fill="currentColor" viewBox="0 0 20 20">
        <path d="M10 5l5 7H5l5-7z"/>
      </svg>
    ) : (
      <svg className="w-3 h-3" fill="currentColor" viewBox="0 0 20 20">
        <path d="M10 15l-5-7h10l-5 7z"/>
      </svg>
    )}
    {formatPercent(value)}
  </span>
);

// Remove icon
const RemoveIcon: React.FC = () => (
  <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
  </svg>
);

// Add icon
const AddIcon: React.FC = () => (
  <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
  </svg>
);

export const Watchlist: React.FC = () => {
  const [showAddModal, setShowAddModal] = useState(false);
  const [availableStocks, setAvailableStocks] = useState<ApiStock[]>([]);
  const [selectedSymbol, setSelectedSymbol] = useState('');
  const [isAdding, setIsAdding] = useState(false);
  const [isRemoving, setIsRemoving] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);

  // Load available stocks when modal opens
  useEffect(() => {
    if (showAddModal) {
      loadAvailableStocks();
    }
  }, [showAddModal]);

  const loadAvailableStocks = async () => {
    try {
      const stocks = await tradingApi.getStocks();
      // Note: We can't filter out watchlist stocks without access to current data
      // The QueryTable handles the data internally, so we show all stocks
      setAvailableStocks(stocks);
      if (stocks.length > 0) {
        setSelectedSymbol(stocks[0].symbol);
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
    } catch (err) {
      setActionError(err instanceof Error ? err.message : 'Failed to add to watchlist');
    } finally {
      setIsAdding(false);
    }
  };

  const handleRemoveFromWatchlist = async (stock: Stock) => {
    setIsRemoving(stock.symbol);
    setActionError(null);
    try {
      await tradingApi.removeFromWatchlist(stock.symbol);
    } catch (err) {
      setActionError(err instanceof Error ? err.message : 'Failed to remove from watchlist');
    } finally {
      setIsRemoving(null);
    }
  };

  const columns: ColumnDef<Stock>[] = [
    {
      key: 'symbol',
      label: 'Symbol',
      className: 'font-medium',
    },
    {
      key: 'name',
      label: 'Name',
      className: 'text-sm text-gray-300',
    },
    {
      key: 'price',
      label: 'Price',
      align: 'right',
      format: (value) => formatCurrency(value),
      className: 'font-mono',
    },
    {
      key: 'changePercent',
      label: 'Change',
      align: 'right',
      format: (value) => <ChangeIndicator value={value} />,
      className: (value) => clsx(
        'font-mono text-sm',
        value >= 0 ? 'text-trading-green' : 'text-trading-red'
      ),
    },
  ];

  const actions: RowAction<Stock>[] = [
    {
      icon: <RemoveIcon />,
      label: 'Remove from watchlist',
      onClick: handleRemoveFromWatchlist,
      className: 'text-gray-500',
      hoverClassName: 'hover:bg-red-900/30 hover:text-red-400',
      loading: (row) => isRemoving === row.symbol,
    },
  ];

  const headerActions = (
    <button
      onClick={() => setShowAddModal(true)}
      className="p-1 rounded hover:bg-trading-border/50 transition-colors text-trading-blue"
      title="Add to watchlist"
    >
      <AddIcon />
    </button>
  );

  return (
    <>
      {actionError && (
        <div className="mb-2 p-2 bg-red-900/30 border border-red-500/50 rounded text-sm text-red-400">
          {actionError}
        </div>
      )}
      
      <QueryTable<Stock>
        queryId="watchlist-query"
        title="Watchlist"
        columns={columns}
        rowKey={(row) => row.symbol}
        animateOnChange="price"
        defaultSort={{ column: 'symbol', direction: 'asc' }}
        actions={actions}
        actionsWidth="w-10"
        headerActions={headerActions}
        emptyMessage="No stocks in watchlist. Click + to add."
      />

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
    </>
  );
};
