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
import { PortfolioSummary } from './PortfolioSummary';
import { PortfolioPosition } from '@/types';
import { tradingApi, Stock as ApiStock } from '@/services/TradingApi';
import { PositionDialog, PositionFormData } from './PositionDialog';
import { formatCurrency, formatPercent } from '@/utils/formatters';
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

// P/L indicator with arrow icon
const PLIndicator: React.FC<{ value: number | null | undefined }> = ({ value }) => {
  if (value == null) return <>-</>;
  
  return (
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
};

// Edit icon
const EditIcon: React.FC = () => (
  <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
  </svg>
);

// Delete icon
const DeleteIcon: React.FC = () => (
  <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
  </svg>
);

// Add icon
const AddIcon: React.FC = () => (
  <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
  </svg>
);

export const Portfolio: React.FC = () => {
  const [dialogMode, setDialogMode] = useState<'add' | 'edit' | null>(null);
  const [deletingPosition, setDeletingPosition] = useState<DeletingPosition | null>(null);
  const [availableStocks, setAvailableStocks] = useState<ApiStock[]>([]);
  const [editingPosition, setEditingPosition] = useState<PositionFormData | null>(null);
  
  // Loading states
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [isDeleting, setIsDeleting] = useState<number | null>(null);

  // Load available stocks when dialog opens in add mode
  useEffect(() => {
    if (dialogMode === 'add') {
      loadAvailableStocks();
    }
  }, [dialogMode]);

  const loadAvailableStocks = async () => {
    try {
      const stocks = await tradingApi.getStocks();
      setAvailableStocks(stocks);
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
    } finally {
      setIsDeleting(null);
    }
  };

  const openEditDialog = (position: PortfolioPosition) => {
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

  const columns: ColumnDef<PortfolioPosition>[] = [
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
      key: 'quantity',
      label: 'Qty',
      align: 'right',
    },
    {
      key: 'purchasePrice',
      label: 'Avg Cost',
      align: 'right',
      format: (value) => formatCurrency(value || 0),
      className: 'font-mono text-sm',
    },
    {
      key: 'currentPrice',
      label: 'Current',
      align: 'right',
      format: (value) => value ? formatCurrency(value) : '-',
      className: 'font-mono text-sm',
    },
    {
      key: 'currentValue',
      label: 'Value',
      align: 'right',
      format: (value) => value ? formatCurrency(value) : '-',
      className: 'font-mono',
    },
    {
      key: 'profitLoss',
      label: 'P/L',
      align: 'right',
      format: (value) => value != null ? formatCurrency(value) : '-',
      className: (value) => clsx(
        'font-mono text-sm',
        value == null ? '' : value >= 0 ? 'text-trading-green' : 'text-trading-red'
      ),
    },
    {
      key: 'profitLossPercent',
      label: 'P/L %',
      align: 'right',
      format: (value) => value != null ? <PLIndicator value={value} /> : '-',
      className: (value) => clsx(
        'font-mono text-sm',
        value == null ? '' : value >= 0 ? 'text-trading-green' : 'text-trading-red'
      ),
    },
  ];

  const actions: RowAction<PortfolioPosition>[] = [
    {
      icon: <EditIcon />,
      label: 'Edit position',
      onClick: openEditDialog,
      className: 'text-gray-500',
      hoverClassName: 'hover:bg-trading-border/50 hover:text-trading-blue',
    },
    {
      icon: <DeleteIcon />,
      label: 'Delete position',
      onClick: openDeleteConfirm,
      className: 'text-gray-500',
      hoverClassName: 'hover:bg-red-900/30 hover:text-red-400',
    },
  ];

  const headerActions = (
    <button
      onClick={() => setDialogMode('add')}
      className="p-1 rounded hover:bg-trading-border/50 transition-colors text-trading-blue"
      title="Add position"
    >
      <AddIcon />
    </button>
  );

  return (
    <>
      <QueryTable<PortfolioPosition>
        queryId="portfolio-query"
        title="Portfolio"
        columns={columns}
        rowKey={(row) => row.symbol}
        animateOnChange="currentPrice"
        defaultSort={{ column: 'symbol', direction: 'asc' }}
        actions={actions}
        headerActions={headerActions}
        headerSlot={<PortfolioSummary />}
        emptyMessage="No positions in portfolio. Click + to add."
      />

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
    </>
  );
};