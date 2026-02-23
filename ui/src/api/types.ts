// Component status types matching the server API
export type ComponentStatus =
  | "Starting"
  | "Running"
  | "Stopping"
  | "Stopped"
  | "Error"
  | "Reconfiguring"
  | "Added"
  | "Removed";

export type ComponentType = "Source" | "Query" | "Reaction";

export type SourceKind = "mock" | "http" | "grpc" | "postgres" | "platform";

// Raw API response shapes (what the server actually returns)
interface ComponentLinks {
  self: string;
  full: string;
}

interface RawComponentSummary {
  id: string;
  status: ComponentStatus;
  links: ComponentLinks;
}

interface RawSourceFull extends RawComponentSummary {
  config: {
    kind: SourceKind;
    id: string;
    autoStart?: boolean;
    [key: string]: unknown;
  };
}

interface RawQueryFull extends RawComponentSummary {
  config: {
    id: string;
    query: string;
    queryLanguage?: string;
    sources: QuerySourceSubscription[];
    autoStart?: boolean;
    enableBootstrap?: boolean;
    [key: string]: unknown;
  };
}

interface RawReactionFull extends RawComponentSummary {
  config: {
    kind: ReactionKind;
    id: string;
    queries: string[];
    autoStart?: boolean;
    [key: string]: unknown;
  };
}

// Normalized types used by UI components
export interface SourceStatusResponse {
  id: string;
  kind: SourceKind;
  status: ComponentStatus;
  autoStart: boolean;
  error?: string;
}

export interface CreateSourceRequest {
  kind: SourceKind;
  id: string;
  autoStart?: boolean;
  [key: string]: unknown;
}

export interface QueryConfigResponse {
  id: string;
  query: string;
  queryLanguage?: string;
  sources: QuerySourceSubscription[];
  autoStart?: boolean;
  enableBootstrap?: boolean;
  status?: ComponentStatus;
}

export interface QuerySourceSubscription {
  sourceId: string;
  nodes?: string[];
  relations?: string[];
}

export interface CreateQueryRequest {
  id: string;
  query: string;
  queryLanguage?: string;
  sources: QuerySourceSubscription[];
  autoStart?: boolean;
}

export interface QueryResultRow {
  [key: string]: unknown;
}

export type ReactionKind =
  | "log"
  | "http"
  | "http-adaptive"
  | "grpc"
  | "grpc-adaptive"
  | "sse"
  | "platform"
  | "profiler";

export interface ReactionStatusResponse {
  id: string;
  kind: ReactionKind;
  status: ComponentStatus;
  queries: string[];
  autoStart: boolean;
  error?: string;
}

export interface CreateReactionRequest {
  kind: ReactionKind;
  id: string;
  queries: string[];
  autoStart?: boolean;
  [key: string]: unknown;
}

export interface ComponentEvent {
  componentId: string;
  componentType: ComponentType;
  status: ComponentStatus;
  timestamp: string;
  message?: string;
}

export interface LogMessage {
  timestamp: string;
  level: "Trace" | "Debug" | "Info" | "Warn" | "Error";
  message: string;
  componentId?: string;
  componentType?: ComponentType;
}

export interface InstanceInfo {
  id: string;
  source_count: number;
  query_count: number;
  reaction_count: number;
}

export interface CreateInstanceRequest {
  id: string;
  persistIndex?: boolean;
  defaultPriorityQueueCapacity?: number;
  defaultDispatchBufferCapacity?: number;
}

export interface ChangeEvent {
  type: "added" | "updated" | "deleted";
  data: Record<string, unknown>;
  before?: Record<string, unknown>;
}

// Normalization helpers
export function normalizeSource(raw: RawSourceFull): SourceStatusResponse {
  return {
    id: raw.id,
    kind: raw.config?.kind ?? "mock",
    status: raw.status,
    autoStart: raw.config?.autoStart ?? false,
  };
}

export function normalizeSourceSummary(raw: RawComponentSummary): SourceStatusResponse {
  return {
    id: raw.id,
    kind: "mock", // Unknown from summary, will be refined on detail fetch
    status: raw.status,
    autoStart: false,
  };
}

export function normalizeQuery(raw: RawQueryFull): QueryConfigResponse {
  return {
    id: raw.id,
    query: raw.config?.query ?? "",
    queryLanguage: raw.config?.queryLanguage,
    sources: raw.config?.sources ?? [],
    autoStart: raw.config?.autoStart,
    enableBootstrap: raw.config?.enableBootstrap,
    status: raw.status,
  };
}

export function normalizeQuerySummary(raw: RawComponentSummary): QueryConfigResponse {
  return {
    id: raw.id,
    query: "",
    sources: [],
    status: raw.status,
  };
}

export function normalizeReaction(raw: RawReactionFull): ReactionStatusResponse {
  return {
    id: raw.id,
    kind: raw.config?.kind ?? "log",
    status: raw.status,
    queries: raw.config?.queries ?? [],
    autoStart: raw.config?.autoStart ?? false,
  };
}

export function normalizeReactionSummary(raw: RawComponentSummary): ReactionStatusResponse {
  return {
    id: raw.id,
    kind: "log",
    status: raw.status,
    queries: [],
    autoStart: false,
  };
}
