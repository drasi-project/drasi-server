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
  
  // Keep track of data by key (symbol for stocks, id for portfolio, etc.)
  const dataMapRef = useRef<Map<string, T>>(new Map());

  useEffect(() => {
    if (!initialized || !client) {
      return;
    }

    setLoading(true);
    setError(null);

    const handleResult = (result: QueryResult) => {
      console.log(`[${queryId}] Received ${result.data.length} items`);
      
      // Convert numeric string values to numbers for portfolio query
      const transformedData = result.data.map(item => {
        const transformed: any = { ...item };

        if (queryId === 'portfolio-query') {
          const numericFields = [
            'quantity', 'purchasePrice', 'currentPrice', 'currentValue', 
            'costBasis', 'profitLoss', 'profitLossPercent', 'changePercent'
          ];
          for (const field of numericFields) {
            if (transformed[field] != null && transformed[field] !== '') {
              const parsed = parseFloat(String(transformed[field]));
              transformed[field] = isNaN(parsed) ? null : parsed;
            }
          }
        }

        return transformed as T;
      });
      
      // Handle deletions - items marked with _deleted flag
      const deletedItems = transformedData.filter((item: any) => item._deleted);
      const regularItems = transformedData.filter((item: any) => !item._deleted);
      
      // Remove deleted items from the map
      deletedItems.forEach((item: any) => {
        const key = getItemKey(item, queryId);
        if (key) {
          console.log(`[${queryId}] Deleting item with key: ${key}`);
          dataMapRef.current.delete(key);
        }
      });
      
      // Add/update regular items
      regularItems.forEach((item: any) => {
        const key = getItemKey(item, queryId);
        if (key) {
          dataMapRef.current.set(key, item);
        }
      });
      
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
        // Sort watchlist alphabetically by symbol
        finalData = finalData.sort((a: any, b: any) => 
          (a.symbol || '').localeCompare(b.symbol || '')
        );
      } else if (queryId === 'portfolio-query') {
        // Portfolio data is accumulated in the map, sort by current value
        console.log(`[${queryId}] Final portfolio has ${finalData.length} items from accumulated data (map size: ${dataMapRef.current.size})`);
        // Debug log the symbols in the portfolio
        const symbols = finalData.map((item: any) => item.symbol).join(', ');
        console.log(`[${queryId}] Portfolio symbols: ${symbols}`);
        finalData = finalData
          .sort((a: any, b: any) => (b.currentValue || 0) - (a.currentValue || 0));
      } else if (queryId === 'sector-performance-query') {
        // Debug log sector performance data
        console.log(`[${queryId}] Final data has ${finalData.length} sectors (map size: ${dataMapRef.current.size})`);
        if (finalData.length > 0) {
          console.log(`[${queryId}] Sectors:`, finalData.map((item: any) => item.sector).join(', '));
          console.log(`[${queryId}] Sample item:`, finalData[0]);
        }
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
  // Portfolio items should use id as the primary key
  // This ensures delete events (which only have id) can match
  if (queryId === 'portfolio-query') {
    if (item.id !== undefined) {
      return `portfolio-id-${item.id}`;
    }
    // Fallback to symbol for backwards compatibility
    if (item.symbol) {
      return `portfolio-${item.symbol}`;
    }
  }
  // Portfolio summary is a single aggregation row — use a stable key
  if (queryId === 'portfolio-summary-query') {
    return 'portfolio-summary';
  }
  // Most items have a symbol as the unique identifier
  if (item.symbol) {
    return item.symbol;
  }
  // Sector performance uses sector as the key
  if (queryId === 'sector-performance-query') {
    if (item.sector) {
      return `sector-${item.sector}`;
    }
    console.warn(`[${queryId}] Item missing sector field:`, item);
    // Fallback to stringifying for sector data
    return `sector-${JSON.stringify(item)}`;
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

/**
 * Hook to get the Drasi Server UI URL for the current instance.
 */
export function useDrasiUiUrl(): string | null {
  const { client, initialized } = useDrasiClient();
  if (!initialized || !client) return null;
  return client.getDrasiUiUrl();
}

/**
 * Hook to fetch a query's full configuration from the Drasi Server.
 * Returns the complete config object and loading state.
 */
export function useQueryDefinition(queryId: string): {
  config: Record<string, any> | null;
  loading: boolean;
} {
  const [config, setConfig] = useState<Record<string, any> | null>(null);
  const [loading, setLoading] = useState(true);
  const { client, initialized } = useDrasiClient();

  useEffect(() => {
    if (!initialized || !client) {
      return;
    }

    let cancelled = false;

    const fetchConfig = async () => {
      setLoading(true);
      const result = await client.getQueryConfig(queryId);
      if (!cancelled) {
        setConfig(result);
        setLoading(false);
      }
    };

    fetchConfig();

    return () => {
      cancelled = true;
    };
  }, [queryId, client, initialized]);

  return { config, loading };
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