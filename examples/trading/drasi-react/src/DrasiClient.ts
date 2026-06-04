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

import { DrasiSSEClient } from './DrasiSSEClient';
import {
  ConnectionStatus,
  QueryDefinition,
  QueryResult,
  ReactionDefinition,
  RouteUnidentified,
} from './types';

/** Configuration for {@link DrasiClient}. */
export interface DrasiClientOptions {
  /** Base URL of the Drasi Server REST API. Defaults to `http://localhost:8280`. */
  serverUrl?: string;
  /** Continuous queries to ensure exist and stream over the shared connection. */
  queries: QueryDefinition[];
  /** The SSE reaction that multiplexes the queries. */
  reaction: ReactionDefinition;
  /** Routes content for change payloads that arrive without a query id. */
  routeUnidentified?: RouteUnidentified;
}

const DEFAULT_REACTION_ID = 'sse-stream';

/**
 * DrasiClient orchestrates a Drasi Server REST API connection for a set of
 * continuous queries and a single SSE reaction. It ensures the queries and
 * reaction exist, seeds initial results from REST, and opens one shared SSE
 * connection (via {@link DrasiSSEClient}) that every query is multiplexed over.
 *
 * The client is application agnostic — all queries, the reaction and any
 * content-based routing are supplied by the host application.
 */
export class DrasiClient {
  private baseUrl: string;
  private sseClient: DrasiSSEClient;
  private queries: Map<string, QueryDefinition> = new Map();
  private reaction: ReactionDefinition;
  private reactionId: string;
  private initialized = false;
  private instanceId: string | null = null;

  constructor(options: DrasiClientOptions) {
    this.baseUrl = options.serverUrl || 'http://localhost:8280';
    this.reaction = options.reaction;
    this.reactionId = options.reaction.id || DEFAULT_REACTION_ID;
    this.sseClient = new DrasiSSEClient(options.routeUnidentified);
    for (const query of options.queries) {
      this.queries.set(query.id, query);
    }
  }

  /** Whether {@link initialize} has completed successfully. */
  isInitialized(): boolean {
    return this.initialized;
  }

  /**
   * Initialize the connection: verify server health, ensure queries and the SSE
   * reaction exist, seed initial results from REST, then open the shared SSE
   * stream for live updates.
   */
  async initialize(): Promise<void> {
    if (this.initialized) {
      return;
    }

    // Check server health
    const healthResponse = await fetch(`${this.baseUrl}/health`);
    if (!healthResponse.ok) {
      throw new Error('Drasi Server is not healthy');
    }

    // Discover the instance id (convenience routes map to the first instance)
    try {
      const instancesResponse = await fetch(`${this.baseUrl}/api/v1/instances`);
      if (instancesResponse.ok) {
        const instancesJson = await instancesResponse.json();
        const instances = instancesJson.data ?? instancesJson;
        if (Array.isArray(instances) && instances.length > 0) {
          this.instanceId = instances[0].id ?? instances[0];
        }
      }
    } catch (err) {
      console.warn('Could not discover instance id:', err);
    }

    // Step 1: Create all queries first (the reaction subscribes to them)
    for (const [, queryDef] of this.queries) {
      await this.ensureQuery(queryDef);
    }

    // Step 2: Wait for bootstrap to complete
    await new Promise((resolve) => setTimeout(resolve, 2000));

    // Step 3: Seed initial data from REST (before live SSE updates arrive)
    const initialResults: Record<string, any[]> = {};
    for (const queryId of this.queries.keys()) {
      try {
        initialResults[queryId] = await this.getQueryResults(queryId);
      } catch (error) {
        initialResults[queryId] = [];
        console.warn(`Failed to fetch initial results for ${queryId}:`, error);
      }
    }

    // Step 4: Create the SSE reaction now that the queries exist
    const sseEndpoint = await this.ensureReaction();

    // Step 5: Connect to the shared SSE stream for real-time updates
    const queryIds = Array.from(this.queries.keys());
    await this.sseClient.connect(queryIds, sseEndpoint, initialResults);

    this.initialized = true;
  }

  /**
   * Ensure the SSE reaction exists and return its public endpoint URL.
   */
  private async ensureReaction(): Promise<string> {
    const checkResponse = await fetch(
      `${this.baseUrl}/api/v1/reactions/${this.reactionId}?view=full`,
    );

    if (checkResponse.status === 404) {
      const reactionConfig: Record<string, any> = {
        kind: this.reaction.kind || 'sse',
        id: this.reactionId,
        queries: Array.from(this.queries.keys()),
        autoStart: true,
        host: this.reaction.host || '0.0.0.0',
        port: this.reaction.port,
        ssePath: this.reaction.ssePath || '/events',
      };
      if (this.reaction.heartbeatIntervalMs !== undefined) {
        reactionConfig.heartbeatIntervalMs = this.reaction.heartbeatIntervalMs;
      }

      const createResponse = await fetch(`${this.baseUrl}/api/v1/reactions`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(reactionConfig),
      });

      if (!createResponse.ok) {
        const error = await createResponse.text();
        throw new Error(`Failed to create reaction ${this.reactionId}: ${error}`);
      }

      await fetch(`${this.baseUrl}/api/v1/reactions/${this.reactionId}/start`, {
        method: 'POST',
      });
      return this.reactionEndpoint();
    }

