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

import { useEffect, useState, useCallback, useRef } from 'react';
import { DrasiClient } from '@/services/DrasiClient';
import { QueryResult, ConnectionStatus } from '@/types';

// Singleton instance
let drasiClient: DrasiClient | null = null;
let initializationPromise: Promise<void> | null = null;

export function useDrasiClient() {
  const [initialized, setInitialized] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const initClient = async () => {
      if (!drasiClient) {
        drasiClient = new DrasiClient();
      }

      if (!initializationPromise) {
        initializationPromise = drasiClient.initialize();
      }

      try {
        await initializationPromise;
        setInitialized(true);
        setError(null);
      } catch (err) {
        setError(String(err));
        console.error('Failed to initialize Drasi client:', err);
      }
    };

    initClient();
  }, []);

  return { client: drasiClient, initialized, error };
}

export function useQuery<T = any>(queryId: string): {
  data: T[] | null;
  loading: boolean;
  error: string | null;
  lastUpdate: Date | null;
} {
  const [data, setData] = useState<T[] | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [lastUpdate, setLastUpdate] = useState<Date | null>(null);
  const { client, initialized } = useDrasiClient();
  const unsubscribeRef = useRef<(() => void) | null>(null);
  
  // Keep track of data by key (symbol for stocks, etc.)
  const dataMapRef = useRef<Map<string, T>>(new Map());

  useEffect(() => {
    if (!initialized || !client) {
      return;
    }

    setLoading(true);
    setError(null);

    const handleResult = (result: QueryResult) => {
      console.log(`[${queryId}] Received ${result.data.length} items`);
      
      // Special logging for portfolio query debugging
      if (queryId === 'portfolio-query') {
        console.log(`[${queryId}] Raw data:`, result.data);
      }
      
      // Transform snake_case from Drasi to camelCase and convert numeric strings
      const transformedData = result.data.map(item => {
        const transformed: any = {};
        for (const [key, value] of Object.entries(item)) {
          // Convert snake_case to camelCase
          const camelKey = key.replace(/_([a-z])/g, (_, letter) => letter.toUpperCase());
          
          // For portfolio query, convert numeric string values to numbers
          if (queryId === 'portfolio-query' && value != null && value !== '') {
            const numericFields = [
              'quantity', 'purchasePrice', 'currentPrice', 'currentValue', 
              'costBasis', 'profitLoss', 'profitLossPercent', 'changePercent'
            ];
            
            if (numericFields.includes(camelKey)) {
              const parsed = parseFloat(String(value));
              transformed[camelKey] = isNaN(parsed) ? null : parsed;
            } else {
              transformed[camelKey] = value;
            }
          } else {
            transformed[camelKey] = value;
          }
        }
        return transformed as T;
      });
      
      // Log transformed portfolio data
      if (queryId === 'portfolio-query' && transformedData.length > 0) {
        console.log(`[${queryId}] Received update with ${transformedData.length} items at ${new Date().toLocaleTimeString()}`);
        console.log(`[${queryId}] Current map size before update: ${dataMapRef.current.size}`);
      }
      
      // Portfolio query needs to accumulate data, not replace it
      if (queryId === 'portfolio-query') {
        // Portfolio updates come incrementally (one stock at a time)
        // Only clear if we get a large batch (bootstrap/initial load)
        if (transformedData.length > 5) {
          // This looks like initial bootstrap data, replace everything
          console.log(`[${queryId}] Received bootstrap data with ${transformedData.length} items, replacing all`);
          dataMapRef.current.clear();
          transformedData.forEach(item => {
            const key = getItemKey(item, queryId);
            if (key) {
              dataMapRef.current.set(key, item);
            }
          });
        } else {
          // This is an incremental update, merge with existing data
          console.log(`[${queryId}] Received incremental update with ${transformedData.length} items, merging`);
          transformedData.forEach(item => {
            const key = getItemKey(item, queryId);
            if (key) {
              dataMapRef.current.set(key, item);
            }
          });
        }
      } else {
        // For other queries, use the accumulation logic
        if (transformedData.length > 5) {
          // This looks like a full dataset, replace everything
          dataMapRef.current.clear();
          transformedData.forEach(item => {
            const key = getItemKey(item, queryId);
            if (key) {
              dataMapRef.current.set(key, item);
            }
          });
        } else {
          // This looks like incremental updates, merge with existing data
          transformedData.forEach(item => {
            const key = getItemKey(item, queryId);
            if (key) {
              // Update or add the item
              dataMapRef.current.set(key, item);
            }
          });
        }
      }
      
      // Convert map back to array and apply query-specific filtering/sorting
      let finalData = Array.from(dataMapRef.current.values());
      
      // Apply query-specific sorting and filtering
      if (queryId === 'top-gainers-query') {
        // Filter for positive changes and sort by change percent descending
        finalData = finalData
          .filter((item: any) => item.changePercent > 0)
          .sort((a: any, b: any) => b.changePercent - a.changePercent)
          .slice(0, 10); // Top 10 gainers
      } else if (queryId === 'top-losers-query') {
        // Filter for negative changes and sort by change percent ascending
        finalData = finalData
          .filter((item: any) => item.changePercent < 0)
          .sort((a: any, b: any) => a.changePercent - b.changePercent)
          .slice(0, 10); // Top 10 losers
      } else if (queryId === 'high-volume-query') {
        // Sort by volume descending
        finalData = finalData
          .sort((a: any, b: any) => (b.volume || 0) - (a.volume || 0))
          .slice(0, 10); // Top 10 by volume
      } else if (queryId === 'watchlist-query') {
        // Keep watchlist items in a specific order
        const watchlistSymbols = ['AAPL', 'MSFT', 'GOOGL', 'TSLA', 'NVDA'];
        finalData = finalData
          .filter((item: any) => watchlistSymbols.includes(item.symbol))
          .sort((a: any, b: any) => {
            const aIndex = watchlistSymbols.indexOf(a.symbol);
            const bIndex = watchlistSymbols.indexOf(b.symbol);
            return aIndex - bIndex;
          });
      } else if (queryId === 'portfolio-query') {
        // Portfolio data is accumulated in the map, sort by current value
        console.log(`[${queryId}] Final portfolio has ${finalData.length} items from accumulated data (map size: ${dataMapRef.current.size})`);
        // Debug log the symbols in the portfolio
        const symbols = finalData.map((item: any) => item.symbol).join(', ');
        console.log(`[${queryId}] Portfolio symbols: ${symbols}`);
        finalData = finalData
          .sort((a: any, b: any) => (b.currentValue || 0) - (a.currentValue || 0));
      }
      
      setData(finalData);
      setLastUpdate(new Date(result.timestamp));
      setLoading(false);
      setError(null);
    };

    // Subscribe returns an unsubscribe function
    unsubscribeRef.current = client.subscribe(queryId, handleResult);

    return () => {
      if (unsubscribeRef.current) {
        unsubscribeRef.current();
        unsubscribeRef.current = null;
      }
      // Clear the data map when unmounting
      dataMapRef.current.clear();
    };
  }, [queryId, client, initialized]);

  return { data, loading, error, lastUpdate };
}

