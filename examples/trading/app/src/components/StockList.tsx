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
import { useQuery } from '@drasi/react';
import { Stock } from '@/types';
import clsx from 'clsx';

interface StockListProps {
  title: string;
  queryId: string;
  filter?: (item: Stock) => boolean;
  sortBy?: (a: Stock, b: Stock) => number;
  showVolume?: boolean;
}

export const StockList: React.FC<StockListProps> = ({ 
  title, 
  queryId, 
  filter, 
  sortBy,
  showVolume = false 
}) => {
  const { data, loading, error, lastUpdate } = useQuery<Stock>(queryId, {
    filter,
    sortBy
  });
  const [prevPrices, setPrevPrices] = useState<Map<string, number>>(new Map());
  const [priceChanges, setPriceChanges] = useState<Map<string, 'up' | 'down' | null>>(new Map());

  useEffect(() => {
    if (data) {
      const newPriceChanges = new Map<string, 'up' | 'down' | null>();
      
      data.forEach(stock => {
        const prevPrice = prevPrices.get(stock.symbol);
        if (prevPrice !== undefined && prevPrice !== stock.price) {
          newPriceChanges.set(stock.symbol, stock.price > prevPrice ? 'up' : 'down');
          // Clear animation after 500ms
          setTimeout(() => {
            setPriceChanges(prev => {
              const updated = new Map(prev);
              updated.set(stock.symbol, null);
              return updated;
            });
          }, 500);
        }
      });

      setPriceChanges(newPriceChanges);
      setPrevPrices(new Map(data.map(s => [s.symbol, s.price])));
    }
  }, [data]);

  const formatPrice = (price: number) => {
    return new Intl.NumberFormat('en-US', {
      style: 'currency',
      currency: 'USD',
      minimumFractionDigits: 2,
      maximumFractionDigits: 2,
    }).format(price);
  };

  const formatPercent = (percent: number) => {
    const formatted = Math.abs(percent).toFixed(2);
    return `${percent >= 0 ? '+' : '-'}${formatted}%`;
  };

  const formatVolume = (volume: number) => {
    if (volume >= 1000000000) {
      return `${(volume / 1000000000).toFixed(2)}B`;
    } else if (volume >= 1000000) {
      return `${(volume / 1000000).toFixed(2)}M`;
    } else if (volume >= 1000) {
      return `${(volume / 1000).toFixed(2)}K`;
    }
    return volume.toString();
  };

  if (loading && !data) {
    return (
      <div className="bg-trading-card rounded-lg p-6 border border-trading-border h-[400px] flex flex-col">
        <h2 className="text-xl font-bold mb-4">{title}</h2>
        <div className="flex items-center justify-center flex-1">
          <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-trading-blue"></div>
        </div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="bg-trading-card rounded-lg p-6 border border-trading-border h-[400px]">
        <h2 className="text-xl font-bold mb-4">{title}</h2>
        <div className="text-trading-red">Error: {error}</div>
      </div>
    );
  }

  return (
    <div className="bg-trading-card rounded-lg border border-trading-border h-[400px] flex flex-col">
      <div className="flex justify-between items-center p-6 pb-4 flex-shrink-0">
        <h2 className="text-xl font-bold">{title}</h2>
        {lastUpdate && (
          <span className="text-xs text-gray-500">
            Updated: {lastUpdate.toLocaleTimeString()}
          </span>
        )}
      </div>
      
      <div className="overflow-auto flex-1 px-6 pb-6">
        <table className="w-full">
          <thead className="sticky top-0 bg-trading-card z-10">
            <tr className="border-b border-trading-border">
              <th className="text-left py-2 px-2 text-sm font-medium text-gray-400">Symbol</th>
              <th className="text-left py-2 px-2 text-sm font-medium text-gray-400">Name</th>
              <th className="text-right py-2 px-2 text-sm font-medium text-gray-400">Price</th>
              <th className="text-right py-2 px-2 text-sm font-medium text-gray-400">
                {showVolume ? 'Volume' : 'Change'}
              </th>
            </tr>
          </thead>
          <tbody>
            {data?.map((stock) => {
              const change = priceChanges.get(stock.symbol);
              return (
                <tr 
                  key={stock.symbol} 
                  className={clsx(
                    "border-b border-trading-border/50 hover:bg-trading-border/20 transition-colors",
                    change === 'up' && 'price-up',
                    change === 'down' && 'price-down'
                  )}
                >
                  <td className="py-3 px-2 font-medium">{stock.symbol}</td>
                  <td className="py-3 px-2 text-sm text-gray-300">{stock.name}</td>
                  <td className="py-3 px-2 text-right font-mono">
                    {formatPrice(stock.price)}
                  </td>
                  <td className={clsx(
                    "py-3 px-2 text-right font-mono text-sm",
                    !showVolume && (stock.changePercent >= 0 ? "text-trading-green" : "text-trading-red")
                  )}>
                    {showVolume ? (
                      <span className="text-gray-200">{formatVolume(stock.volume || 0)}</span>
                    ) : (
                      <span className="inline-flex items-center gap-1">
                        {stock.changePercent >= 0 ? (
                          <svg className="w-3 h-3" fill="currentColor" viewBox="0 0 20 20">
                            <path d="M10 5l5 7H5l5-7z"/>
                          </svg>
                        ) : (
                          <svg className="w-3 h-3" fill="currentColor" viewBox="0 0 20 20">
                            <path d="M10 15l-5-7h10l-5 7z"/>
                          </svg>
                        )}
                        {formatPercent(stock.changePercent)}
                      </span>
                    )}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </div>
  );
};