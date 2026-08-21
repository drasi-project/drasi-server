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

import {
  DrasiSSEClient,
  DrasiSSEClientOptions,
  EventSourceFactory,
} from './DrasiSSEClient';
import {
  ConnectionStatus,
  QueryDefinition,
  QueryResult,
  ReactionDefinition,
  RouteUnidentified,
} from '../types';

/** Configuration for {@link DrasiClient}. */
export interface DrasiClientOptions {
  /** Base URL of the Drasi Server REST API. Defaults to `http://localhost:8280`. */
  serverUrl?: string;
  /** Continuous queries to ensure exist and stream over the shared connection. */
  queries: QueryDefinition[];
  /** The SSE reaction that multiplexes the queries. */
  reaction: ReactionDefinition;
  /** Routes content for change payloads that arrive without a query id. */
  routeUnidentified?: RouteUnidentified;
  /** Fetch implementation, primarily for authenticated clients and tests. */
  fetch?: typeof globalThis.fetch;
  /** EventSource factory, primarily for polyfills and tests. */
  eventSourceFactory?: EventSourceFactory;
  /** Overrides for reconnect behavior. */
  reconnect?: Pick<
    DrasiSSEClientOptions,
    | 'maxReconnectAttempts'
    | 'initialReconnectDelayMs'
    | 'maxReconnectDelayMs'
  >;
}

const DEFAULT_REACTION_ID = 'sse-stream';
const SNAPSHOT_RETRY_INITIAL_DELAY_MS = 1000;
const SNAPSHOT_RETRY_MAX_DELAY_MS = 30000;

function isAbortError(error: unknown): boolean {
  return error instanceof DOMException && error.name === 'AbortError';
}

function toError(error: unknown): Error {
  return error instanceof Error ? error : new Error(String(error));
}

function componentIsActive(payload: any, config: any): boolean {
  const status = payload?.status ?? config?.status;
  if (typeof status !== 'string') return false;
  return ['running', 'starting', 'reconfiguring'].includes(
    status.toLowerCase(),
  );
}

/**
 * Orchestrates query/reaction lifecycle and one shared SSE connection. Each
 * query subscription starts listening before fetching its REST snapshot, then
 * replays buffered deltas, closing the snapshot-to-stream race.
 */
export class DrasiClient {
  private readonly baseUrl: string;
  private readonly sseClient: DrasiSSEClient;
  private readonly queries = new Map<string, QueryDefinition>();
  private readonly reaction: ReactionDefinition;
  private readonly reactionId: string;
  private readonly fetcher: typeof globalThis.fetch;
  private initialized = false;
  private initPromise: Promise<void> | null = null;
  private initController: AbortController | null = null;
  private instanceId: string | null = null;

  constructor(options: DrasiClientOptions) {
    this.baseUrl = (options.serverUrl || 'http://localhost:8280').replace(
      /\/$/,
      '',
    );
    this.reaction = options.reaction;
    this.reactionId = options.reaction.id || DEFAULT_REACTION_ID;
    this.fetcher = options.fetch ?? globalThis.fetch.bind(globalThis);
    this.sseClient = new DrasiSSEClient({
      routeUnidentified: options.routeUnidentified,
      eventSourceFactory: options.eventSourceFactory,
      ...options.reconnect,
    });
    for (const query of options.queries) {
      this.queries.set(query.id, query);
    }
  }

  isInitialized(): boolean {
    return this.initialized;
  }

  async initialize(): Promise<void> {
    if (this.initialized) return;
    if (this.initPromise) return this.initPromise;

    const controller = new AbortController();
    this.initController = controller;
    const promise = this.doInitialize(controller.signal);
    this.initPromise = promise;

    try {
      await promise;
      if (this.initPromise === promise && !controller.signal.aborted) {
        this.initialized = true;
      }
    } finally {
      if (this.initPromise === promise) {
        this.initPromise = null;
        this.initController = null;
      }
    }
  }

