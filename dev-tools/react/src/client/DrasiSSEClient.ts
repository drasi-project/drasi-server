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

import type {
  ConnectionStatus, DrasiHeaders, DrasiHeadersProvider, EventSourceOptions, QueryDelta, ReconnectOptions,
  ResultAdapter,
} from './types';
import { DrasiError, asDrasiError, isAbortError, type DrasiErrorDetails } from './errors';
import { isIdentifier, isRecord } from './resources';
import { abortable, requestHeaders, resolveHeaders, validateCredentials } from './transport';
import { readAdaptedResults, sse034ResultAdapter } from './results';

export interface EventSourceLike {
  onopen: ((event: Event) => void) | null;
  onmessage: ((event: MessageEvent<string>) => void) | null;
  onerror: ((event: Event) => void) | null;
  addEventListener(type: string, listener: EventListener): void;
  close(): void;
}

/** Synchronous factory; credentials/headers are resolved afresh before each invocation. */
export type EventSourceFactory = (url: string, options: EventSourceOptions) => EventSourceLike;

export interface DrasiSSEClientOptions extends ReconnectOptions {
  /** Defaults to the strict, untemplated SSE 0.3.4 adapter. */
  resultAdapter?: ResultAdapter;
  eventSourceFactory?: EventSourceFactory;
  credentials?: RequestCredentials;
  headers?: DrasiHeaders;
  /** Read-only resource validation, including after opaque EventSource errors. */
  validate?: (signal: AbortSignal) => Promise<void>;
  errorDetails?: DrasiErrorDetails;
}

interface PendingConnection {
  generation: number;
  resolve: () => void;
  reject: (error: unknown) => void;
  cleanupAbort: () => void;
}

interface ResultSubscriber {
  callback: (result: QueryDelta) => void;
  onError?: (error: DrasiError) => void;
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
    Set<ResultSubscriber>
  >();
  private readonly queryErrors = new Map<string, DrasiError>();
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
  private readonly resultAdapter: ResultAdapter;
  private readonly eventSourceFactory: EventSourceFactory;
  private generation = 0;
  private manuallyDisconnected = true;
  private pendingConnection: PendingConnection | null = null;
  private attemptController: AbortController | null = null;
  private openTimer: ReturnType<typeof setTimeout> | null = null;
  private readonly connectionTimeoutMs: number;
  private readonly validate?: DrasiSSEClientOptions['validate'];
  private readonly errorDetails: DrasiErrorDetails;
  private readonly credentials: RequestCredentials;
  private readonly headers: Headers | DrasiHeadersProvider;

  constructor(options: DrasiSSEClientOptions = {}) {
    this.resultAdapter = options.resultAdapter ?? sse034ResultAdapter;
    this.errorDetails = options.errorDetails ?? {};
    this.credentials = validateCredentials(options.credentials, this.errorDetails);
    this.headers = typeof options.headers === 'function' ? options.headers
      : requestHeaders(options.headers, undefined, this.errorDetails);
    this.eventSourceFactory =
      options.eventSourceFactory ??
      ((url, { credentials }) => {
        // REST-only inspection remains usable in Node. Reject unsupported
        // native stream authentication at execution, never silently omit it.
        if (typeof EventSource === 'undefined' || credentials === 'omit' ||
            typeof this.headers === 'function' || !this.headers.keys().next().done) {
          throw new DrasiError('INVALID_CONFIGURATION', this.errorDetails);
        }
        return new EventSource(url, { withCredentials: credentials === 'include' });
      });
    this.maxReconnectAttempts = options.maxReconnectAttempts ?? 10;
    this.initialReconnectDelayMs = options.initialReconnectDelayMs ?? 1000;
    this.maxReconnectDelayMs = options.maxReconnectDelayMs ?? 30000;
    this.connectionTimeoutMs = options.connectionTimeoutMs ?? 10000;
    this.validate = options.validate;
    if (typeof this.resultAdapter !== 'function' ||
        !Number.isInteger(this.maxReconnectAttempts) || this.maxReconnectAttempts < 0 ||
        [this.initialReconnectDelayMs, this.maxReconnectDelayMs, this.connectionTimeoutMs]
          .some(value => !Number.isFinite(value) || value <= 0)) {
      throw new DrasiError('INVALID_CONFIGURATION', this.errorDetails);
    }
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
          this.manuallyDisconnected = true;
          this.generation += 1;
          this.stopConnection(signal?.reason ?? abortError());
          this.updateConnectionStatus({ connected: false, reconnecting: false });
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

    const controller = new AbortController();
    this.attemptController = controller;
    if (this.validate) {
      void this.validate(controller.signal).then(() => {
        if (!controller.signal.aborted && generation === this.generation) this.createSource(generation);
      }).catch(error => {
        if (!controller.signal.aborted && !isAbortError(error)) this.handleConnectionFailure(generation, error);
      });
    } else {
      this.createSource(generation);
    }
  }

