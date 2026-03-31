import axios from "axios";
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

export async function createInstance(req: CreateInstanceRequest): Promise<void> {
  await api.post("/instances", req);
}

export async function cloneInstance(
  targetInstanceId: string,
  sourceInstanceId: string,
): Promise<CloneInstanceResult> {
  return unwrap(
    await api.post(`/instances/${targetInstanceId}/clone`, { sourceInstanceId }),
  );
}

// Sources — fetch full view for each source to get kind/config
export async function listSources(
  instanceId?: string,
): Promise<SourceStatusResponse[]> {
  const path = instanceId
    ? `/instances/${instanceId}/sources`
    : "/sources";
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const summaries: any[] = unwrap(await api.get(path));
  // Filter out internal components (introspection source, etc.)
  const visible = summaries.filter((s) => !s.id?.startsWith("__"));
  // Fetch full details in parallel for kind/config
  const results = await Promise.all(
    visible.map(async (s) => {
      try {
        const fullPath = instanceId
          ? `/instances/${instanceId}/sources/${s.id}?view=full`
          : `/sources/${s.id}?view=full`;
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
  instanceId?: string,
): Promise<SourceStatusResponse> {
  const path = instanceId
    ? `/instances/${instanceId}/sources/${id}?view=full`
    : `/sources/${id}?view=full`;
  return normalizeSource(unwrap(await api.get(path)));
}

export async function createSource(
  req: CreateSourceRequest,
  instanceId?: string,
): Promise<void> {
  const path = instanceId
    ? `/instances/${instanceId}/sources`
    : "/sources";
  await api.post(path, req);
}

export async function deleteSource(
  id: string,
  instanceId?: string,
): Promise<void> {
  const path = instanceId
    ? `/instances/${instanceId}/sources/${id}`
    : `/sources/${id}`;
  await api.delete(path);
}

export async function startSource(
  id: string,
  instanceId?: string,
): Promise<void> {
  const path = instanceId
    ? `/instances/${instanceId}/sources/${id}/start`
    : `/sources/${id}/start`;
  await api.post(path);
}

export async function stopSource(
  id: string,
  instanceId?: string,
): Promise<void> {
  const path = instanceId
    ? `/instances/${instanceId}/sources/${id}/stop`
    : `/sources/${id}/stop`;
  await api.post(path);
}

// Queries — fetch full view for each query to get config
export async function listQueries(
  instanceId?: string,
): Promise<QueryConfigResponse[]> {
  const path = instanceId
    ? `/instances/${instanceId}/queries`
    : "/queries";
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const summaries: any[] = unwrap(await api.get(path));
  const results = await Promise.all(
    summaries.map(async (q) => {
      try {
        const fullPath = instanceId
          ? `/instances/${instanceId}/queries/${q.id}?view=full`
          : `/queries/${q.id}?view=full`;
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
  instanceId?: string,
): Promise<QueryConfigResponse> {
  const path = instanceId
    ? `/instances/${instanceId}/queries/${id}?view=full`
    : `/queries/${id}?view=full`;
  return normalizeQuery(unwrap(await api.get(path)));
}

export async function createQuery(
  req: CreateQueryRequest,
  instanceId?: string,
): Promise<void> {
  const path = instanceId
    ? `/instances/${instanceId}/queries`
    : "/queries";
  await api.post(path, req);
}

export async function deleteQuery(
  id: string,
  instanceId?: string,
): Promise<void> {
  const path = instanceId
    ? `/instances/${instanceId}/queries/${id}`
    : `/queries/${id}`;
  await api.delete(path);
}

export async function startQuery(
  id: string,
  instanceId?: string,
): Promise<void> {
  const path = instanceId
    ? `/instances/${instanceId}/queries/${id}/start`
    : `/queries/${id}/start`;
  await api.post(path);
}

export async function stopQuery(
  id: string,
  instanceId?: string,
): Promise<void> {
  const path = instanceId
    ? `/instances/${instanceId}/queries/${id}/stop`
    : `/queries/${id}/stop`;
  await api.post(path);
}

export async function getQueryResults(
  id: string,
  instanceId?: string,
): Promise<QueryResultRow[]> {
  const path = instanceId
    ? `/instances/${instanceId}/queries/${id}/results`
    : `/queries/${id}/results`;
  return unwrap(await api.get(path));
}

// Reactions — fetch full view for each reaction to get kind/config
export async function listReactions(
  instanceId?: string,
): Promise<ReactionStatusResponse[]> {
  const path = instanceId
    ? `/instances/${instanceId}/reactions`
    : "/reactions";
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const summaries: any[] = unwrap(await api.get(path));
  // Filter out internal components (attach reactions, etc.)
  const visible = summaries.filter((r) => !r.id?.startsWith("__"));
  const results = await Promise.all(
    visible.map(async (r) => {
      try {
        const fullPath = instanceId
          ? `/instances/${instanceId}/reactions/${r.id}?view=full`
          : `/reactions/${r.id}?view=full`;
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
  instanceId?: string,
): Promise<ReactionStatusResponse> {
  const path = instanceId
    ? `/instances/${instanceId}/reactions/${id}?view=full`
    : `/reactions/${id}?view=full`;
  return normalizeReaction(unwrap(await api.get(path)));
}

export async function createReaction(
  req: CreateReactionRequest,
  instanceId?: string,
): Promise<void> {
  const path = instanceId
    ? `/instances/${instanceId}/reactions`
    : "/reactions";
  await api.post(path, req);
}

export async function deleteReaction(
  id: string,
  instanceId?: string,
): Promise<void> {
  const path = instanceId
    ? `/instances/${instanceId}/reactions/${id}`
    : `/reactions/${id}`;
  await api.delete(path);
}

export async function startReaction(
  id: string,
  instanceId?: string,
): Promise<void> {
  const path = instanceId
    ? `/instances/${instanceId}/reactions/${id}/start`
    : `/reactions/${id}/start`;
  await api.post(path);
}

export async function stopReaction(
  id: string,
  instanceId?: string,
): Promise<void> {
  const path = instanceId
    ? `/instances/${instanceId}/reactions/${id}/stop`
    : `/reactions/${id}/stop`;
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
  return unwrap(await api.post(`/instances/${instanceId}/solutions`, req));
}

export async function deleteSolution(id: string): Promise<void> {
  await api.delete(`/catalog/solutions/${id}`);
}

export async function createSolutionTemplate(
  instanceId: string,
  req: CreateSolutionTemplateRequest,
): Promise<CreateSolutionTemplateResponse> {
  return unwrap(await api.post(`/instances/${instanceId}/catalog/solutions`, req));
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
