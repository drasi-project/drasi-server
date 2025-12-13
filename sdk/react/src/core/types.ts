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

/**
 * Query result delivered via SSE stream
 */
export interface QueryResult<T = any> {
  queryId: string;
  timestamp: number;
  data: T[];
  error?: string;
}

/**
 * Connection status for SSE stream
 */
export interface ConnectionStatus {
  connected: boolean;
  error?: string;
  reconnecting?: boolean;
  lastConnected?: Date;
}

/**
 * Query subscription handle
 */
export interface QuerySubscription {
  queryId: string;
  callback: (result: QueryResult) => void;
  unsubscribe: () => void;
}

/**
 * Query definition for Drasi Server
 */
export interface QueryDefinition {
  id: string;
  query: string;
  sources: SourceSubscription[];
  joins?: QueryJoin[];
  parameters?: Record<string, any>;
  auto_start?: boolean;
}

/**
 * Source subscription configuration
 */
export interface SourceSubscription {
  source_id: string;
  pipeline?: string[];
}

/**
 * Query join configuration for synthetic relationships
 */
export interface QueryJoin {
  id: string;
  keys: QueryJoinKey[];
}

/**
 * Join key configuration
 */
export interface QueryJoinKey {
  label: string;
  property: string;
}

/**
 * Reaction definition for Drasi Server
 */
export interface ReactionDefinition {
  id: string;
  kind: string;
  queries: string[];
  auto_start?: boolean;
  properties?: Record<string, any>;
}

/**
 * SSE client configuration
 */
export interface SSEClientConfig {
  endpoint: string;
  heartbeat_interval_ms?: number;
}

/**
 * REST API client configuration
 */
export interface RestClientConfig {
  baseUrl: string;
}

/**
 * Drasi provider configuration - combines both clients
 */
export interface DrasiConfig {
  rest: RestClientConfig;
  sse: SSEClientConfig;
  queries?: QueryDefinition[];
}

/**
 * Query options for useQuery hook
 */
export interface QueryOptions<TInput = any, TOutput = any> {
  transform?: (item: TInput) => TOutput;
  accumulationStrategy?: AccumulationStrategy;
  sortBy?: (a: TOutput, b: TOutput) => number;
  filter?: (item: TOutput) => boolean;
  getItemKey?: (item: TOutput) => string;
}

/**
 * Accumulation strategy for query results
 */
export type AccumulationStrategy = 'replace' | 'merge' | 'append';

/**
 * Query state returned by useQuery hook
 */
export interface QueryState<T> {
  data: T[] | null;
  loading: boolean;
  error: string | null;
  lastUpdate: Date | null;
}

/**
 * Source definition from Drasi Server API
 */
export interface SourceInfo {
  id: string;
  kind: string;
  status?: string;
}

/**
 * Query info from Drasi Server API
 */
export interface QueryInfo {
  id: string;
  query: string;
  status?: string;
  sources?: SourceSubscription[];
}

/**
 * Reaction info from Drasi Server API
 */
export interface ReactionInfo {
  id: string;
  kind: string;
  queries: string[];
  status?: string;
  config?: any;
  properties?: Record<string, any>;
}
