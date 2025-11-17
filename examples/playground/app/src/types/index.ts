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

// Drasi Server Resource Types

export type SourceStatus = 'Stopped' | 'Starting' | 'Running' | 'Stopping' | 'Failed';
export type QueryStatus = 'Stopped' | 'Starting' | 'Running' | 'Stopping' | 'Failed';
export type ReactionStatus = 'Stopped' | 'Starting' | 'Running' | 'Stopping' | 'Failed';

export interface Source {
  id: string;
  source_type: 'http' | 'mock' | 'application' | 'postgres' | 'grpc';
  auto_start: boolean;
  status?: SourceStatus;
  [key: string]: any; // Allow additional source-specific properties
}

export interface Join {
  id: string;
  keys: Array<{
    label: string;
    property: string;
  }>;
}

export interface Query {
  id: string;
  query: string;
  sources: string[];
  joins?: Join[];
  auto_start: boolean;
  status?: QueryStatus;
}

export interface Reaction {
  id: string;
  reaction_type: 'sse' | 'http' | 'grpc' | 'log' | 'application';
  queries: string[];
  auto_start: boolean;
  status?: ReactionStatus;
  [key: string]: any; // Allow additional reaction-specific properties
}

export interface QueryResult {
  query_id: string;
  sequence?: number;
  timestamp?: string;
  results: Array<{
    data: Record<string, any>;
  }>;
}

// UI Form Types

export interface CreateSourceForm {
  id: string;
  source_type: 'http' | 'mock' | 'application';
  auto_start: boolean;
  properties?: Record<string, any>;
}

export interface CreateQueryForm {
  id: string;
  query: string;
  sources: string[];
  joins?: Join[];
  auto_start: boolean;
}

export interface EditDataForm {
  [key: string]: any;
}

// SSE Event Types

export interface QueryResultEvent {
  query_id: string;
  sequence: number;
  timestamp: string;
  results: Array<{
    data: Record<string, any>;
  }>;
}

export interface ChangeEvent {
  type: 'insert' | 'update' | 'delete';
  data: Record<string, any>;
}

// Data Injection Types

export interface DataEvent {
  operation: 'insert' | 'update' | 'delete';
  element: {
    type: 'node';
    id: string;
    labels: string[];
    properties: Record<string, any>;
    before?: Record<string, any>;  // For update/delete operations
    after?: Record<string, any>;   // For insert/update operations
  };
  timestamp?: number;
}

// Component Props Types

export interface DataTableProps {
  sourceId: string;
  sourceName?: string;
  client: any; // DrasiClient instance
}

export interface QueryResultsProps {
  queryId: string;
}

// Application State Types

export interface ConnectionStatus {
  connected: boolean;
  error?: string;
  reconnecting?: boolean;
}

export interface DrasiClientState {
  initialized: boolean;
  loading: boolean;
  error?: string;
}
