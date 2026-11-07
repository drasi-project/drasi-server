import axios, { AxiosInstance, AxiosResponse } from 'axios';
import { ApiResponse, ComponentEvent, ComponentIdentity, ComponentListItem, CreateInstanceRequest, InstanceListItem, LogMessage } from './models/common';
import { randomUUID } from 'node:crypto';
import type { ConnectionRegistry } from './sdk/config';

interface ManagedConfiguration {
  revision: number;
  persistent: boolean;
  desired: {
    topology: { components: { descriptor: { id: string } }[] };
    ordinary?: { id: string }[];
  };
}

export class DrasiClient {
  private registry: Pick<ConnectionRegistry, 'getCurrentConnection'>;
  private readonly timeout = 10000;
  private _apiVersion: string = 'v1';

  constructor(registry: Pick<ConnectionRegistry, 'getCurrentConnection'>) {
    this.registry = registry;
  }

  get apiVersion(): string {
    return this._apiVersion;
  }

  set apiVersion(version: string) {
    this._apiVersion = version;
  }

  private get apiBase(): string {
    return `/api/${this._apiVersion}`;
  }

  private get baseUrl(): string {
    return this.registry.getCurrentConnection().url;
  }

  private async get<T>(path: string): Promise<AxiosResponse<T>> {
    return axios.get<T>(`${this.baseUrl}${path}`, {
      validateStatus: () => true,
      timeout: this.timeout,
    });
  }

  private async post<T>(path: string, data?: any): Promise<AxiosResponse<T>> {
    return axios.post<T>(`${this.baseUrl}${path}`, data, {
      validateStatus: () => true,
      timeout: this.timeout,
    });
  }

  private async put<T>(path: string, data?: any): Promise<AxiosResponse<T>> {
    return axios.put<T>(`${this.baseUrl}${path}`, data, {
      validateStatus: () => true,
      timeout: this.timeout,
    });
  }

  private async delete<T>(path: string): Promise<AxiosResponse<T>> {
    return axios.delete<T>(`${this.baseUrl}${path}`, {
      validateStatus: () => true,
      timeout: this.timeout,
    });
  }

  async listInstances(): Promise<InstanceListItem[]> {
    const res = await this.get<ApiResponse<InstanceListItem[]>>(`${this.apiBase}/instances`);
    if (!res.data?.success) {
      throw new Error(res.data?.message ?? res.data?.error ?? res.statusText);
    }
    return res.data.data ?? [];
  }

  async createInstance(request: CreateInstanceRequest): Promise<void> {
    const res = await this.post<ApiResponse<any>>(`${this.apiBase}/instances`, request);
    if (!res.data?.success) {
      throw new Error(res.data?.message ?? res.data?.error ?? res.statusText);
    }
  }

  async checkHealth(): Promise<boolean> {
    return DrasiClient.checkHealthForUrl(this.baseUrl);
  }

  static async checkHealthForUrl(baseUrl: string): Promise<boolean> {
    try {
      const res = await axios.get(`${baseUrl}/health`, {
        validateStatus: () => true,
        timeout: 3000,
      });
      return res.status === 200;
    } catch {
      return false;
    }
  }

  async getCurrentInstanceId(): Promise<string> {
    const connection = this.registry.getCurrentConnection();
    if (connection.instanceId) {
      return connection.instanceId;
    }
    throw new Error('No instance selected. Select or create an instance before operating on components.');
  }

  async listSources(): Promise<ComponentListItem[]> {
    const instanceId = await this.getCurrentInstanceId();
    const res = await this.get<ApiResponse<ComponentListItem[]>>(`${this.apiBase}/instances/${instanceId}/sources`);
    if (!res.data?.success) {
      throw new Error(res.data?.message ?? res.data?.error ?? res.statusText);
    }
    return res.data.data ?? [];
  }

  async listQueries(): Promise<ComponentListItem[]> {
    const instanceId = await this.getCurrentInstanceId();
    const res = await this.get<ApiResponse<ComponentListItem[]>>(`${this.apiBase}/instances/${instanceId}/queries`);
    if (!res.data?.success) {
      throw new Error(res.data?.message ?? res.data?.error ?? res.statusText);
    }
    return res.data.data ?? [];
  }

