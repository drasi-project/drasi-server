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

import React, {
  useCallback,
  createContext,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import {
  DrasiClient,
  DrasiClientOptions,
} from '../client/DrasiClient';
import type { EventSourceFactory } from '../client/DrasiSSEClient';
import {
  ConnectionStatus,
  QueryDefinition,
  QueryResult,
  ReactionDefinition,
  RouteUnidentified,
  UseDrasiQueryOptions,
} from '../types';

export interface DrasiContextValue {
  client: DrasiClient | null;
  initialized: boolean;
  error: string | null;
  retry: () => void;
}

const DrasiContext = createContext<DrasiContextValue | undefined>(undefined);

/** Props for {@link DrasiProvider}. */
export interface DrasiProviderProps {
  /** Base URL of the Drasi Server REST API. Defaults to `http://localhost:8280`. */
  serverUrl?: string;
  /** Continuous queries multiplexed over the shared connection. */
  queries: QueryDefinition[];
  /** The SSE reaction that multiplexes the queries. */
  reaction: ReactionDefinition;
  /** Routes content for change payloads that arrive without a query id. */
  routeUnidentified?: RouteUnidentified;
  /** Fetch implementation for authenticated clients, polyfills, and tests. */
  fetch?: typeof globalThis.fetch;
  /** EventSource implementation for polyfills and tests. */
  eventSourceFactory?: EventSourceFactory;
  /** Reconnect behavior overrides. */
  reconnect?: DrasiClientOptions['reconnect'];
  children: React.ReactNode;
}

/**
 * DrasiProvider establishes a single shared connection to a Drasi Server and
 * makes it available to descendant components. All queries are created/started
 * and streamed over one multiplexed SSE connection.
 *
 * Wrap your application once near the root:
 * ```tsx
 * <DrasiProvider serverUrl="http://localhost:8280" queries={QUERIES} reaction={REACTION}>
 *   <App />
 * </DrasiProvider>
 * ```
 */
export const DrasiProvider: React.FC<DrasiProviderProps> = ({
  serverUrl,
  queries,
  reaction,
  routeUnidentified,
  fetch: fetcher,
  eventSourceFactory,
  reconnect,
  children,
}) => {
  const [initialized, setInitialized] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [attempt, setAttempt] = useState(0);

  const client = useMemo(
    () =>
      new DrasiClient({
      serverUrl,
      queries,
      reaction,
      routeUnidentified,
        fetch: fetcher,
        eventSourceFactory,
        reconnect,
      }),
    [
      serverUrl,
      queries,
      reaction,
      routeUnidentified,
      fetcher,
      eventSourceFactory,
      reconnect,
    ],
  );

  const retry = useCallback(() => {
    setAttempt((current) => current + 1);
  }, []);

  useEffect(() => {
    let cancelled = false;
    setInitialized(false);
    setError(null);

    client
      .initialize()
      .then(() => {
        if (!cancelled) {
          setInitialized(true);
          setError(null);
        }
      })
      .catch((err) => {
        const aborted =
          err instanceof DOMException && err.name === 'AbortError';
        if (!cancelled && !aborted) {
          setError(String(err));
          setInitialized(false);
          console.error('Failed to initialize Drasi client:', err);
        }
      });

    return () => {
      cancelled = true;
      void client.disconnect();
    };
  }, [client, attempt]);

  const value = useMemo<DrasiContextValue>(
    () => ({ client, initialized, error, retry }),
    [client, initialized, error, retry],
  );

  return <DrasiContext.Provider value={value}>{children}</DrasiContext.Provider>;
};

/** Access the shared Drasi client and its initialization state. */
export function useDrasiClient(): DrasiContextValue {
  const ctx = useContext(DrasiContext);
  if (!ctx) {
    throw new Error('useDrasiClient must be used within a <DrasiProvider>.');
  }
  return ctx;
}

/** Default row key extractor used when none is supplied. */
function defaultGetKey(row: any): string | null {
  if (row == null) return null;
  if (row.id !== undefined && row.id !== null) return String(row.id);
  if (row.symbol) return String(row.symbol);
  return JSON.stringify(row);
}

/**
 * Subscribe to a continuous query over the shared connection and maintain its
 * accumulated result set.
 *
 * Rows are accumulated across update batches keyed by {@link
 * UseDrasiQueryOptions.getKey}; rows flagged with `_deleted` are removed.
 * Optional `transform` and `postProcess` callbacks let the caller normalize
 * rows and sort/filter the final array without coupling the library to any
 * particular data model.
 */
export function useDrasiQuery<T = any>(
  queryId: string,
  options?: UseDrasiQueryOptions<T>,
): {
  data: T[] | null;
  loading: boolean;
  error: string | null;
  lastUpdate: Date | null;
} {
  const {
    client,
    initialized,
    error: providerError,
  } = useDrasiClient();
  const [data, setData] = useState<T[] | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [lastUpdate, setLastUpdate] = useState<Date | null>(null);

  const dataMapRef = useRef<Map<string, T>>(new Map());

  // Keep the latest options without forcing a resubscribe on every render.
  const optionsRef = useRef(options);
  optionsRef.current = options;

  useEffect(() => {
    if (!initialized || !client) {
      if (providerError) {
        setError(providerError);
        setLoading(false);
      }
      return;
    }

    setLoading(true);
    setError(null);
    dataMapRef.current.clear();

    const handleResult = (result: QueryResult) => {
      try {
        const opts = optionsRef.current;
        const getKey = opts?.getKey ?? defaultGetKey;
        const transform = opts?.transform;

        if (result.snapshot) {
          dataMapRef.current.clear();
        }

        result.data.forEach((rawItem: any) => {
          if (rawItem == null) return;
          const deleted = rawItem._deleted === true;
          const transformed = transform ? transform(rawItem) : rawItem;
          if (transformed == null) return;
          const item =
            deleted && typeof transformed === 'object'
              ? { ...transformed, _deleted: true }
              : transformed;
          const key = getKey(item);
          if (key === null) return;

          if (deleted) {
            dataMapRef.current.delete(key);
          } else {
            dataMapRef.current.set(key, item as T);
          }
        });

        let finalData = Array.from(dataMapRef.current.values());
        if (opts?.postProcess) {
          finalData = opts.postProcess([...finalData]);
        }

        setData(finalData);
        setLastUpdate(new Date(result.timestamp));
        setLoading(false);
        setError(null);
      } catch (resultError) {
        setError(String(resultError));
        setLoading(false);
      }
    };

    const unsubscribe = client.subscribe(queryId, handleResult, (queryError) => {
      setError(queryError.message);
      setLoading(false);
    });

    return () => {
      unsubscribe();
      dataMapRef.current.clear();
    };
  }, [queryId, client, initialized, providerError]);

  return { data, loading, error, lastUpdate };
}

/** Track the shared connection status. */
export function useDrasiConnectionStatus(): ConnectionStatus {
  const {
    client,
    initialized,
    error: providerError,
  } = useDrasiClient();
  const [status, setStatus] = useState<ConnectionStatus>({ connected: false });

  useEffect(() => {
    if (!initialized || !client) {
      if (providerError) {
        setStatus({ connected: false, error: providerError });
      }
      return;
    }
    return client.onConnectionStatusChange(setStatus);
  }, [client, initialized, providerError]);

  return status;
}

/** Get the Drasi Server UI URL for the connected instance, if available. */
export function useDrasiServerUiUrl(): string | null {
  const { client, initialized } = useDrasiClient();
  if (!initialized || !client) return null;
  return client.getServerUiUrl();
}

/** Fetch a query's full configuration from the Drasi Server. */
export function useDrasiQueryDefinition(queryId: string): {
  config: Record<string, any> | null;
  loading: boolean;
  error: string | null;
} {
  const {
    client,
    initialized,
    error: providerError,
  } = useDrasiClient();
  const [config, setConfig] = useState<Record<string, any> | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!initialized || !client) {
      if (providerError) {
        setError(providerError);
        setLoading(false);
      }
      return;
    }

    let cancelled = false;
    const controller = new AbortController();
    setLoading(true);
    setError(null);
    client
      .getQueryConfig(queryId, controller.signal)
      .then((result) => {
        if (!cancelled) {
          setConfig(result);
          setLoading(false);
        }
      })
      .catch((queryError) => {
        const aborted =
          queryError instanceof DOMException &&
          queryError.name === 'AbortError';
        if (!cancelled && !aborted) {
          setError(String(queryError));
          setLoading(false);
        }
      });

    return () => {
      cancelled = true;
      controller.abort();
    };
  }, [queryId, client, initialized, providerError]);

  return { config, loading, error };
}
