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
import { DeleteIcon, AddIcon, ConfirmDialog, DetailItem } from './shared';
import { LimitOrderResult, OrderAlert } from '@/types';
import { tradingApi, Stock as ApiStock } from '@/services/TradingApi';
import { OrderDialog, OrderFormData } from './OrderDialog';
import { useQuery } from '@/hooks/useDrasi';
import { formatCurrency } from '@/utils/formatters';
import clsx from 'clsx';

// Code snippet for presentation display - React component only
const CODE_SNIPPET = `<QueryTable<LimitOrderResult>
  queryId="active-orders-query"
  title="Limit Orders"
  columns={[
    { key: 'symbol', label: 'Symbol' },
    { key: 'orderType', label: 'Type' },
    { key: 'targetPrice', label: 'Target', align: 'right',
      format: (value) => formatCurrency(value) },
    { key: 'currentPrice', label: 'Current', align: 'right',
      format: (value) => formatCurrency(value) },
    { key: 'distancePercent', label: 'Distance', align: 'right' },
    { key: 'status', label: 'Status' },
  ]}
  rowKey={(row) => String(row.id)}
  animateOnChange="currentPrice"
  actions={[
    { icon: <DeleteIcon />, label: 'Cancel order',
      onClick: (row) => cancelOrder(row.id) }
  ]}
/>`;

interface DeletingOrder {
  id: number;
  symbol: string;
  orderType: string;
  targetPrice: number;
  quantity: number;
  status: string;
}

/**
 * Hook to automatically update order status based on Drasi future function queries.
 * TEMPORARY: In the future, a postgres reaction will handle this automatically.
 */
const useOrderStatusUpdates = () => {
  const { data: staleAlerts } = useQuery<OrderAlert>('stale-orders-query');
  const { data: expiredAlerts } = useQuery<OrderAlert>('expiring-orders-query');
  
  // Track which orders we've already updated to avoid duplicate API calls
  const [updatedStaleIds, setUpdatedStaleIds] = useState<Set<number>>(new Set());
  const [updatedExpiredIds, setUpdatedExpiredIds] = useState<Set<number>>(new Set());
  
  // Update order status when stale alerts appear
  useEffect(() => {
    if (!staleAlerts || staleAlerts.length === 0) return;
    
    staleAlerts.forEach(async (alert) => {
      if (!updatedStaleIds.has(alert.id)) {
        console.log(`[Status Update] Order ${alert.id} marked as stale`);
        try {
          await tradingApi.updateOrderStatus(alert.id, 'stale');
          setUpdatedStaleIds(prev => new Set([...prev, alert.id]));
        } catch (err) {
          console.error(`Failed to update order ${alert.id} to stale:`, err);
        }
      }
    });
  }, [staleAlerts, updatedStaleIds]);
  
  // Update order status when expired alerts appear
  useEffect(() => {
    if (!expiredAlerts || expiredAlerts.length === 0) return;
    
    expiredAlerts.forEach(async (alert) => {
      if (!updatedExpiredIds.has(alert.id)) {
        console.log(`[Status Update] Order ${alert.id} marked as expired`);
        try {
          await tradingApi.updateOrderStatus(alert.id, 'expired');
          setUpdatedExpiredIds(prev => new Set([...prev, alert.id]));
        } catch (err) {
          console.error(`Failed to update order ${alert.id} to expired:`, err);
        }
      }
    });
  }, [expiredAlerts, updatedExpiredIds]);
};