  async listReactions(): Promise<ComponentListItem[]> {
    const instanceId = await this.getCurrentInstanceId();
    const res = await this.get<ApiResponse<ComponentListItem[]>>(`${this.apiBase}/instances/${instanceId}/reactions`);
    if (!res.data?.success) {
      throw new Error(res.data?.message ?? res.data?.error ?? res.statusText);
    }
    return res.data.data ?? [];
  }

  async deleteSource(id: string) {
    return this.deleteComponent('sources', id);
  }

  async deleteQuery(id: string) {
    return this.deleteComponent('queries', id);
  }

  async deleteReaction(id: string) {
    return this.deleteComponent('reactions', id);
  }

  async startSource(id: string) {
    const instanceId = await this.getCurrentInstanceId();
    const res = await this.post<ApiResponse<any>>(`${this.apiBase}/instances/${instanceId}/sources/${id}/start`);
    if (!res.data?.success) {
      throw new Error(res.data?.message ?? res.data?.error ?? res.statusText);
    }
  }

  async stopSource(id: string) {
    const instanceId = await this.getCurrentInstanceId();
    const res = await this.post<ApiResponse<any>>(`${this.apiBase}/instances/${instanceId}/sources/${id}/stop`);
    if (!res.data?.success) {
      throw new Error(res.data?.message ?? res.data?.error ?? res.statusText);
    }
  }

  async startQuery(id: string) {
    const instanceId = await this.getCurrentInstanceId();
    const res = await this.post<ApiResponse<any>>(`${this.apiBase}/instances/${instanceId}/queries/${id}/start`);
    if (!res.data?.success) {
      throw new Error(res.data?.message ?? res.data?.error ?? res.statusText);
    }
  }

  async stopQuery(id: string) {
    const instanceId = await this.getCurrentInstanceId();
    const res = await this.post<ApiResponse<any>>(`${this.apiBase}/instances/${instanceId}/queries/${id}/stop`);
    if (!res.data?.success) {
      throw new Error(res.data?.message ?? res.data?.error ?? res.statusText);
    }
  }

  async startReaction(id: string) {
    const instanceId = await this.getCurrentInstanceId();
    const res = await this.post<ApiResponse<any>>(`${this.apiBase}/instances/${instanceId}/reactions/${id}/start`);
    if (!res.data?.success) {
      throw new Error(res.data?.message ?? res.data?.error ?? res.statusText);
    }
  }

  async stopReaction(id: string) {
    const instanceId = await this.getCurrentInstanceId();
    const res = await this.post<ApiResponse<any>>(`${this.apiBase}/instances/${instanceId}/reactions/${id}/stop`);
    if (!res.data?.success) {
      throw new Error(res.data?.message ?? res.data?.error ?? res.statusText);
    }
  }

  async getQueryResults(id: string): Promise<any[]> {
    const instanceId = await this.getCurrentInstanceId();
    const res = await this.get<ApiResponse<any[]>>(`${this.apiBase}/instances/${instanceId}/queries/${id}/results`);
    if (!res.data?.success) {
      throw new Error(res.data?.message ?? res.data?.error ?? res.statusText);
    }
    return res.data.data ?? [];
  }

  async getSourceEvents(id: string, limit = 100): Promise<ComponentEvent[]> {
    const instanceId = await this.getCurrentInstanceId();
    const res = await this.get<ApiResponse<ComponentEvent[]>>(
      `${this.apiBase}/instances/${instanceId}/sources/${id}/events?limit=${limit}`
    );
    if (!res.data?.success) {
      throw new Error(res.data?.message ?? res.data?.error ?? res.statusText);
    }
    return res.data.data ?? [];
  }

  async getQueryEvents(id: string, limit = 100): Promise<ComponentEvent[]> {
    const instanceId = await this.getCurrentInstanceId();
    const res = await this.get<ApiResponse<ComponentEvent[]>>(
      `${this.apiBase}/instances/${instanceId}/queries/${id}/events?limit=${limit}`
    );
    if (!res.data?.success) {
      throw new Error(res.data?.message ?? res.data?.error ?? res.statusText);
    }
    return res.data.data ?? [];
  }

