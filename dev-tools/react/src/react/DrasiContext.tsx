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
import { DrasiError, asDrasiError, isAbortError } from '../client/errors';
import { isRecord } from '../client/resources';
import { configurationKey } from './configuration';
import type {
  ConnectionStatus,
  QueryConfig,
  QueryResult,
  ResultRow,
} from '../client/types';
import type { UseDrasiQueryOptions, UseDrasiQueryResult, UseDrasiQueryDefinitionResult } from './types';

export interface DrasiContextValue {
  client: DrasiClient | null;
  initialized: boolean;
  error: DrasiError | null;
  retry: () => void;
}

const DrasiContext = createContext<DrasiContextValue | undefined>(undefined);

/** Props for the connect-only {@link DrasiProvider}. */
export interface DrasiProviderProps extends DrasiClientOptions {
  children: React.ReactNode;
}

/**
 * Bind hooks to an app-owned client lifecycle without opening a second stream.
 * The owner supplies state/retry and must disconnect its client on cleanup.
 * This binding performs no initialization, retries or resource management.
 */
export const DrasiClientProvider = DrasiContext.Provider;

/**
 * DrasiProvider establishes a single shared connection to a Drasi Server and
 * makes it available to descendant components. Resources must already exist
 * and be running. Initialization and explicit retry are read-only.
 *
 * Wrap your application once near the root:
 * ```tsx
 * <DrasiProvider serverUrl="https://drasi.example" instanceId="analytics"
 *   queryIds={['readings']} reaction={{ id: 'events', endpoint: 'https://events.example/sse' }}>
 *   <App />
 * </DrasiProvider>
 * ```
 */
