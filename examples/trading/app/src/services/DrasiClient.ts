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

interface QueryDefinition {
  id: string;
  query: string;
  sources: Array<{ source_id: string; pipeline: string[] }>;
  joins?: QueryJoin[];
}

interface QueryJoin {
  id: string;
  keys: QueryJoinKey[];
}

interface QueryJoinKey {
  label: string;
  property: string;
}

export class DrasiClient {
  private baseUrl: string;
  private sseClient: DrasiSSEClient;
  private queries: Map<string, QueryDefinition> = new Map();
  private initialized = false;
  private reactionId = 'sse-stream';
  private createdQueries: Set<string> = new Set();
  private customQueries: Set<string> = new Set();

  constructor(baseUrl?: string) {
    // Use direct URL - Drasi Server should have CORS enabled
    this.baseUrl = baseUrl || 'http://localhost:8080';
  this.sseClient = new DrasiSSEClient();
    this.initializeQueries();
  }

  private initializeQueries() {
    // Define synthetic joins for cross-source relationships
    const hasPrice: QueryJoin = {
      id: 'HAS_PRICE',
      keys: [
        { label: 'stocks', property: 'symbol' },
        { label: 'stock_prices', property: 'symbol' }
      ]
    };

    const ownsStock: QueryJoin = {
      id: 'OWNS_STOCK',
      keys: [
        { label: 'portfolio', property: 'symbol' },
        { label: 'stocks', property: 'symbol' }
      ]
    };

    // Define all queries with synthetic joins
    this.queries.set('watchlist-query', {
      id: 'watchlist-query',
      query: `
        MATCH (s:stocks)-[:HAS_PRICE]->(sp:stock_prices)
        WHERE s.symbol IN ['AAPL', 'MSFT', 'GOOGL', 'TSLA', 'NVDA']
        RETURN s.symbol AS symbol,
               s.name AS name,
               sp.price AS price,
               sp.previous_close AS previous_close,
               ((sp.price - sp.previous_close) / sp.previous_close * 100) AS change_percent
      `,
      sources: [
        { source_id: 'postgres-stocks', pipeline: [] },
        { source_id: 'price-feed', pipeline: [] }
      ],
      joins: [hasPrice]
    });

    this.queries.set('portfolio-query', {
      id: 'portfolio-query',
      query: `
        MATCH (p:portfolio)-[:OWNS_STOCK]->(s:stocks)
        OPTIONAL MATCH (s)-[:HAS_PRICE]->(sp:stock_prices)
        RETURN  p.symbol AS symbol,
                s.name AS name,
                p.quantity AS quantity,
                p.purchase_price AS purchase_price,
                sp.price AS current_price,
                (sp.price * p.quantity) AS current_value,
                (p.purchase_price * p.quantity) AS cost_basis,
                ((sp.price - p.purchase_price) * p.quantity) AS profit_loss,
                ((sp.price - p.purchase_price) / p.purchase_price * 100) AS profit_loss_percent
      `,
      sources: [
        { source_id: 'postgres-stocks', pipeline: [] },
        { source_id: 'price-feed', pipeline: [] }
      ],
      joins: [ownsStock, hasPrice]
    });

    this.queries.set('top-gainers-query', {
      id: 'top-gainers-query',
      query: `
        MATCH (s:stocks)-[:HAS_PRICE]->(sp:stock_prices)
        WHERE sp.price > sp.previous_close
        WITH s, sp, ((sp.price - sp.previous_close) / sp.previous_close * 100) AS change_percent
        RETURN s.symbol AS symbol,
               s.name AS name,
               sp.price AS price,
               sp.previous_close AS previous_close,
               change_percent
      `,
      sources: [
        { source_id: 'postgres-stocks', pipeline: [] },
        { source_id: 'price-feed', pipeline: [] }
      ],
      joins: [hasPrice]
    });

    this.queries.set('top-losers-query', {
      id: 'top-losers-query',
      query: `
        MATCH (s:stocks)-[:HAS_PRICE]->(sp:stock_prices)
        WHERE sp.price < sp.previous_close
        WITH s, sp, ((sp.price - sp.previous_close) / sp.previous_close * 100) AS change_percent
        RETURN s.symbol AS symbol,
               s.name AS name,
               sp.price AS price,
               sp.previous_close AS previous_close,
               change_percent
      `,
      sources: [
        { source_id: 'postgres-stocks', pipeline: [] },
        { source_id: 'price-feed', pipeline: [] }
      ],
      joins: [hasPrice]
    });

    this.queries.set('high-volume-query', {
      id: 'high-volume-query',
      query: `
        MATCH (s:stocks)-[:HAS_PRICE]->(sp:stock_prices)
        WHERE sp.volume > 10000000
        RETURN s.symbol AS symbol,
               s.name AS name,
               sp.price AS price,
               sp.volume AS volume,
               ((sp.price - sp.previous_close) / sp.previous_close * 100) AS change_percent
      `,
      sources: [
        { source_id: 'postgres-stocks', pipeline: [] },
        { source_id: 'price-feed', pipeline: [] }
      ],
      joins: [hasPrice]
    });

    // Add a simple price ticker query that only uses price-feed
    this.queries.set('price-ticker-query', {
      id: 'price-ticker-query',
      query: `
        MATCH (sp:stock_prices)
        RETURN sp.symbol AS symbol,
               sp.price AS price,
               sp.previous_close AS previous_close,
               ((sp.price - sp.previous_close) / sp.previous_close * 100) AS change_percent
      `,
      sources: [
        { source_id: 'price-feed', pipeline: [] }
      ],
      joins: [] // No joins needed - single source
    });
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

      // Check if sources exist
      const sourcesResponse = await fetch(`${this.baseUrl}/sources`);
      const sourcesData = await sourcesResponse.json();
      const sources = sourcesData.data || sourcesData; // Handle both wrapped and unwrapped responses
      
      const requiredSources = ['postgres-stocks', 'price-feed'];
      const existingSources = Array.isArray(sources) ? sources.map((s: any) => s.id) : [];
      
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
          if (results.length > 0) {
            initialResults[queryId] = results;
            console.log(`Got ${results.length} initial results for ${queryId}`);
          }
        } catch (error) {
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
      const checkResponse = await fetch(`${this.baseUrl}/reactions/${this.reactionId}`);
      
      if (checkResponse.status === 404) {
        // Reaction doesn't exist, create it
        console.log(`Creating SSE reaction: ${this.reactionId}`);
        
        const reactionConfig = {
          kind: 'sse',
          id: this.reactionId,
          queries: Array.from(this.queries.keys()), // This will include price-ticker-query
          auto_start: true,
          // SSE reaction config fields (flattened, not in properties)
          host: '0.0.0.0',
          port: 50051,
          sse_path: '/events',
          heartbeat_interval_ms: 15000
        };

        const createResponse = await fetch(`${this.baseUrl}/reactions`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(reactionConfig)
        });

        if (!createResponse.ok) {
          const error = await createResponse.text();
          throw new Error(`Failed to create reaction ${this.reactionId}: ${error}`);
        }

        // Start the reaction
        await fetch(`${this.baseUrl}/reactions/${this.reactionId}/start`, { method: 'POST' });
        return 'http://localhost:50051/events';
      } else if (checkResponse.ok) {
        // Reaction exists, make sure it's running
        const reaction = await checkResponse.json();
        if (reaction.status !== 'running') {
          console.log(`Starting reaction: ${this.reactionId}`);
          await fetch(`${this.baseUrl}/reactions/${this.reactionId}/start`, { method: 'POST' });
        }
        // Derive endpoint from existing reaction properties
        const props = reaction.config?.properties || reaction.properties || {};
        const host = props.host || 'localhost';
        const port = props.port || 50051;
        const path = props.sse_path || '/events';
        return `http://${host === '0.0.0.0' ? 'localhost' : host}:${port}${path}`;
      }
    } catch (error) {
      console.error(`Failed to ensure reaction ${this.reactionId}:`, error);
      throw error;
    }
    // Fallback default
    return 'http://localhost:50051/events';
  }

  /**
   * Ensure a query exists in Drasi Server
   */
  private async ensureQuery(queryDef: QueryDefinition): Promise<void> {
    try {
      // Check if query exists
      const checkResponse = await fetch(`${this.baseUrl}/queries/${queryDef.id}`);
      
      if (checkResponse.status === 404) {
        // Query doesn't exist, create it
        console.log(`Creating query: ${queryDef.id}`);
        
        const queryConfig = {
          id: queryDef.id,
          query: queryDef.query,
          sources: queryDef.sources,
          joins: queryDef.joins,
          auto_start: true
        };

        const createResponse = await fetch(`${this.baseUrl}/queries`, {
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
        await fetch(`${this.baseUrl}/queries/${queryDef.id}/start`, { method: 'POST' });
        
      } else if (checkResponse.ok) {
        // Query exists, make sure it's running
        const query = await checkResponse.json();
        if (query.status !== 'running') {
          console.log(`Starting query: ${queryDef.id}`);
          await fetch(`${this.baseUrl}/queries/${queryDef.id}/start`, { method: 'POST' });
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
    const hasPrice: QueryJoin = {
      id: 'HAS_PRICE',
      keys: [
        { label: 'stocks', property: 'symbol' },
        { label: 'stock_prices', property: 'symbol' }
      ]
    };

    const baseFields = [
      's.symbol AS symbol',
      's.name AS name',
      's.sector AS sector',
      'sp.price AS price',
      'sp.volume AS volume',
      '((sp.price - sp.previous_close) / sp.previous_close * 100) AS change_percent'
    ];

    const allFields = [...baseFields, ...(additionalFields || [])];

    const queryDef: QueryDefinition = {
      id,
      query: `
        MATCH (s:stocks)-[:HAS_PRICE]->(sp:stock_prices)
        WHERE ${whereClause}
        RETURN ${allFields.join(',\n               ')}
        ORDER BY sp.volume DESC
      `,
      sources: [
        { source_id: 'postgres-stocks', pipeline: [] },
        { source_id: 'price-feed', pipeline: [] }
      ],
      joins: [hasPrice]
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

    const updateResponse = await fetch(`${this.baseUrl}/reactions/${this.reactionId}`, {
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
   * Get initial query results from REST API
   */
  async getQueryResults(queryId: string): Promise<any[]> {
    try {
      const response = await fetch(`${this.baseUrl}/queries/${queryId}/results`);
      if (!response.ok) {
        console.warn(`No results available for query ${queryId}`);
        return [];
      }
      const data = await response.json();
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
    console.log('Cleaning up Drasi Client resources...');

    // IMPORTANT: Stop and delete reaction FIRST (before queries)
    try {
      await fetch(`${this.baseUrl}/reactions/${this.reactionId}/stop`, { method: 'POST' });
      await fetch(`${this.baseUrl}/reactions/${this.reactionId}`, { method: 'DELETE' });
      console.log(`Deleted reaction: ${this.reactionId}`);
    } catch (error) {
      console.error(`Failed to cleanup reaction ${this.reactionId}:`, error);
    }

    // Then stop and delete all queries
    for (const queryId of this.createdQueries) {
      try {
        await fetch(`${this.baseUrl}/queries/${queryId}/stop`, { method: 'POST' });
        await fetch(`${this.baseUrl}/queries/${queryId}`, { method: 'DELETE' });
        console.log(`Deleted query: ${queryId}`);
      } catch (error) {
        console.error(`Failed to cleanup query ${queryId}:`, error);
      }
    }

    // Stop and delete custom queries
    for (const queryId of this.customQueries) {
      try {
        await fetch(`${this.baseUrl}/queries/${queryId}/stop`, { method: 'POST' });
        await fetch(`${this.baseUrl}/queries/${queryId}`, { method: 'DELETE' });
        console.log(`Deleted custom query: ${queryId}`);
      } catch (error) {
        console.error(`Failed to cleanup custom query ${queryId}:`, error);
      }
    }

    // Clear tracking sets
    this.createdQueries.clear();
    this.customQueries.clear();
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