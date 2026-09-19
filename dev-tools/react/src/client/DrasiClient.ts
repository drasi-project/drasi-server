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

import { DrasiSSEClient, type DrasiSSEClientOptions, type EventSourceFactory } from './DrasiSSEClient';
import type {
  Component, ConnectionStatus, QueryConfig, QueryResult, ReactionConfig,
  ReactionReference, RouteUnidentified,
} from '../types';
import { DrasiError, asDrasiError, isAbortError, type DrasiErrorDetails } from './errors';
import { instancePath, isIdentifier, readQuery, readReaction, readResponse, requireRunning, validateHttpUrl } from './resources';

/** References to pre-existing resources. No option enables resource management. */
export interface DrasiClientOptions {
  /** Absolute HTTP(S) server base URL. No default or instance discovery. */
  serverUrl: string;
  instanceId: string;
  queryIds: readonly string[];
  reaction: ReactionReference;
  routeUnidentified?: RouteUnidentified;
  fetch?: typeof globalThis.fetch;
  eventSourceFactory?: EventSourceFactory;
  /** REST timeout per request, including reading its body (default 10000 ms). */
  requestTimeoutMs?: number;
  /** SSE and snapshot retries are bounded by the same policy. */
  reconnect?: Pick<DrasiSSEClientOptions,
    'maxReconnectAttempts' | 'initialReconnectDelayMs' | 'maxReconnectDelayMs' | 'connectionTimeoutMs'>;
}

/**
 * Read-only resource validation, snapshots and one multiplexed SSE connection.
 * It never creates, starts, stops, updates or deletes resources. Buffered deltas
 * are replayed after snapshots; this is not an atomic/exactly-once handoff.
 */
export class DrasiClient {
  private readonly baseUrl: string;
  private readonly sseClient: DrasiSSEClient;
  private readonly queryIds: Set<string>;
  private readonly reaction: ReactionReference;
  private readonly fetcher: typeof globalThis.fetch;
  private readonly requestTimeoutMs: number;
  private readonly maxRetries: number;
  private readonly retryDelay: number;
  private readonly maxRetryDelay: number;
  private readonly subscriptions = new Set<() => void>();
  private initialized = false;
  private initPromise: Promise<void> | null = null;
  private initController: AbortController | null = null;
  readonly instanceId: string;

  constructor(options: DrasiClientOptions) {
    this.instanceId = options.instanceId;
    const details = { instanceId: this.instanceId };
    if (!isIdentifier(options.instanceId) || !Array.isArray(options.queryIds) ||
        options.queryIds.some(id => !isIdentifier(id)) ||
        new Set(options.queryIds).size !== options.queryIds.length || !isIdentifier(options.reaction?.id)) {
      throw new DrasiError('INVALID_CONFIGURATION', details);
    }
    this.baseUrl = validateHttpUrl(options.serverUrl, details);
    if (new URL(this.baseUrl).search) throw new DrasiError('INVALID_CONFIGURATION', details);
    this.reaction = {
      id: options.reaction.id,
      endpoint: validateHttpUrl(options.reaction.endpoint, details),
    };
    this.queryIds = new Set(options.queryIds);
    this.fetcher = (options.fetch ?? globalThis.fetch).bind(globalThis);
    this.requestTimeoutMs = options.requestTimeoutMs ?? 10000;
    this.maxRetries = options.reconnect?.maxReconnectAttempts ?? 10;
    this.retryDelay = options.reconnect?.initialReconnectDelayMs ?? 1000;
    this.maxRetryDelay = options.reconnect?.maxReconnectDelayMs ?? 30000;
    if (!Number.isFinite(this.requestTimeoutMs) || this.requestTimeoutMs <= 0) {
      throw new DrasiError('INVALID_CONFIGURATION', details);
    }
    this.sseClient = new DrasiSSEClient({
      ...options.reconnect,
      routeUnidentified: options.routeUnidentified,
      eventSourceFactory: options.eventSourceFactory,
      errorDetails: this.details('reaction', this.reaction.id),
      validate: signal => this.validateResources(signal),
    });
  }

