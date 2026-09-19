// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import type { DrasiError } from './errors';

/** JSON values in opaque, plugin-owned read-only configuration. */
export type JsonValue = null | boolean | number | string
  | readonly JsonValue[] | { readonly [key: string]: JsonValue };

/** Validated object row. Applications must narrow/transform its unknown fields. */
export type ResultRow = Record<string, unknown>;

/** Existing snapshot/delta boundary; not an ordered or atomic server cursor. */
export interface QueryResult<T = ResultRow> {
  queryId: string;
  /** Legacy deletions carry `_deleted: true`; identity/adapters are a separate contract. */
  data: T[];
  /** Epoch milliseconds. This is not a server ordering guarantee. */
  timestamp: number;
  snapshot?: boolean;
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