  async getReactionEvents(id: string, limit = 100): Promise<ComponentEvent[]> {
    const instanceId = await this.getCurrentInstanceId();
    const res = await this.get<ApiResponse<ComponentEvent[]>>(
      `${this.apiBase}/instances/${instanceId}/reactions/${id}/events?limit=${limit}`
    );
    if (!res.data?.success) {
      throw new Error(res.data?.message ?? res.data?.error ?? res.statusText);
    }
    return res.data.data ?? [];
  }

  async getSourceLogs(id: string, limit = 100): Promise<LogMessage[]> {
    const instanceId = await this.getCurrentInstanceId();
    const res = await this.get<ApiResponse<LogMessage[]>>(
      `${this.apiBase}/instances/${instanceId}/sources/${id}/logs?limit=${limit}`
    );
    if (!res.data?.success) {
      throw new Error(res.data?.message ?? res.data?.error ?? res.statusText);
    }
    return res.data.data ?? [];
  }

  async getQueryLogs(id: string, limit = 100): Promise<LogMessage[]> {
    const instanceId = await this.getCurrentInstanceId();
    const res = await this.get<ApiResponse<LogMessage[]>>(
      `${this.apiBase}/instances/${instanceId}/queries/${id}/logs?limit=${limit}`
    );
    if (!res.data?.success) {
      throw new Error(res.data?.message ?? res.data?.error ?? res.statusText);
    }
    return res.data.data ?? [];
  }

  async getReactionLogs(id: string, limit = 100): Promise<LogMessage[]> {
    const instanceId = await this.getCurrentInstanceId();
    const res = await this.get<ApiResponse<LogMessage[]>>(
      `${this.apiBase}/instances/${instanceId}/reactions/${id}/logs?limit=${limit}`
    );
    if (!res.data?.success) {
      throw new Error(res.data?.message ?? res.data?.error ?? res.statusText);
    }
    return res.data.data ?? [];
  }

  getSourceEventsStreamUrl(id: string): string {
    const instanceId = this.registry.getCurrentConnection().instanceId;
    if (!instanceId) {
      throw new Error('No instance selected for event stream');
    }
    return `${this.baseUrl}${this.apiBase}/instances/${instanceId}/sources/${id}/events/stream`;
  }

  getQueryEventsStreamUrl(id: string): string {
    const instanceId = this.registry.getCurrentConnection().instanceId;
    if (!instanceId) {
      throw new Error('No instance selected for event stream');
    }
    return `${this.baseUrl}${this.apiBase}/instances/${instanceId}/queries/${id}/events/stream`;
  }

  getReactionEventsStreamUrl(id: string): string {
    const instanceId = this.registry.getCurrentConnection().instanceId;
    if (!instanceId) {
      throw new Error('No instance selected for event stream');
    }
    return `${this.baseUrl}${this.apiBase}/instances/${instanceId}/reactions/${id}/events/stream`;
  }

  getSourceLogsStreamUrl(id: string): string {
    const instanceId = this.registry.getCurrentConnection().instanceId;
    if (!instanceId) {
      throw new Error('No instance selected for log stream');
    }
    return `${this.baseUrl}${this.apiBase}/instances/${instanceId}/sources/${id}/logs/stream`;
  }

  getQueryLogsStreamUrl(id: string): string {
    const instanceId = this.registry.getCurrentConnection().instanceId;
    if (!instanceId) {
      throw new Error('No instance selected for log stream');
    }
    return `${this.baseUrl}${this.apiBase}/instances/${instanceId}/queries/${id}/logs/stream`;
  }

  getReactionLogsStreamUrl(id: string): string {
    const instanceId = this.registry.getCurrentConnection().instanceId;
    if (!instanceId) {
      throw new Error('No instance selected for log stream');
    }
    return `${this.baseUrl}${this.apiBase}/instances/${instanceId}/reactions/${id}/logs/stream`;
  }

  getQueryAttachUrl(id: string): string {
    const instanceId = this.registry.getCurrentConnection().instanceId;
    if (!instanceId) {
      throw new Error('No instance selected for query attach');
    }
    return `${this.baseUrl}${this.apiBase}/instances/${encodeURIComponent(instanceId)}/queries/${encodeURIComponent(id)}/attach`;
  }

  async getSourceConfig(id: string): Promise<Record<string, unknown>> {
    return this.getComponentConfig('sources', id);
  }