  private createSource(generation: number): void {
    const controller = this.attemptController;
    if (!controller || controller.signal.aborted || generation !== this.generation || !this.sseEndpoint) return;
    this.openTimer = setTimeout(() => {
      if (this.eventSource) this.handleConnectionError(generation, this.eventSource);
      else this.handleConnectionFailure(generation, new DrasiError('STREAM_UNAVAILABLE', this.errorDetails));
    }, this.connectionTimeoutMs);
    if (typeof this.headers === 'function') {
      void abortable(resolveHeaders(this.headers, {
        url: this.sseEndpoint, transport: 'sse', signal: controller.signal,
        instanceId: this.errorDetails.instanceId,
      }, this.errorDetails), controller.signal).then(headers => {
        if (!controller.signal.aborted && generation === this.generation) this.attachSource(generation, headers);
      }).catch(error => {
        if (!controller.signal.aborted) this.handleConnectionFailure(generation, error);
      });
    } else {
      this.attachSource(generation, requestHeaders(this.headers, 'sse', this.errorDetails));
    }
  }

  private attachSource(generation: number, headers: Headers): void {
    try {
      const source = this.eventSourceFactory(this.sseEndpoint!, {
        headers, credentials: this.credentials, signal: this.attemptController!.signal,
      });
      this.eventSource = source;

      source.onopen = () => {
        if (generation !== this.generation || source !== this.eventSource) {
          source.close();
          return;
        }
        this.clearOpenTimer();
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
        this.parseMessage(event.data, generation);
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

      source.addEventListener('query-result', (event: Event) => {
        if (generation !== this.generation || source !== this.eventSource) {
          return;
        }
        this.parseMessage('data' in event ? event.data : undefined, generation);
      });
    } catch (error) {
      this.handleConnectionFailure(generation, error);
    }
  }

  private parseMessage(rawData: unknown, generation: number): void {
    let details = this.errorDetails;
    try {
      if (typeof rawData !== 'string') throw new DrasiError('INVALID_PAYLOAD', this.errorDetails);
      const data: unknown = JSON.parse(rawData);
      if (isRecord(data)) {
        const id = data.queryId ?? data.query_id;
        if (isIdentifier(id)) details = { instanceId: details.instanceId, resourceKind: 'query', resourceId: id };
      }
      const context = { ...this.errorDetails, receivedAt: Date.now() };
      const results = readAdaptedResults(this.resultAdapter(data, context), context);
      for (const result of results) this.handleQueryResult(result);
    } catch (error) {
      const failure = asDrasiError(error, details);
      if (failure.resourceKind === 'query' && failure.resourceId) {
        this.queryErrors.set(failure.resourceId, failure);
        this.subscribers.get(failure.resourceId)?.forEach(subscriber => this.reportSubscriberError(subscriber, failure));
      } else {
        this.handleConnectionFailure(generation, failure);
      }
    }
  }

  private handleConnectionError(
    generation: number,
    source: EventSourceLike,
  ): void {
    if (generation !== this.generation || source !== this.eventSource) return;
    this.clearOpenTimer();
    source.close();
    if (source === this.eventSource) {
      this.eventSource = null;
    }
    const error = new DrasiError('STREAM_UNAVAILABLE', this.errorDetails);
    this.updateConnectionStatus({ connected: false, reconnecting: true, error });
    const controller = this.attemptController!;
    // EventSource exposes no HTTP status. Only REST can establish missing or
    // stopped resources; a generic stream failure never implies absence.
    if (this.validate) {
      void this.validate(controller.signal)
        .then(() => this.handleConnectionFailure(generation, error))
        .catch(failure => {
          if (!controller.signal.aborted) this.handleConnectionFailure(generation, failure);
        });
    } else {
      this.handleConnectionFailure(generation, error);
    }
  }

  private handleConnectionFailure(
    generation: number,
    error: unknown,
  ): void {
    if (generation !== this.generation || this.manuallyDisconnected) {
      return;
    }

    this.clearOpenTimer();
    this.eventSource?.close();
    this.eventSource = null;
    this.attemptController?.abort();
    this.attemptController = null;
    const connectionError = asDrasiError(error, this.errorDetails, 'STREAM_UNAVAILABLE');
    if (!connectionError.retryable || this.reconnectAttempts >= this.maxReconnectAttempts) {
      this.clearReconnectTimer();
      this.manuallyDisconnected = true;
      this.updateConnectionStatus({
        connected: false,
        reconnecting: false,
        error: connectionError,
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
      error: connectionError,
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

  private rejectPendingConnection(generation: number, error: unknown): void {
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

  private clearOpenTimer(): void {
    if (this.openTimer !== null) clearTimeout(this.openTimer);
    this.openTimer = null;
  }

  private stopConnection(error: unknown): void {
    this.clearReconnectTimer();
    this.clearOpenTimer();
    this.attemptController?.abort();
    this.attemptController = null;
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

  private handleQueryResult(result: QueryDelta): void {
    this.queryErrors.delete(result.queryId);
    const subscribers = this.subscribers.get(result.queryId);
    // A multiplexed reaction may include valid queries not selected by this consumer.
    if (!subscribers || result.changes.length === 0) return;

    subscribers.forEach(subscriber => {
      try {
        subscriber.callback(result);
      } catch (error) {
        this.reportSubscriberError(subscriber, asDrasiError(error, {
          instanceId: this.errorDetails.instanceId, resourceKind: 'query', resourceId: result.queryId,
        }, 'RESULT_PROCESSING_FAILED'));
      }
    });
  }

  private reportSubscriberError(subscriber: ResultSubscriber, error: DrasiError): void {
    if (error.resourceKind === 'query' && error.resourceId) this.queryErrors.set(error.resourceId, error);
    try {
      if (subscriber.onError) subscriber.onError(error);
      else console.error('Drasi result subscriber failed:', error.code);
    } catch {
      console.error('Drasi result error listener failed.');
    }
  }

  /** Query-scoped protocol errors do not close the shared socket. */
  getQueryError(queryId: string): DrasiError | null {
    return this.queryErrors.get(queryId) ?? null;
  }

  subscribe(
    queryId: string,
    callback: (result: QueryDelta) => void,
    onError?: (error: DrasiError) => void,
  ): () => void {
    let callbacks = this.subscribers.get(queryId);
    if (!callbacks) {
      callbacks = new Set();
      this.subscribers.set(queryId, callbacks);
    }
    const subscriber = { callback, onError };
    callbacks.add(subscriber);

    return () => {
      const currentCallbacks = this.subscribers.get(queryId);
      currentCallbacks?.delete(subscriber);
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
      } catch {
        console.error('Drasi connection status listener failed.');
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
    this.queryErrors.clear();
    this.statusListeners.clear();
  }

  isConnected(): boolean {
    return this.connectionStatus.connected;
  }
}