  private async doInitialize(signal: AbortSignal): Promise<void> {
    const healthResponse = await this.fetcher(`${this.baseUrl}/health`, {
      signal,
    });
    if (!healthResponse.ok) {
      throw new Error(
        `Drasi Server health check failed (${healthResponse.status})`,
      );
    }

    try {
      const instancesResponse = await this.fetcher(
        `${this.baseUrl}/api/v1/instances`,
        { signal },
      );
      if (instancesResponse.ok) {
        const instancesJson = await instancesResponse.json();
        const instances = instancesJson.data ?? instancesJson;
        if (Array.isArray(instances) && instances.length > 0) {
          this.instanceId = instances[0].id ?? instances[0];
        }
      }
    } catch (error) {
      if (isAbortError(error)) throw error;
      console.warn('Could not discover Drasi instance id:', error);
    }

    for (const queryDef of this.queries.values()) {
      await this.ensureQuery(queryDef, signal);
    }

    const sseEndpoint = await this.ensureReaction(signal);
    await this.sseClient.connect(
      Array.from(this.queries.keys()),
      sseEndpoint,
      signal,
    );
  }

  private async ensureReaction(signal: AbortSignal): Promise<string> {
    let checkResponse = await this.fetcher(
      `${this.baseUrl}/api/v1/reactions/${encodeURIComponent(this.reactionId)}?view=full`,
      { signal },
    );

    if (checkResponse.status === 404) {
      const reactionConfig: Record<string, any> = {
        kind: this.reaction.kind || 'sse',
        id: this.reactionId,
        queries: Array.from(this.queries.keys()),
        autoStart: true,
        host: this.reaction.host || '0.0.0.0',
        port: this.reaction.port,
        ssePath: this.reaction.ssePath || '/events',
      };
      if (this.reaction.heartbeatIntervalMs !== undefined) {
        reactionConfig.heartbeatIntervalMs =
          this.reaction.heartbeatIntervalMs;
      }

      const createResponse = await this.fetcher(
        `${this.baseUrl}/api/v1/reactions`,
        {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(reactionConfig),
          signal,
        },
      );

      if (createResponse.ok) {
        return this.reactionEndpoint();
      }
      if (createResponse.status !== 409) {
        throw new Error(
          `Failed to create reaction ${this.reactionId} (${createResponse.status}): ${await createResponse.text()}`,
        );
      }

      checkResponse = await this.fetcher(
        `${this.baseUrl}/api/v1/reactions/${encodeURIComponent(this.reactionId)}?view=full`,
        { signal },
      );
    }

    if (!checkResponse.ok) {
      throw new Error(
        `Failed to read reaction ${this.reactionId} (${checkResponse.status})`,
      );
    }

    const reaction = await checkResponse.json();
    let payload = reaction.data ?? reaction;
    let config = payload?.config ?? payload;
    if (!componentIsActive(payload, config)) {
      const startResponse = await this.fetcher(
        `${this.baseUrl}/api/v1/reactions/${encodeURIComponent(this.reactionId)}/start`,
        { method: 'POST', signal },
      );
      if (!startResponse.ok) {
        const startError = await startResponse.text();
        const refreshResponse = await this.fetcher(
          `${this.baseUrl}/api/v1/reactions/${encodeURIComponent(this.reactionId)}?view=full`,
          { signal },
        );
        if (refreshResponse.ok) {
          const refreshedReaction = await refreshResponse.json();
          payload = refreshedReaction.data ?? refreshedReaction;
          config = payload?.config ?? payload;
        }
        if (!refreshResponse.ok || !componentIsActive(payload, config)) {
          throw new Error(
            `Failed to start reaction ${this.reactionId} (${startResponse.status}): ${startError}`,
          );
        }
      }
    }

    const props = config?.properties || config || {};
    return this.reactionEndpoint(
      props.host,
      props.port,
      props.ssePath,
    );
  }

