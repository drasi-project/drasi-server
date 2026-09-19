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

import { DrasiSSEClient, type EventSourceFactory } from './DrasiSSEClient';
import type {
  Component, ConnectionStatus, DrasiHeaders, QueryConfig, QueryResult, QuerySubscription,
  QuerySubscriptionState, ReactionConfig, ReactionReference, ReconnectOptions, ResultAdapter,
  ResultReconciliationOptions, ResultRow,
} from './types';
import { DrasiError, asDrasiError, type DrasiErrorDetails } from './errors';
import { instancePath, isIdentifier, readQuery, readReaction, readResponse, readRows, requireRunning, validateHttpUrl } from './resources';
import { abortable, requestHeaders, resolveHeaders, validateCredentials } from './transport';
import { subscribeToQuery } from './subscription';

/** References to pre-existing resources. No option enables resource management. */
export interface DrasiClientOptions {
  /** Absolute HTTP(S) server base URL. No default or instance discovery. */
  serverUrl: string;
  instanceId: string;
  queryIds: readonly string[];
  reaction: ReactionReference;
  /** Strict SSE 0.3.4 by default; use a named adapter for legacy/custom formats. */
  resultAdapter?: ResultAdapter;
  reconciliation?: ResultReconciliationOptions;
  /** Invoked with the global receiver; must honor RequestInit.signal. GETs only. */
  fetch?: typeof globalThis.fetch;
  /** REST and custom SSE credentials; default same-origin. Native SSE cannot honor omit. */
  credentials?: RequestCredentials;
  /** Shared auth. Custom headers require an eventSourceFactory when opening a stream, not for REST-only reads. */
  headers?: DrasiHeaders;
  eventSourceFactory?: EventSourceFactory;
  /** REST timeout per request, including reading its body (default 10000 ms). */
  requestTimeoutMs?: number;
  /** SSE and snapshot retries are bounded by the same policy. */
  reconnect?: ReconnectOptions;
}

/**
 * Read-only resource validation, snapshots and one multiplexed SSE connection.
 * It never creates, starts, stops, updates or deletes resources. Known overlap
 * triggers a bounded refresh, not guessed replay order or an atomic handoff.
 */
export class DrasiClient {
  private readonly baseUrl: string;
  private readonly sseClient: DrasiSSEClient;
  private readonly queryIds: Set<string>;
  private readonly reaction: ReactionReference;
  private readonly fetcher: typeof globalThis.fetch;
  private readonly credentials: RequestCredentials;
  private readonly headers?: DrasiHeaders;
  private readonly requestTimeoutMs: number;
  private readonly maxRetries: number;
  private readonly retryDelay: number;
  private readonly maxRetryDelay: number;
  private readonly maxPendingChanges: number;
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
    const fetcher = options.fetch ?? globalThis.fetch;
    if (typeof fetcher !== 'function') throw new DrasiError('INVALID_CONFIGURATION', details);
    this.fetcher = fetcher.bind(globalThis);
    this.credentials = validateCredentials(options.credentials, details);
    this.headers = typeof options.headers === 'function'
      ? options.headers : requestHeaders(options.headers, undefined, details);
    this.requestTimeoutMs = options.requestTimeoutMs ?? 10000;
    this.maxRetries = options.reconnect?.maxReconnectAttempts ?? 10;
    this.retryDelay = options.reconnect?.initialReconnectDelayMs ?? 1000;
    this.maxRetryDelay = options.reconnect?.maxReconnectDelayMs ?? 30000;
    this.maxPendingChanges = options.reconciliation?.maxPendingChanges ?? 10000;
    if (!Number.isFinite(this.requestTimeoutMs) || this.requestTimeoutMs <= 0 ||
        !Number.isSafeInteger(this.maxPendingChanges) || this.maxPendingChanges < 1) {
      throw new DrasiError('INVALID_CONFIGURATION', details);
    }
    this.sseClient = new DrasiSSEClient({
      ...options.reconnect,
      resultAdapter: options.resultAdapter,
      eventSourceFactory: options.eventSourceFactory,
      headers: this.headers,
      credentials: this.credentials,
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
      const data = await abortable((async () => {
        const headers = typeof this.headers === 'function'
          ? await resolveHeaders(this.headers, {
            url, transport: 'rest', instanceId: this.instanceId, signal: controller.signal,
          }, details)
          : requestHeaders(this.headers, 'rest', details);
        const response = await this.fetcher(url, {
          method: 'GET', headers, credentials: this.credentials,
          redirect: 'manual', signal: controller.signal,
        });
        controller.signal.throwIfAborted();
        if (response.redirected) throw new DrasiError('INCOMPATIBLE_RESOURCE', details);
        return readResponse(response, details);
      })(), controller.signal);
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
    const queries = await Promise.allSettled([...this.queryIds].map(async queryId => {
      requireRunning(await this.getQuery(queryId, signal), this.details('query', queryId));
    }));
    // Drain a validation batch before handing an error to an app that may
    // immediately retry. Explicit cancellation still aborts every request.
    for (const query of queries) {
      if (query.status === 'rejected') throw query.reason;
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

  async getQueryResults(queryId: string, signal?: AbortSignal): Promise<ResultRow[]> {
    requireRunning(await this.getQuery(queryId, signal), this.details('query', queryId));
    const data = await this.read('queries', queryId, 'results', signal);
    return readRows(data, this.details('query', queryId));
  }

  /**
   * Each subscription has independent REST reconciliation/retry work on the
   * same socket. A snapshot overlapping any delta is not replayed: it triggers
   * a bounded refresh with visible resynchronizing state. A no-known-overlap
   * snapshot establishes a best-effort baseline, not gap-free synchronization.
   * The returned callable unsubscribes; its retry() never restarts the socket.
   */
  subscribe(
    queryId: string,
    callback: (result: QueryResult) => void,
    onError?: (error: DrasiError) => void,
    onStateChange?: (state: QuerySubscriptionState) => void,
  ): QuerySubscription {
    if (!this.queryIds.has(queryId)) {
      const error = new DrasiError('INVALID_CONFIGURATION', this.details('query', queryId));
      if (!onError) throw error;
      const state: QuerySubscriptionState = {
        status: 'terminal-error', stale: false, error, errorScope: 'query',
      };
      onError(error);
      onStateChange?.(state);
      return Object.assign(() => {}, { retry: () => onError(error), getState: () => ({ ...state }) });
    }
    const subscription = subscribeToQuery({
      queryId, source: this.sseClient,
      snapshot: signal => this.getQueryResults(queryId, signal),
      details: this.details('query', queryId),
      maxRetries: this.maxRetries, retryDelay: this.retryDelay, maxRetryDelay: this.maxRetryDelay,
      maxPendingChanges: this.maxPendingChanges,
      onStop: () => { this.subscriptions.delete(subscription); },
    }, callback, onError, onStateChange);
    this.subscriptions.add(subscription);
    return subscription;
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
