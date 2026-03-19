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
import { useQuery } from '@/hooks/useDrasi';
import { PortfolioSummary as PortfolioSummaryType } from '@/types';
import { formatCurrency, formatPercent } from '@/utils/formatters';
import clsx from 'clsx';

interface PortfolioSummaryProps {
  className?: string;
}

/**
 * Displays real-time portfolio summary statistics using the portfolio-summary-query.
 * Shows total value, total cost, total P/L, and total return percentage.
 */
export const PortfolioSummary: React.FC<PortfolioSummaryProps> = ({ className }) => {
  const { data, loading } = useQuery<PortfolioSummaryType>('portfolio-summary-query');
  
  // Get the first (and only) result from the aggregation query
  const summary = data?.[0];
  
  // Default values when loading or no data
  const totalValue = summary?.totalValue ?? 0;
  const totalCost = summary?.totalCost ?? 0;
  const totalProfitLoss = summary?.totalProfitLoss ?? 0;
  const totalProfitLossPercent = summary?.totalProfitLossPercent ?? 0;

  if (loading && !summary) {
    return (
      <div className={clsx("grid grid-cols-2 md:grid-cols-4 gap-4", className)}>
        {[1, 2, 3, 4].map((i) => (
          <div key={i} className="bg-trading-bg rounded p-3 animate-pulse">
            <div className="h-3 bg-trading-border rounded w-16 mb-2"></div>
            <div className="h-6 bg-trading-border rounded w-24"></div>
          </div>
        ))}
      </div>
    );
  }

  return (
    <div className={clsx("grid grid-cols-2 md:grid-cols-4 gap-4", className)}>
      <div className="bg-trading-bg rounded p-3">
        <div className="text-xs text-gray-400 mb-1">Total Value</div>
        <div className="text-lg font-bold">{formatCurrency(totalValue)}</div>
      </div>
      <div className="bg-trading-bg rounded p-3">
        <div className="text-xs text-gray-400 mb-1">Total Cost</div>
        <div className="text-lg font-bold">{formatCurrency(totalCost)}</div>
      </div>
      <div className="bg-trading-bg rounded p-3">
        <div className="text-xs text-gray-400 mb-1">Total P/L</div>
        <div className={clsx(
          "text-lg font-bold",
          totalProfitLoss >= 0 ? "text-trading-green" : "text-trading-red"
        )}>
          {formatCurrency(totalProfitLoss)}
        </div>
      </div>
      <div className="bg-trading-bg rounded p-3">
        <div className="text-xs text-gray-400 mb-1">Total Return</div>
        <div className={clsx(
          "text-lg font-bold",
          totalProfitLossPercent >= 0 ? "text-trading-green" : "text-trading-red"
        )}>
          {formatPercent(totalProfitLossPercent)}
        </div>
      </div>
    </div>
  );
};