  async getQueryConfig(id: string): Promise<Record<string, unknown>> {
    return this.getComponentConfig('queries', id);
  }

  async getReactionConfig(id: string): Promise<Record<string, unknown>> {
    return this.getComponentConfig('reactions', id);
  }

  private async getComponentConfig(kind: string, id: string): Promise<Record<string, unknown>> {
    const res = await this.instanceClient().get<ApiResponse<ComponentListItem>>(
      `/${kind}/${encodeURIComponent(id)}?view=full`
    );
    if (!res.data?.success || !res.data.data) {
      throw new Error(res.data?.message ?? res.data?.error ?? res.statusText);
    }
    const component = res.data.data;
    if (!component.config) throw new Error(`Configuration is not available for component ${id}.`);
    return { ...component.config, id: component.id, name: component.name };
  }

  async applySource(resource: Record<string, unknown>) {
    const body = normalizeResource(resource, { dropKind: false });
    return this.applyComponent('sources', body);
  }

  async applyQuery(resource: Record<string, unknown>) {
    return this.applyComponent('queries', normalizeResource(resource, { dropKind: true }));
  }

  async applyReaction(resource: Record<string, unknown>) {
    return this.applyComponent('reactions', normalizeResource(resource, { dropKind: false }));
  }

  async lookupComponent(name: string): Promise<ComponentIdentity | undefined> {
    return this.findComponent(this.instanceClient(), name);
  }

  private instanceClient(): AxiosInstance {
    const { url, instanceId } = this.registry.getCurrentConnection();
    if (!instanceId) throw new Error('No instance selected. Select or create an instance before operating on components.');
    return axios.create({
      baseURL: `${url}${this.apiBase}/instances/${encodeURIComponent(instanceId)}`,
      validateStatus: () => true,
      timeout: this.timeout,
    });
  }

  private async findComponent(client: AxiosInstance, name: string): Promise<ComponentIdentity | undefined> {
    const res = await client.get<ApiResponse<ComponentIdentity>>(
      `/components/by-name/${encodeURIComponent(name)}`
    );
    if (res.status === 404 && res.data?.code === 'COMPUTATION_NOT_FOUND') {
      return undefined;
    }
    if (!res.data?.success || !res.data.data) {
      throw new Error(res.data?.message ?? res.data?.error ?? res.statusText);
    }
    return res.data.data;
  }

  async renameComponent(id: string, name: string): Promise<void> {
    const client = this.instanceClient();
    const request: { name: string; expectedRevision?: number; requestId?: string } = { name };
    const current = await this.managedConfiguration(client);
    if (current && (current.desired.topology.components.some(component => component.descriptor.id === id)
      || current.desired.ordinary?.some(component => component.id === id))) {
      request.expectedRevision = current.revision;
      request.requestId = randomUUID();
    }
    const res = await client.post<ApiResponse<ComponentIdentity>>(
      `/components/${encodeURIComponent(id)}/rename`, request
    );
    if (!res.data?.success) {
      throw new Error(res.data?.message ?? res.data?.error ?? res.statusText);
    }
  }

  private async managedConfiguration(client: AxiosInstance): Promise<ManagedConfiguration | undefined> {
    const res = await client.get<ApiResponse<ManagedConfiguration>>('/computation/desired');
    if (res.data?.code === 'MANAGED_CONFIGURATION_REQUIRED') return undefined;
    if (!res.data?.success || !res.data.data) {
      throw new Error(res.data?.message ?? res.data?.error ?? res.statusText);
    }
    if (typeof res.data.data.persistent !== 'boolean') throw new Error('Invalid configuration ownership response');
    return res.data.data;
  }

  private async deleteComponent(kind: string, id: string): Promise<void> {
    const client = this.instanceClient();
    const current = await this.managedConfiguration(client);
    const managed = current?.desired.ordinary?.some(component => component.id === id);
    const res = await client.delete<ApiResponse<unknown>>(
      managed ? `/computation/ordinary-definitions/${encodeURIComponent(id)}` : `/${kind}/${encodeURIComponent(id)}`,
      managed && current ? { data: { expectedRevision: current.revision, requestId: randomUUID() } } : undefined,
    );
    if (!res.data?.success) throw new Error(res.data?.message ?? res.data?.error ?? res.statusText);
  }

