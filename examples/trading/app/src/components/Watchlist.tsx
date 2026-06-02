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
import { ChangeIndicator, RemoveIcon, AddIcon, SelectDialog } from './shared';
import { Stock } from '@/types';
import { tradingApi, Stock as ApiStock } from '@/services/TradingApi';
import { formatCurrency } from '@/utils/formatters';
import clsx from 'clsx';

// Code snippet for presentation display
const CODE_SNIPPET = `<QueryTable<Stock>
  queryId="watchlist-query"
  title="Watchlist"
  columns={[
    { key: 'symbol', label: 'Symbol' },
    { key: 'name', label: 'Name' },
    { key: 'price', label: 'Price', align: 'right',
      format: (value) => formatCurrency(value) },
    { key: 'changePercent', label: 'Change', align: 'right',
      format: (value) => <ChangeIndicator value={value} /> },
  ]}
  rowKey={(row) => row.symbol}
  animateOnChange="price"
  defaultSort={{ column: 'symbol', direction: 'asc' }}
  actions={[
    { icon: <RemoveIcon />, label: 'Remove',
      onClick: (row) => removeFromWatchlist(row.symbol) }
  ]}
/>`;

export const Watchlist: React.FC = () => {
  const [showAddModal, setShowAddModal] = useState(false);
  const [availableStocks, setAvailableStocks] = useState<ApiStock[]>([]);
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
      setAvailableStocks(stocks);
    } catch (err) {
      console.error('Failed to load stocks:', err);
      setActionError('Failed to load available stocks');
    }
  };

  const handleAddToWatchlist = async (symbol: string) => {
    setIsAdding(true);
    setActionError(null);
    try {
      await tradingApi.addToWatchlist(symbol);
      setShowAddModal(false);
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
      width: 'w-20',
    },
    {
      key: 'name',
      label: 'Name',
      className: 'text-sm text-gray-300 truncate max-w-[120px]',
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
        codeSnippet={CODE_SNIPPET}
      />

      {/* Add to Watchlist Dialog */}
      <SelectDialog
        isOpen={showAddModal}
        onClose={() => setShowAddModal(false)}
        onConfirm={handleAddToWatchlist}
        title="Add to Watchlist"
        selectLabel="Select Stock"
        options={availableStocks.map(stock => ({
          value: stock.symbol,
          label: `${stock.symbol} - ${stock.name}`
        }))}
        emptyMessage="No more stocks available to add."
        confirmText="Add"
        isLoading={isAdding}
        loadingText="Adding..."
      />
    </>
  );
};
