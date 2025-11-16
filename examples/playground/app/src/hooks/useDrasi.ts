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

import { useState, useEffect, useRef } from 'react';
import { DrasiClient } from '@/services/DrasiClient';
import { Source, Query, QueryResult, ConnectionStatus } from '@/types';

// Singleton client instance
let drasiClientInstance: DrasiClient | null = null;

/**
 * Get or create the singleton DrasiClient instance
 */
export function useDrasiClient() {
  const [client, setClient] = useState<DrasiClient | null>(drasiClientInstance);
  const [initialized, setInitialized] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!drasiClientInstance) {
      drasiClientInstance = new DrasiClient();
      setClient(drasiClientInstance);

      // Initialize the client
      drasiClientInstance
        .initialize()
        .then(() => {
          setInitialized(true);
        })
        .catch((err) => {
          console.error('Failed to initialize Drasi client:', err);
          setError(err.message || 'Failed to initialize');
        });
    } else {
      setClient(drasiClientInstance);
      setInitialized(true);
    }

    return () => {
      // Cleanup on unmount (optional - may want to keep connection alive)
    };
  }, []);

  return { client, initialized, error };
}

/**
 * Subscribe to query results with automatic updates
 */
export function useQuery<T = any>(queryId: string | null) {
  const { client, initialized } = useDrasiClient();
  const [data, setData] = useState<T[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [lastUpdate, setLastUpdate] = useState<number | null>(null);

  useEffect(() => {
    if (!client || !initialized || !queryId) {
      return;
    }

    let unsubscribe: (() => void) | null = null;

    // Fetch initial results
    client
      .getQueryResults(queryId)
      .then((results) => {
        if (results && Array.isArray(results)) {
          const transformedData = transformResults<T>(results);
          setData(transformedData);
        } else {
          // No results yet, that's okay
          setData([]);
        }
        setLoading(false);
        setLastUpdate(Date.now());
      })
      .catch((err) => {
        // For new queries, there might not be results yet, which is fine
        console.log(`No initial results for ${queryId} (this is normal for new queries)`);
        setData([]);
        setError(null); // Don't show an error for empty results
        setLoading(false);
      });

    // Subscribe to real-time updates
    unsubscribe = client.subscribe(queryId, (result: QueryResult) => {
      // Handle different result structures
      let rawData: any[] = [];
      if (Array.isArray(result)) {
        rawData = result;
      } else if (result && Array.isArray(result.results)) {
        rawData = result.results.map(r => r.data || r);
      } else if (result && result.data) {
        rawData = Array.isArray(result.data) ? result.data : [result.data];
      }

      const transformedData = transformResults<T>(rawData);
      setData(transformedData);
      setLastUpdate(Date.now());
      setError(null); // Clear any previous errors
    });

    return () => {
      if (unsubscribe) {
        unsubscribe();
      }
    };
  }, [client, initialized, queryId]);

  return { data, loading, error, lastUpdate };
}

/**
 * Monitor SSE connection status
 */
export function useConnectionStatus() {
  const { client, initialized } = useDrasiClient();
  const [status, setStatus] = useState<ConnectionStatus>({ connected: false });

  useEffect(() => {
    if (!client || !initialized) {
      return;
    }

    const unsubscribe = client.onConnectionStatusChange((newStatus) => {
      setStatus(newStatus);
    });

    return unsubscribe;
  }, [client, initialized]);

  return status;
}

/**
 * List and manage sources
 */
export function useSources() {
  const { client, initialized } = useDrasiClient();
  const [sources, setSources] = useState<Source[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const fetchSources = async () => {
    if (!client) return;

    try {
      setLoading(true);
      const data = await client.listSources();
      setSources(data);
      setError(null);
    } catch (err: any) {
      setError(err.message);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    if (client && initialized) {
      fetchSources();
    }
  }, [client, initialized]);

  const createSource = async (source: Partial<Source>) => {
    if (!client) throw new Error('Client not initialized');
    const newSource = await client.createSource(source);
    await fetchSources();
    return newSource;
  };

  const deleteSource = async (id: string) => {
    if (!client) throw new Error('Client not initialized');
    await client.deleteSource(id);
    await fetchSources();
  };

  return { sources, loading, error, refetch: fetchSources, createSource, deleteSource };
}

/**
 * List and manage queries
 */
export function useQueries() {
  const { client, initialized } = useDrasiClient();
  const [queries, setQueries] = useState<Query[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const fetchQueries = async () => {
    if (!client) return;

    try {
      setLoading(true);
      const data = await client.listQueries();
      setQueries(data);
      setError(null);
    } catch (err: any) {
      setError(err.message);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    if (client && initialized) {
      fetchQueries();
    }
  }, [client, initialized]);

  const createQuery = async (query: Partial<Query>) => {
    if (!client) throw new Error('Client not initialized');
    const newQuery = await client.createQuery(query);
    await fetchQueries();
    return newQuery;
  };

  const deleteQuery = async (id: string) => {
    if (!client) throw new Error('Client not initialized');
    await client.deleteQuery(id);
    await fetchQueries();
  };

  return { queries, loading, error, refetch: fetchQueries, createQuery, deleteQuery };
}

// ========== Utility Functions ==========

/**
 * Transform snake_case keys to camelCase
 */
function toCamelCase(str: string): string {
  return str.replace(/_([a-z])/g, (_, letter) => letter.toUpperCase());
}

/**
 * Transform query results from snake_case to camelCase
 */
function transformResults<T>(results: any[]): T[] {
  if (!results || !Array.isArray(results)) {
    return [];
  }

  return results.map((result) => {
    if (!result || typeof result !== 'object') {
      return result as T;
    }

    const transformed: any = {};
    for (const [key, value] of Object.entries(result)) {
      const camelKey = toCamelCase(key);
      transformed[camelKey] = value;
    }
    return transformed as T;
  });
}