  private reactionEndpoint(
    host?: string,
    port?: number,
    path?: string,
  ): string {
    if (this.reaction.endpoint) return this.reaction.endpoint;

    const base = new URL(this.baseUrl);
    const configuredHost = host || this.reaction.host;
    if (
      configuredHost &&
      configuredHost !== '0.0.0.0' &&
      configuredHost !== '::'
    ) {
      base.hostname = configuredHost;
    }
    base.port = String(port || this.reaction.port);
    base.pathname = path || this.reaction.ssePath || '/events';
    base.search = '';
    base.hash = '';
    return base.toString();
  }

  private async ensureQuery(
    queryDef: QueryDefinition,
    signal: AbortSignal,
  ): Promise<void> {
    const queryId = encodeURIComponent(queryDef.id);
    const checkResponse = await this.fetcher(
      `${this.baseUrl}/api/v1/queries/${queryId}?view=full`,
      { signal },
    );

    if (checkResponse.status === 404) {
      const queryConfig = {
        id: queryDef.id,
        query: queryDef.query,
        queryLanguage: queryDef.queryLanguage || 'Cypher',
        sources: queryDef.sources,
        joins: queryDef.joins ?? [],
        autoStart: true,
      };
      const createResponse = await this.fetcher(
        `${this.baseUrl}/api/v1/queries`,
        {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(queryConfig),
          signal,
        },
      );
      if (!createResponse.ok && createResponse.status !== 409) {
        throw new Error(
          `Failed to create query ${queryDef.id} (${createResponse.status}): ${await createResponse.text()}`,
        );
      }
      return;
    }

    if (!checkResponse.ok) {
      throw new Error(
        `Failed to read query ${queryDef.id} (${checkResponse.status})`,
      );
    }

    const query = await checkResponse.json();
    const payload = query.data ?? query;
    const config = payload?.config ?? payload;
    if (!componentIsActive(payload, config)) {
      const startResponse = await this.fetcher(
        `${this.baseUrl}/api/v1/queries/${queryId}/start`,
        { method: 'POST', signal },
      );
      if (!startResponse.ok) {
        const startError = await startResponse.text();
        const refreshResponse = await this.fetcher(
          `${this.baseUrl}/api/v1/queries/${queryId}?view=full`,
          { signal },
        );
        if (refreshResponse.ok) {
          const refreshedQuery = await refreshResponse.json();
          const refreshedPayload = refreshedQuery.data ?? refreshedQuery;
          const refreshedConfig = refreshedPayload?.config ?? refreshedPayload;
          if (componentIsActive(refreshedPayload, refreshedConfig)) return;
        }
        throw new Error(
          `Failed to start query ${queryDef.id} (${startResponse.status}): ${startError}`,
        );
      }
    }
  }

  async getQueryConfig(
    queryId: string,
    signal?: AbortSignal,
  ): Promise<Record<string, any> | null> {
    const response = await this.fetcher(
      `${this.baseUrl}/api/v1/queries/${encodeURIComponent(queryId)}?view=full`,
      { signal },
    );
    if (response.status === 404) return null;
    if (!response.ok) {
      throw new Error(
        `Failed to get query ${queryId} (${response.status}): ${await response.text()}`,
      );
    }
    const json = await response.json();
    const payload = json.data ?? json;
    return payload?.config ?? payload ?? null;
  }

  async getQueryResults(
    queryId: string,
    signal?: AbortSignal,
  ): Promise<any[]> {
    const response = await this.fetcher(
      `${this.baseUrl}/api/v1/queries/${encodeURIComponent(queryId)}/results`,
      { signal },
    );
    if (!response.ok) {
      throw new Error(
        `Failed to get results for query ${queryId} (${response.status}): ${await response.text()}`,
      );
    }
    const json = await response.json();
    const data = json.data ?? json;
    if (!Array.isArray(data)) {
      throw new Error(`Query ${queryId} returned a non-array result`);
    }
    return data;
  }