export const DrasiProvider: React.FC<DrasiProviderProps> = ({
  serverUrl,
  instanceId,
  queryIds,
  reaction,
  routeUnidentified,
  fetch: fetcher,
  headers,
  credentials,
  eventSourceFactory,
  reconnect,
  requestTimeoutMs,
  children,
}) => {
  const [attempt, setAttempt] = useState(0);
  const [state, setState] = useState<{
    client: DrasiClient | null; attempt: number; initialized: boolean; error: DrasiError | null;
  }>({ client: null, attempt: 0, initialized: false, error: null });
  const key = configurationKey({
    serverUrl, instanceId, queryIds, reaction, headers, credentials, reconnect, requestTimeoutMs,
  });
  const headersProvider = typeof headers === 'function' ? headers : undefined;

  const creation = useMemo(
    () => {
      try {
        if (key === null) throw new DrasiError('INVALID_CONFIGURATION', { instanceId });
        return { client: new DrasiClient({
          serverUrl,
          instanceId,
          queryIds,
          reaction,
          routeUnidentified,
          fetch: fetcher,
          headers,
          credentials,
          eventSourceFactory,
          reconnect,
          requestTimeoutMs,
        }), error: null };
      } catch (failure) {
        return { client: null, error: asDrasiError(failure, { instanceId }, 'INVALID_CONFIGURATION') };
      }
    },
    [
      key,
      instanceId,
      routeUnidentified,
      fetcher,
      eventSourceFactory,
      headersProvider,
    ],
  );
  const client = creation.client;
  const activeCreation = useRef<typeof creation | null>(null);
  const initialized = state.client === client && state.attempt === attempt && state.initialized;
  const error = creation.error ??
    (state.client === client && state.attempt === attempt ? state.error : null);

  const retry = useCallback(() => {
    if (activeCreation.current === creation) setAttempt((current) => current + 1);
  }, [creation]);

  useEffect(() => {
    let cancelled = false;
    activeCreation.current = creation;
    setState({ client, attempt, initialized: false, error: creation.error });
    if (!client) {
      return () => {
        if (activeCreation.current === creation) activeCreation.current = null;
      };
    }
    const unsubscribe = client.onConnectionStatusChange(status => {
      if (!cancelled && status.error && !status.reconnecting) {
        setState({ client, attempt, initialized: false, error: status.error });
      }
    });

    client
      .initialize()
      .then(() => {
        if (!cancelled) {
          setState({ client, attempt, initialized: true, error: null });
        }
      })
      .catch((err) => {
        if (!cancelled && !isAbortError(err)) {
          setState({ client, attempt, initialized: false, error: asDrasiError(err, { instanceId }) });
        }
      });

    return () => {
      cancelled = true;
      if (activeCreation.current === creation) activeCreation.current = null;
      unsubscribe();
      void client.disconnect();
    };
  }, [client, attempt, instanceId, creation]);

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
function defaultGetKey(row: unknown): string | null {
  if (row == null) return null;
  if (isRecord(row) && row.id !== undefined && row.id !== null) return String(row.id);
  if (isRecord(row) && row.symbol) return String(row.symbol);
  return JSON.stringify(row) ?? null;
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
export function useDrasiQuery<T = ResultRow>(
  queryId: string,
  options?: UseDrasiQueryOptions<T>,
): UseDrasiQueryResult<T> {
  const {
    client,
    initialized,
    error: providerError,
  } = useDrasiClient();
  const scope = useMemo(() => ({ client, queryId }), [client, queryId]);
  const [result, setResult] = useState<UseDrasiQueryResult<T> & { scope: typeof scope }>({
    scope, data: null, loading: true, error: null, lastUpdate: null,
  });

  const dataMapRef = useRef<Map<string, T>>(new Map());

  // Keep the latest options without forcing a resubscribe on every render.
  const optionsRef = useRef(options);
  optionsRef.current = options;

  useEffect(() => {
    let active = true;
    setResult(current => ({
      ...(current.scope === scope ? current : { scope, data: null, lastUpdate: null }),
      loading: !providerError, error: providerError,
    }));
    if (!initialized || !client) {
      return;
    }

    dataMapRef.current.clear();

    const handleResult = (result: QueryResult) => {
      if (!active) return;
      try {
        const opts = optionsRef.current;
        const getKey = opts?.getKey ?? defaultGetKey;
        const transform = opts?.transform;

        if (result.snapshot) {
          dataMapRef.current.clear();
        }

        result.data.forEach(rawItem => {
          if (rawItem == null) return;
          const deleted = rawItem._deleted === true;
          // With no transform, T is the caller's row-schema assertion. The wire
          // boundary proves only ResultRow; use transform to validate its fields.
          const transformed = transform ? transform(rawItem) : rawItem as T;
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
            dataMapRef.current.set(key, item);
          }
        });

        let finalData = Array.from(dataMapRef.current.values());
        if (opts?.postProcess) {
          finalData = opts.postProcess([...finalData]);
        }

        setResult({ scope, data: finalData, lastUpdate: new Date(result.timestamp), loading: false, error: null });
      } catch (resultError) {
        setResult(current => ({ ...current, loading: false, error: asDrasiError(resultError, {
          instanceId: client.instanceId, resourceKind: 'query', resourceId: queryId,
        }) }));
      }
    };

    const unsubscribe = client.subscribe(queryId, handleResult, (queryError) => {
      if (active) setResult(current => ({ ...current, error: queryError, loading: false }));
    });

    return () => {
      active = false;
      unsubscribe();
      dataMapRef.current.clear();
    };
  }, [queryId, client, initialized, providerError, scope]);

  return result.scope === scope
    ? { data: result.data, loading: result.loading, error: result.error, lastUpdate: result.lastUpdate }
    : { data: null, loading: !providerError, error: providerError, lastUpdate: null };
}

/** Track the shared connection status. */
export function useDrasiConnectionStatus(): ConnectionStatus {
  const {
    client,
    initialized,
    error: providerError,
  } = useDrasiClient();
  const [state, setState] = useState<{ client: DrasiClient | null; status: ConnectionStatus }>({
    client, status: { connected: false },
  });

  useEffect(() => {
    if (!client) {
      setState({ client, status: { connected: false, error: providerError ?? undefined } });
      return;
    }
    let active = true;
    const stop = client.onConnectionStatusChange(status => {
      if (active) setState({ client, status });
    });
    return () => { active = false; stop(); };
  }, [client, initialized, providerError]);

  return providerError ? { connected: false, error: providerError }
    : state.client === client ? state.status : { connected: false };
}

/** Get the Drasi Server UI URL for the connected instance, if available. */
export function useDrasiServerUiUrl(): string | null {
  const { client, initialized } = useDrasiClient();
  if (!initialized || !client) return null;
  return client.getServerUiUrl();
}

/** Fetch a query's full configuration from the Drasi Server. */
export function useDrasiQueryDefinition(queryId: string): UseDrasiQueryDefinitionResult {
  const {
    client,
    initialized,
    error: providerError,
  } = useDrasiClient();
  const [config, setConfig] = useState<QueryConfig | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<DrasiError | null>(null);
  const scope = useMemo(() => ({ client, queryId }), [client, queryId]);
  const [owner, setOwner] = useState(scope);

  useEffect(() => {
    if (owner !== scope) {
      setOwner(scope);
      setConfig(null);
    }
    if (!initialized || !client) {
      if (providerError) {
        setError(providerError);
        setLoading(false);
      } else {
        setError(null);
        setLoading(true);
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
        if (!cancelled && !isAbortError(queryError)) {
          setError(asDrasiError(queryError, {
            instanceId: client.instanceId, resourceKind: 'query', resourceId: queryId,
          }));
          setLoading(false);
        }
      });

    return () => {
      cancelled = true;
      controller.abort();
    };
  }, [queryId, client, initialized, providerError, scope]);

  return owner === scope ? { config, loading, error }
    : { config: null, loading: !providerError, error: providerError };
}
