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

import { QueryResult, ConnectionStatus } from '@/types';
import { DrasiSSEClient } from './grpc/SSEClient';
import { QueryDefinition, ALL_QUERIES, HAS_PRICE } from './queries';

export class DrasiClient {
  private baseUrl: string;
  private sseClient: DrasiSSEClient;
  private queries: Map<string, QueryDefinition> = new Map();
  private initialized = false;
  private reactionId = 'sse-stream';
  private createdQueries: Set<string> = new Set();
  private createdReaction = false;
  private customQueries: Set<string> = new Set();
  private instanceId: string | null = null;

  constructor(baseUrl?: string) {
    // Use direct URL - Drasi Server should have CORS enabled
    this.baseUrl = baseUrl || 'http://localhost:8280';
  this.sseClient = new DrasiSSEClient();
    this.initializeQueries();
  }

  private initializeQueries() {
    // Load all queries from the queries.ts definitions
    for (const query of ALL_QUERIES) {
      this.queries.set(query.id, query);
    }
  }

  /**
   * Initialize connection to Drasi Server and create queries and reaction
   */
  async initialize(): Promise<void> {
    if (this.initialized) {
      return;
    }

    try {
      // Check server health
      const healthResponse = await fetch(`${this.baseUrl}/health`);
      if (!healthResponse.ok) {
        throw new Error('Drasi Server is not healthy');
      }

      // Discover the instance ID (convenience routes map to the first instance)
      try {
        const instancesResponse = await fetch(`${this.baseUrl}/api/v1/instances`);
        if (instancesResponse.ok) {
          const instancesJson = await instancesResponse.json();
          const instances = instancesJson.data ?? instancesJson;
          if (Array.isArray(instances) && instances.length > 0) {
            this.instanceId = instances[0].id ?? instances[0];
            console.log(`Discovered instance ID: ${this.instanceId}`);
          }
        }
      } catch (err) {
        console.warn('Could not discover instance ID:', err);
      }

      // Check if sources exist
    const sourcesResponse = await fetch(`${this.baseUrl}/api/v1/sources`);
    const sourcesData = await sourcesResponse.json();
    const sources = sourcesData.data || sourcesData; // Handle both wrapped and unwrapped responses

    const requiredSources = ['postgres-stocks', 'price-feed'];
    const existingSources = Array.isArray(sources)
      ? sources.map((s: any) => (s.config ?? s).id)
      : [];
      
      for (const sourceId of requiredSources) {
        if (!existingSources.includes(sourceId)) {
          console.warn(`Required source ${sourceId} not found. Please ensure server is configured correctly.`);
        }
      }

      // Step 1: Create all queries FIRST (required for reaction to subscribe to them)
      console.log('Creating queries...');
      for (const [, queryDef] of this.queries) {
        await this.ensureQuery(queryDef);
      }

      // Step 2: Wait for bootstrap to complete
      console.log('Waiting for bootstrap data...');
      await new Promise(resolve => setTimeout(resolve, 2000));

      // Step 3: Fetch initial data from REST API (will seed before SSE live updates)
      console.log('Fetching initial query results...');
      const initialResults: Record<string, any[]> = {};
      for (const queryId of this.queries.keys()) {
        try {
          const results = await this.getQueryResults(queryId);
          // Always store results (even empty) so loading state completes
          initialResults[queryId] = results;
          if (results.length > 0) {
            console.log(`Got ${results.length} initial results for ${queryId}`);
          } else {
            console.log(`No initial results for ${queryId} (empty)`);
          }
        } catch (error) {
          // On error, still store empty array so loading completes
          initialResults[queryId] = [];
          console.warn(`Failed to fetch initial results for ${queryId}:`, error);
        }
      }

      // Step 4: Create SSE reaction AFTER queries exist
      console.log('Creating SSE reaction...');
      const sseEndpoint = await this.ensureReaction();

      // Step 5: Connect to SSE stream for real-time updates
      console.log('Connecting to SSE stream at', sseEndpoint);
      const queryIds = Array.from(this.queries.keys());
      await this.sseClient.connect(queryIds, sseEndpoint, initialResults);

      // Register cleanup handlers
      this.registerCleanupHandlers();

      this.initialized = true;
      console.log('Drasi Client initialized successfully with dynamic queries and reaction');
      
    } catch (error) {
      console.error('Failed to initialize Drasi Client:', error);
      // Cleanup any partially created resources
      await this.cleanup();
      throw error;
    }
  }