  private details(resourceKind: 'query' | 'reaction', resourceId: string): DrasiErrorDetails {
    return { instanceId: this.instanceId, resourceKind, resourceId };
  }

  private async read(
    kind: 'queries' | 'reactions',
    id: string,
    suffix: 'config' | 'results',
    signal?: AbortSignal,
  ): Promise<unknown> {
    const details = this.details(kind === 'queries' ? 'query' : 'reaction', id);
    const controller = new AbortController();
    const abort = () => controller.abort();
    signal?.addEventListener('abort', abort, { once: true });
    const timer = setTimeout(abort, this.requestTimeoutMs);
    try {
      signal?.throwIfAborted();
      const url = suffix === 'config'
        ? `${instancePath(this.baseUrl, this.instanceId, kind, id)}?view=full`
        : instancePath(this.baseUrl, this.instanceId, kind, id, 'results');
      const response = await this.fetcher(url, { method: 'GET', signal: controller.signal });
      const data = await readResponse(response, details);
      controller.signal.throwIfAborted();
      return data;
    } catch (error) {
      signal?.throwIfAborted();
      if (controller.signal.aborted) throw new DrasiError('SERVER_UNAVAILABLE', details);
      throw asDrasiError(error, details, 'SERVER_UNAVAILABLE');
    } finally {
      clearTimeout(timer);
      signal?.removeEventListener('abort', abort);
    }
  }

  /** Read the server's full-view query DTO, including lifecycle status. */
  async getQuery(queryId: string, signal?: AbortSignal): Promise<Component<QueryConfig>> {
    return readQuery(await this.read('queries', queryId, 'config', signal), this.details('query', queryId));
  }

  /** Read the configured reaction's full-view DTO; properties are not nested. */
  async getReaction(signal?: AbortSignal): Promise<Component<ReactionConfig>> {
    return readReaction(
      await this.read('reactions', this.reaction.id, 'config', signal),
      this.details('reaction', this.reaction.id),
    );
  }

  /** Validate references/usability, not desired query text or deployment settings. */
  async validateResources(signal?: AbortSignal): Promise<void> {
    for (const queryId of this.queryIds) {
      requireRunning(await this.getQuery(queryId, signal), this.details('query', queryId));
    }
    const reaction = await this.getReaction(signal);
    const details = this.details('reaction', this.reaction.id);
    if (reaction.config.kind !== 'sse' ||
        [...this.queryIds].some(id => !reaction.config.queries.includes(id))) {
      throw new DrasiError('INCOMPATIBLE_RESOURCE', details);
    }
    requireRunning(reaction, details);
  }

  isInitialized(): boolean {
    return this.initialized;
  }

  async initialize(): Promise<void> {
    if (this.initialized && this.sseClient.isConnected()) return;
    if (this.initPromise) return this.initPromise;
    const controller = new AbortController();
    this.initController = controller;
    const promise = this.sseClient.connect([...this.queryIds], this.reaction.endpoint, controller.signal);
    this.initPromise = promise;
    try {
      await promise;
      if (this.initPromise === promise && !controller.signal.aborted) this.initialized = true;
    } finally {
      if (this.initPromise === promise) {
        this.initPromise = null;
        this.initController = null;
      }
    }
  }

  /** Optional definition read for tooling/code viewers; never a null-on-error fallback. */
  async getQueryConfig(queryId: string, signal?: AbortSignal): Promise<QueryConfig> {
    return (await this.getQuery(queryId, signal)).config;
  }

  async getQueryResults(queryId: string, signal?: AbortSignal): Promise<any[]> {
    requireRunning(await this.getQuery(queryId, signal), this.details('query', queryId));
    const data = await this.read('queries', queryId, 'results', signal);
    if (!Array.isArray(data)) throw new DrasiError('INVALID_PAYLOAD', this.details('query', queryId));
    return data;
  }

