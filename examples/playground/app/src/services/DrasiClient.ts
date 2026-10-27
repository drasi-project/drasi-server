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

import axios, { AxiosInstance } from 'axios';
import { Source, Query, Reaction, QueryResult, DataEvent, ConnectionStatus } from '@/types';
import { DrasiSSEClient } from './SSEClient';

export class DrasiClient {
  private apiClient: AxiosInstance;
  private sseClient: DrasiSSEClient;
  private initialized = false;
  private readonly instanceApi: string;
  private sseReactionBasePort = 8381;
  private queryReactions: Map<string, string> = new Map(); // queryId -> reactionId

  constructor(baseUrl?: string, instanceId = 'playground-server') {
    if (!instanceId || instanceId === '.' || instanceId === '..') {
      throw new Error('A valid playground instance ID is required.');
    }
    this.instanceApi = `/api/v1/instances/${encodeURIComponent(instanceId)}`;
    const url = baseUrl || 'http://localhost:8380';
    this.apiClient = axios.create({
      baseURL: url,
      headers: { 'Content-Type': 'application/json' },
    });
    this.sseClient = new DrasiSSEClient();
  }

  /**
   * Initialize connection to Drasi Server
   */
  async initialize(): Promise<void> {
    if (this.initialized) {
      return;
    }

    try {
      // Check server health
      await this.healthCheck();

      // Fetch existing resources
      const sources = await this.listSources();
      const queries = await this.listQueries();
      const reactions = await this.listReactions();

      console.log(`Found ${sources?.length || 0} sources, ${queries?.length || 0} queries, ${reactions?.length || 0} reactions`);

      // Create SSE reaction for each query and connect
      for (const query of queries) {
        await this.ensureQuerySSEReaction(query.id);
      }

      // If no queries exist yet, mark as connected (server is healthy)
      if (queries.length === 0) {
        this.sseClient.setConnected();
      }

      this.initialized = true;
      console.log('Drasi Client initialized successfully');
    } catch (error) {
      console.error('Failed to initialize Drasi Client:', error);
      throw error;
    }
  }

  /**
   * Check if Drasi Server is healthy
   */
  async healthCheck(): Promise<void> {
    const response = await this.apiClient.get('/health');
    if (response.status !== 200) {
      throw new Error('Drasi Server is not healthy');
    }
  }

  // ========== Source Management ==========

  /**
   * List all sources
   */
  async listSources(): Promise<Source[]> {
    const response = await this.apiClient.get(`${this.instanceApi}/sources`);
    const data = response.data;
    // Handle both direct array and wrapped {data: [...]} responses
    let sources = [];
    if (Array.isArray(data)) {
      sources = data;
    } else if (data && Array.isArray(data.data)) {
      sources = data.data;
    }
    return sources.map((source: any) => ({
      ...(source.config ?? source),
      status: source.status ?? source.config?.status,
      error_message: source.error_message ?? source.config?.error_message,
    }));
  }

  /**
   * Get a specific source
   */
  async getSource(id: string): Promise<Source> {
    const response = await this.apiClient.get(`${this.instanceApi}/sources/${encodeURIComponent(id)}?view=full`);
    const data = response.data?.data ?? response.data;
    return {
      ...(data?.config ?? data),
      status: data?.status ?? data?.config?.status,
      error_message: data?.error_message ?? data?.config?.error_message,
    };
  }

  /**
   * Create a new source
   */
  async createSource(source: Partial<Source>): Promise<Source> {
    const response = await this.apiClient.post(`${this.instanceApi}/sources`, source);
    return response.data;
  }

  /**
   * Delete a source
   */
  async deleteSource(id: string): Promise<void> {
    await this.apiClient.delete(`${this.instanceApi}/sources/${encodeURIComponent(id)}`);
  }

  /**
   * Start a source
   */
  async startSource(id: string): Promise<void> {
    await this.apiClient.post(`${this.instanceApi}/sources/${encodeURIComponent(id)}/start`);
  }

  /**
   * Stop a source
   */
  async stopSource(id: string): Promise<void> {
    await this.apiClient.post(`${this.instanceApi}/sources/${encodeURIComponent(id)}/stop`);
  }

  // ========== Query Management ==========

  /**
   * List all queries
   */
  async listQueries(): Promise<Query[]> {
    const response = await this.apiClient.get(`${this.instanceApi}/queries`);
    const data = response.data;
    // Handle both direct array and wrapped {data: [...]} responses
    let queries = [];
    if (Array.isArray(data)) {
      queries = data;
    } else if (data && Array.isArray(data.data)) {
      queries = data.data;
    }

    // Map API response to Query interface
    return queries.map((item: any) => {
      const q = item.config ?? item;
      return {
        ...q,
        status: item.status ?? q.status,
        error_message: item.error_message ?? q.error_message,
        inputs: q.inputs?.map((input: any) =>
          typeof input === 'string' ? input : input.component
        ) || []
      };
    });
  }

