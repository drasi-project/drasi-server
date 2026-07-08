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

import { ConnectionStatus, QueryResult, RouteUnidentified } from './types';

const DEBUG_SSE =
  (globalThis as any)?.process?.env?.NODE_ENV === 'development';

/**
 * DrasiSSEClient maintains a single shared `EventSource` connection to a Drasi
 * SSE reaction endpoint and multiplexes the updates for every active query
 * across it. Components subscribe per query id and only receive the batches for
 * the query they care about.
 *
 * The client is intentionally application agnostic: it understands the wire
 * formats emitted by the Drasi SSE reaction (query-id keyed batches and
 * added/updated/deleted change sets) but contains no knowledge of any specific
 * data model. When a payload arrives without a query id, the optional
 * {@link RouteUnidentified} callback supplied by the host application decides
 * which query(s) the rows belong to.
 */
export class DrasiSSEClient {
  private eventSource: EventSource | null = null;
  private subscribers: Map<string, Set<(result: QueryResult) => void>> = new Map();
  private connectionStatus: ConnectionStatus = { connected: false };
  private statusListeners: Set<(status: ConnectionStatus) => void> = new Set();
  private reconnectAttempts = 0;
  private maxReconnectAttempts = 10;
  private reconnectDelay = 1000; // Start with 1 second
  private sseEndpoint: string | null = null;
  private queryCache: Map<string, QueryResult> = new Map();
  private routeUnidentified?: RouteUnidentified;

  constructor(routeUnidentified?: RouteUnidentified) {
    this.routeUnidentified = routeUnidentified;
  }

  /**
   * Connect to the Drasi reaction's SSE stream.
   *
   * @param queryIds The queries multiplexed over this connection.
   * @param sseEndpoint The SSE endpoint URL.
   * @param initialResults Optional REST-seeded results delivered once on open.
   */
  async connect(
    queryIds: string[],
    sseEndpoint: string,
    initialResults?: Record<string, any[]>,
  ): Promise<void> {
    return new Promise((resolve, reject) => {
      try {
        this.sseEndpoint = sseEndpoint;
        DEBUG_SSE && console.log(`Connecting to SSE endpoint: ${this.sseEndpoint}`);

        // Close existing connection if any
        if (this.eventSource) {
          this.eventSource.close();
        }

        // Create new EventSource connection
        this.eventSource = new EventSource(this.sseEndpoint);

        // Handle connection open
        this.eventSource.onopen = () => {
          DEBUG_SSE && console.log('SSE connection established');
          this.reconnectAttempts = 0;
          this.reconnectDelay = 1000;
          this.updateConnectionStatus({ connected: true });
          // Seed initial results if provided
          if (initialResults) {
            Object.entries(initialResults).forEach(([queryId, results]) => {
              this.handleQueryResult({
                queryId,
                data: results,
                timestamp: Date.now(),
              });
            });
          }
          resolve();
        };

        // Handle incoming messages
        this.eventSource.onmessage = (event) => {
          try {
            const data = JSON.parse(event.data);
            this.handleSSEMessage(data);
          } catch (error) {
            console.error('Failed to parse SSE message:', error, event.data);
          }
        };

        // Handle errors
        this.eventSource.onerror = (error) => {
          console.error('SSE connection error:', error);
          this.updateConnectionStatus({
            connected: false,
            error: 'SSE connection lost',
          });

          // Attempt reconnection with exponential backoff
          if (this.reconnectAttempts < this.maxReconnectAttempts) {
            this.reconnectAttempts++;
            const delay = Math.min(
              this.reconnectDelay * Math.pow(2, this.reconnectAttempts - 1),
              30000,
            );
            DEBUG_SSE &&
              console.log(
                `Reconnecting in ${delay}ms (attempt ${this.reconnectAttempts}/${this.maxReconnectAttempts})`,
              );

            setTimeout(() => {
              if (this.sseEndpoint) {
                this.connect(queryIds, this.sseEndpoint);
              }
            }, delay);
          } else {
            reject(new Error('Max reconnection attempts reached'));
          }
        };

        // Named events emitted by some reaction configurations
        this.eventSource.addEventListener('query-result', (event: MessageEvent) => {
          try {
            const data = JSON.parse(event.data);
            this.handleQueryResult(data);
          } catch (error) {
            console.error('Failed to parse query-result event:', error);
          }
        });

        this.eventSource.addEventListener('heartbeat', (event: MessageEvent) => {
          DEBUG_SSE && console.log('>>> Heartbeat event received:', event.data);
        });
      } catch (error) {
        console.error('Failed to create SSE connection:', error);
        reject(error);
      }
    });
  }

