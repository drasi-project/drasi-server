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

import { RestClientConfig, QueryDefinition, QueryResult } from './types';

/**
 * REST API client for Drasi Server
 * Handles queries, reactions, sources via REST endpoints
 */
export class DrasiRestClient {
  private baseUrl: string;

  constructor(config: RestClientConfig) {
    if (!config.baseUrl) {
      throw new Error('[@drasi/react] baseUrl is required in RestClientConfig');
    }
    this.baseUrl = config.baseUrl;
  }

  /**
   * Get base URL
   */
  getBaseUrl(): string {
    return this.baseUrl;
  }

  /**
   * Create a query
   */
  async createQuery(query: QueryDefinition): Promise<void> {
    const response = await fetch(`${this.baseUrl}/queries`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(query)
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to create query ${query.id}: ${error}`);
    }
  }

  /**
   * Start a query
   */
  async startQuery(queryId: string): Promise<void> {
    const response = await fetch(`${this.baseUrl}/queries/${queryId}/start`, {
      method: 'POST'
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to start query ${queryId}: ${error}`);
    }
  }

  /**
   * Stop a query
   */
  async stopQuery(queryId: string): Promise<void> {
    const response = await fetch(`${this.baseUrl}/queries/${queryId}/stop`, {
      method: 'POST'
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to stop query ${queryId}: ${error}`);
    }
  }

  /**
   * Delete a query
   */
  async deleteQuery(queryId: string): Promise<void> {
    const response = await fetch(`${this.baseUrl}/queries/${queryId}`, {
      method: 'DELETE'
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to delete query ${queryId}: ${error}`);
    }
  }

  /**
   * Get query results (bootstrap data)
   */
  async getQueryResults<T = any>(queryId: string): Promise<QueryResult<T>[]> {
    const response = await fetch(`${this.baseUrl}/queries/${queryId}/results`);
    
    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to get results for query ${queryId}: ${error}`);
    }

    return response.json();
  }

  /**
   * Check health
   */
  async checkHealth(): Promise<boolean> {
    try {
      const response = await fetch(`${this.baseUrl}/health`);
      return response.ok;
    } catch {
      return false;
    }
  }
}
