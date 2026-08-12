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

import { ConnectionStatus, QueryResult, RouteUnidentified } from '../types';

const DEBUG_SSE =
  (
    globalThis as typeof globalThis & {
      process?: { env?: { NODE_ENV?: string } };
    }
  ).process?.env?.NODE_ENV === 'development';

export interface EventSourceLike {
  onopen: ((event: Event) => void) | null;
  onmessage: ((event: MessageEvent) => void) | null;
  onerror: ((event: Event) => void) | null;
  addEventListener(type: string, listener: EventListener): void;
  close(): void;
}

export type EventSourceFactory = (url: string) => EventSourceLike;

export interface DrasiSSEClientOptions {
  routeUnidentified?: RouteUnidentified;
  eventSourceFactory?: EventSourceFactory;
  maxReconnectAttempts?: number;
  initialReconnectDelayMs?: number;
  maxReconnectDelayMs?: number;
}

interface PendingConnection {
  generation: number;
  resolve: () => void;
  reject: (error: Error) => void;
  cleanupAbort: () => void;
}

function abortError(message = 'SSE connection aborted'): Error {
  return new DOMException(message, 'AbortError');
}

/**
 * Maintains one explicitly managed EventSource connection and multiplexes
 * result batches to query-specific subscribers.
 */
export class DrasiSSEClient {
  private eventSource: EventSourceLike | null = null;
  private readonly subscribers = new Map<
    string,
    Set<(result: QueryResult) => void>
  >();
  private connectionStatus: ConnectionStatus = { connected: false };
  private readonly statusListeners = new Set<
    (status: ConnectionStatus) => void
  >();
  private reconnectAttempts = 0;
  private readonly maxReconnectAttempts: number;
  private readonly initialReconnectDelayMs: number;
  private readonly maxReconnectDelayMs: number;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private sseEndpoint: string | null = null;
  private readonly routeUnidentified?: RouteUnidentified;
  private readonly eventSourceFactory: EventSourceFactory;
  private generation = 0;
  private manuallyDisconnected = true;
  private pendingConnection: PendingConnection | null = null;

  constructor(options: DrasiSSEClientOptions = {}) {
    this.routeUnidentified = options.routeUnidentified;
    this.eventSourceFactory =
      options.eventSourceFactory ??
      ((url) => new EventSource(url) as EventSourceLike);
    this.maxReconnectAttempts = options.maxReconnectAttempts ?? 10;
    this.initialReconnectDelayMs = options.initialReconnectDelayMs ?? 1000;
    this.maxReconnectDelayMs = options.maxReconnectDelayMs ?? 30000;
  }

  /**
   * Connect to the Drasi reaction's SSE stream. Native EventSource retries are
   * disabled by closing a failed source before scheduling the library's own
   * bounded exponential-backoff retry.
   */
  async connect(
    _queryIds: string[],
    sseEndpoint: string,
    signal?: AbortSignal,
  ): Promise<void> {
    this.stopConnection(abortError('SSE connection replaced'));

    this.manuallyDisconnected = false;
    this.sseEndpoint = sseEndpoint;
    this.reconnectAttempts = 0;
    const generation = ++this.generation;

    return new Promise<void>((resolve, reject) => {
      const onAbort = () => {
        if (generation === this.generation) {
          this.stopConnection(abortError());
        }
      };
      signal?.addEventListener('abort', onAbort, { once: true });

      this.pendingConnection = {
        generation,
        resolve,
        reject,
        cleanupAbort: () => signal?.removeEventListener('abort', onAbort),
      };

      if (signal?.aborted) {
        onAbort();
        return;
      }

      this.openConnection(generation);
    });
  }