export const Orders: React.FC = () => {
  // Hook to handle automatic status updates from Drasi queries
  useOrderStatusUpdates();
  
  const [dialogOpen, setDialogOpen] = useState(false);
  const [deletingOrder, setDeletingOrder] = useState<DeletingOrder | null>(null);
  const [availableStocks, setAvailableStocks] = useState<ApiStock[]>([]);
  
  // Loading states
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [isDeleting, setIsDeleting] = useState<number | null>(null);

  // Load available stocks when dialog opens
  useEffect(() => {
    if (dialogOpen) {
      loadAvailableStocks();
    }
  }, [dialogOpen]);

  const loadAvailableStocks = async () => {
    try {
      const stocks = await tradingApi.getStocks();
      setAvailableStocks(stocks);
    } catch (err) {
      console.error('Failed to load stocks:', err);
    }
  };

  const handleOrderSubmit = async (formData: OrderFormData) => {
    setIsSubmitting(true);
    try {
      await tradingApi.createOrder(
        formData.symbol,
        formData.orderType,
        formData.targetPrice,
        formData.quantity,
        formData.expiresAt,
        formData.staleDuration,
        formData.expireDuration
      );
      setDialogOpen(false);
    } finally {
      setIsSubmitting(false);
    }
  };

  const handleDeleteOrder = async () => {
    if (!deletingOrder) return;
    
    setIsDeleting(deletingOrder.id);
    try {
      await tradingApi.cancelOrder(deletingOrder.id);
      setDeletingOrder(null);
    } finally {
      setIsDeleting(null);
    }
  };

  const openDeleteConfirm = (order: LimitOrderResult) => {
    setDeletingOrder({
      id: order.id,
      symbol: order.symbol,
      orderType: order.orderType,
      targetPrice: order.targetPrice,
      quantity: order.quantity,
      status: order.status,
    });
  };

  const columns: ColumnDef<LimitOrderResult>[] = [
    {
      key: 'symbol',
      label: 'Symbol',
      className: 'font-medium',
    },
    {
      key: 'orderType',
      label: 'Type',
      format: (value) => (
        <span className={clsx(
          'px-2 py-0.5 rounded text-xs font-medium uppercase',
          value === 'buy' ? 'bg-trading-green/20 text-trading-green' : 'bg-trading-red/20 text-trading-red'
        )}>
          {value}
        </span>
      ),
    },
    {
      key: 'targetPrice',
      label: 'Target',
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
      key: 'distancePercent',
      label: 'Distance',
      align: 'right',
      format: (value) => {
        if (value == null) return '-';
        const formatted = `${value >= 0 ? '+' : ''}${value.toFixed(2)}%`;
        return (
          <span className={clsx(
            'font-mono text-sm',
            Math.abs(value) < 1 ? 'text-yellow-400' : 'text-gray-400'
          )}>
            {formatted}
          </span>
        );
      },
    },
    {
      key: 'quantity',
      label: 'Qty',
      align: 'right',
    },
    {
      key: 'status',
      label: 'Status',
      format: (value) => {
        const statusConfig: Record<string, string> = {
          pending: 'text-gray-400',
          stale: 'text-yellow-400',
          filled: 'text-trading-green',
          expired: 'text-trading-red',
          cancelled: 'text-gray-500',
        };
        const className = statusConfig[value] || statusConfig.pending;
        return (
          <span className={clsx('text-sm', className)}>
            {value}
          </span>
        );
      },
    },
  ];

  const actions: RowAction<LimitOrderResult>[] = [
    {
      icon: <DeleteIcon />,
      label: 'Cancel order',
      onClick: openDeleteConfirm,
      className: 'text-gray-500',
      hoverClassName: 'hover:bg-red-900/30 hover:text-red-400',
      disabled: () => false,
    },
  ];

  const headerActions = (
    <button
      onClick={() => setDialogOpen(true)}
      className="p-1 rounded hover:bg-trading-border/50 transition-colors text-trading-blue"
      title="New limit order"
    >
      <AddIcon />
    </button>
  );

  return (
    <>
      <QueryTable<LimitOrderResult>
        queryId="active-orders-query"
        title="Limit Orders"
        columns={columns}
        rowKey={(row) => String(row.id)}
        animateOnChange="status"
        defaultSort={{ column: 'createdAt', direction: 'desc' }}
        actions={actions}
        headerActions={headerActions}
        emptyMessage="No active orders. Click + to create a limit order."
        codeSnippet={CODE_SNIPPET}
      />

      {/* New Order Dialog */}
      <OrderDialog
        isOpen={dialogOpen}
        availableStocks={availableStocks}
        isSubmitting={isSubmitting}
        onSubmit={handleOrderSubmit}
        onCancel={() => setDialogOpen(false)}
      />

      {/* Delete Confirmation Dialog */}
      <ConfirmDialog
        isOpen={deletingOrder !== null}
        onClose={() => setDeletingOrder(null)}
        onConfirm={handleDeleteOrder}
        title="Cancel Order"
        details={deletingOrder ? [
          { label: 'Symbol', value: deletingOrder.symbol, valueClassName: 'font-bold' },
          { label: 'Type', value: deletingOrder.orderType.toUpperCase(), valueClassName: deletingOrder.orderType === 'buy' ? 'text-trading-green' : 'text-trading-red' },
          { label: 'Target Price', value: formatCurrency(deletingOrder.targetPrice) },
          { label: 'Quantity', value: `${deletingOrder.quantity} shares` },
          { label: 'Status', value: deletingOrder.status },
        ] as DetailItem[] : []}
        message="This will cancel the order. This action cannot be undone."
        confirmText="Cancel Order"
        isLoading={isDeleting !== null}
        loadingText="Cancelling..."
      />
    </>
  );
};
