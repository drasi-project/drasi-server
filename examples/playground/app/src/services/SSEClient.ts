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

/**
 * SSE Client for consuming Drasi Server's SSE reaction stream
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

  /**
   * Connect to the SSE stream
   */
  async connect(queryIds: string[], sseEndpoint: string, initialResults?: Record<string, any[]>): Promise<void> {
    return new Promise((resolve, reject) => {
      try {
        this.sseEndpoint = sseEndpoint;
        console.log(`Connecting to SSE endpoint: ${this.sseEndpoint}`);

        // Close existing connection if any
        if (this.eventSource) {
          this.eventSource.close();
        }

        // Create new EventSource connection
        this.eventSource = new EventSource(this.sseEndpoint);

        // Handle connection open
        this.eventSource.onopen = () => {
          console.log('SSE connection established');
          this.reconnectAttempts = 0;
          this.reconnectDelay = 1000;
          this.updateConnectionStatus({ connected: true });

          // Seed initial results if provided
          if (initialResults) {
            Object.entries(initialResults).forEach(([queryId, results]) => {
              const qr: QueryResult = {
                query_id: queryId,
                results: results.map(data => ({ data })),
              };
              this.handleQueryResult(qr);
            });
          }

          resolve();
        };

        // Handle incoming messages
        this.eventSource.onmessage = (event) => {
          console.log('SSE Message received:', event.data);

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
              30000
            );
            console.log(
              `Reconnecting in ${delay}ms (attempt ${this.reconnectAttempts}/${this.maxReconnectAttempts})`
            );

            this.updateConnectionStatus({
              connected: false,
              reconnecting: true,
            });

            setTimeout(() => {
              if (this.sseEndpoint) {
                this.connect(queryIds, this.sseEndpoint);
              }
            }, delay);
          } else {
            reject(new Error('Max reconnection attempts reached'));
          }
        };

        // Handle heartbeat events
        this.eventSource.addEventListener('heartbeat', (event: MessageEvent) => {
          console.log('Heartbeat received');
        });
      } catch (error) {
        console.error('Failed to create SSE connection:', error);
        reject(error);
      }
    });
  }

  /**
   * Handle incoming SSE messages
   */
  private handleSSEMessage(data: any) {
    // Handle heartbeat messages
    if (data.type === 'heartbeat') {
      return;
    }

    // Drasi Server SSE reaction format:
    // { queryId: string, timestamp: number, results: [{type: "ADD"|"UPDATE"|"DELETE", data: {...}}] }
    // OR legacy format: { query_id: string, sequence: number, timestamp: string, results: [...] }
    if (data.queryId || data.query_id) {
      // Normalize to query_id for internal handling
      const normalized = {
        ...data,
        query_id: data.queryId || data.query_id
      };
      this.handleQueryResult(normalized);
    } else {
      console.warn('Unrecognized SSE message format:', data);
    }
  }

  /**
   * Handle a query result
   */
  private handleQueryResult(result: QueryResult) {
    console.log(`Received result for query: ${result.query_id}`);

    // Cache the result
    this.queryCache.set(result.query_id, result);

    const subscribers = this.subscribers.get(result.query_id);

    if (subscribers && subscribers.size > 0) {
      console.log(`Delivering to ${subscribers.size} subscribers for ${result.query_id}`);
      subscribers.forEach((callback) => {
        try {
          callback(result);
        } catch (error) {
          console.error(`Error in subscriber callback for ${result.query_id}:`, error);
        }
      });
    } else {
      console.log(`No subscribers for query ${result.query_id}, caching for later`);
    }
  }

  /**
   * Subscribe to query results
   */
  subscribe(queryId: string, callback: (result: QueryResult) => void): () => void {
    if (!this.subscribers.has(queryId)) {
      this.subscribers.set(queryId, new Set());
    }

    this.subscribers.get(queryId)!.add(callback);
    console.log(`Subscribed to query ${queryId} (${this.subscribers.get(queryId)!.size} subscribers)`);

    // If we have cached data for this query, deliver it immediately
    const cachedResult = this.queryCache.get(queryId);
    if (cachedResult) {
      console.log(`Delivering cached result for ${queryId}`);
      setTimeout(() => {
        try {
          callback(cachedResult);
        } catch (error) {
          console.error(`Error delivering cached result for ${queryId}:`, error);
        }
      }, 0);
    }

    // Return unsubscribe function
    return () => {
      const callbacks = this.subscribers.get(queryId);
      if (callbacks) {
        callbacks.delete(callback);
        console.log(`Unsubscribed from query ${queryId} (${callbacks.size} subscribers remaining)`);
        if (callbacks.size === 0) {
          this.subscribers.delete(queryId);
        }
      }
    };
  }

  /**
   * Get current connection status
   */
  getConnectionStatus(): ConnectionStatus {
    return { ...this.connectionStatus };
  }

  /**
   * Subscribe to connection status changes
   */
  onConnectionStatusChange(callback: (status: ConnectionStatus) => void): () => void {
    this.statusListeners.add(callback);
    // Immediately call with current status
    callback(this.connectionStatus);

    return () => {
      this.statusListeners.delete(callback);
    };
  }

  /**
   * Update connection status and notify listeners
   */
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

  /**
   * Disconnect from SSE stream
   */
  async disconnect(): Promise<void> {
    if (this.eventSource) {
      this.eventSource.close();
      this.eventSource = null;
    }

    this.updateConnectionStatus({ connected: false });
    this.subscribers.clear();
    this.statusListeners.clear();
    console.log('SSE client disconnected');
  }

  /**
   * Check if connected
   */
  isConnected(): boolean {
    return this.connectionStatus.connected;
  }
}
