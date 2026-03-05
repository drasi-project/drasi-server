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
import { ChangeIndicator } from './shared';
import { SectorPerformance as SectorPerformanceType } from '@/types';
import { formatCompactNumber, formatCurrency } from '@/utils/formatters';
import clsx from 'clsx';

// Code snippet for presentation display
const CODE_SNIPPET = `<QueryTable<SectorPerformance>
  queryId="sector-performance-query"
  title="Sector Performance"
  columns={[
    { key: 'sector', label: 'Sector' },
    { key: 'stockCount', label: 'Stocks', align: 'right' },
    { key: 'avgChangePercent', label: 'Avg Change', align: 'right',
      format: (value) => <ChangeIndicator value={value} /> },
    { key: 'totalVolume', label: 'Volume', align: 'right',
      format: (value) => formatCompactNumber(value) },
    { key: 'minPrice', label: 'Price Range', align: 'right',
      format: (_, row) => \`\${formatCurrency(row.minPrice)} - \${formatCurrency(row.maxPrice)}\` },
  ]}
  rowKey={(row) => row.sector}
  animateOnChange="avgChangePercent"
  defaultSort={{ column: 'sector', direction: 'asc' }}
/>`;

const columns: ColumnDef<SectorPerformanceType>[] = [
  {
    key: 'sector',
    label: 'Sector',
    className: 'font-medium',
    format: (value) => value || 'Unknown',
  },
  {
    key: 'stockCount',
    label: 'Stocks',
    align: 'right',
    className: 'text-sm text-gray-300',
    format: (value) => value ?? 0,
  },
  {
    key: 'avgChangePercent',
    label: 'Avg Change',
    align: 'right',
    format: (value) => <ChangeIndicator value={value} />,
    className: (value) => clsx(
      'font-mono text-sm',
      value == null ? '' : value >= 0 ? 'text-trading-green' : 'text-trading-red'
    ),
  },
  {
    key: 'totalVolume',
    label: 'Volume',
    align: 'right',
    format: (value) => formatCompactNumber(value),
    className: 'text-sm text-gray-300',
  },
  {
    key: 'minPrice',
    label: 'Price Range',
    align: 'right',
    format: (_value, row) => (
      <span className="font-mono text-sm text-gray-300">
        {formatCurrency(row.minPrice)} - {formatCurrency(row.maxPrice)}
      </span>
    ),
    sortable: false,
  },
];

export const SectorPerformance: React.FC = () => {
  return (
    <QueryTable<SectorPerformanceType>
      queryId="sector-performance-query"
      title="Sector Performance"
      columns={columns}
      rowKey={(row) => row.sector || 'unknown'}
      animateOnChange="avgChangePercent"
      defaultSort={{ column: 'sector', direction: 'asc' }}
      emptyMessage="No sector data available"
      codeSnippet={CODE_SNIPPET}
    />
  );
};
