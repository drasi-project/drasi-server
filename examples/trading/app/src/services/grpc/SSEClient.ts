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

const DEBUG_SSE = process.env.NODE_ENV === 'development';

/**
 * SSE Client for consuming gRPC reaction's Server-Sent Events stream
 * This connects to the gRPC reaction's SSE endpoint to receive real-time updates
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
  private queryCache: Map<string, QueryResult> = new Map(); // Cache last result per query

  constructor() {
    // Endpoint will be provided dynamically by reaction creation
  }

  /**
   * Connect to the gRPC reaction's SSE stream
   */
  async connect(queryIds: string[], sseEndpoint: string, initialResults?: Record<string, any[]>): Promise<void> {
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
        
        // Log ALL events (for debugging)
        const originalAddEventListener = this.eventSource.addEventListener.bind(this.eventSource);
        this.eventSource.addEventListener = function(type: string, listener: any, options?: any) {
          DEBUG_SSE && console.log(`>>> EventSource listener added for type: ${type}`);
          return originalAddEventListener(type, listener, options);
        };

        // Handle connection open
        this.eventSource.onopen = () => {
          DEBUG_SSE && console.log('SSE connection established');
          this.reconnectAttempts = 0;
          this.reconnectDelay = 1000;
          this.updateConnectionStatus({ connected: true });
          // Seed initial results if provided
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

        // Handle incoming messages
        this.eventSource.onmessage = (event) => {
          DEBUG_SSE && console.log('>>> SSE Message received:', {
            data: event.data,
            type: event.type,
            lastEventId: event.lastEventId,
            origin: event.origin
          });
          
          try {
            const data = JSON.parse(event.data);
            DEBUG_SSE && console.log('>>> Parsed SSE data:', data);
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
            error: 'SSE connection lost' 
          });

          // Attempt reconnection with exponential backoff
          if (this.reconnectAttempts < this.maxReconnectAttempts) {
            this.reconnectAttempts++;
            const delay = Math.min(this.reconnectDelay * Math.pow(2, this.reconnectAttempts - 1), 30000);
            DEBUG_SSE && console.log(`Reconnecting in ${delay}ms (attempt ${this.reconnectAttempts}/${this.maxReconnectAttempts})`);
            
            setTimeout(() => {
              if (this.sseEndpoint) {
                this.connect(queryIds, this.sseEndpoint);
              }
            }, delay);
          } else {
            reject(new Error('Max reconnection attempts reached'));
          }
        };

        // Handle specific event types if the gRPC reaction sends them
        this.eventSource.addEventListener('query-result', (event: MessageEvent) => {
          try {
            const data = JSON.parse(event.data);
            this.handleQueryResult(data);
          } catch (error) {
            console.error('Failed to parse query-result event:', error);
          }
        });

        this.eventSource.addEventListener('heartbeat', (event: MessageEvent) => {
          // Keep connection alive
          DEBUG_SSE && console.log('>>> Heartbeat event received:', event.data);
        });

      } catch (error) {
        console.error('Failed to create SSE connection:', error);
        reject(error);
      }
    });
  }

  /**
   * Handle incoming SSE messages from Drasi Server SSE reaction
   */
  private handleSSEMessage(data: any) {
    DEBUG_SSE && console.log('>>> handleSSEMessage called with:', data);
    
    // Handle heartbeat messages
    if (data.type === 'heartbeat') {
      DEBUG_SSE && console.log('>>> Heartbeat received at:', data.ts || data.timestamp);
      return;
    }
    
    // Handle the addedResults/updatedResults/deletedResults format
    // This format is used for streaming changes from aggregation queries
    // Structure: { addedResults: [...], updatedResults: [...], deletedResults: [...], sequence: number }
    if (data.addedResults !== undefined || data.updatedResults !== undefined || data.deletedResults !== undefined) {
      DEBUG_SSE && console.log('>>> Detected addedResults/updatedResults/deletedResults format');
      
      const allResults: any[] = [];
      
      // Process added results
      if (data.addedResults && Array.isArray(data.addedResults)) {
        DEBUG_SSE && console.log(`>>> Processing ${data.addedResults.length} added results`);
        for (const result of data.addedResults) {
          // Extract the actual data - could be in 'after' or direct
          const item = result.after || result;
          allResults.push(item);
        }
      }
      
      // Process updated results
      if (data.updatedResults && Array.isArray(data.updatedResults)) {
        DEBUG_SSE && console.log(`>>> Processing ${data.updatedResults.length} updated results`);
        for (const result of data.updatedResults) {
          // Extract the actual data from 'after' field
          const item = result.after || result;
          allResults.push(item);
        }
      }
      
      // Process deleted results - mark them with _deleted flag
      if (data.deletedResults && Array.isArray(data.deletedResults)) {
        DEBUG_SSE && console.log(`>>> Processing ${data.deletedResults.length} deleted results`);
        for (const result of data.deletedResults) {
          // Extract data from 'before' field and mark as deleted
          const item = result.before || result;
          DEBUG_SSE && console.log(`>>> DELETE item:`, item);
          allResults.push({ ...item, _deleted: true });
        }
      }
      
      if (allResults.length > 0) {
        // Route to appropriate subscribers based on data content
        DEBUG_SSE && console.log(`>>> Routing ${allResults.length} results from addedResults/updatedResults/deletedResults format`);
        this.routeDataToSubscribers(allResults);
      }
      return;
    }
    
    // Drasi Server SSE reaction format:
    // The SSE reaction sends events with this structure:
    // { query_id: string, sequence: number, timestamp: string, results: [...] }
    // OR for changes:
    // { query_id: string, type: "ADD" | "UPDATE" | "DELETE", data: {...} }
    // OR for aggregations (from attach stream):
    // { query_id: string, results: [{ type: "aggregation", before: {...}, after: {...} }] }
    
    // Check for query_id (Drasi Server format)
    if (data.query_id) {
      const queryId = data.query_id;
      
      // Handle batch results (initial data or full refresh)
      if (data.results && Array.isArray(data.results)) {
        DEBUG_SSE && console.log(`>>> Found query_id: ${queryId} with ${data.results.length} results`);
        DEBUG_SSE && console.log(`>>> Raw results[0]:`, JSON.stringify(data.results[0], null, 2));
        
        // Extract the actual data from results
        const extractedData = data.results.map((result: any) => {
          // Debug: log op and after values
          if (result.op) {
            DEBUG_SSE && console.log(`>>> CDC event: op=${result.op}, after=${result.after}, hasAfter=${result.hasOwnProperty('after')}, afterIsNull=${result.after === null}`);
          }
          
          // Handle aggregation results (have type: "aggregation" with before/after)
          if (result.type === 'aggregation' && result.after) {
            DEBUG_SSE && console.log(`>>> Extracting aggregation after:`, result.after);
            return result.after;
          }
          // Handle CDC format DELETE (op: "d" or op: "u" with after: null/undefined)
          if (result.op === 'd' || (result.op === 'u' && !result.after)) {
            if (result.before) {
              DEBUG_SSE && console.log(`>>> CDC DELETE detected for:`, result.before);
              return { ...result.before, _deleted: true };
            }
          }
          // Handle CDC format INSERT/UPDATE
          if ((result.op === 'c' || result.op === 'r' || result.op === 'u') && result.after) {
            DEBUG_SSE && console.log(`>>> CDC ${result.op} detected:`, result.after);
            return result.after;
          }
          // Handle DELETE results - extract the key for deletion
          if ((result.type === 'delete' || result.type === 'DELETE')) {
            const deleteData = result.before || result.data;
            if (deleteData) {
              DEBUG_SSE && console.log(`>>> DELETE detected for:`, deleteData);
              // Mark as deletion by adding _deleted flag
              return { ...deleteData, _deleted: true };
            }
          }
          // Handle add/update results
          if ((result.type === 'add' || result.type === 'ADD') && result.data) {
            return result.data;
          }
          if ((result.type === 'update' || result.type === 'UPDATE') && result.after) {
            return result.after;
          }
          // If result has a data field, extract it
          if (result.data) {
            return result.data;
          }
          // Otherwise, the result itself is the data
          DEBUG_SSE && console.log(`>>> Returning result as-is:`, JSON.stringify(result, null, 2));
          return result;
        }).filter((item: any) => item != null);
        
        DEBUG_SSE && console.log(`>>> Extracted data[0]:`, JSON.stringify(extractedData[0], null, 2));
        
        if (extractedData.length > 0) {
          this.handleQueryResult({
            queryId: queryId,
            data: extractedData,
            timestamp: data.timestamp ? new Date(data.timestamp).getTime() : Date.now()
          });
        }
      }
      // Handle single change events
      else if (data.type && data.data) {
        DEBUG_SSE && console.log(`>>> Found change event for ${queryId}: ${data.type}`);
        
        // For now, treat all changes as full data updates
        // In the future, we could handle ADD/UPDATE/DELETE separately
        this.handleQueryResult({
          queryId: queryId,
          data: [data.data],
          timestamp: data.timestamp ? new Date(data.timestamp).getTime() : Date.now()
        });
      }
      // Handle data array directly
      else if (data.data && Array.isArray(data.data)) {
        DEBUG_SSE && console.log(`>>> Found query_id: ${queryId} with data array of ${data.data.length} items`);
        this.handleQueryResult({
          queryId: queryId,
          data: data.data,
          timestamp: data.timestamp ? new Date(data.timestamp).getTime() : Date.now()
        });
      }
    }
    // Alternative format with queryId (camelCase)
    else if (data.queryId) {
      const queryId = data.queryId;
      
      if (data.results && Array.isArray(data.results)) {
        DEBUG_SSE && console.log(`>>> Found queryId: ${queryId} with ${data.results.length} results`);
        
        const extractedData = data.results.map((result: any) => {
          // Handle aggregation results (have type: "aggregation" with before/after)
          if (result.type === 'aggregation' && result.after) {
            DEBUG_SSE && console.log(`>>> Extracting aggregation after:`, result.after);
            return result.after;
          }
          // Handle CDC format DELETE (op: "d" or op: "u" with after: null)
          if (result.op === 'd' || (result.op === 'u' && result.after === null)) {
            if (result.before) {
              DEBUG_SSE && console.log(`>>> CDC DELETE detected for:`, result.before);
              return { ...result.before, _deleted: true };
            }
          }
          // Handle CDC format INSERT/UPDATE
          if ((result.op === 'c' || result.op === 'r' || result.op === 'u') && result.after) {
            DEBUG_SSE && console.log(`>>> CDC ${result.op} detected:`, result.after);
            return result.after;
          }
          // Handle DELETE results - extract the key for deletion
          if (result.type === 'DELETE') {
            const deleteData = result.before || result.data;
            if (deleteData) {
              DEBUG_SSE && console.log(`>>> DELETE detected for:`, deleteData);
              // Mark as deletion by adding _deleted flag
              return { ...deleteData, _deleted: true };
            }
          }
          // Handle ADD results
          if (result.type === 'ADD' && result.data) {
            return result.data;
          }
          // Handle UPDATE results  
          if (result.type === 'UPDATE' && result.after) {
            return result.after;
          }
          // If result has a data field, extract it
          if (result.data) {
            return result.data;
          }
          return result;
        }).filter((item: any) => item != null);
        
        this.handleQueryResult({
          queryId: queryId,
          data: extractedData,
          timestamp: data.timestamp || Date.now()
        });
      } else if (data.data) {
        DEBUG_SSE && console.log(`>>> Found queryId: ${queryId} with data`);
        this.handleQueryResult({
          queryId: queryId,
          data: Array.isArray(data.data) ? data.data : [data.data],
          timestamp: data.timestamp || Date.now()
        });
      }
    }
    // If no query ID but has recognizable stock data structure
    else if (data.symbol && (data.price !== undefined || data.name !== undefined)) {
      DEBUG_SSE && console.log('>>> Detected stock data without query ID, routing to all subscribers');
      // This might be a direct data push, route to all subscribers
      this.routeDataToSubscribers([data]);
    }
    else {
      DEBUG_SSE && console.log('>>> Unrecognized SSE format:', Object.keys(data));
      DEBUG_SSE && console.log('>>> Full data:', JSON.stringify(data, null, 2));
    }
  }

  /**
   * Route data to appropriate subscribers based on content
   */
  private routeDataToSubscribers(data: any) {
    // This is a fallback mechanism when we can't determine the query ID
    // We'll try to match the data to the appropriate query based on content
    
    const dataArray = Array.isArray(data) ? data : [data];
    
    DEBUG_SSE && console.log(`>>> routeDataToSubscribers with ${dataArray.length} items, first item:`, dataArray[0]);
    
    // Check if data looks like portfolio summary (has total_value, total_cost, position_count)
    if (dataArray[0]?.total_value !== undefined && dataArray[0]?.total_cost !== undefined) {
      DEBUG_SSE && console.log(`>>> Routing to portfolio-summary-query`);
      this.deliverToQuery('portfolio-summary-query', dataArray);
    }
    // Check if data looks like limit order data (has order_type and target_price)
    else if (dataArray[0]?.order_type !== undefined && dataArray[0]?.target_price !== undefined) {
      DEBUG_SSE && console.log(`>>> Routing to active-orders-query`);
      this.deliverToQuery('active-orders-query', dataArray);
    }
    // Check if data looks like portfolio data
    // Full portfolio data has quantity, purchase_price
    // Delete events may only have id (the portfolio table's primary key)
    else if (dataArray[0]?.id !== undefined || 
        (dataArray[0]?.quantity !== undefined && dataArray[0]?.purchase_price !== undefined)) {
      DEBUG_SSE && console.log(`>>> Routing to portfolio-query`);
      this.deliverToQuery('portfolio-query', dataArray);
    }
    // Check if data looks like sector performance (has sector, stockCount or avgChangePercent)
    else if (dataArray[0]?.sector !== undefined && (dataArray[0]?.stockCount !== undefined || dataArray[0]?.avgChangePercent !== undefined)) {
      this.deliverToQuery('sector-performance-query', dataArray);
    }
    // Check if data looks like watchlist data (has watchlist_id)
    else if (dataArray[0]?.watchlist_id !== undefined) {
      DEBUG_SSE && console.log(`>>> Routing to watchlist-query`);
      this.deliverToQuery('watchlist-query', dataArray);
    }
    // Default stock data (has symbol and price)
    else if (dataArray[0]?.symbol !== undefined && dataArray[0]?.price !== undefined) {
      // Could be watchlist, gainers, losers, etc.
      // Deliver to all stock-related queries
      ['watchlist-query', 'top-gainers-query', 'top-losers-query', 'high-volume-query', 'price-ticker-query', 'price-screener-query'].forEach(queryId => {
        this.deliverToQuery(queryId, dataArray);
      });
    }
    else {
      // Unknown data format, log for debugging
      console.warn('Unable to route data to specific query, data structure:', dataArray[0]);
    }
  }
  
  /**
   * Deliver data to a specific query's subscribers
   */
  private deliverToQuery(queryId: string, data: any[]) {
    const callbacks = this.subscribers.get(queryId);
    if (callbacks && callbacks.size > 0) {
      const result: QueryResult = {
        queryId,
        data,
        timestamp: Date.now()
      };
      
      callbacks.forEach(callback => {
        try {
          callback(result);
        } catch (error) {
          console.error(`Error in subscriber callback for ${queryId}:`, error);
        }
      });
    }
  }

  /**
   * Handle a query result
   */
  private handleQueryResult(result: QueryResult) {
    DEBUG_SSE && console.log(`>>> handleQueryResult called for queryId: ${result.queryId}`);
    DEBUG_SSE && console.log(`>>> Result data:`, result.data);
    DEBUG_SSE && console.log(`>>> Current subscribers:`, Array.from(this.subscribers.keys()));
    
    // Cache the result
    this.queryCache.set(result.queryId, result);
    
    const subscribers = this.subscribers.get(result.queryId);
    
    if (subscribers && subscribers.size > 0) {
      DEBUG_SSE && console.log(`>>> Delivering to ${subscribers.size} subscribers for ${result.queryId}`);
      subscribers.forEach(callback => {
        try {
          callback(result);
          DEBUG_SSE && console.log(`>>> Successfully delivered result to subscriber for ${result.queryId}`);
        } catch (error) {
          console.error(`Error in subscriber callback for ${result.queryId}:`, error);
        }
      });
    } else {
      DEBUG_SSE && console.log(`>>> WARNING: No subscribers for query ${result.queryId}, caching for later`);
      DEBUG_SSE && console.log(`>>> Available subscriptions:`, this.subscribers);
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
    DEBUG_SSE && console.log(`Subscribed to query ${queryId} (${this.subscribers.get(queryId)!.size} subscribers)`);
    
    // If we have cached data for this query, deliver it immediately
    const cachedResult = this.queryCache.get(queryId);
    if (cachedResult) {
      DEBUG_SSE && console.log(`Delivering cached result for ${queryId}`);
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
        DEBUG_SSE && console.log(`Unsubscribed from query ${queryId} (${callbacks.size} subscribers remaining)`);
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
    this.statusListeners.forEach(listener => {
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
    DEBUG_SSE && console.log('SSE client disconnected');
  }

  /**
   * Check if connected
   */
  isConnected(): boolean {
    return this.connectionStatus.connected;
  }
}