  /**
   * Interpret a parsed SSE payload and dispatch it to the right subscribers.
   */
  private handleSSEMessage(data: any) {
    // Heartbeat keep-alive messages
    if (data.type === 'heartbeat') {
      return;
    }

    // Streaming change-set format: { addedResults, updatedResults, deletedResults }.
    // These payloads do not carry a query id, so rows are routed by content.
    if (
      data.addedResults !== undefined ||
      data.updatedResults !== undefined ||
      data.deletedResults !== undefined
    ) {
      const allResults: any[] = [];

      if (Array.isArray(data.addedResults)) {
        for (const result of data.addedResults) {
          allResults.push(result.after || result);
        }
      }
      if (Array.isArray(data.updatedResults)) {
        for (const result of data.updatedResults) {
          allResults.push(result.after || result);
        }
      }
      if (Array.isArray(data.deletedResults)) {
        for (const result of data.deletedResults) {
          const item = result.before || result;
          allResults.push({ ...item, _deleted: true });
        }
      }

      if (allResults.length > 0) {
        this.routeContentBasedResults(allResults);
      }
      return;
    }

    // Drasi SSE reaction format keyed by `query_id` (snake_case).
    if (data.query_id) {
      this.handleKeyedBatch(data.query_id, data);
      return;
    }

    // Alternative format keyed by `queryId` (camelCase).
    if (data.queryId) {
      this.handleKeyedBatch(data.queryId, data);
      return;
    }

    // Single row pushed without a query id — route by content.
    if (data && typeof data === 'object' && Object.keys(data).length > 0) {
      this.routeContentBasedResults([data]);
    }
  }

  /**
   * Handle a batch of results that is explicitly keyed by a query id.
   */
  private handleKeyedBatch(queryId: string, data: any) {
    if (Array.isArray(data.results)) {
      const extractedData = data.results
        .map((result: any) => this.extractRow(result))
        .filter((item: any) => item != null);

      if (extractedData.length > 0) {
        this.handleQueryResult({
          queryId,
          data: extractedData,
          timestamp: data.timestamp ? new Date(data.timestamp).getTime() : Date.now(),
        });
      }
      return;
    }

    if (data.type && data.data) {
      this.handleQueryResult({
        queryId,
        data: [data.data],
        timestamp: data.timestamp ? new Date(data.timestamp).getTime() : Date.now(),
      });
      return;
    }

    if (data.data !== undefined) {
      this.handleQueryResult({
        queryId,
        data: Array.isArray(data.data) ? data.data : [data.data],
        timestamp: data.timestamp ? new Date(data.timestamp).getTime() : Date.now(),
      });
    }
  }