  /**
   * Get a specific query
   */
  async getQuery(id: string): Promise<Query> {
    const response = await this.apiClient.get(`${this.instanceApi}/queries/${encodeURIComponent(id)}?view=full`);
    const payload = response.data.data || response.data;
    const q = payload?.config ?? payload;
    return {
      ...q,
      status: payload?.status ?? q.status,
      error_message: payload?.error_message ?? q.error_message,
      inputs: q.inputs?.map((input: any) =>
        typeof input === 'string' ? input : input.component
      ) || []
    };
  }

  /**
   * Create a new query
   */
  async createQuery(query: Partial<Query>): Promise<Query> {
    // Convert to API format (camelCase; each source is a bare upstream component name in `inputs`)
    const apiQuery: any = {
      id: query.id,
      query: query.query,
      autoStart: query.auto_start ?? true,
      inputs: query.inputs ?? [],
      queryLanguage: 'Cypher',
      enableBootstrap: true,
    };

    await this.apiClient.post(`${this.instanceApi}/queries`, apiQuery);

    // API doesn't return the query object, so fetch it
    const createdQuery = await this.getQuery(query.id!);

    // Create SSE reaction for this query
    await this.ensureQuerySSEReaction(createdQuery.id);

    return createdQuery;
  }

  /**
   * Delete a query
   */
  async deleteQuery(id: string): Promise<void> {
    await this.apiClient.delete(`${this.instanceApi}/queries/${encodeURIComponent(id)}`);

    // Delete the SSE reaction for this query
    const reactionId = this.queryReactions.get(id);
    if (reactionId) {
      try {
        await this.deleteReaction(reactionId);
        this.queryReactions.delete(id);
      } catch (error) {
        console.warn(`Failed to delete SSE reaction ${reactionId}:`, error);
      }
    }
  }

  /**
   * Start a query
   */
  async startQuery(id: string): Promise<void> {
    await this.apiClient.post(`${this.instanceApi}/queries/${encodeURIComponent(id)}/start`);
  }

  /**
   * Stop a query
   */
  async stopQuery(id: string): Promise<void> {
    await this.apiClient.post(`${this.instanceApi}/queries/${encodeURIComponent(id)}/stop`);
  }

  /**
   * Get query results
   */
  async getQueryResults(queryId: string): Promise<any[]> {
    try {
      const response = await this.apiClient.get(`${this.instanceApi}/queries/${encodeURIComponent(queryId)}/results`);
      const data = response.data;
      // Handle both direct array and wrapped {data: [...]} responses
      if (Array.isArray(data)) {
        return data;
      } else if (data && Array.isArray(data.data)) {
        return data.data;
      }
      return [];
    } catch (error) {
      console.warn(`No results available for query ${queryId}`);
      return [];
    }
  }

  // ========== Reaction Management ==========

  /**
   * List all reactions
   */
  async listReactions(): Promise<Reaction[]> {
    const response = await this.apiClient.get(`${this.instanceApi}/reactions`);
    const data = response.data;
    // Handle both direct array and wrapped {data: [...]} responses
    let reactions = [];
    if (Array.isArray(data)) {
      reactions = data;
    } else if (data && Array.isArray(data.data)) {
      reactions = data.data;
    }
    return reactions.map((reaction: any) => ({
      ...(reaction.config ?? reaction),
      status: reaction.status ?? reaction.config?.status,
      error_message: reaction.error_message ?? reaction.config?.error_message,
    }));
  }

  /**
   * Get a specific reaction
   */
  async getReaction(id: string): Promise<Reaction> {
    const response = await this.apiClient.get(`${this.instanceApi}/reactions/${encodeURIComponent(id)}?view=full`);
    const data = response.data?.data ?? response.data;
    return {
      ...(data?.config ?? data),
      status: data?.status ?? data?.config?.status,
      error_message: data?.error_message ?? data?.config?.error_message,
    };
  }

  /**
   * Create a new reaction
   */
  async createReaction(reaction: Partial<Reaction>): Promise<Reaction> {
    const response = await this.apiClient.post(`${this.instanceApi}/reactions`, reaction);
    return response.data;
  }

  /**
   * Delete a reaction
   */
  async deleteReaction(id: string): Promise<void> {
    await this.apiClient.delete(`${this.instanceApi}/reactions/${encodeURIComponent(id)}`);
  }

  /**
   * Start a reaction
   */
  async startReaction(id: string): Promise<void> {
    await this.apiClient.post(`${this.instanceApi}/reactions/${encodeURIComponent(id)}/start`);
  }