  /**
   * Ensure the SSE reaction exists and return its endpoint URL
   */
  private async ensureReaction(): Promise<string> {
    try {
      // Check if reaction exists
      const checkResponse = await fetch(`${this.baseUrl}/api/v1/reactions/${this.reactionId}?view=full`);
      
      if (checkResponse.status === 404) {
        // Reaction doesn't exist, create it
        console.log(`Creating SSE reaction: ${this.reactionId}`);
        
        const reactionConfig = {
          kind: 'sse',
          id: this.reactionId,
          queries: Array.from(this.queries.keys()), // This will include price-ticker-query
          autoStart: true,
          // SSE reaction config fields (camelCase for nested SseReactionConfigDto)
          host: '0.0.0.0',
          port: 8281,
          ssePath: '/events',
          heartbeatIntervalMs: 15000
        };

        const createResponse = await fetch(`${this.baseUrl}/api/v1/reactions`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(reactionConfig)
        });

        if (!createResponse.ok) {
          const error = await createResponse.text();
          throw new Error(`Failed to create reaction ${this.reactionId}: ${error}`);
        }

        // Track that this session created the reaction
        this.createdReaction = true;

        // Start the reaction
        await fetch(`${this.baseUrl}/api/v1/reactions/${this.reactionId}/start`, { method: 'POST' });
        return 'http://localhost:8281/events';
      } else if (checkResponse.ok) {
        // Reaction exists, make sure it's running
        const reaction = await checkResponse.json();
        const payload = reaction.data ?? reaction;
        const config = payload?.config ?? payload;
        if ((payload?.status ?? config.status) !== 'running') {
          console.log(`Starting reaction: ${this.reactionId}`);
          await fetch(`${this.baseUrl}/api/v1/reactions/${this.reactionId}/start`, { method: 'POST' });
        }
        // Derive endpoint from existing reaction properties
        const props = config?.properties || config || {};
        const host = props.host || 'localhost';
        const port = props.port || 50051;
        const path = props.ssePath || '/events';
        return `http://${host === '0.0.0.0' ? 'localhost' : host}:${port}${path}`;
      }
    } catch (error) {
      console.error(`Failed to ensure reaction ${this.reactionId}:`, error);
      throw error;
    }
    // Fallback default
    return 'http://localhost:8281/events';
  }

  /**
   * Ensure a query exists in Drasi Server
   */
  private async ensureQuery(queryDef: QueryDefinition): Promise<void> {
    try {
      // Check if query exists
      const checkResponse = await fetch(`${this.baseUrl}/api/v1/queries/${queryDef.id}?view=full`);
      
      if (checkResponse.status === 404) {
        // Query doesn't exist, create it
        console.log(`Creating query: ${queryDef.id}`);
        
        const queryConfig = {
          id: queryDef.id,
          query: queryDef.query,
          queryLanguage: 'Cypher',
          sources: queryDef.sources,
          joins: queryDef.joins,
          autoStart: true
        };

        const createResponse = await fetch(`${this.baseUrl}/api/v1/queries`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(queryConfig)
        });

        if (!createResponse.ok) {
          const error = await createResponse.text();
          throw new Error(`Failed to create query ${queryDef.id}: ${error}`);
        }

        // Track created query for cleanup
        this.createdQueries.add(queryDef.id);

        // Start the query
        await fetch(`${this.baseUrl}/api/v1/queries/${queryDef.id}/start`, { method: 'POST' });
        
      } else if (checkResponse.ok) {
        // Query exists, make sure it's running
        const query = await checkResponse.json();
        const payload = query.data ?? query;
        const config = payload?.config ?? payload;
        if ((payload?.status ?? config.status) !== 'running') {
          console.log(`Starting query: ${queryDef.id}`);
          await fetch(`${this.baseUrl}/api/v1/queries/${queryDef.id}/start`, { method: 'POST' });
        }
      }
    } catch (error) {
      console.error(`Failed to ensure query ${queryDef.id}:`, error);
      throw error;
    }
  }

  /**
   * Create a custom screener query dynamically
   */
  async createCustomQuery(
    id: string, 
    name: string, 
    whereClause: string,
    additionalFields?: string[]
  ): Promise<void> {
    const baseFields = [
      's.symbol AS symbol',
      's.name AS name',
      's.sector AS sector',
      'sp.price AS price',
      'sp.volume AS volume',
      '((sp.price - sp.previous_close) / sp.previous_close * 100) AS changePercent'
    ];

    const allFields = [...baseFields, ...(additionalFields || [])];

    const queryDef: QueryDefinition = {
      id,
      description: name,
      query: `
        MATCH (s:stocks)-[:HAS_PRICE]->(sp:stock_prices)
        WHERE ${whereClause}
        RETURN ${allFields.join(',\n               ')}
        ORDER BY sp.volume DESC
      `,
      sources: [
        { sourceId: 'postgres-stocks', pipeline: [] },
        { sourceId: 'price-feed', pipeline: [] }
      ],
      joins: [HAS_PRICE]
    };

    await this.ensureQuery(queryDef);
    this.customQueries.add(id);

    // Update reaction to include new query
    await this.updateReactionQueries();
    
    console.log(`Created custom query: ${id} - ${name}`);
  }

  /**
   * Update reaction with current list of queries
   */
  private async updateReactionQueries(): Promise<void> {
    const allQueryIds = [
      ...Array.from(this.queries.keys()),
      ...Array.from(this.customQueries)
    ];

    const updateResponse = await fetch(`${this.baseUrl}/api/v1/reactions/${this.reactionId}`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        queries: allQueryIds
      })
    });

    if (!updateResponse.ok) {
      console.error('Failed to update reaction with new queries');
    }
  }

  /**
   * Get a query's full configuration from the Drasi Server
   */
  async getQueryConfig(queryId: string): Promise<Record<string, any> | null> {
    try {
      const response = await fetch(`${this.baseUrl}/api/v1/queries/${queryId}?view=full`);
      if (!response.ok) {
        console.warn(`Failed to fetch query config for ${queryId}: ${response.status}`);
        return null;
      }
      const json = await response.json();
      const payload = json.data ?? json;
      const config = payload?.config ?? payload;
      return config ?? null;
    } catch (error) {
      console.error(`Failed to get query config for ${queryId}:`, error);
      return null;
    }
  }

  /**
   * Get initial query results from REST API
   */
  async getQueryResults(queryId: string): Promise<any[]> {
    try {
      const response = await fetch(`${this.baseUrl}/api/v1/queries/${queryId}/results`);
      if (!response.ok) {
        console.warn(`No results available for query ${queryId}`);
        return [];
      }
      const json = await response.json();
      // API returns { success: true, data: [...] }
      const data = json.data ?? json;
      return Array.isArray(data) ? data : [];
    } catch (error) {
      console.error(`Failed to get results for query ${queryId}:`, error);
      return [];
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
   * Get the Drasi Server UI URL for the current instance
   */
  getDrasiUiUrl(): string | null {
    if (!this.instanceId) return null;
    return `${this.baseUrl}/ui?instance=${encodeURIComponent(this.instanceId)}`;
  }

  /**
   * Register cleanup handlers for browser events
   */
  private registerCleanupHandlers() {
    // Handle page unload
    window.addEventListener('beforeunload', () => {
      this.cleanup();
    });

    // Handle visibility change (tab switching)
    document.addEventListener('visibilitychange', () => {
      if (document.visibilityState === 'hidden') {
        console.log('Page hidden, preparing for potential cleanup...');
      }
    });
  }

  /**
   * Clean up all created resources
   */
  async cleanup(): Promise<void> {
    console.log('Drasi Client closing — leaving queries and reactions in place');

    // Clear tracking sets
    this.createdQueries.clear();
    this.customQueries.clear();
    this.createdReaction = false;
  }

  /**
   * Disconnect from Drasi Server
   */
  async disconnect(): Promise<void> {
    await this.cleanup();
    await this.sseClient.disconnect();
    this.initialized = false;
    console.log('Drasi Client disconnected and cleaned up');
  }
}
