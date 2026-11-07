import axios from "axios";
import { instancePath } from "./paths";
import type {
  SourceStatusResponse,
  CreateSourceRequest,
  QueryConfigResponse,
  CreateQueryRequest,
  QueryResultRow,
  ReactionStatusResponse,
  CreateReactionRequest,
  InstanceInfo,
  CreateInstanceRequest,
  CloneInstanceResult,
  ComputationInspection,
  ComponentIdentity,
  ComponentCreatedResponse,
} from "./types";
import {
  normalizeSource,
  normalizeSourceSummary,
  normalizeQuery,
  normalizeQuerySummary,
  normalizeReaction,
  normalizeReactionSummary,
} from "./types";

const api = axios.create({
  baseURL: "/api/v1",
  headers: { "Content-Type": "application/json" },
});

// Reject API responses where the server returns HTTP 200 but success=false.
// Many Drasi Server handlers return { success: false, error: "..." } with status 200,
// which axios considers a successful response. This interceptor turns them into
// thrown errors so callers' catch blocks work as expected.
api.interceptors.response.use((response) => {
  const body = response.data;
  if (
    body &&
    typeof body === "object" &&
    "success" in body &&
    body.success === false
  ) {
    const message = body.error || "Operation failed";
    return Promise.reject(new Error(message));
  }
  return response;
});

// Server wraps responses in { success, data, error } envelope
// eslint-disable-next-line @typescript-eslint/no-explicit-any
function unwrap<T>(response: { data: any }): T {
  const body = response.data;
  if (body && typeof body === "object" && "data" in body) {
    return body.data as T;
  }
  return body as T;
}

// Instances
export async function listInstances(): Promise<InstanceInfo[]> {
  return unwrap(await api.get("/instances"));
}

export async function inspectComputation(
  instanceId: string,
  signal?: AbortSignal,
): Promise<ComputationInspection> {
  return unwrap(await api.get(instancePath(instanceId, "computation"), { signal }));
}

export function apiErrorMessage(error: unknown): string {
  if (axios.isAxiosError<{ message?: unknown }>(error)
    && typeof error.response?.data?.message === "string") {
    return error.response.data.message;
  }
  return error instanceof Error ? error.message : String(error);
}

export async function componentByName(
  name: string,
  instanceId: string,
): Promise<ComponentIdentity> {
  return unwrap(await api.get(instancePath(instanceId, "components", "by-name", name)));
}

interface ManagedConfiguration {
  revision: number;
  persistent: boolean;
  desired: {
    topology: { components: { descriptor: { id: string } }[] };
    ordinary?: { id: string }[];
  };
}

async function managedConfiguration(instanceId: string): Promise<ManagedConfiguration | undefined> {
  try {
    const current = unwrap<ManagedConfiguration>(
      await api.get(instancePath(instanceId, "computation", "desired")));
    if (typeof current.persistent !== "boolean") throw new Error("Invalid configuration ownership response");
    return current;
  } catch (error) {
    if (axios.isAxiosError<{ code?: string }>(error)
      && error.response?.data?.code === "MANAGED_CONFIGURATION_REQUIRED") return undefined;
    throw error;
  }
}

type OrdinaryKind = "source" | "query" | "reaction";
const ordinaryPaths = { source: "sources", query: "queries", reaction: "reactions" };
type OrdinaryRequest = CreateSourceRequest | CreateQueryRequest | CreateReactionRequest;

async function createOrdinary(kind: OrdinaryKind, definition: OrdinaryRequest, instanceId: string): Promise<ComponentCreatedResponse> {
  const current = await managedConfiguration(instanceId);
  if (!current?.persistent) {
    return unwrap(await api.post(instancePath(instanceId, ordinaryPaths[kind]), definition));
  }
  const receipt = unwrap<{ components: ComponentIdentity[] }>(await api.post(
    instancePath(instanceId, "computation", "ordinary-definitions"), {
      expectedRevision: current.revision, requestId: crypto.randomUUID(),
      component: { type: kind, definition },
    }));
  if (receipt.components?.length !== 1 || receipt.components[0].name !== definition.name) {
    throw new Error("Configuration receipt did not confirm the requested component assignment");
  }
  return { ...receipt.components[0], message: "Component declaration accepted" };
}

