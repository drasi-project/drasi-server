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
 * drasi-react — reusable React building blocks for Drasi Server applications.
 *
 * - {@link DrasiProvider}: opens a single shared SSE connection to a Drasi
 *   Server and multiplexes every continuous query over it.
 * - {@link useDrasiQuery} and friends: subscribe to live query results.
 * - {@link QueryTable}: a sortable, animated table bound to a query.
 */

export { DrasiClient } from './DrasiClient';
export type { DrasiClientOptions } from './DrasiClient';
export { DrasiSSEClient } from './DrasiSSEClient';
export {
  DrasiProvider,
  useDrasiClient,
  useDrasiQuery,
  useDrasiConnectionStatus,
  useDrasiServerUiUrl,
  useDrasiQueryDefinition,
} from './context';
export type { DrasiProviderProps } from './context';
export { QueryTable } from './QueryTable';
export type { QueryTableProps } from './QueryTable';
export { CodeViewerDialog } from './CodeViewerDialog';
export type { CodeViewerDialogProps } from './CodeViewerDialog';
export { useRowAnimation } from './useRowAnimation';
export type {
  AnimationDirection,
  UseRowAnimationOptions,
  UseRowAnimationResult,
} from './useRowAnimation';
export { CodeIcon, ExpandIcon, CollapseIcon } from './icons';

export type {
  QueryResult,
  ConnectionStatus,
  QuerySource,
  QueryJoin,
  QueryJoinKey,
  QueryDefinition,
  ReactionDefinition,
  RouteUnidentified,
  UseDrasiQueryOptions,
  ColumnDef,
  RowAction,
  SortConfig,
} from './types';