    if (checkResponse.ok) {
      const reaction = await checkResponse.json();
      const payload = reaction.data ?? reaction;
      const config = payload?.config ?? payload;
      if ((payload?.status ?? config.status) !== 'running') {
        await fetch(`${this.baseUrl}/api/v1/reactions/${this.reactionId}/start`, {
          method: 'POST',
        });
      }
      // Prefer the live server properties, falling back to our configuration.
      const props = config?.properties || config || {};
      const host = props.host || this.reaction.host || 'localhost';
      const port = props.port || this.reaction.port;
      const path = props.ssePath || this.reaction.ssePath || '/events';
      return this.reactionEndpoint(host, port, path);
    }

    return this.reactionEndpoint();
  }

  /** Compute the public SSE endpoint URL. */
  private reactionEndpoint(host?: string, port?: number, path?: string): string {
    if (this.reaction.endpoint) {
      return this.reaction.endpoint;
    }
    const h = host || this.reaction.host || 'localhost';
    const p = port || this.reaction.port;
    const pa = path || this.reaction.ssePath || '/events';
    return `http://${h === '0.0.0.0' ? 'localhost' : h}:${p}${pa}`;
  }

  /**
   * Ensure a query exists on the Drasi Server (creating and starting it when
   * missing, or starting it when stopped).
   */
  private async ensureQuery(queryDef: QueryDefinition): Promise<void> {
    const checkResponse = await fetch(
      `${this.baseUrl}/api/v1/queries/${queryDef.id}?view=full`,
    );

    if (checkResponse.status === 404) {
      const queryConfig = {
        id: queryDef.id,
        query: queryDef.query,
        queryLanguage: queryDef.queryLanguage || 'Cypher',
        sources: queryDef.sources,
        joins: queryDef.joins ?? [],
        autoStart: true,
      };

      const createResponse = await fetch(`${this.baseUrl}/api/v1/queries`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(queryConfig),
      });

      if (!createResponse.ok) {
        const error = await createResponse.text();
        throw new Error(`Failed to create query ${queryDef.id}: ${error}`);
      }

      await fetch(`${this.baseUrl}/api/v1/queries/${queryDef.id}/start`, {
        method: 'POST',
      });
    } else if (checkResponse.ok) {
      const query = await checkResponse.json();
      const payload = query.data ?? query;
      const config = payload?.config ?? payload;
      if ((payload?.status ?? config.status) !== 'running') {
        await fetch(`${this.baseUrl}/api/v1/queries/${queryDef.id}/start`, {
          method: 'POST',
        });
      }
    }
  }

  /** Fetch a query's full configuration from the Drasi Server. */
  async getQueryConfig(queryId: string): Promise<Record<string, any> | null> {
    try {
      const response = await fetch(`${this.baseUrl}/api/v1/queries/${queryId}?view=full`);
      if (!response.ok) {
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

  /** Fetch the current results of a query from the REST API. */
  async getQueryResults(queryId: string): Promise<any[]> {
    try {
      const response = await fetch(`${this.baseUrl}/api/v1/queries/${queryId}/results`);
      if (!response.ok) {
        return [];
      }
      const json = await response.json();
      const data = json.data ?? json;
      return Array.isArray(data) ? data : [];
    } catch (error) {
      console.error(`Failed to get results for query ${queryId}:`, error);
      return [];
    }
  }

  /** Subscribe to real-time updates for a query. Returns an unsubscribe fn. */
  subscribe(queryId: string, callback: (result: QueryResult) => void): () => void {
    return this.sseClient.subscribe(queryId, callback);
  }

  /** Current connection status. */
  getConnectionStatus(): ConnectionStatus {
    return this.sseClient.getConnectionStatus();
  }

  /** Subscribe to connection status changes. Returns an unsubscribe fn. */
  onConnectionStatusChange(callback: (status: ConnectionStatus) => void): () => void {
    return this.sseClient.onConnectionStatusChange(callback);
  }

  /** URL of the Drasi Server UI for the discovered instance, if available. */
  getServerUiUrl(): string | null {
    if (!this.instanceId) return null;
    return `${this.baseUrl}/ui?instance=${encodeURIComponent(this.instanceId)}`;
  }

  /** Disconnect the shared SSE stream. */
  async disconnect(): Promise<void> {
    await this.sseClient.disconnect();
    this.initialized = false;
  }
}
