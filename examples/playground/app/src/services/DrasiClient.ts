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
  private reactionId = 'playground-sse-stream';

  constructor(baseUrl?: string) {
    const url = baseUrl || 'http://localhost:8080';
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

      // Create SSE reaction if needed
      const sseEndpoint = await this.ensureSSEReaction(queries.map(q => q.id));

      // Connect to SSE stream
      console.log('Connecting to SSE stream at', sseEndpoint);
      await this.sseClient.connect(queries.map(q => q.id), sseEndpoint);

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
    const response = await this.apiClient.get('/sources');
    const data = response.data;
    // Handle both direct array and wrapped {data: [...]} responses
    if (Array.isArray(data)) {
      return data;
    } else if (data && Array.isArray(data.data)) {
      return data.data;
    }
    return [];
  }

  /**
   * Get a specific source
   */
  async getSource(id: string): Promise<Source> {
    const response = await this.apiClient.get(`/sources/${id}`);
    return response.data;
  }

  /**
   * Create a new source
   */
  async createSource(source: Partial<Source>): Promise<Source> {
    const response = await this.apiClient.post('/sources', source);
    return response.data;
  }

  /**
   * Delete a source
   */
  async deleteSource(id: string): Promise<void> {
    await this.apiClient.delete(`/sources/${id}`);
  }

  /**
   * Start a source
   */
  async startSource(id: string): Promise<void> {
    await this.apiClient.post(`/sources/${id}/start`);
  }

  /**
   * Stop a source
   */
  async stopSource(id: string): Promise<void> {
    await this.apiClient.post(`/sources/${id}/stop`);
  }

  // ========== Query Management ==========

  /**
   * List all queries
   */
  async listQueries(): Promise<Query[]> {
    const response = await this.apiClient.get('/queries');
    const data = response.data;
    // Handle both direct array and wrapped {data: [...]} responses
    if (Array.isArray(data)) {
      return data;
    } else if (data && Array.isArray(data.data)) {
      return data.data;
    }
    return [];
  }

  /**
   * Get a specific query
   */
  async getQuery(id: string): Promise<Query> {
    const response = await this.apiClient.get(`/queries/${id}`);
    return response.data;
  }

  /**
   * Create a new query
   */
  async createQuery(query: Partial<Query>): Promise<Query> {
    const response = await this.apiClient.post('/queries', query);

    // Update SSE reaction to include new query
    await this.updateSSEReactionQueries();

    return response.data;
  }

  /**
   * Delete a query
   */
  async deleteQuery(id: string): Promise<void> {
    await this.apiClient.delete(`/queries/${id}`);

    // Update SSE reaction to remove query
    await this.updateSSEReactionQueries();
  }

  /**
   * Start a query
   */
  async startQuery(id: string): Promise<void> {
    await this.apiClient.post(`/queries/${id}/start`);
  }

  /**
   * Stop a query
   */
  async stopQuery(id: string): Promise<void> {
    await this.apiClient.post(`/queries/${id}/stop`);
  }

  /**
   * Get query results
   */
  async getQueryResults(queryId: string): Promise<any[]> {
    try {
      const response = await this.apiClient.get(`/queries/${queryId}/results`);
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
    const response = await this.apiClient.get('/reactions');
    const data = response.data;
    // Handle both direct array and wrapped {data: [...]} responses
    if (Array.isArray(data)) {
      return data;
    } else if (data && Array.isArray(data.data)) {
      return data.data;
    }
    return [];
  }

  /**
   * Get a specific reaction
   */
  async getReaction(id: string): Promise<Reaction> {
    const response = await this.apiClient.get(`/reactions/${id}`);
    return response.data;
  }

  /**
   * Create a new reaction
   */
  async createReaction(reaction: Partial<Reaction>): Promise<Reaction> {
    const response = await this.apiClient.post('/reactions', reaction);
    return response.data;
  }

  /**
   * Delete a reaction
   */
  async deleteReaction(id: string): Promise<void> {
    await this.apiClient.delete(`/reactions/${id}`);
  }

  /**
   * Start a reaction
   */
  async startReaction(id: string): Promise<void> {
    await this.apiClient.post(`/reactions/${id}/start`);
  }

  /**
   * Stop a reaction
   */
  async stopReaction(id: string): Promise<void> {
    await this.apiClient.post(`/reactions/${id}/stop`);
  }

  // ========== Data Injection ==========

  /**
   * Inject data into a source
   * For now, we'll send everything through a proxy to avoid CORS issues
   */
  async injectData(sourceId: string, event: DataEvent): Promise<void> {
    // Add timestamp if not provided
    if (!event.timestamp) {
      event.timestamp = Date.now() * 1000000; // Convert to nanoseconds
    }

    // Transform to HTTP source format
    const httpSourceEvent = {
      operation: event.op?.toLowerCase() || 'insert',
      element: {
        type: 'node',
        id: event.id,
        labels: event.labels,
        properties: event.properties
      }
    };

    // Send through our proxy endpoint to avoid CORS issues
    // We'll configure Vite to proxy this to the HTTP source
    await axios.post(`/api/inject/${sourceId}`, httpSourceEvent);
  }

  // ========== SSE Management ==========

  /**
   * Ensure SSE reaction exists and return its endpoint
   */
  private async ensureSSEReaction(queryIds: string[]): Promise<string> {
    try {
      const checkResponse = await this.apiClient.get(`/reactions/${this.reactionId}`);

      if (checkResponse.status === 200) {
        // Reaction exists, ensure it's running
        const reaction = checkResponse.data;
        if (reaction.status !== 'Running') {
          await this.startReaction(this.reactionId);
        }
        return this.buildSSEEndpoint(reaction);
      }
    } catch (error: any) {
      if (error.response?.status === 404) {
        // Reaction doesn't exist, create it
        console.log('Creating SSE reaction...');

        const reactionConfig: Partial<Reaction> = {
          id: this.reactionId,
          reaction_type: 'sse',
          queries: queryIds,
          auto_start: true,
          host: '0.0.0.0',
          port: 50051,
          sse_path: '/events',
          heartbeat_interval_ms: 15000,
        };

        await this.createReaction(reactionConfig);
        await this.startReaction(this.reactionId);
        return 'http://localhost:50051/events';
      }
      throw error;
    }

    return 'http://localhost:50051/events';
  }

  /**
   * Build SSE endpoint URL from reaction config
   */
  private buildSSEEndpoint(reaction: any): string {
    const host = reaction.host || 'localhost';
    const port = reaction.port || 50051;
    const path = reaction.sse_path || '/events';
    return `http://${host === '0.0.0.0' ? 'localhost' : host}:${port}${path}`;
  }

  /**
   * Update SSE reaction with current list of queries
   */
  private async updateSSEReactionQueries(): Promise<void> {
    try {
      const queries = await this.listQueries();
      const queryIds = queries.map(q => q.id);

      // Check if reaction exists
      try {
        await this.getReaction(this.reactionId);

        // Update reaction with new query list
        await this.apiClient.put(`/reactions/${this.reactionId}`, {
          queries: queryIds,
        });

        console.log('Updated SSE reaction with queries:', queryIds);
      } catch (error: any) {
        if (error.response?.status === 404) {
          // Reaction doesn't exist yet, will be created on next ensureSSEReaction call
          console.log('SSE reaction does not exist yet');
        }
      }
    } catch (error) {
      console.error('Failed to update SSE reaction queries:', error);
    }
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