  private async applyComponent(
    kind: 'sources' | 'queries' | 'reactions',
    body: Record<string, unknown>
  ): Promise<ComponentIdentity> {
    if (typeof body.name !== 'string' || !body.name) {
      throw new Error('A component name is required; id is an optional UUID.');
    }
    const client = this.instanceClient();
    const base = `/${kind}`;
    const name = body.name;
    const requestedId = body.id;
    if (requestedId !== undefined && (typeof requestedId !== 'string'
      || !/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(requestedId))) {
      throw new Error('Component id must be a UUID. Use name for the editable component name.');
    }
    let id: string | undefined;
    if (requestedId === undefined) {
      id = (await this.findComponent(client, name))?.id;
    } else if (typeof requestedId === 'string') {
      id = requestedId.toLowerCase();
      body = { ...body, id };
      const existing = await client.get<ApiResponse<ComponentListItem>>(`${base}/${encodeURIComponent(id)}`);
      if (!existing.data?.success) {
        const notFoundCode = { sources: 'SOURCE_NOT_FOUND', queries: 'QUERY_NOT_FOUND', reactions: 'REACTION_NOT_FOUND' }[kind];
        if (existing.status !== 404 || existing.data?.code !== notFoundCode) {
          throw new Error(existing.data?.message ?? existing.data?.error ?? existing.statusText);
        }
        id = undefined;
      }
    }
    const current = await this.managedConfiguration(client);
    const type = { sources: 'source', queries: 'query', reactions: 'reaction' }[kind];
    if (typeof id === 'string') {
      const managed = current?.desired.ordinary?.some(component => component.id === id);
      const definition = { ...body, id };
      const res = await client.put<ApiResponse<unknown>>(
        managed ? `/computation/ordinary-definitions/${encodeURIComponent(id)}` : `${base}/${encodeURIComponent(id)}`,
        managed && current ? {
          expectedRevision: current.revision, requestId: randomUUID(), component: { type, definition },
        } : definition,
      );
      if (!res.data?.success) throw new Error(res.data?.message ?? res.data?.error ?? res.statusText);
      return { id, name };
    }
    if (current?.persistent) {
      const res = await client.post<ApiResponse<{ components: ComponentIdentity[] }>>('/computation/ordinary-definitions', {
        expectedRevision: current.revision, requestId: randomUUID(), component: { type, definition: body },
      });
      if (!res.data?.success || res.data.data?.components?.length !== 1
        || res.data.data.components[0].name !== name) {
        throw new Error(res.data?.message ?? res.data?.error ?? 'Configuration did not confirm the requested component assignment');
      }
      return res.data.data.components[0];
    }
    const res = await client.post<ApiResponse<ComponentIdentity>>(base, body);
    if (!res.data?.success || !res.data.data) {
      throw new Error(res.data?.message ?? res.data?.error ?? res.statusText);
    }
    return res.data.data;
  }
}

function normalizeResource(resource: Record<string, unknown>, options: { dropKind: boolean }) {
  const sanitized: Record<string, unknown> = { ...resource };
  if (resource.kind === 'Source' || resource.kind === 'Query' || resource.kind === 'Reaction') {
    if (resource.spec && typeof resource.spec === 'object' && !Array.isArray(resource.spec)) {
      const spec = resource.spec as Record<string, unknown>;
      if (spec.id !== undefined && resource.id !== undefined && spec.id !== resource.id) {
        throw new Error('Resource and spec UUIDs disagree.');
      }
      if (spec.name !== undefined && resource.name !== undefined && spec.name !== resource.name) {
        throw new Error('Resource and spec names disagree.');
      }
      sanitized.id = spec.id ?? sanitized.id;
      sanitized.name = spec.name ?? sanitized.name;
      if (!options.dropKind && spec.kind) {
        sanitized.kind = spec.kind;
      }
      Object.assign(sanitized, spec);
      delete sanitized.spec;
    } else {
      throw new Error('A resource wrapper requires an object spec.');
    }
  }

  delete sanitized.apiVersion;
  if (options.dropKind) {
    delete sanitized.kind;
  }
  return sanitized;
}