  private openConnection(generation: number): void {
    if (
      generation !== this.generation ||
      this.manuallyDisconnected ||
      !this.sseEndpoint
    ) {
      return;
    }

    try {
      DEBUG_SSE &&
        console.log(`Connecting to SSE endpoint: ${this.sseEndpoint}`);
      const source = this.eventSourceFactory(this.sseEndpoint);
      this.eventSource = source;

      source.onopen = () => {
        if (generation !== this.generation || source !== this.eventSource) {
          source.close();
          return;
        }
        this.reconnectAttempts = 0;
        this.updateConnectionStatus({
          connected: true,
          reconnecting: false,
          lastConnected: new Date(),
        });
        this.resolvePendingConnection(generation);
      };

      source.onmessage = (event) => {
        if (generation !== this.generation || source !== this.eventSource) {
          return;
        }
        this.parseMessage(event.data, 'SSE message');
      };

      source.onerror = () => {
        if (
          generation !== this.generation ||
          this.manuallyDisconnected ||
          source !== this.eventSource
        ) {
          return;
        }
        this.handleConnectionError(generation, source);
      };

      source.addEventListener('query-result', ((event: MessageEvent) => {
        if (generation !== this.generation || source !== this.eventSource) {
          return;
        }
        this.parseMessage(event.data, 'query-result event');
      }) as EventListener);

      source.addEventListener('heartbeat', ((event: MessageEvent) => {
        if (generation !== this.generation || source !== this.eventSource) {
          return;
        }
        DEBUG_SSE && console.log('Heartbeat received:', event.data);
      }) as EventListener);
    } catch (error) {
      this.handleConnectionFailure(generation, error);
    }
  }

  private parseMessage(rawData: string, description: string): void {
    try {
      this.handleSSEMessage(JSON.parse(rawData));
    } catch (error) {
      console.error(`Failed to parse ${description}:`, error);
    }
  }

  private handleConnectionError(
    generation: number,
    source: EventSourceLike,
  ): void {
    source.close();
    if (source === this.eventSource) {
      this.eventSource = null;
    }
    this.handleConnectionFailure(
      generation,
      new Error('SSE connection lost'),
    );
  }