async function updateOrdinary(kind: OrdinaryKind, id: string, definition: OrdinaryRequest, instanceId: string): Promise<void> {
  const current = await managedConfiguration(instanceId);
  if (current?.desired.ordinary?.some(component => component.id === id)) {
    await api.put(instancePath(instanceId, "computation", "ordinary-definitions", id), {
      expectedRevision: current.revision, requestId: crypto.randomUUID(),
      component: { type: kind, definition },
    });
  } else {
    await api.put(instancePath(instanceId, ordinaryPaths[kind], id), definition);
  }
}

async function deleteOrdinary(kind: OrdinaryKind, id: string, instanceId: string): Promise<void> {
  const current = await managedConfiguration(instanceId);
  if (current?.desired.ordinary?.some(component => component.id === id)) {
    await api.delete(instancePath(instanceId, "computation", "ordinary-definitions", id), {
      data: { expectedRevision: current.revision, requestId: crypto.randomUUID() },
    });
  } else {
    await api.delete(instancePath(instanceId, ordinaryPaths[kind], id));
  }
}

export async function renameComponent(
  id: string,
  name: string,
  instanceId: string,
): Promise<ComponentIdentity> {
  let request: { name: string; expectedRevision?: number; requestId?: string } = { name };
  const current = await managedConfiguration(instanceId);
  if (current && (current.desired.topology.components.some((component) => component.descriptor.id === id)
    || current.desired.ordinary?.some(component => component.id === id))) {
    request = { name, expectedRevision: current.revision, requestId: crypto.randomUUID() };
  }
  return unwrap(await api.post(instancePath(instanceId, "components", id, "rename"), request));
}

export async function updateSource(id: string, req: CreateSourceRequest, instanceId: string): Promise<void> {
  await updateOrdinary("source", id, req, instanceId);
}

export async function updateQuery(id: string, req: CreateQueryRequest, instanceId: string): Promise<void> {
  await updateOrdinary("query", id, req, instanceId);
}

export async function updateReaction(id: string, req: CreateReactionRequest, instanceId: string): Promise<void> {
  await updateOrdinary("reaction", id, req, instanceId);
}

export async function createInstance(req: CreateInstanceRequest): Promise<void> {
  await api.post("/instances", req);
}

export async function cloneInstance(
  targetInstanceId: string,
  sourceInstanceId: string,
): Promise<CloneInstanceResult> {
  return unwrap(
    await api.post(instancePath(targetInstanceId, "clone"), { sourceInstanceId }),
  );
}

// Sources — fetch full view for each source to get kind/config
export async function listSources(
  instanceId: string,
): Promise<SourceStatusResponse[]> {
  const path = instancePath(instanceId, "sources");
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const summaries: any[] = unwrap(await api.get(path));
  // Fetch full details in parallel for kind/config
  const results = await Promise.all(
    summaries.map(async (s) => {
      try {
        const fullPath = `${instancePath(instanceId, "sources", s.id)}?view=full`;
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        const full: any = unwrap(await api.get(fullPath));
        return normalizeSource(full);
      } catch {
        return normalizeSourceSummary(s);
      }
    }),
  );
  return results;
}

export async function getSource(
  id: string,
  instanceId: string,
): Promise<SourceStatusResponse> {
  const path = `${instancePath(instanceId, "sources", id)}?view=full`;
  return normalizeSource(unwrap(await api.get(path)));
}

export async function createSource(
  req: CreateSourceRequest,
  instanceId: string,
): Promise<ComponentCreatedResponse> {
  return createOrdinary("source", req, instanceId);
}

export async function deleteSource(
  id: string,
  instanceId: string,
): Promise<void> {
  await deleteOrdinary("source", id, instanceId);
}

export async function startSource(
  id: string,
  instanceId: string,
): Promise<void> {
  const path = instancePath(instanceId, "sources", id, "start");
  await api.post(path);
}

export async function stopSource(
  id: string,
  instanceId: string,
): Promise<void> {
  const path = instancePath(instanceId, "sources", id, "stop");
  await api.post(path);
}

// Queries — fetch full view for each query to get config
export async function listQueries(
  instanceId: string,
): Promise<QueryConfigResponse[]> {
  const path = instancePath(instanceId, "queries");
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const summaries: any[] = unwrap(await api.get(path));
  const results = await Promise.all(
    summaries.map(async (q) => {
      try {
        const fullPath = `${instancePath(instanceId, "queries", q.id)}?view=full`;
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        const full: any = unwrap(await api.get(fullPath));
        return normalizeQuery(full);
      } catch {
        return normalizeQuerySummary(q);
      }
    }),
  );
  return results;
}

