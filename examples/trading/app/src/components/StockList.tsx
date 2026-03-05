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

import React from 'react';
import { QueryTable, ColumnDef } from './QueryTable';
import { Stock } from '@/types';
import { formatCurrency, formatPercent, formatVolume } from '@/utils/formatters';
import clsx from 'clsx';

interface StockListProps {
  title: string;
  queryId: string;
}

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

export const StockList: React.FC<StockListProps> = ({ title, queryId }) => {
  // Different columns for high-volume query (shows volume instead of change)
  const isVolumeQuery = queryId === 'high-volume-query';

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
    isVolumeQuery
      ? {
          key: 'volume',
          label: 'Volume',
          align: 'right',
          format: (value) => formatVolume(value),
          className: 'text-sm text-gray-200',
        }
      : {
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

  return (
    <QueryTable<Stock>
      queryId={queryId}
      title={title}
      columns={columns}
      rowKey={(row) => row.symbol}
      animateOnChange="price"
      defaultSort={{ column: 'changePercent', direction: 'desc' }}
      emptyMessage="No stocks found"
    />
  );
};