  private handleConnectionFailure(
    generation: number,
    error: unknown,
  ): void {
    if (generation !== this.generation || this.manuallyDisconnected) {
      return;
    }

    const connectionError =
      error instanceof Error ? error : new Error(String(error));
    if (this.reconnectAttempts >= this.maxReconnectAttempts) {
      this.updateConnectionStatus({
        connected: false,
        reconnecting: false,
        error: connectionError.message,
      });
      this.rejectPendingConnection(generation, connectionError);
      return;
    }

    this.reconnectAttempts += 1;
    const delay = Math.min(
      this.initialReconnectDelayMs *
        Math.pow(2, this.reconnectAttempts - 1),
      this.maxReconnectDelayMs,
    );
    this.updateConnectionStatus({
      connected: false,
      reconnecting: true,
      error: connectionError.message,
    });

    this.clearReconnectTimer();
    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = null;
      this.openConnection(generation);
    }, delay);
  }

  private resolvePendingConnection(generation: number): void {
    const pending = this.pendingConnection;
    if (!pending || pending.generation !== generation) return;
    this.pendingConnection = null;
    pending.cleanupAbort();
    pending.resolve();
  }

  private rejectPendingConnection(generation: number, error: Error): void {
    const pending = this.pendingConnection;
    if (!pending || pending.generation !== generation) return;
    this.pendingConnection = null;
    pending.cleanupAbort();
    pending.reject(error);
  }

  private clearReconnectTimer(): void {
    if (this.reconnectTimer !== null) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
  }

  private stopConnection(error: Error): void {
    this.clearReconnectTimer();
    if (this.eventSource) {
      this.eventSource.close();
      this.eventSource = null;
    }
    const pending = this.pendingConnection;
    if (pending) {
      this.pendingConnection = null;
      pending.cleanupAbort();
      pending.reject(error);
    }
  }

  private handleSSEMessage(data: any): void {
    if (data.type === 'heartbeat') {
      return;
    }

    if (
      data.addedResults !== undefined ||
      data.updatedResults !== undefined ||
      data.deletedResults !== undefined
    ) {
      const allResults: any[] = [];

      if (Array.isArray(data.addedResults)) {
        for (const result of data.addedResults) {
          allResults.push(result.after || result);
        }
      }
      if (Array.isArray(data.updatedResults)) {
        for (const result of data.updatedResults) {
          allResults.push(result.after || result);
        }
      }
      if (Array.isArray(data.deletedResults)) {
        for (const result of data.deletedResults) {
          const item = result.before || result;
          allResults.push({ ...item, _deleted: true });
        }
      }

      if (allResults.length > 0) {
        this.routeContentBasedResults(allResults);
      }
      return;
    }

    if (data.query_id) {
      this.handleKeyedBatch(data.query_id, data);
      return;
    }

    if (data.queryId) {
      this.handleKeyedBatch(data.queryId, data);
      return;
    }

    if (data && typeof data === 'object' && Object.keys(data).length > 0) {
      this.routeContentBasedResults([data]);
    }
  }

  private handleKeyedBatch(queryId: string, data: any): void {
    if (Array.isArray(data.results)) {
      const extractedData = data.results
        .map((result: any) => this.extractRow(result))
        .filter((item: any) => item != null);

      if (extractedData.length > 0) {
        this.handleQueryResult({
          queryId,
          data: extractedData,
          timestamp: data.timestamp
            ? new Date(data.timestamp).getTime()
            : Date.now(),
        });
      }
      return;
    }

    if (data.type && data.data) {
      this.handleQueryResult({
        queryId,
        data: [data.data],
        timestamp: data.timestamp
          ? new Date(data.timestamp).getTime()
          : Date.now(),
      });
      return;
    }

    if (data.data !== undefined) {
      this.handleQueryResult({
        queryId,
        data: Array.isArray(data.data) ? data.data : [data.data],
        timestamp: data.timestamp
          ? new Date(data.timestamp).getTime()
          : Date.now(),
      });
    }
  }

  private extractRow(result: any): any {
    if (result == null || typeof result !== 'object') {
      return result;
    }
    if (result.type === 'aggregation' && result.after) {
      return result.after;
    }
    if (result.op === 'd' || (result.op === 'u' && !result.after)) {
      if (result.before) {
        return { ...result.before, _deleted: true };
      }
    }
    if (
      (result.op === 'c' || result.op === 'r' || result.op === 'u') &&
      result.after
    ) {
      return result.after;
    }
    if (result.type === 'delete' || result.type === 'DELETE') {
      const deleteData = result.before || result.data;
      if (deleteData) {
        return { ...deleteData, _deleted: true };
      }
    }
    if ((result.type === 'add' || result.type === 'ADD') && result.data) {
      return result.data;
    }
    if (
      (result.type === 'update' || result.type === 'UPDATE') &&
      result.after
    ) {
      return result.after;
    }
    if (result.data !== undefined) {
      return result.data;
    }
    return result;
  }

  private routeContentBasedResults(rows: any[]): void {
    if (this.routeUnidentified) {
      try {
        this.routeUnidentified(rows, (queryId, data) =>
          this.deliverToQuery(queryId, data),
        );
      } catch (error) {
        console.error('Failed to route unidentified SSE results:', error);
      }
      return;
    }
    DEBUG_SSE &&
      console.warn(
        'Received results without a query id and no routeUnidentified handler is configured.',
        rows[0],
      );
  }

  private deliverToQuery(queryId: string, data: any[]): void {
    this.handleQueryResult({ queryId, data, timestamp: Date.now() });
  }

  private handleQueryResult(result: QueryResult): void {
    const subscribers = this.subscribers.get(result.queryId);
    if (!subscribers) return;

    subscribers.forEach((callback) => {
      try {
        callback(result);
      } catch (error) {
        console.error(
          `Error in subscriber callback for ${result.queryId}:`,
          error,
        );
      }
    });
  }

  subscribe(
    queryId: string,
    callback: (result: QueryResult) => void,
  ): () => void {
    let callbacks = this.subscribers.get(queryId);
    if (!callbacks) {
      callbacks = new Set();
      this.subscribers.set(queryId, callbacks);
    }
    callbacks.add(callback);

    return () => {
      const currentCallbacks = this.subscribers.get(queryId);
      currentCallbacks?.delete(callback);
      if (currentCallbacks?.size === 0) {
        this.subscribers.delete(queryId);
      }
    };
  }

  getConnectionStatus(): ConnectionStatus {
    return { ...this.connectionStatus };
  }

  onConnectionStatusChange(
    callback: (status: ConnectionStatus) => void,
  ): () => void {
    this.statusListeners.add(callback);
    callback({ ...this.connectionStatus });
    return () => {
      this.statusListeners.delete(callback);
    };
  }

  private updateConnectionStatus(status: ConnectionStatus): void {
    this.connectionStatus = status;
    this.statusListeners.forEach((listener) => {
      try {
        listener({ ...status });
      } catch (error) {
        console.error('Error in status listener:', error);
      }
    });
  }

  async disconnect(): Promise<void> {
    this.manuallyDisconnected = true;
    this.generation += 1;
    this.stopConnection(abortError('SSE client disconnected'));
    this.sseEndpoint = null;
    this.reconnectAttempts = 0;
    this.updateConnectionStatus({ connected: false, reconnecting: false });
    this.subscribers.clear();
    this.statusListeners.clear();
  }

  isConnected(): boolean {
    return this.connectionStatus.connected;
  }
}
