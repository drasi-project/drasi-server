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

export type ComponentLogType =
  | ComponentType
  | "BootstrapProvider"
  | "IdentityProvider"
  | "Transformer"
  | "Sink"
  | "Service";

export type SourceKind = "mock" | "http" | "grpc" | "postgres" | "platform";

export interface ComputationPort {
  id: string;
  direction: "Input" | "Output";
  schema: { id: string; version: number; encoding: string; definition: number[] };
  requirements: { required: string[] };
}

export interface ComputationComponent {
  id: string;
  name: string;
  role: string;
  implementation?: {
    name: string;
    version: string;
    plugin: { id: string; version: string } | null;
  } | null;
  autoStart: boolean;
  ports: ComputationPort[];
  realization: string | null;
  lifecycle: string | null;
  health: string | null;
  failurePhase: string | null;
}

export interface ComputationEndpoint {
  component: string;
  port: string;
}

export type ComputationRelationship =
  | {
      representation: "NativeProvider";
      from: ComputationEndpoint;
      to: ComputationEndpoint;
      binding: string | null;
      availability: string | null;
    }
  | {
      representation: "HostSubscription";
      from: string;
      to: string;
      producerStarted: boolean | null;
      consumerStarted: boolean | null;
    }
  | { representation: "ControlConnection"; from: string; to: string };

export interface ComputationInspection {
  graph: { id: string; state: string; revision: number; driverFailed: boolean };
  components: ComputationComponent[];
  relationships: ComputationRelationship[];
  resources: { id: string; role: string; ownership: string; realization: string | null }[];
}

// Raw API response shapes (what the server actually returns)
interface ComponentLinks {
  self: string;
  full: string;
}

interface RawComponentSummary {
  id: string;
  name: string;
  status: ComponentStatus;
  links: ComponentLinks;
  errorMessage?: string;
}

interface RawSourceFull extends RawComponentSummary {
  config: {
    kind: SourceKind;
    id: string;
    name: string;
    autoStart?: boolean;
    [key: string]: unknown;
  };
}

interface RawQueryFull extends RawComponentSummary {
  config: {
    id: string;
    name: string;
    query: string;
    queryLanguage?: string;
    inputs: QueryInput[];
    result?: QueryResultOptions;
    autoStart?: boolean;
    enableBootstrap?: boolean;
    [key: string]: unknown;
  };
}

interface RawReactionFull extends RawComponentSummary {
  config: {
    kind: ReactionKind;
    id: string;
    name: string;
    queries: ComponentReference[];
    autoStart?: boolean;
    [key: string]: unknown;
  };
}

// Normalized types used by UI components
export interface SourceStatusResponse {
  id: string;
  name: string;
  kind: SourceKind;
  status: ComponentStatus;
  autoStart: boolean;
  properties?: Record<string, unknown>;
  error?: string;
}

export interface CreateSourceRequest {
  kind: SourceKind;
  name: string;
  id?: string;
  autoStart?: boolean;
  [key: string]: unknown;
}

export interface QueryConfigResponse {
  id: string;
  name: string;
  query: string;
  queryLanguage?: string;
  inputs: QueryInput[];
  result?: QueryResultOptions;
  autoStart?: boolean;
  enableBootstrap?: boolean;
  status?: ComponentStatus;
  error?: string;
}

/** Expanded query input; the server omits unset and empty optional fields. */
export interface QueryInputObject {
  component: ComponentReference;
  port?: string;
  nodes?: string[];
  relations?: string[];
  pipeline?: string[];
}

/** A bare upstream component name (its primary output) or an expanded input object. */
export type ComponentReference = string | { id: string };
export type QueryInput = ComponentReference | QueryInputObject;

export interface ComponentIdentity {
  id: string;
  name: string;
}

export interface ComponentCreatedResponse extends ComponentIdentity {
  message: string;
}

export interface QueryResultOptions {
  label?: string;
}

export function referenceComponent(
  reference: ComponentReference,
  components: readonly ComponentIdentity[],
): string | undefined {
  return typeof reference === "string"
    ? components.find((component) => component.name === reference)?.id
    : reference.id;
}

export function inputComponent(
  input: QueryInput,
  components: readonly ComponentIdentity[],
): string | undefined {
  const reference = typeof input === "object" && "component" in input
    ? input.component : input;
  return referenceComponent(reference, components);
}

export interface CreateQueryRequest {
  name: string;
  id?: string;
  query: string;
  queryLanguage?: string;
  inputs: QueryInput[];
  result?: QueryResultOptions;
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
  name: string;
  kind: ReactionKind;
  status: ComponentStatus;
  queries: ComponentReference[];
  autoStart: boolean;
  properties?: Record<string, unknown>;
  error?: string;
}

export interface CreateReactionRequest {
  kind: ReactionKind;
  name: string;
  id?: string;
  queries: ComponentReference[];
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
  componentType?: ComponentLogType;
}

export interface InstanceInfo {
  id: string;
  source_count: number;
  query_count: number;
  reaction_count: number;
}

export interface CreateInstanceRequest {
  id: string;
  allowExternalComponentIds?: boolean;
  persistIndex?: boolean;
  memoryBudgetMiB?: number;
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
  const { kind: _kind, id: _id, name: _name, autoStart: _auto, ...rest } = raw.config ?? {} as Record<string, unknown>;
  return {
    id: raw.id,
    name: raw.name,
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
    name: raw.name,
    kind: "mock", // Unknown from summary, will be refined on detail fetch
    status: raw.status,
    autoStart: false,
    error: raw.errorMessage,
  };
}

export function normalizeQuery(raw: RawQueryFull): QueryConfigResponse {
  return {
    id: raw.id,
    name: raw.name,
    query: raw.config?.query ?? "",
    queryLanguage: raw.config?.queryLanguage,
    inputs: raw.config?.inputs ?? [],
    result: raw.config?.result,
    autoStart: raw.config?.autoStart,
    enableBootstrap: raw.config?.enableBootstrap,
    status: raw.status,
    error: raw.errorMessage,
  };
}

export function normalizeQuerySummary(raw: RawComponentSummary): QueryConfigResponse {
  return {
    id: raw.id,
    name: raw.name,
    query: "",
    inputs: [],
    status: raw.status,
    error: raw.errorMessage,
  };
}

export function normalizeReaction(raw: RawReactionFull): ReactionStatusResponse {
  const { kind: _kind, id: _id, name: _name, queries: _q, autoStart: _auto, ...rest } = raw.config ?? {} as Record<string, unknown>;
  return {
    id: raw.id,
    name: raw.name,
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
    name: raw.name,
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
  sourceNames: string[];
  queryNames: string[];
  reactionNames: string[];
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
  componentName?: string;
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

// Plugin registry types

export interface RegistryPlugin {
  reference: string;
  fullReference: string;
  version: string;
  filename: string;
  source: "local" | "oci";
}

export interface InstallPluginRequest {
  ref: string;
  registry?: string;
}
