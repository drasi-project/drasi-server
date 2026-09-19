// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { useCallback, useEffect, useMemo, useState, type ReactNode } from 'react';
import { DrasiClient, DrasiError, type DrasiClientOptions } from '@drasi/react/client';
import { DrasiClientProvider, type DrasiContextValue } from '@drasi/react/react';
import { DRASI_SERVER_URL, TRADING_QUERY_IDS, TRADING_STREAM, tradingResultAdapter } from './config';
import { canPrepareTrading, ensureTradingResources, resolveTradingInstance } from './ensureTradingResources';

type TradingProviderProps = Partial<Pick<DrasiClientOptions,
  'serverUrl' | 'instanceId' | 'fetch' | 'eventSourceFactory' | 'reconnect'>> & { children: ReactNode };

/** Owns demo setup and the same single client/stream consumed by package hooks. */
export function TradingProvider({
  serverUrl = DRASI_SERVER_URL, instanceId, fetch: fetcher = globalThis.fetch,
  eventSourceFactory, reconnect, children,
}: TradingProviderProps) {
  const [attempt, setAttempt] = useState(0);
  const [state, setState] = useState<Omit<DrasiContextValue, 'retry'>>({
    client: null, initialized: false, error: null,
  });
  const retry = useCallback(() => setAttempt(current => current + 1), []);
  const stableReconnect = useMemo(() => reconnect, [
    reconnect?.maxReconnectAttempts, reconnect?.initialReconnectDelayMs,
    reconnect?.maxReconnectDelayMs, reconnect?.connectionTimeoutMs,
  ]);

  useEffect(() => {
    const controller = new AbortController();
    let client: DrasiClient | null = null;
    let unsubscribe = () => {};
    setState({ client: null, initialized: false, error: null });
    const connect = async () => {
      const baseUrl = serverUrl.replace(/\/+$/, '');
      const resolvedId = await resolveTradingInstance(baseUrl, fetcher, controller.signal, instanceId);
      controller.signal.throwIfAborted();
      client = new DrasiClient({
        serverUrl: baseUrl, instanceId: resolvedId, queryIds: TRADING_QUERY_IDS, reaction: TRADING_STREAM,
        resultAdapter: tradingResultAdapter, fetch: fetcher, eventSourceFactory, reconnect: stableReconnect,
      });
      setState({ client, initialized: false, error: null });
      // Initial failures are handled below. Later terminal connection failures
      // remain actionable through the same hooks and explicit Retry button.
      let ready = false;
      unsubscribe = client.onConnectionStatusChange(status => {
        if (ready && !controller.signal.aborted && status.error && !status.reconnecting) {
          setState({ client, initialized: false, error: status.error });
        }
      });
      try {
        await client.initialize();
      } catch (error) {
        controller.signal.throwIfAborted();
        if (!canPrepareTrading(error, resolvedId)) throw error;
        await ensureTradingResources({ client, serverUrl: baseUrl, fetch: fetcher }, error, controller.signal);
        controller.signal.throwIfAborted();
        await client.initialize(); // One setup pass, one retry. No blind create loop.
      }
      controller.signal.throwIfAborted();
      ready = true;
      setState({ client, initialized: true, error: null });
    };
    void connect().catch(error => {
      if (!controller.signal.aborted) {
        setState({ client, initialized: false, error: error instanceof DrasiError
          ? error : new DrasiError('INVALID_PAYLOAD', { instanceId }) });
      }
    });
    return () => {
      controller.abort();
      unsubscribe();
      void client?.disconnect();
    };
  }, [serverUrl, instanceId, fetcher, eventSourceFactory, stableReconnect, attempt]);

  const value = useMemo(() => ({ ...state, retry }), [state, retry]);
  return <DrasiClientProvider value={value}>
    {state.error && <div role="alert" className="text-red-400 p-4">
      {state.error.message} <button type="button" onClick={retry}>Retry connection</button>
    </div>}
    {children}
  </DrasiClientProvider>;
}
