// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import type { DrasiError, DrasiErrorDetails } from './errors';

/** JSON values in opaque, plugin-owned read-only configuration. */
export type JsonValue = null | boolean | number | string
  | readonly JsonValue[] | { readonly [key: string]: JsonValue };

/** Validated object row. Applications must narrow/transform its unknown fields. */
export type ResultRow = Record<string, unknown>;

/** Insert/replace by the application's stable key, or an explicit before/after change. */
export type ResultChange<T = ResultRow> =
  | { readonly kind: 'upsert'; readonly after: T }
  | { readonly kind: 'update'; readonly before: T; readonly after: T }
  | { readonly kind: 'delete'; readonly before: T };

export interface QuerySnapshot<T = ResultRow> {
  readonly kind: 'snapshot';
  readonly queryId: string;
  readonly rows: readonly T[];
  /** Local receipt time for display only, never an ordering cursor. */
  readonly receivedAt: number;
}

export interface QueryDelta<T = ResultRow> {
  readonly kind: 'delta';
  readonly queryId: string;
  readonly changes: readonly ResultChange<T>[];
  readonly receivedAt: number;
  /** SSE reaction wall-clock time, when supplied. Not a snapshot cursor. */
  readonly sourceTimestamp?: number;
}

/** The only accumulated-result boundary. Wire alternatives must use a named adapter. */
export type QueryResult<T = ResultRow> = QuerySnapshot<T> | QueryDelta<T>;

export interface ResultAdapterContext extends Readonly<DrasiErrorDetails> {
  readonly receivedAt: number;
}

/** Synchronous wire validation/normalization. Empty output means a validated heartbeat/no-op. */
export type ResultAdapter = (
  payload: unknown,
  context: ResultAdapterContext,
) => readonly QueryDelta[];

export interface LegacyResultAdapterOptions {
  /** Only consulted for payloads without either query ID spelling. Never overrides an envelope ID. */
  routeUnidentified?: RouteUnidentified;
}

/** Raw identity must also work on sparse deletes. No default, transform, or serialization fallback. */
export type RowKey = (row: Readonly<ResultRow>) => string;

export type QueryStatus =
  | 'initial-loading' | 'live' | 'empty' | 'reconnecting' | 'resynchronizing'
  | 'stale-last-good-data' | 'terminal-error';

export type QueryErrorScope = 'query' | 'connection';

/** Subscription work state; hooks additionally distinguish an empty accumulated result set. */
export interface QuerySubscriptionState {
  readonly status: Exclude<QueryStatus, 'empty'>;
  readonly stale: boolean;
  readonly error: DrasiError | null;
  readonly errorScope: QueryErrorScope | null;
}

/** Calling the handle unsubscribes; retry restarts only this subscription's REST reconciliation. */
export interface QuerySubscription {
  (): void;
  retry(): void;
  getState(): QuerySubscriptionState;
}

export interface ResultReconciliationOptions {
  /** Maximum changes observed during a pending snapshot (default 10000), not a result-set limit. */
  maxPendingChanges?: number;
}

/** Live transport status, not proof that each query snapshot is synchronized. */
export interface ConnectionStatus {
  connected: boolean;
  reconnecting?: boolean;
  error?: DrasiError;
  lastConnected?: Date;
}

/** Source subscription as serialized by the v1 full-view query endpoint. */
export interface QuerySource {
  readonly sourceId: string;
  readonly pipeline: readonly string[];
  readonly nodes: readonly string[];
  readonly relations: readonly string[];
}

export interface QueryJoinKey {
  readonly label: string;
  readonly property: string;
}

export interface QueryJoin {
  readonly id: string;
  readonly keys: readonly QueryJoinKey[];
}

/** Middleware configuration is owned by its server plugin, not executed here. */
export interface QueryMiddleware {
  readonly kind: string;
  readonly name: string;
  readonly config: Readonly<Record<string, JsonValue>>;
}

export type QueryLanguage = 'Cypher' | 'GQL';

/** Complete v1 query definition READ, not a query creation request. No defaults are selected. */
export interface QueryConfig {
  readonly id: string;
  readonly autoStart: boolean;
  readonly query: string;
  readonly queryLanguage: QueryLanguage;
  readonly middleware: readonly QueryMiddleware[];
  readonly sources: readonly QuerySource[];
  readonly enableBootstrap: boolean;
  readonly bootstrapBufferSize: number;
  readonly joins?: readonly QueryJoin[];
  readonly priorityQueueCapacity?: number;
  readonly dispatchBufferCapacity?: number;
  readonly dispatchMode?: 'Channel';
  /** Opaque serde JSON configuration; the client does not interpret storage backends. */
  readonly storageBackend?: JsonValue;
  readonly outboxCapacity: number;
  readonly bootstrapTimeoutSecs: number;
}

/** Read-only full-view reaction. Plugin properties are flattened by the server. */
export interface ReactionConfig {
  readonly id: string;
  readonly kind: string;
  readonly queries: readonly string[];
  readonly [property: string]: JsonValue | undefined;
}

/** Observed configuration of the supported SSE plugin; never used to derive the browser URL. */
export interface SseReactionConfig extends ReactionConfig {
  readonly kind: 'sse';
  readonly host: string;
  readonly port: number;
  readonly ssePath: string;
  readonly heartbeatIntervalMs: number;
}

export type ComponentStatus =
  | 'Starting' | 'Running' | 'Stopping' | 'Stopped'
  | 'Error' | 'Reconfiguring' | 'Added' | 'Removed';

export interface ComponentLinks {
  readonly self: string;
  readonly full: string;
}

/** Full-view v1 component DTO. Links are validated against the requested instance/resource. */
export interface Component<T> {
  readonly id: string;
  readonly status: ComponentStatus;
  readonly error_message?: string;
  readonly links: ComponentLinks;
  readonly config: T;
}

/** Existing SSE reaction reference, not a deployment definition. */
export interface ReactionReference {
  readonly id: string;
  /** Absolute browser-reachable HTTP(S) URL, possibly served by a reverse proxy. */
  readonly endpoint: string;
}

/** Explicit legacy adapter for unidentified rows; routing remains application-owned. */
export type RouteUnidentified = (
  rows: ResultRow[],
  deliver: (queryId: string, rows: ResultRow[]) => void,
) => void;

/** Context for fresh per-request authentication, including each stream opening. */
export interface DrasiRequestContext {
  readonly url: string;
  readonly transport: 'rest' | 'sse';
  readonly signal: AbortSignal;
  readonly instanceId?: string;
}

/** Re-evaluated for every REST request and stream attempt. Never logged or cached as a token. */
export type DrasiHeadersProvider = (
  context: DrasiRequestContext,
) => HeadersInit | Promise<HeadersInit>;

export type DrasiHeaders = HeadersInit | DrasiHeadersProvider;

/** Passed to custom stream factories; each attempt receives its own headers and cancellation signal. */
export interface EventSourceOptions {
  readonly headers: Headers;
  readonly credentials: RequestCredentials;
  readonly signal: AbortSignal;
}

/** Defaults: ten retries after the first attempt, 1s exponential backoff capped at 30s, 10s open timeout. */
export interface ReconnectOptions {
  maxReconnectAttempts?: number;
  initialReconnectDelayMs?: number;
  maxReconnectDelayMs?: number;
  connectionTimeoutMs?: number;
}
