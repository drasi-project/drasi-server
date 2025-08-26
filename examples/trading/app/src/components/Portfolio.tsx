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

import React, { useMemo } from 'react';
import { useQuery } from '@/hooks/useDrasi';
import { PortfolioPosition } from '@/types';
import clsx from 'clsx';

export const Portfolio: React.FC = () => {
  const { data, loading, error, lastUpdate } = useQuery<PortfolioPosition>('portfolio-query');

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

    // Handle null/undefined values in calculations
    // Debug log the first item to check data types
    if (data.length > 0) {
      console.log('[Portfolio] First item data types:', {
        currentValue: typeof data[0].currentValue,
        costBasis: typeof data[0].costBasis,
        purchasePrice: typeof data[0].purchasePrice,
        quantity: typeof data[0].quantity,
        actualValues: {
          currentValue: data[0].currentValue,
          costBasis: data[0].costBasis,
          purchasePrice: data[0].purchasePrice,
          quantity: data[0].quantity
        }
      });
    }
    
    const totalValue = data.reduce((sum, pos) => {
      // Ensure we're working with numbers
      const value = Number(pos.currentValue) || 0;
      return sum + (isNaN(value) ? 0 : value);
    }, 0);
    
    const totalCost = data.reduce((sum, pos) => {
      // Try costBasis first, then calculate from purchasePrice * quantity
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
        <h2 className="text-xl font-bold">Portfolio</h2>
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
            </tr>
          </thead>
          <tbody>
            {data?.map((position) => (
              <tr 
                key={position.symbol} 
                className="border-b border-trading-border/50 hover:bg-trading-border/20 transition-colors"
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
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
};