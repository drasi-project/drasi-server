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
  errorMessage?: string;
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
  properties?: Record<string, unknown>;
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
  error?: string;
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
  properties?: Record<string, unknown>;
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
  const { kind: _kind, id: _id, autoStart: _auto, ...rest } = raw.config ?? {} as Record<string, unknown>;
  return {
    id: raw.id,
    kind: raw.config?.kind ?? "mock",
    status: raw.status,
    autoStart: raw.config?.autoStart ?? false,
    properties: Object.keys(rest).length > 0 ? rest : undefined,
    error: raw.errorMessage,
  };
}

export function normalizeSourceSummary(raw: RawComponentSummary): SourceStatusResponse {
  return {
    id: raw.id,
    kind: "mock", // Unknown from summary, will be refined on detail fetch
    status: raw.status,
    autoStart: false,
    error: raw.errorMessage,
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
    error: raw.errorMessage,
  };
}

export function normalizeQuerySummary(raw: RawComponentSummary): QueryConfigResponse {
  return {
    id: raw.id,
    query: "",
    sources: [],
    status: raw.status,
    error: raw.errorMessage,
  };
}

export function normalizeReaction(raw: RawReactionFull): ReactionStatusResponse {
  const { kind: _kind, id: _id, queries: _q, autoStart: _auto, ...rest } = raw.config ?? {} as Record<string, unknown>;
  return {
    id: raw.id,
    kind: raw.config?.kind ?? "log",
    status: raw.status,
    queries: raw.config?.queries ?? [],
    autoStart: raw.config?.autoStart ?? false,
    properties: Object.keys(rest).length > 0 ? rest : undefined,
    error: raw.errorMessage,
  };
}

export function normalizeReactionSummary(raw: RawComponentSummary): ReactionStatusResponse {
  return {
    id: raw.id,
    kind: "log",
    status: raw.status,
    queries: [],
    autoStart: false,
    error: raw.errorMessage,
  };
}

// Solution template types
export interface SolutionVariable {
  name: string;
  default?: string;
  required: boolean;
  description?: string;
  usedBy?: string[];
}

export interface SolutionTemplateSummary {
  id: string;
  name: string;
  description?: string;
  version?: string;
  author?: string;
  license?: string;
  defaultInstanceId?: string;
  sourceCount: number;
  queryCount: number;
  reactionCount: number;
}

export interface SolutionTemplateDetail {
  id: string;
  name: string;
  description?: string;
  version?: string;
  author?: string;
  license?: string;
  defaultInstanceId?: string;
  variables: SolutionVariable[];
  sourceIds: string[];
  queryIds: string[];
  reactionIds: string[];
}

export interface SolutionDeployRequest {
  templateId?: string;
  yaml?: string;
  variables: Record<string, string>;
}

export type DeployPhase = "validation" | "creation" | "start";

export interface SolutionDeployError {
  phase: DeployPhase;
  componentType?: string;
  componentId?: string;
  message: string;
}

export interface SolutionDeployResponse {
  success: boolean;
  sourcesCreated: string[];
  queriesCreated: string[];
  reactionsCreated: string[];
  componentsStarted: string[];
  errors: SolutionDeployError[];
}

// Clone instance types
export interface CloneInstanceResult {
  success: boolean;
  sourcesCreated: string[];
  queriesCreated: string[];
  reactionsCreated: string[];
  errors: string[];
}

// Create Solution Template types
export interface CreateSolutionTemplateRequest {
  id: string;
  name: string;
  description?: string;
  version?: string;
  author?: string;
  license?: string;
  sourceIds: string[];
  queryIds: string[];
  reactionIds: string[];
}

export interface CreateSolutionTemplateResponse {
  success: boolean;
  templateId?: string;
  error?: string;
}
