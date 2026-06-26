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
  createContext,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import { DrasiClient } from './DrasiClient';
import {
  ConnectionStatus,
  QueryDefinition,
  QueryResult,
  ReactionDefinition,
  RouteUnidentified,
  UseDrasiQueryOptions,
} from './types';

interface DrasiContextValue {
  client: DrasiClient | null;
  initialized: boolean;
  error: string | null;
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
  children,
}) => {
  const [initialized, setInitialized] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // A single client instance for the lifetime of the provider.
  const clientRef = useRef<DrasiClient | null>(null);
  if (clientRef.current === null) {
    clientRef.current = new DrasiClient({
      serverUrl,
      queries,
      reaction,
      routeUnidentified,
    });
  }

  useEffect(() => {
    let cancelled = false;
    const client = clientRef.current!;
    client
      .initialize()
      .then(() => {
        if (!cancelled) {
          setInitialized(true);
          setError(null);
        }
      })
      .catch((err) => {
        if (!cancelled) {
          setError(String(err));
        }
        console.error('Failed to initialize Drasi client:', err);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const value = useMemo<DrasiContextValue>(
    () => ({ client: clientRef.current, initialized, error }),
    [initialized, error],
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
  const { client, initialized } = useDrasiClient();
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
      return;
    }

    setLoading(true);
    setError(null);
    dataMapRef.current.clear();

    const handleResult = (result: QueryResult) => {
      const opts = optionsRef.current;
      const getKey = opts?.getKey ?? defaultGetKey;
      const transform = opts?.transform;

      const rows = result.data.map((item) => (transform ? transform(item) : item));

      // Apply deletions first, then adds/updates.
      rows
        .filter((item: any) => item && item._deleted)
        .forEach((item: any) => {
          const key = getKey(item);
          if (key) dataMapRef.current.delete(key);
        });

      rows
        .filter((item: any) => !item || !item._deleted)
        .forEach((item: any) => {
          const key = getKey(item);
          if (key) dataMapRef.current.set(key, item as T);
        });

      let finalData = Array.from(dataMapRef.current.values());
      if (opts?.postProcess) {
        finalData = opts.postProcess(finalData);
      }

      setData(finalData);
      setLastUpdate(new Date(result.timestamp));
      setLoading(false);
      setError(null);
    };

    const unsubscribe = client.subscribe(queryId, handleResult);

    return () => {
      unsubscribe();
      dataMapRef.current.clear();
    };
  }, [queryId, client, initialized]);

  return { data, loading, error, lastUpdate };
}

/** Track the shared connection status. */
export function useDrasiConnectionStatus(): ConnectionStatus {
  const { client, initialized } = useDrasiClient();
  const [status, setStatus] = useState<ConnectionStatus>({ connected: false });

  useEffect(() => {
    if (!initialized || !client) {
      return;
    }
    return client.onConnectionStatusChange(setStatus);
  }, [client, initialized]);

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
} {
  const { client, initialized } = useDrasiClient();
  const [config, setConfig] = useState<Record<string, any> | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    if (!initialized || !client) {
      return;
    }
    let cancelled = false;
    setLoading(true);
    client.getQueryConfig(queryId).then((result) => {
      if (!cancelled) {
        setConfig(result);
        setLoading(false);
      }
    });
    return () => {
      cancelled = true;
    };
  }, [queryId, client, initialized]);

  return { config, loading };
}