// Helper function to get a unique key for each data item
function getItemKey(item: any, queryId: string): string | null {
  // Portfolio items should use symbol as key
  if (queryId === 'portfolio-query' && item.symbol) {
    return `portfolio-${item.symbol}`;
  }
  // Most items have a symbol as the unique identifier
  if (item.symbol) {
    return item.symbol;
  }
  // Sector performance uses sector as the key
  if (item.sector && queryId === 'sector-performance-query') {
    return item.sector;
  }
  // If no clear key, generate one from available properties
  if (item.id) {
    return item.id;
  }
  // Fallback to stringifying the object (not ideal but ensures uniqueness)
  return JSON.stringify(item);
}

export function useConnectionStatus(): ConnectionStatus {
  const [status, setStatus] = useState<ConnectionStatus>({ connected: false });
  const { client, initialized } = useDrasiClient();

  useEffect(() => {
    if (!initialized || !client) {
      return;
    }

    const checkStatus = () => {
      setStatus(client.getConnectionStatus());
    };

    checkStatus();
    const interval = setInterval(checkStatus, 5000); // Check every 5 seconds

    return () => clearInterval(interval);
  }, [client, initialized]);

  return status;
}

export function useQueryParameters(queryId: string) {
  const { client, initialized } = useDrasiClient();
  
  const updateParameters = useCallback(async (parameters: Record<string, any>) => {
    if (!initialized || !client) {
      throw new Error('Drasi client not initialized');
    }
    
    // TODO: Implement updateQueryParameters in DrasiClient
    console.warn(`updateQueryParameters not yet implemented for ${queryId}`, parameters);
    // For now, this is a no-op
  }, [client, queryId, initialized]);

  return { updateParameters };
}