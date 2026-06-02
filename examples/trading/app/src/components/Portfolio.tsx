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
import { ChangeIndicator, EditIcon, DeleteIcon, AddIcon, ConfirmDialog, DetailItem } from './shared';
import { PortfolioPosition } from '@/types';
import { tradingApi, Stock as ApiStock } from '@/services/TradingApi';
import { PositionDialog, PositionFormData } from './PositionDialog';
import { formatCurrency } from '@/utils/formatters';
import clsx from 'clsx';

// Code snippet for presentation display
const CODE_SNIPPET = `<QueryTable<PortfolioPosition>
  queryId="portfolio-query"
  title="Portfolio"
  columns={[
    { key: 'symbol', label: 'Symbol' },
    { key: 'name', label: 'Name' },
    { key: 'quantity', label: 'Qty', align: 'right' },
    { key: 'purchasePrice', label: 'Avg Cost', align: 'right',
      format: (value) => formatCurrency(value) },
    { key: 'currentPrice', label: 'Current', align: 'right',
      format: (value) => formatCurrency(value) },
    { key: 'currentValue', label: 'Value', align: 'right',
      format: (value) => formatCurrency(value) },
    { key: 'profitLoss', label: 'P/L', align: 'right',
      format: (value) => formatCurrency(value) },
    { key: 'profitLossPercent', label: 'P/L %', align: 'right',
      format: (value) => <ChangeIndicator value={value} /> },
  ]}
  rowKey={(row) => row.symbol}
  animateOnChange="currentPrice"
  headerSlot={<PortfolioSummary />}
/>`;

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
      format: (value) => value != null ? <ChangeIndicator value={value} /> : '-',
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
        codeSnippet={CODE_SNIPPET}
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

      {/* Delete Confirmation Dialog */}
      <ConfirmDialog
        isOpen={deletingPosition !== null}
        onClose={() => setDeletingPosition(null)}
        onConfirm={handleDeletePosition}
        title="Delete Position"
        details={deletingPosition ? [
          { label: 'Stock', value: `${deletingPosition.symbol} - ${deletingPosition.name}`, valueClassName: 'font-bold' },
          { label: 'Quantity', value: `${deletingPosition.quantity} shares` },
          { label: 'Purchase Price', value: formatCurrency(deletingPosition.purchasePrice) },
          { label: 'Purchase Date', value: deletingPosition.purchaseDate || 'N/A' },
          ...(deletingPosition.currentPrice ? [{ label: 'Current Price', value: formatCurrency(deletingPosition.currentPrice) }] : []),
          ...(deletingPosition.currentValue ? [{ label: 'Current Value', value: formatCurrency(deletingPosition.currentValue), valueClassName: 'font-bold' }] : []),
          ...(deletingPosition.profitLoss != null ? [{
            label: 'P/L',
            value: formatCurrency(deletingPosition.profitLoss),
            valueClassName: clsx('font-bold', deletingPosition.profitLoss >= 0 ? 'text-trading-green' : 'text-trading-red')
          }] : []),
        ] as DetailItem[] : []}
        message="This action cannot be undone."
        confirmText="Delete"
        isLoading={isDeleting !== null}
        loadingText="Deleting..."
      />
    </>
  );
};