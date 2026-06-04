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

import type React from 'react';

/**
 * A batch of results for a single continuous query, delivered over the shared
 * SSE connection.
 */
export interface QueryResult<T = any> {
  /** The id of the query these results belong to. */
  queryId: string;
  /** The result rows. Deleted rows are flagged with `_deleted: true`. */
  data: T[];
  /** Epoch milliseconds of when the result was produced/received. */
  timestamp: number;
}

/** Live status of the shared connection to the Drasi Server. */
export interface ConnectionStatus {
  connected: boolean;
  reconnecting?: boolean;
  error?: string;
  lastConnected?: Date;
}

/** A source subscription for a continuous query. */
export interface QuerySource {
  sourceId: string;
  pipeline?: string[];
  nodes?: string[];
  relations?: string[];
}

/** A single key in a synthetic join. */
export interface QueryJoinKey {
  label: string;
  property: string;
}

/** A synthetic join between elements from (possibly different) sources. */
export interface QueryJoin {
  id: string;
  keys: QueryJoinKey[];
}

/**
 * A continuous query definition. This is the minimal shape the library needs
 * in order to create/start the query on the Drasi Server. Extra fields are
 * passed through untouched.
 */
export interface QueryDefinition {
  id: string;
  query: string;
  sources: QuerySource[];
  joins?: QueryJoin[];
  /** Query language. Defaults to `Cypher` when omitted. */
  queryLanguage?: string;
  [key: string]: any;
}

/**
 * Configuration for the SSE reaction that the library ensures exists on the
 * Drasi Server. A single reaction multiplexes every query in the connection.
 */
export interface ReactionDefinition {
  /** Reaction id. Defaults to `sse-stream`. */
  id?: string;
  /** Reaction kind. Defaults to `sse`. */
  kind?: string;
  /** Host the reaction binds to on the server. Defaults to `0.0.0.0`. */
  host?: string;
  /** Port the reaction's SSE endpoint listens on. */
  port: number;
  /** Path of the SSE endpoint. Defaults to `/events`. */
  ssePath?: string;
  /** Heartbeat interval in milliseconds. */
  heartbeatIntervalMs?: number;
  /**
   * Public URL of the SSE endpoint the browser should connect to. When omitted
   * it is derived from `host`/`port`/`ssePath` (mapping `0.0.0.0` to
   * `localhost`).
   */
  endpoint?: string;
}

/**
 * Callback used to route results that arrive without an explicit query id
 * (for example aggregation change events). Implementations should inspect the
 * row content and call `deliver(queryId, rows)` for each query the rows belong
 * to. This keeps all application-specific routing out of the reusable library.
 */
export type RouteUnidentified = (
  rows: any[],
  deliver: (queryId: string, rows: any[]) => void,
) => void;

/** Options controlling how {@link QueryResult} batches are folded into state. */
export interface UseDrasiQueryOptions<T = any> {
  /**
   * Extract a stable unique key for a row so that adds/updates/deletes can be
   * accumulated across batches. Returning `null` skips the row.
   * Defaults to `row.id ?? row.symbol ?? JSON.stringify(row)`.
   */
  getKey?: (row: any) => string | null;
  /** Transform/normalize a raw row before it is stored. */
  transform?: (row: any) => T;
  /** Post-process the accumulated rows (sort/filter/slice) before render. */
  postProcess?: (rows: T[]) => T[];
}

/** Column definition for {@link QueryTable}. */
export interface ColumnDef<T> {
  /** Property key on the data object, or a custom string for computed columns. */
  key: keyof T | string;
  /** Column header label. */
  label: string;
  /** Custom formatter/renderer for cell content. */
  format?: (value: any, row: T) => React.ReactNode;
  /** Whether this column is sortable (default: true). */
  sortable?: boolean;
  /** Text alignment (default: 'left'). */
  align?: 'left' | 'center' | 'right';
  /** Additional CSS classes for cells. */
  className?: string | ((value: any, row: T) => string);
  /** Additional CSS classes for the header cell. */
  headerClassName?: string;
  /** Width hint (e.g., 'w-20', 'w-32'). */
  width?: string;
}

/** Row action definition (edit, delete, etc.) for {@link QueryTable}. */
export interface RowAction<T> {
  /** Icon element to display. */
  icon: React.ReactNode;
  /** Accessibility label. */
  label: string;
  /** Click handler. */
  onClick: (row: T) => void;
  /** Additional CSS classes. */
  className?: string;
  /** Hover CSS classes. */
  hoverClassName?: string;
  /** Whether the action is disabled for this row. */
  disabled?: (row: T) => boolean;
  /** Whether the action is loading for this row. */
  loading?: (row: T) => boolean;
}

/** Sort configuration for {@link QueryTable}. */
export interface SortConfig {
  column: string;
  direction: 'asc' | 'desc';
}
