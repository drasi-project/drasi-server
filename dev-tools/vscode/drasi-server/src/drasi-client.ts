import axios, { AxiosResponse } from 'axios';
import { ApiResponse, ComponentEvent, ComponentListItem, CreateInstanceRequest, InstanceListItem, LogMessage } from './models/common';
import { ConnectionRegistry } from './sdk/config';

export class DrasiClient {
  private registry: ConnectionRegistry;
  private readonly timeout = 10000;

  constructor(registry: ConnectionRegistry) {
    this.registry = registry;
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
    const res = await this.get<ApiResponse<InstanceListItem[]>>('/api/v1/instances');
    if (!res.data?.success) {
      throw new Error(res.data?.error ?? res.statusText);
    }
    return res.data.data ?? [];
  }

  async createInstance(request: CreateInstanceRequest): Promise<void> {
    const res = await this.post<ApiResponse<any>>('/api/v1/instances', request);
    if (!res.data?.success) {
      throw new Error(res.data?.error ?? res.statusText);
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
    const instances = await this.listInstances();
    if (instances.length === 0) {
      throw new Error('No instances available');
    }
    return instances[0].id;
  }

  async listSources(): Promise<ComponentListItem[]> {
    const instanceId = await this.getCurrentInstanceId();
    const res = await this.get<ApiResponse<ComponentListItem[]>>(`/api/v1/instances/${instanceId}/sources`);
    if (!res.data?.success) {
      throw new Error(res.data?.error ?? res.statusText);
    }
    return res.data.data ?? [];
  }

  async listQueries(): Promise<ComponentListItem[]> {
    const instanceId = await this.getCurrentInstanceId();
    const res = await this.get<ApiResponse<ComponentListItem[]>>(`/api/v1/instances/${instanceId}/queries`);
    if (!res.data?.success) {
      throw new Error(res.data?.error ?? res.statusText);
    }
    return res.data.data ?? [];
  }

  async listReactions(): Promise<ComponentListItem[]> {
    const instanceId = await this.getCurrentInstanceId();
    const res = await this.get<ApiResponse<ComponentListItem[]>>(`/api/v1/instances/${instanceId}/reactions`);
    if (!res.data?.success) {
      throw new Error(res.data?.error ?? res.statusText);
    }
    return res.data.data ?? [];
  }

  async deleteSource(id: string) {
    const instanceId = await this.getCurrentInstanceId();
    const res = await this.delete<ApiResponse<any>>(`/api/v1/instances/${instanceId}/sources/${id}`);
    if (!res.data?.success) {
      throw new Error(res.data?.error ?? res.statusText);
    }
  }

  async deleteQuery(id: string) {
    const instanceId = await this.getCurrentInstanceId();
    const res = await this.delete<ApiResponse<any>>(`/api/v1/instances/${instanceId}/queries/${id}`);
    if (!res.data?.success) {
      throw new Error(res.data?.error ?? res.statusText);
    }
  }

  async deleteReaction(id: string) {
    const instanceId = await this.getCurrentInstanceId();
    const res = await this.delete<ApiResponse<any>>(`/api/v1/instances/${instanceId}/reactions/${id}`);
    if (!res.data?.success) {
      throw new Error(res.data?.error ?? res.statusText);
    }
  }

  async startSource(id: string) {
    const instanceId = await this.getCurrentInstanceId();
    const res = await this.post<ApiResponse<any>>(`/api/v1/instances/${instanceId}/sources/${id}/start`);
    if (!res.data?.success) {
      throw new Error(res.data?.error ?? res.statusText);
    }
  }

  async stopSource(id: string) {
    const instanceId = await this.getCurrentInstanceId();
    const res = await this.post<ApiResponse<any>>(`/api/v1/instances/${instanceId}/sources/${id}/stop`);
    if (!res.data?.success) {
      throw new Error(res.data?.error ?? res.statusText);
    }
  }

  async startQuery(id: string) {
    const instanceId = await this.getCurrentInstanceId();
    const res = await this.post<ApiResponse<any>>(`/api/v1/instances/${instanceId}/queries/${id}/start`);
    if (!res.data?.success) {
      throw new Error(res.data?.error ?? res.statusText);
    }
  }

  async stopQuery(id: string) {
    const instanceId = await this.getCurrentInstanceId();
    const res = await this.post<ApiResponse<any>>(`/api/v1/instances/${instanceId}/queries/${id}/stop`);
    if (!res.data?.success) {
      throw new Error(res.data?.error ?? res.statusText);
    }
  }

  async startReaction(id: string) {
    const instanceId = await this.getCurrentInstanceId();
    const res = await this.post<ApiResponse<any>>(`/api/v1/instances/${instanceId}/reactions/${id}/start`);
    if (!res.data?.success) {
      throw new Error(res.data?.error ?? res.statusText);
    }
  }

  async stopReaction(id: string) {
    const instanceId = await this.getCurrentInstanceId();
    const res = await this.post<ApiResponse<any>>(`/api/v1/instances/${instanceId}/reactions/${id}/stop`);
    if (!res.data?.success) {
      throw new Error(res.data?.error ?? res.statusText);
    }
  }

  async getQueryResults(id: string): Promise<any[]> {
    const instanceId = await this.getCurrentInstanceId();
    const res = await this.get<ApiResponse<any[]>>(`/api/v1/instances/${instanceId}/queries/${id}/results`);
    if (!res.data?.success) {
      throw new Error(res.data?.error ?? res.statusText);
    }
    return res.data.data ?? [];
  }

  async getSourceEvents(id: string, limit = 100): Promise<ComponentEvent[]> {
    const instanceId = await this.getCurrentInstanceId();
    const res = await this.get<ApiResponse<ComponentEvent[]>>(
      `/api/v1/instances/${instanceId}/sources/${id}/events?limit=${limit}`
    );
    if (!res.data?.success) {
      throw new Error(res.data?.error ?? res.statusText);
    }
    return res.data.data ?? [];
  }

  async getQueryEvents(id: string, limit = 100): Promise<ComponentEvent[]> {
    const instanceId = await this.getCurrentInstanceId();
    const res = await this.get<ApiResponse<ComponentEvent[]>>(
      `/api/v1/instances/${instanceId}/queries/${id}/events?limit=${limit}`
    );
    if (!res.data?.success) {
      throw new Error(res.data?.error ?? res.statusText);
    }
    return res.data.data ?? [];
  }

  async getReactionEvents(id: string, limit = 100): Promise<ComponentEvent[]> {
    const instanceId = await this.getCurrentInstanceId();
    const res = await this.get<ApiResponse<ComponentEvent[]>>(
      `/api/v1/instances/${instanceId}/reactions/${id}/events?limit=${limit}`
    );
    if (!res.data?.success) {
      throw new Error(res.data?.error ?? res.statusText);
    }
    return res.data.data ?? [];
  }

  async getSourceLogs(id: string, limit = 100): Promise<LogMessage[]> {
    const instanceId = await this.getCurrentInstanceId();
    const res = await this.get<ApiResponse<LogMessage[]>>(
      `/api/v1/instances/${instanceId}/sources/${id}/logs?limit=${limit}`
    );
    if (!res.data?.success) {
      throw new Error(res.data?.error ?? res.statusText);
    }
    return res.data.data ?? [];
  }

  async getQueryLogs(id: string, limit = 100): Promise<LogMessage[]> {
    const instanceId = await this.getCurrentInstanceId();
    const res = await this.get<ApiResponse<LogMessage[]>>(
      `/api/v1/instances/${instanceId}/queries/${id}/logs?limit=${limit}`
    );
    if (!res.data?.success) {
      throw new Error(res.data?.error ?? res.statusText);
    }
    return res.data.data ?? [];
  }

  async getReactionLogs(id: string, limit = 100): Promise<LogMessage[]> {
    const instanceId = await this.getCurrentInstanceId();
    const res = await this.get<ApiResponse<LogMessage[]>>(
      `/api/v1/instances/${instanceId}/reactions/${id}/logs?limit=${limit}`
    );
    if (!res.data?.success) {
      throw new Error(res.data?.error ?? res.statusText);
    }
    return res.data.data ?? [];
  }

  getSourceEventsStreamUrl(id: string): string {
    const instanceId = this.registry.getCurrentConnection().instanceId;
    if (!instanceId) {
      throw new Error('No instance selected for event stream');
    }
    return `${this.baseUrl}/api/v1/instances/${instanceId}/sources/${id}/events/stream`;
  }

  getQueryEventsStreamUrl(id: string): string {
    const instanceId = this.registry.getCurrentConnection().instanceId;
    if (!instanceId) {
      throw new Error('No instance selected for event stream');
    }
    return `${this.baseUrl}/api/v1/instances/${instanceId}/queries/${id}/events/stream`;
  }

  getReactionEventsStreamUrl(id: string): string {
    const instanceId = this.registry.getCurrentConnection().instanceId;
    if (!instanceId) {
      throw new Error('No instance selected for event stream');
    }
    return `${this.baseUrl}/api/v1/instances/${instanceId}/reactions/${id}/events/stream`;
  }

  getSourceLogsStreamUrl(id: string): string {
    const instanceId = this.registry.getCurrentConnection().instanceId;
    if (!instanceId) {
      throw new Error('No instance selected for log stream');
    }
    return `${this.baseUrl}/api/v1/instances/${instanceId}/sources/${id}/logs/stream`;
  }

  getQueryLogsStreamUrl(id: string): string {
    const instanceId = this.registry.getCurrentConnection().instanceId;
    if (!instanceId) {
      throw new Error('No instance selected for log stream');
    }
    return `${this.baseUrl}/api/v1/instances/${instanceId}/queries/${id}/logs/stream`;
  }

  getReactionLogsStreamUrl(id: string): string {
    const instanceId = this.registry.getCurrentConnection().instanceId;
    if (!instanceId) {
      throw new Error('No instance selected for log stream');
    }
    return `${this.baseUrl}/api/v1/instances/${instanceId}/reactions/${id}/logs/stream`;
  }

  getQueryAttachUrl(id: string): string {
    const instanceId = this.registry.getCurrentConnection().instanceId;
    if (!instanceId) {
      throw new Error('No instance selected for query attach');
    }
    return `${this.baseUrl}/api/v1/instances/${instanceId}/queries/${id}/attach`;
  }

  async getSourceConfig(id: string): Promise<Record<string, unknown>> {
    const instanceId = await this.getCurrentInstanceId();
    const res = await this.get<ApiResponse<ComponentListItem>>(
      `/api/v1/instances/${instanceId}/sources/${id}?view=full`
    );
    if (!res.data?.success) {
      throw new Error(res.data?.error ?? res.statusText);
    }
    return res.data.data?.config ?? {};
  }

  async getQueryConfig(id: string): Promise<Record<string, unknown>> {
    const instanceId = await this.getCurrentInstanceId();
    const res = await this.get<ApiResponse<ComponentListItem>>(
      `/api/v1/instances/${instanceId}/queries/${id}?view=full`
    );
    if (!res.data?.success) {
      throw new Error(res.data?.error ?? res.statusText);
    }
    return res.data.data?.config ?? {};
  }

  async getReactionConfig(id: string): Promise<Record<string, unknown>> {
    const instanceId = await this.getCurrentInstanceId();
    const res = await this.get<ApiResponse<ComponentListItem>>(
      `/api/v1/instances/${instanceId}/reactions/${id}?view=full`
    );
    if (!res.data?.success) {
      throw new Error(res.data?.error ?? res.statusText);
    }
    return res.data.data?.config ?? {};
  }

  async applySource(resource: Record<string, unknown>) {
    const instanceId = await this.getCurrentInstanceId();
    const res = await this.put<ApiResponse<any>>(
      `/api/v1/instances/${instanceId}/sources`,
      normalizeResource(resource, { dropKind: false })
    );
    if (!res.data?.success) {
      throw new Error(res.data?.error ?? res.statusText);
    }
  }

  async applyQuery(resource: Record<string, unknown>) {
    const instanceId = await this.getCurrentInstanceId();
    const res = await this.post<ApiResponse<any>>(
      `/api/v1/instances/${instanceId}/queries`,
      normalizeResource(resource, { dropKind: true })
    );
    if (!res.data?.success) {
      throw new Error(res.data?.error ?? res.statusText);
    }
  }

  async applyReaction(resource: Record<string, unknown>) {
    const instanceId = await this.getCurrentInstanceId();
    const res = await this.put<ApiResponse<any>>(
      `/api/v1/instances/${instanceId}/reactions`,
      normalizeResource(resource, { dropKind: false })
    );
    if (!res.data?.success) {
      throw new Error(res.data?.error ?? res.statusText);
    }
  }
}

function normalizeResource(resource: Record<string, unknown>, options: { dropKind: boolean }) {
  const sanitized: Record<string, unknown> = { ...resource };
  if (resource.kind === 'Source' || resource.kind === 'Query' || resource.kind === 'Reaction') {
    if (resource.spec && typeof resource.spec === 'object') {
      const spec = resource.spec as Record<string, unknown>;
      sanitized.id = spec.id ?? sanitized.id;
      if (!options.dropKind && spec.kind) {
        sanitized.kind = spec.kind;
      }
      Object.assign(sanitized, spec);
      delete sanitized.spec;
    }
  }

  delete sanitized.apiVersion;
  if (options.dropKind) {
    delete sanitized.kind;
  }
  return sanitized;
}