  /**
   * Listen before fetching a snapshot; replay buffered deltas afterward. On
   * reconnect, discard stale rows with a fresh snapshot. Permanent failures
   * terminate this subscription; transient snapshot failures have bounded retries.
   */
  subscribe(
    queryId: string,
    callback: (result: QueryResult) => void,
    onError?: (error: DrasiError) => void,
  ): () => void {
    if (!this.queryIds.has(queryId)) {
      const error = new DrasiError('INVALID_CONFIGURATION', this.details('query', queryId));
      if (!onError) throw error;
      onError(error);
      return () => {};
    }
    const queuedResults: QueryResult[] = [];
    let active = true;
    let snapshotReady = false;
    let snapshotGeneration = 0;
    let snapshotController: AbortController | null = null;
    let snapshotRetryAttempts = 0;
    let snapshotRetryTimer: ReturnType<typeof setTimeout> | null = null;
    let connectionWasInterrupted = !this.sseClient.isConnected();
    let unsubscribeStatus = () => {};

    const unsubscribe = this.sseClient.subscribe(queryId, result => {
      if (!active) return;
      if (!snapshotReady) queuedResults.push(result);
      else callback(result);
    });
    const clearSnapshotRetry = () => {
      if (snapshotRetryTimer !== null) clearTimeout(snapshotRetryTimer);
      snapshotRetryTimer = null;
    };
    const suspendSnapshot = () => {
      snapshotReady = false;
      queuedResults.length = 0;
      clearSnapshotRetry();
      snapshotController?.abort();
      snapshotController = null;
      snapshotGeneration += 1;
    };
    const stop = () => {
      active = false;
      suspendSnapshot();
      unsubscribeStatus();
      unsubscribe();
      this.subscriptions.delete(stop);
    };
    const fetchSnapshot = () => {
      if (!active || !this.sseClient.isConnected()) return;
      suspendSnapshot();
      const controller = new AbortController();
      snapshotController = controller;
      const generation = ++snapshotGeneration;
      void this.getQueryResults(queryId, controller.signal).then(rows => {
        if (!active || generation !== snapshotGeneration) return;
        snapshotController = null;
        snapshotRetryAttempts = 0;
        callback({ queryId, data: rows, timestamp: Date.now(), snapshot: true });
        snapshotReady = true;
        queuedResults.splice(0).forEach(callback);
      }).catch(error => {
        if (!active || generation !== snapshotGeneration || isAbortError(error)) return;
        const failure = asDrasiError(error, this.details('query', queryId));
        snapshotController = null;
        onError?.(failure);
        if (!failure.retryable || snapshotRetryAttempts >= this.maxRetries) {
          stop();
          return;
        }
        const delay = Math.min(this.retryDelay * 2 ** snapshotRetryAttempts++, this.maxRetryDelay);
        snapshotRetryTimer = setTimeout(fetchSnapshot, delay);
      });
    };
    this.subscriptions.add(stop);
    unsubscribeStatus = this.sseClient.onConnectionStatusChange(status => {
      if (!active) return;
      if (!status.connected) {
        connectionWasInterrupted = true;
        suspendSnapshot();
        if (status.error && !status.reconnecting) {
          onError?.(status.error);
          stop();
        }
        return;
      }
      if (connectionWasInterrupted) {
        connectionWasInterrupted = false;
        snapshotRetryAttempts = 0;
        fetchSnapshot();
      }
    });
    if (!active) unsubscribeStatus();
    if (this.sseClient.isConnected() && snapshotGeneration === 0) fetchSnapshot();
    return stop;
  }

  getConnectionStatus(): ConnectionStatus {
    return this.sseClient.getConnectionStatus();
  }

  onConnectionStatusChange(callback: (status: ConnectionStatus) => void): () => void {
    return this.sseClient.onConnectionStatusChange(callback);
  }

  getServerUiUrl(): string {
    return `${this.baseUrl}/ui?instance=${encodeURIComponent(this.instanceId)}`;
  }

  async disconnect(): Promise<void> {
    this.initialized = false;
    this.initController?.abort();
    this.initController = null;
    this.initPromise = null;
    for (const stop of this.subscriptions) stop();
    await this.sseClient.disconnect();
  }
}
