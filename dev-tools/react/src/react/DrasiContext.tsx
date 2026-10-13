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
  useInsertionEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import {
  DrasiClient,
  DrasiClientOptions,
} from '../client/DrasiClient';
import { DrasiError, asDrasiError, isAbortError } from '../client/errors';
import { accumulateResult } from '../client/accumulation';
import { configurationKey } from './configuration';
import type {
  ConnectionStatus,
  QueryConfig,
  QueryResult,
  QuerySubscription,
  QuerySubscriptionState,
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
  resultAdapter,
  reconciliation,
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
    serverUrl, instanceId, queryIds, reaction, headers, credentials, reconnect, requestTimeoutMs, reconciliation,
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
          resultAdapter,
          reconciliation,
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
      resultAdapter,
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

/**
 * Accumulate raw rows by required domain identity, then derive a typed view.
 * Projection/key changes recompute retained rows without another socket or
 * subscription. Live batches use only the latest committed key callback;
 * suspended/abandoned renders cannot publish it. Sparse deletes never transform.
 */
export function useDrasiQuery<T extends object = ResultRow>(
  queryId: string,
  options: UseDrasiQueryOptions<T>,
): UseDrasiQueryResult<T> {
  const {
    client,
    initialized,
    error: providerError,
  } = useDrasiClient();
  const scope = useMemo(() => ({ client, queryId }), [client, queryId]);
  const [result, setResult] = useState<{
    scope: typeof scope;
    rows: ResultRow[] | null;
    lastUpdate: Date | null;
    state: QuerySubscriptionState;
  }>({
    scope, rows: null, lastUpdate: null,
    state: { status: 'initial-loading', stale: false, error: null, errorScope: null },
  });
  const getKey = options.getKey;
  const committedGetKey = useRef(getKey);
  // Publish before layout effects can deliver events, never during a speculative
  // render. Insertion effects are inert during SSR and need no browser probe.
  useInsertionEffect(() => {
    committedGetKey.current = getKey;
  }, [getKey]);
  const subscriptionRef = useRef<{ scope: typeof scope; subscription: QuerySubscription } | null>(null);
  const lastGood = useRef<{ scope: typeof scope; data: T[]; lastUpdate: Date | null } | null>(null);
  const retry = useCallback(() => {
    if (subscriptionRef.current?.scope === scope) subscriptionRef.current.subscription.retry();
  }, [scope]);

  useEffect(() => {
    let active = true;
    let rawRows: ResultRow[] = [];
    const stateChanged = (state: QuerySubscriptionState) => {
      if (!active) return;
      setResult(current => ({
        ...(current.scope === scope ? current : { scope, rows: null, lastUpdate: null }), state,
      }));
    };
    stateChanged({
      status: providerError ? 'terminal-error' : 'initial-loading', stale: false,
      error: providerError, errorScope: providerError ? 'connection' : null,
    });
    if (!initialized || !client) return () => { active = false; };
    const handleResult = (batch: QueryResult) => {
      if (!active) return;
      rawRows = accumulateResult(rawRows, batch, committedGetKey.current, {
        instanceId: client.instanceId, resourceKind: 'query', resourceId: queryId,
      });
      const rows = rawRows;
      setResult(current => ({ ...current, scope, rows, lastUpdate: new Date(batch.receivedAt) }));
    };
    const subscription = client.subscribe(queryId, handleResult, error => {
      if (active) setResult(current => ({ ...current, state: { ...current.state, error } }));
    }, stateChanged);
    subscriptionRef.current = { scope, subscription };
    return () => {
      active = false;
      if (subscriptionRef.current?.subscription === subscription) subscriptionRef.current = null;
      subscription();
    };
  }, [queryId, client, initialized, providerError, scope]);

  const rows = result.scope === scope ? result.rows : null;
  const projection = useMemo(() => {
    if (rows === null) return { data: null, error: null };
    const details = { instanceId: client?.instanceId, resourceKind: 'query' as const, resourceId: queryId };
    try {
      const keyed = accumulateResult([], { kind: 'snapshot', queryId, rows, receivedAt: 0 }, options.getKey, details);
      const transformed: T[] = [];
      for (const row of keyed) {
        const value = options.transform(row);
        if (value === null) continue;
        if (typeof value !== 'object') throw new DrasiError('RESULT_PROCESSING_FAILED', details);
        transformed.push(value);
      }
      const data = options.postProcess ? options.postProcess(transformed) : transformed;
      if (!Array.isArray(data) || data.some(value => value === null || typeof value !== 'object')) {
        throw new DrasiError('RESULT_PROCESSING_FAILED', details);
      }
      return { data, error: null };
    } catch (error) {
      return { data: null, error: asDrasiError(error, details, 'RESULT_PROCESSING_FAILED') };
    }
  }, [rows, options.getKey, options.transform, options.postProcess, client, queryId]);
  useEffect(() => {
    if (projection.data !== null) lastGood.current = { scope, data: projection.data, lastUpdate: result.lastUpdate };
  }, [projection, scope, result.lastUpdate]);

  const previous = lastGood.current?.scope === scope ? lastGood.current : null;
  const data = projection.error ? previous?.data ?? null : projection.data;
  const state = result.scope === scope ? result.state : null;
  const error = providerError ?? state?.error ?? projection.error;
  const phase = providerError || projection.error ? 'terminal-error' : state?.status ?? 'initial-loading';
  const status = phase === 'initial-loading' && data !== null ? 'resynchronizing'
    : phase === 'live' && data?.length === 0 ? 'empty' : phase;
  return {
    data, status, stale: data !== null && (status !== 'live' && status !== 'empty'),
    loading: data === null && error === null && status !== 'terminal-error',
    error, errorScope: providerError ? 'connection' : state?.errorScope ?? (projection.error ? 'query' : null),
    lastUpdate: projection.error ? previous?.lastUpdate ?? null : result.scope === scope ? result.lastUpdate : null,
    retry,
  };
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