export async function getQuery(
  id: string,
  instanceId: string,
): Promise<QueryConfigResponse> {
  const path = `${instancePath(instanceId, "queries", id)}?view=full`;
  return normalizeQuery(unwrap(await api.get(path)));
}

export async function createQuery(
  req: CreateQueryRequest,
  instanceId: string,
): Promise<ComponentCreatedResponse> {
  return createOrdinary("query", req, instanceId);
}

export async function deleteQuery(
  id: string,
  instanceId: string,
): Promise<void> {
  await deleteOrdinary("query", id, instanceId);
}

export async function startQuery(
  id: string,
  instanceId: string,
): Promise<void> {
  const path = instancePath(instanceId, "queries", id, "start");
  await api.post(path);
}

export async function stopQuery(
  id: string,
  instanceId: string,
): Promise<void> {
  const path = instancePath(instanceId, "queries", id, "stop");
  await api.post(path);
}

export async function getQueryResults(
  id: string,
  instanceId: string,
): Promise<QueryResultRow[]> {
  const path = instancePath(instanceId, "queries", id, "results");
  return unwrap(await api.get(path));
}

// Reactions — fetch full view for each reaction to get kind/config
export async function listReactions(
  instanceId: string,
): Promise<ReactionStatusResponse[]> {
  const path = instancePath(instanceId, "reactions");
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const summaries: any[] = unwrap(await api.get(path));
  const results = await Promise.all(
    summaries.map(async (r) => {
      try {
        const fullPath = `${instancePath(instanceId, "reactions", r.id)}?view=full`;
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        const full: any = unwrap(await api.get(fullPath));
        return normalizeReaction(full);
      } catch {
        return normalizeReactionSummary(r);
      }
    }),
  );
  return results;
}

export async function getReaction(
  id: string,
  instanceId: string,
): Promise<ReactionStatusResponse> {
  const path = `${instancePath(instanceId, "reactions", id)}?view=full`;
  return normalizeReaction(unwrap(await api.get(path)));
}

export async function createReaction(
  req: CreateReactionRequest,
  instanceId: string,
): Promise<ComponentCreatedResponse> {
  return createOrdinary("reaction", req, instanceId);
}

export async function deleteReaction(
  id: string,
  instanceId: string,
): Promise<void> {
  await deleteOrdinary("reaction", id, instanceId);
}

export async function startReaction(
  id: string,
  instanceId: string,
): Promise<void> {
  const path = instancePath(instanceId, "reactions", id, "start");
  await api.post(path);
}

export async function stopReaction(
  id: string,
  instanceId: string,
): Promise<void> {
  const path = instancePath(instanceId, "reactions", id, "stop");
  await api.post(path);
}

// Solutions (Catalog)
import type {
  SolutionTemplateSummary,
  SolutionTemplateDetail,
  SolutionDeployRequest,
  SolutionDeployResponse,
  CreateSolutionTemplateRequest,
  CreateSolutionTemplateResponse,
} from "./types";

export async function listSolutions(): Promise<SolutionTemplateSummary[]> {
  return unwrap(await api.get("/catalog/solutions"));
}

export async function getSolution(id: string): Promise<SolutionTemplateDetail> {
  return unwrap(await api.get(`/catalog/solutions/${id}`));
}

export async function deploySolution(
  instanceId: string,
  req: SolutionDeployRequest,
): Promise<SolutionDeployResponse> {
  return unwrap(await api.post(instancePath(instanceId, "solutions"), req));
}

export async function deleteSolution(id: string): Promise<void> {
  await api.delete(`/catalog/solutions/${id}`);
}

export async function createSolutionTemplate(
  instanceId: string,
  req: CreateSolutionTemplateRequest,
): Promise<CreateSolutionTemplateResponse> {
  return unwrap(await api.post(instancePath(instanceId, "catalog", "solutions"), req));
}

// Plugins

import type { RegistryPlugin } from "./types";

export async function searchRegistry(
  query = "*",
  registry?: string,
): Promise<RegistryPlugin[]> {
  const params = new URLSearchParams({ q: query });
  if (registry) params.set("registry", registry);
  const resp = await api.get(`/plugins/registry/search?${params}`);
  return resp.data as RegistryPlugin[];
}

export async function installPlugin(
  pluginRef: string,
  registry?: string,
): Promise<void> {
  await api.post("/plugins/install", { ref: pluginRef, registry });
}