  /**
   * Stop a reaction
   */
  async stopReaction(id: string): Promise<void> {
    await this.apiClient.post(`${this.instanceApi}/reactions/${encodeURIComponent(id)}/stop`);
  }

  // ========== Data Injection ==========

  /**
   * Inject data into a source
   */
  async injectData(sourceId: string, event: DataEvent): Promise<void> {
    // Add timestamp if not provided
    if (!event.timestamp) {
      event.timestamp = Date.now() * 1000000; // Convert to nanoseconds
    }

    // Transform to HTTP source format
    // DELETE has different structure: { operation, id, labels }
    // INSERT/UPDATE have: { operation, element }
    let httpSourceEvent: any;
    if (event.operation?.toLowerCase() === 'delete') {
      // DELETE: use flat structure with id and labels
      httpSourceEvent = {
        operation: 'delete',
        id: (event as any).id,
        labels: (event as any).labels,
        timestamp: event.timestamp
      };
    } else {
      // INSERT/UPDATE: use element structure
      httpSourceEvent = {
        operation: event.operation?.toLowerCase() || 'insert',
        element: event.element
      };
    }

    const url = `${this.instanceApi}/sources/${encodeURIComponent(sourceId)}/push`;

    try {
      console.log(`Sending to HTTP source:`, JSON.stringify(httpSourceEvent, null, 2));
      await this.apiClient.post(url, httpSourceEvent);
      console.log(`Data injected successfully to ${sourceId}`);
    } catch (error: any) {
      console.error(`Failed to inject data to ${sourceId}:`, error);
      console.error(`Payload was:`, JSON.stringify(httpSourceEvent, null, 2));
      throw error;
    }
  }

  // ========== SSE Management ==========

  /**
   * Ensure SSE reaction exists for a specific query
   */
  private async ensureQuerySSEReaction(queryId: string): Promise<void> {
    const reactionId = `sse-${queryId}`;

    // Check if we already have this reaction
    if (this.queryReactions.has(queryId)) {
      return;
    }

    try {
      // Check if reaction already exists
      const checkResponse = await this.apiClient.get(`${this.instanceApi}/reactions/${encodeURIComponent(reactionId)}?view=full`);

      if (checkResponse.status === 200) {
        const payload = checkResponse.data.data || checkResponse.data;
        const reaction = payload?.config ?? payload;
        this.queryReactions.set(queryId, reactionId);

        // Ensure it's running
        if ((payload?.status ?? reaction.status) !== 'Running') {
          await this.startReaction(reactionId);
        }

        // Connect SSE client to this reaction's endpoint
        const endpoint = this.buildSSEEndpoint(reaction);
        await this.sseClient.connect([queryId], endpoint);
        console.log(`Connected to SSE reaction for query ${queryId} at ${endpoint}`);
        return;
      }
    } catch (error: any) {
      if (error.response?.status !== 404) {
        throw error;
      }
    }

    // Reaction doesn't exist, create it
    console.log(`Creating SSE reaction for query: ${queryId}`);

    // Use incrementing port numbers for each reaction
    const port = this.sseReactionBasePort + this.queryReactions.size;

    const reactionConfig = {
      kind: 'sse',
      id: reactionId,
      queries: [queryId],
      autoStart: true,
      host: '0.0.0.0',
      port,
      ssePath: '/events',
      heartbeatIntervalMs: 15000,
    };

    await this.createReaction(reactionConfig as any);
    this.queryReactions.set(queryId, reactionId);

    // Connect SSE client to the new reaction's endpoint
    const endpoint = `http://localhost:${port}/events`;
    await this.sseClient.connect([queryId], endpoint);
    console.log(`Created and connected to SSE reaction for query ${queryId} at ${endpoint}`);
  }

  /**
   * Build SSE endpoint URL from reaction config
   */
  private buildSSEEndpoint(reaction: any): string {
    const host = reaction.host || 'localhost';
    const port = reaction.port || 8381;
    const path = reaction.ssePath || reaction.sse_path || '/events';
    return `http://${host === '0.0.0.0' ? 'localhost' : host}:${port}${path}`;
  }

  /**
   * Subscribe to real-time query updates
   */
  subscribe(queryId: string, callback: (result: QueryResult) => void): () => void {
    return this.sseClient.subscribe(queryId, callback);
  }

  /**
   * Get connection status
   */
  getConnectionStatus(): ConnectionStatus {
    return this.sseClient.getConnectionStatus();
  }

  /**
   * Subscribe to connection status changes
   */
  onConnectionStatusChange(callback: (status: ConnectionStatus) => void): () => void {
    return this.sseClient.onConnectionStatusChange(callback);
  }

  /**
   * Disconnect from Drasi Server
   */
  async disconnect(): Promise<void> {
    await this.sseClient.disconnect();
    this.initialized = false;
    console.log('Drasi Client disconnected');
  }
}