  /**
   * Subscribe before fetching the current snapshot. Deltas received while a
   * snapshot request is in flight are buffered and replayed after the snapshot.
   * A fresh snapshot is fetched after each SSE reconnection so changes emitted
   * while the stream was unavailable cannot leave the accumulated result stale.
   */
  subscribe(
    queryId: string,
    callback: (result: QueryResult) => void,
    onError?: (error: Error) => void,
  ): () => void {
    const queuedResults: QueryResult[] = [];
    let active = true;
    let snapshotReady = false;
    let snapshotGeneration = 0;
    let snapshotController: AbortController | null = null;
    let snapshotRetryAttempts = 0;
    let snapshotRetryTimer: ReturnType<typeof setTimeout> | null = null;
    let connectionWasInterrupted = !this.sseClient.isConnected();

    const deliverLiveResult = (result: QueryResult) => {
      if (!active) return;
      if (!snapshotReady) {
        queuedResults.push(result);
        return;
      }
      callback(result);
    };
    const unsubscribe = this.sseClient.subscribe(
      queryId,
      deliverLiveResult,
    );

    const clearSnapshotRetry = () => {
      if (snapshotRetryTimer !== null) {
        clearTimeout(snapshotRetryTimer);
        snapshotRetryTimer = null;
      }
    };

    const suspendSnapshot = () => {
      snapshotReady = false;
      queuedResults.length = 0;
      clearSnapshotRetry();
      snapshotController?.abort();
      snapshotController = null;
      snapshotGeneration += 1;
    };

    const fetchSnapshot = () => {
      if (!active || !this.sseClient.isConnected()) return;

      clearSnapshotRetry();
      snapshotController?.abort();
      snapshotReady = false;
      queuedResults.length = 0;

      const controller = new AbortController();
      snapshotController = controller;
      const generation = ++snapshotGeneration;

      void this.getQueryResults(queryId, controller.signal)
        .then((rows) => {
          if (!active || generation !== snapshotGeneration) return;
          snapshotController = null;
          snapshotRetryAttempts = 0;
          callback({
            queryId,
            data: rows,
            timestamp: Date.now(),
            snapshot: true,
          });
          snapshotReady = true;
          queuedResults.splice(0).forEach(callback);
        })
        .catch((error) => {
          if (
            !active ||
            generation !== snapshotGeneration ||
            isAbortError(error)
          ) {
            return;
          }

          snapshotController = null;
          snapshotReady = false;
          onError?.(toError(error));

          const delay = Math.min(
            SNAPSHOT_RETRY_INITIAL_DELAY_MS *
              Math.pow(2, snapshotRetryAttempts),
            SNAPSHOT_RETRY_MAX_DELAY_MS,
          );
          snapshotRetryAttempts = Math.min(snapshotRetryAttempts + 1, 5);
          snapshotRetryTimer = setTimeout(fetchSnapshot, delay);
        });
    };

    const unsubscribeStatus = this.sseClient.onConnectionStatusChange(
      (status) => {
        if (!active) return;
        if (!status.connected) {
          connectionWasInterrupted = true;
          suspendSnapshot();
          return;
        }
        if (connectionWasInterrupted) {
          connectionWasInterrupted = false;
          snapshotRetryAttempts = 0;
          fetchSnapshot();
        }
      },
    );

    if (this.sseClient.isConnected() && snapshotGeneration === 0) {
      fetchSnapshot();
    }

    return () => {
      active = false;
      clearSnapshotRetry();
      snapshotController?.abort();
      snapshotController = null;
      queuedResults.length = 0;
      unsubscribeStatus();
      unsubscribe();
    };
  }

  getConnectionStatus(): ConnectionStatus {
    return this.sseClient.getConnectionStatus();
  }

  onConnectionStatusChange(
    callback: (status: ConnectionStatus) => void,
  ): () => void {
    return this.sseClient.onConnectionStatusChange(callback);
  }

  getServerUiUrl(): string | null {
    if (!this.instanceId) return null;
    return `${this.baseUrl}/ui?instance=${encodeURIComponent(this.instanceId)}`;
  }

  async disconnect(): Promise<void> {
    this.initialized = false;
    this.initController?.abort();
    this.initController = null;
    this.initPromise = null;
    await this.sseClient.disconnect();
  }
}