  /**
   * Normalize a single result entry from any of the supported change formats
   * (CDC `op`, aggregation before/after, typed add/update/delete) into a plain
   * row. Deleted rows are flagged with `_deleted: true`.
   */
  private extractRow(result: any): any {
    if (result == null || typeof result !== 'object') {
      return result;
    }

    // Aggregation results carry before/after snapshots.
    if (result.type === 'aggregation' && result.after) {
      return result.after;
    }
    // CDC delete (op: d, or op: u with no after).
    if (result.op === 'd' || (result.op === 'u' && !result.after)) {
      if (result.before) {
        return { ...result.before, _deleted: true };
      }
    }
    // CDC insert/read/update.
    if ((result.op === 'c' || result.op === 'r' || result.op === 'u') && result.after) {
      return result.after;
    }
    // Typed delete.
    if (result.type === 'delete' || result.type === 'DELETE') {
      const deleteData = result.before || result.data;
      if (deleteData) {
        return { ...deleteData, _deleted: true };
      }
    }
    // Typed add.
    if ((result.type === 'add' || result.type === 'ADD') && result.data) {
      return result.data;
    }
    // Typed update.
    if ((result.type === 'update' || result.type === 'UPDATE') && result.after) {
      return result.after;
    }
    if (result.data !== undefined) {
      return result.data;
    }
    return result;
  }

  /**
   * Route rows that arrived without a query id using the host-supplied routing
   * callback. When no callback is configured the rows are dropped with a
   * warning.
   */
  private routeContentBasedResults(rows: any[]) {
    if (this.routeUnidentified) {
      this.routeUnidentified(rows, (queryId, data) => this.deliverToQuery(queryId, data));
      return;
    }
    DEBUG_SSE &&
      console.warn(
        'Received results without a query id and no routeUnidentified handler is configured.',
        rows[0],
      );
  }

  /**
   * Deliver data to a specific query's subscribers.
   */
  private deliverToQuery(queryId: string, data: any[]) {
    this.handleQueryResult({ queryId, data, timestamp: Date.now() });
  }

  /**
   * Cache and dispatch a query result batch to its subscribers.
   */
  private handleQueryResult(result: QueryResult) {
    this.queryCache.set(result.queryId, result);

    const subscribers = this.subscribers.get(result.queryId);
    if (subscribers && subscribers.size > 0) {
      subscribers.forEach((callback) => {
        try {
          callback(result);
        } catch (error) {
          console.error(`Error in subscriber callback for ${result.queryId}:`, error);
        }
      });
    }
  }

  /**
   * Subscribe to a query's result batches. Returns an unsubscribe function.
   */
  subscribe(queryId: string, callback: (result: QueryResult) => void): () => void {
    if (!this.subscribers.has(queryId)) {
      this.subscribers.set(queryId, new Set());
    }
    this.subscribers.get(queryId)!.add(callback);

    // Deliver cached data (if any) immediately so late subscribers catch up.
    const cachedResult = this.queryCache.get(queryId);
    if (cachedResult) {
      setTimeout(() => {
        try {
          callback(cachedResult);
        } catch (error) {
          console.error(`Error delivering cached result for ${queryId}:`, error);
        }
      }, 0);
    }

    return () => {
      const callbacks = this.subscribers.get(queryId);
      if (callbacks) {
        callbacks.delete(callback);
        if (callbacks.size === 0) {
          this.subscribers.delete(queryId);
        }
      }
    };
  }

  /** Current connection status. */
  getConnectionStatus(): ConnectionStatus {
    return { ...this.connectionStatus };
  }

  /** Subscribe to connection status changes. Returns an unsubscribe function. */
  onConnectionStatusChange(callback: (status: ConnectionStatus) => void): () => void {
    this.statusListeners.add(callback);
    callback(this.connectionStatus);
    return () => {
      this.statusListeners.delete(callback);
    };
  }

  private updateConnectionStatus(status: ConnectionStatus) {
    this.connectionStatus = status;
    this.statusListeners.forEach((listener) => {
      try {
        listener(status);
      } catch (error) {
        console.error('Error in status listener:', error);
      }
    });
  }

  /** Disconnect from the SSE stream and clear all subscribers. */
  async disconnect(): Promise<void> {
    if (this.eventSource) {
      this.eventSource.close();
      this.eventSource = null;
    }
    this.updateConnectionStatus({ connected: false });
    this.subscribers.clear();
    this.statusListeners.clear();
  }

  /** Whether the shared connection is currently open. */
  isConnected(): boolean {
    return this.connectionStatus.connected;
  }
}
