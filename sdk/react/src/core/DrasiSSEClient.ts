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

import { QueryResult, ConnectionStatus, SSEClientConfig } from './types';

/**
 * SSE Client for consuming Drasi Server's SSE stream
 * Handles connection management, reconnection, and message routing
 */
export class DrasiSSEClient {
  private endpoint: string;
  private eventSource: EventSource | null = null;
  private subscribers: Map<string, Set<(result: QueryResult) => void>> = new Map();
  private connectionStatus: ConnectionStatus = { connected: false };
  private statusListeners: Set<(status: ConnectionStatus) => void> = new Set();
  private reconnectAttempts = 0;
  private maxReconnectAttempts = 10;
  private reconnectDelay = 1000;
  private queryCache: Map<string, QueryResult> = new Map();

  constructor(config: SSEClientConfig) {
    if (!config.endpoint) {
      throw new Error('[@drasi/react] endpoint is required in SSEClientConfig');
    }
    this.endpoint = config.endpoint;
  }

  /**
   * Connect to the SSE stream
   */
  async connect(queryIds: string[], initialResults?: Record<string, any[]>): Promise<void> {
    return new Promise((resolve, reject) => {
      try {
        console.log(`[@drasi/react] Connecting to SSE endpoint: ${this.endpoint}`);
        
        if (this.eventSource) {
          this.eventSource.close();
        }

        this.eventSource = new EventSource(this.endpoint);
        
        this.eventSource.onopen = () => {
          console.log('[@drasi/react] SSE connection established');
          this.reconnectAttempts = 0;
          this.reconnectDelay = 1000;
          this.updateConnectionStatus({ connected: true });
          
          if (initialResults) {
            Object.entries(initialResults).forEach(([queryId, results]) => {
              const qr: QueryResult = {
                queryId,
                data: results,
                timestamp: Date.now()
              };
              this.handleQueryResult(qr);
            });
          }
          resolve();
        };

        this.eventSource.onmessage = (event) => {
          try {
            const data = JSON.parse(event.data);
            this.handleSSEMessage(data);
          } catch (error) {
            console.error('[@drasi/react] Failed to parse SSE message:', error, event.data);
          }
        };

        this.eventSource.onerror = (error) => {
          console.error('[@drasi/react] SSE connection error:', error);
          this.updateConnectionStatus({ 
            connected: false, 
            error: 'SSE connection lost' 
          });

          if (this.reconnectAttempts < this.maxReconnectAttempts) {
            this.reconnectAttempts++;
            const delay = Math.min(this.reconnectDelay * Math.pow(2, this.reconnectAttempts - 1), 30000);
            console.log(`[@drasi/react] Reconnecting in ${delay}ms (attempt ${this.reconnectAttempts}/${this.maxReconnectAttempts})`);
            
            setTimeout(() => {
              this.connect(queryIds);
            }, delay);
          } else {
            reject(new Error('Max reconnection attempts reached'));
          }
        };

        this.eventSource.addEventListener('query-result', (event: MessageEvent) => {
          try {
            const data = JSON.parse(event.data);
            this.handleQueryResult(data);
          } catch (error) {
            console.error('[@drasi/react] Failed to parse query-result event:', error);
          }
        });

        this.eventSource.addEventListener('heartbeat', () => {
          // Keep connection alive
        });

      } catch (error) {
        console.error('[@drasi/react] Failed to create SSE connection:', error);
        reject(error);
      }
    });
  }

  /**
   * Handle incoming SSE messages from Drasi Server
   */
  private handleSSEMessage(data: any) {
    if (data.type === 'heartbeat') {
      return;
    }
    
    if (data.query_id) {
      const queryId = data.query_id;
      
      if (data.results && Array.isArray(data.results)) {
        const extractedData = data.results.map((result: any) => {
          return result.data || result;
        });
        
        this.handleQueryResult({
          queryId: queryId,
          data: extractedData,
          timestamp: data.timestamp ? new Date(data.timestamp).getTime() : Date.now()
        });
      } else if (data.type && data.data) {
        this.handleQueryResult({
          queryId: queryId,
          data: [data.data],
          timestamp: data.timestamp ? new Date(data.timestamp).getTime() : Date.now()
        });
      } else if (data.data && Array.isArray(data.data)) {
        this.handleQueryResult({
          queryId: queryId,
          data: data.data,
          timestamp: data.timestamp ? new Date(data.timestamp).getTime() : Date.now()
        });
      }
    } else if (data.queryId) {
      const queryId = data.queryId;
      
      if (data.results && Array.isArray(data.results)) {
        const extractedData = data.results.map((result: any) => {
          return result.data || result;
        });
        
        this.handleQueryResult({
          queryId: queryId,
          data: extractedData,
          timestamp: data.timestamp || Date.now()
        });
      } else if (data.data) {
        this.handleQueryResult({
          queryId: queryId,
          data: Array.isArray(data.data) ? data.data : [data.data],
          timestamp: data.timestamp || Date.now()
        });
      }
    }
  }

  /**
   * Handle a query result and deliver to subscribers
   */
  private handleQueryResult(result: QueryResult) {
    this.queryCache.set(result.queryId, result);
    
    const subscribers = this.subscribers.get(result.queryId);
    
    if (subscribers && subscribers.size > 0) {
      subscribers.forEach(callback => {
        try {
          callback(result);
        } catch (error) {
          console.error(`[@drasi/react] Error in subscriber callback for ${result.queryId}:`, error);
        }
      });
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
    
    const cachedResult = this.queryCache.get(queryId);
    if (cachedResult) {
      setTimeout(() => {
        try {
          callback(cachedResult);
        } catch (error) {
          console.error(`[@drasi/react] Error delivering cached result for ${queryId}:`, error);
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
    this.statusListeners.forEach(listener => {
      try {
        listener(status);
      } catch (error) {
        console.error('[@drasi/react] Error in status listener:', error);
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
    console.log('[@drasi/react] SSE client disconnected');
  }

  /**
   * Check if connected
   */
  isConnected(): boolean {
    return this.connectionStatus.connected;
  }
}
