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

import React, { useMemo, useState, useEffect } from 'react';
import { useQuery } from '@/hooks/useDrasi';
import { SectorPerformance as SectorPerformanceType } from '@/types';
import clsx from 'clsx';

export const SectorPerformance: React.FC = () => {
  const { data, loading, error, lastUpdate } = useQuery<SectorPerformanceType>('sector-performance-query');
  const [prevValues, setPrevValues] = useState<Map<string, number>>(new Map());
  const [changes, setChanges] = useState<Map<string, 'up' | 'down' | null>>(new Map());

  // Track changes for highlight effect
  useEffect(() => {
    if (data) {
      const newChanges = new Map<string, 'up' | 'down' | null>();
      
      data.forEach(sector => {
        if (!sector.sector) return;
        const prevValue = prevValues.get(sector.sector);
        const currentValue = sector.avgChangePercent || 0;
        if (prevValue !== undefined && prevValue !== currentValue) {
          newChanges.set(sector.sector, currentValue > prevValue ? 'up' : 'down');
          // Clear animation after 500ms
          setTimeout(() => {
            setChanges(prev => {
              const updated = new Map(prev);
              updated.set(sector.sector, null);
              return updated;
            });
          }, 500);
        }
      });

      setChanges(newChanges);
      setPrevValues(new Map(data.filter(s => s.sector).map(s => [s.sector, s.avgChangePercent || 0])));
    }
  }, [data]);

  // Sort sectors alphabetically by name
  const sortedData = useMemo(() => {
    if (!data || data.length === 0) return [];
    return [...data].sort((a, b) => (a.sector || '').localeCompare(b.sector || ''));
  }, [data]);

  const formatPercent = (percent: number | null | undefined) => {
    if (percent == null) return '-';
    const formatted = Math.abs(percent).toFixed(2);
    return `${percent >= 0 ? '+' : '-'}${formatted}%`;
  };

  const formatVolume = (volume: number | null | undefined) => {
    if (volume == null) return '-';
    if (volume >= 1_000_000_000) {
      return `${(volume / 1_000_000_000).toFixed(1)}B`;
    }
    if (volume >= 1_000_000) {
      return `${(volume / 1_000_000).toFixed(1)}M`;
    }
    if (volume >= 1_000) {
      return `${(volume / 1_000).toFixed(1)}K`;
    }
    return volume.toLocaleString();
  };

  const formatPrice = (price: number | null | undefined) => {
    if (price == null) return '-';
    return `$${price.toFixed(2)}`;
  };

  if (loading && !data) {
    return (
      <div className="bg-trading-card rounded-lg p-6 border border-trading-border h-[400px] flex flex-col">
        <h2 className="text-xl font-bold mb-4">Sector Performance</h2>
        <div className="flex items-center justify-center flex-1">
          <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-trading-blue"></div>
        </div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="bg-trading-card rounded-lg p-6 border border-trading-border h-[400px]">
        <h2 className="text-xl font-bold mb-4">Sector Performance</h2>
        <div className="text-trading-red">Error: {error}</div>
      </div>
    );
  }

  return (
    <div className="bg-trading-card rounded-lg border border-trading-border h-[400px] flex flex-col">
      <div className="flex justify-between items-center p-6 pb-4 flex-shrink-0">
        <h2 className="text-xl font-bold">Sector Performance</h2>
        {lastUpdate && (
          <span className="text-xs text-gray-500">
            Updated: {lastUpdate.toLocaleTimeString()}
          </span>
        )}
      </div>

      {/* Sector Table */}
      <div className="overflow-auto flex-1 px-6 pb-6">
        <table className="w-full">
          <thead className="sticky top-0 bg-trading-card z-10">
            <tr className="border-b border-trading-border">
              <th className="text-left py-2 px-2 text-sm font-medium text-gray-400">Sector</th>
              <th className="text-right py-2 px-2 text-sm font-medium text-gray-400">Stocks</th>
              <th className="text-right py-2 px-2 text-sm font-medium text-gray-400">Avg Change</th>
              <th className="text-right py-2 px-2 text-sm font-medium text-gray-400">Volume</th>
              <th className="text-right py-2 px-2 text-sm font-medium text-gray-400">Price Range</th>
            </tr>
          </thead>
          <tbody>
            {sortedData.map((sector) => {
              const change = changes.get(sector.sector);
              return (
                <tr 
                  key={sector.sector} 
                  className={clsx(
                    "border-b border-trading-border/50 hover:bg-trading-border/20 transition-colors",
                    change === 'up' && 'price-up',
                    change === 'down' && 'price-down'
                  )}
                >
                  <td className="py-3 px-2 font-medium">{sector.sector || 'Unknown'}</td>
                  <td className="py-3 px-2 text-right text-sm text-gray-300">{sector.stockCount || 0}</td>
                  <td className={clsx(
                    "py-3 px-2 text-right font-mono text-sm",
                    sector.avgChangePercent == null ? "" :
                    sector.avgChangePercent >= 0 ? "text-trading-green" : "text-trading-red"
                  )}>
                    <span className="inline-flex items-center gap-1">
                      {sector.avgChangePercent != null && (
                        sector.avgChangePercent >= 0 ? (
                          <svg className="w-3 h-3" fill="currentColor" viewBox="0 0 20 20">
                            <path d="M10 5l5 7H5l5-7z"/>
                          </svg>
                        ) : (
                          <svg className="w-3 h-3" fill="currentColor" viewBox="0 0 20 20">
                            <path d="M10 15l-5-7h10l-5 7z"/>
                          </svg>
                        )
                      )}
                      {formatPercent(sector.avgChangePercent)}
                    </span>
                  </td>
                  <td className="py-3 px-2 text-right text-sm text-gray-300">
                    {formatVolume(sector.totalVolume)}
                  </td>
                  <td className="py-3 px-2 text-right font-mono text-sm text-gray-300">
                    {formatPrice(sector.minPrice)} - {formatPrice(sector.maxPrice)}
                  </td>
                </tr>
              );
            })}
            {sortedData.length === 0 && (
              <tr>
                <td colSpan={5} className="py-8 text-center text-gray-500">
                  No sector data available
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
};
