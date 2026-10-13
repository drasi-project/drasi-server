// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { DrasiError, asDrasiError, isAbortError, type DrasiErrorDetails } from './errors';
import type { DrasiSSEClient } from './DrasiSSEClient';
import type {
  ConnectionStatus, QueryResult, QuerySubscription, QuerySubscriptionState, ResultRow,
} from './types';

interface SubscriptionOptions {
  queryId: string;
  source: DrasiSSEClient;
  snapshot: (signal: AbortSignal) => Promise<ResultRow[]>;
  details: DrasiErrorDetails;
  maxRetries: number;
  retryDelay: number;
  maxRetryDelay: number;
  maxPendingChanges: number;
  onStop: () => void;
}

/**
 * A known-overlap refresh policy, not an atomic snapshot/stream handoff.
 * Pending changes are counted, not retained/replayed: without a shared cursor
 * neither the snapshot nor an overlapping delta can be chosen as "newer".
 */
export function subscribeToQuery(
  options: SubscriptionOptions,
  callback: (result: QueryResult) => void,
  onError?: (error: DrasiError) => void,
  onStateChange?: (state: QuerySubscriptionState) => void,
): QuerySubscription {
  const { source, queryId, details } = options;
  let active = true;
  let queryTerminal = false;
  let hasSnapshot = false;
  let started = false;
  let snapshotReady = false;
  let interrupted = true;
  let generation = 0;
  let pendingChanges = 0;
  let retries = 0;
  let controller: AbortController | null = null;
  let timer: ReturnType<typeof setTimeout> | null = null;
  let unsubscribeStatus = () => {};
  let state: QuerySubscriptionState = {
    status: 'initial-loading', stale: false, error: null, errorScope: null,
  };

  const publish = (next: QuerySubscriptionState) => {
    state = next;
    try {
      onStateChange?.({ ...state });
    } catch {
      console.error('Drasi query state listener failed.');
    }
  };
  const report = (error: DrasiError) => {
    try {
      if (onError) onError(error);
      else console.error('Drasi query subscription failed:', error.code);
    } catch {
      console.error('Drasi query error listener failed.');
    }
  };
  const suspend = () => {
    snapshotReady = false;
    pendingChanges = 0;
    generation += 1;
    controller?.abort();
    controller = null;
    if (timer !== null) clearTimeout(timer);
    timer = null;
  };
  const fail = (error: DrasiError) => {
    suspend();
    const failedGeneration = generation;
    const retrying = error.retryable && retries < options.maxRetries;
    queryTerminal = !retrying;
    const overlap = error.code === 'SNAPSHOT_OVERLAP' || error.code === 'RESULT_BUFFER_OVERFLOW';
    publish({
      status: !retrying ? 'terminal-error'
        : hasSnapshot && !overlap ? 'stale-last-good-data' : 'resynchronizing',
      stale: hasSnapshot, error, errorScope: 'query',
    });
    report(error);
    if (active && failedGeneration === generation && retrying && source.isConnected()) {
      const delay = Math.min(options.retryDelay * 2 ** retries++, options.maxRetryDelay);
      timer = setTimeout(fetchSnapshot, delay);
    }
  };
  const deliver = (result: QueryResult): boolean => {
    try {
      callback(result);
      return true;
    } catch (error) {
      if (active) fail(asDrasiError(error, details, 'RESULT_PROCESSING_FAILED'));
      return false;
    }
  };
  const fetchSnapshot = () => {
    if (!active || queryTerminal || !source.isConnected()) return;
    suspend();
    const current = generation;
    const request = new AbortController();
    controller = request;
    publish({
      status: started ? 'resynchronizing' : 'initial-loading',
      stale: hasSnapshot, error: state.error, errorScope: state.errorScope,
    });
    started = true;
    if (!active || current !== generation) return;
    void options.snapshot(request.signal).then(rows => {
      if (!active || current !== generation) return;
      if (pendingChanges > 0) {
        fail(new DrasiError('SNAPSHOT_OVERLAP', details));
        return;
      }
      controller = null;
      if (!deliver({ kind: 'snapshot', queryId, rows, receivedAt: Date.now() }) ||
          !active || current !== generation) return;
      hasSnapshot = true;
      snapshotReady = true;
      retries = 0;
      publish({ status: 'live', stale: false, error: null, errorScope: null });
    }).catch(error => {
      if (active && current === generation && !isAbortError(error)) {
        fail(asDrasiError(error, details));
      }
    });
  };
  const unsubscribe = source.subscribe(queryId, result => {
    if (!active || queryTerminal || !source.isConnected()) return;
    if (controller) {
      pendingChanges += result.changes.length;
      if (pendingChanges > options.maxPendingChanges) {
        fail(new DrasiError('RESULT_BUFFER_OVERFLOW', details));
      }
    } else if (snapshotReady) {
      deliver(result);
    }
    // During retry backoff data stays visibly stale; the next REST read replaces
    // it. No unbounded pending row bodies or silent "live" truncation.
  }, error => {
    if (active && !queryTerminal) fail(error);
  });
  const connectionChanged = (connection: ConnectionStatus) => {
    if (!active) return;
    if (!connection.connected) {
      interrupted = true;
      suspend();
      if (queryTerminal) return;
      publish({
        status: connection.error && !connection.reconnecting ? 'terminal-error'
          : started || connection.reconnecting ? 'reconnecting' : 'initial-loading',
        stale: hasSnapshot,
        error: connection.error ?? null,
        errorScope: connection.error ? 'connection' : null,
      });
      if (connection.error && !connection.reconnecting) report(connection.error);
    } else if (interrupted && !queryTerminal) {
      interrupted = false;
      retries = 0;
      fetchSnapshot();
    }
  };
  const stop = () => {
    active = false;
    suspend();
    unsubscribe();
    unsubscribeStatus();
    options.onStop();
  };
  const subscription: QuerySubscription = Object.assign(stop, {
    retry: () => {
      if (!active) return;
      queryTerminal = false;
      retries = 0;
      suspend();
      state = { ...state, error: null, errorScope: null };
      if (source.isConnected()) fetchSnapshot();
      else connectionChanged(source.getConnectionStatus());
    },
    getState: () => ({ ...state }),
  });
  unsubscribeStatus = source.onConnectionStatusChange(connectionChanged);
  return subscription;
}
