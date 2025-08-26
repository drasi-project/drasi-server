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

import { QueryResult } from '@/types';

/**
 * Simplified client that polls Drasi REST API for query results
 * In production, this would use WebSockets or SSE for real-time streaming
 */
export class DrasiGrpcClient {
  private baseUrl: string;
  private subscribers: Map<string, Set<(result: QueryResult) => void>> = new Map();
  private connectionStatus: { connected: boolean; error?: string } = { connected: false };
  private pollIntervals: Map<string, NodeJS.Timeout> = new Map();

  constructor(baseUrl: string = 'http://localhost:8080') {
    // Use REST API base URL, not gRPC port
    this.baseUrl = baseUrl;
  }

  /**
   * Connect and start polling for updates
   * Note: This is a temporary solution. In production, use WebSockets or SSE
   */
  async connect(queryIds: string[]): Promise<void> {
    try {
      // Test connection to Drasi Server
      const response = await fetch(`${this.baseUrl}/health`);
      if (!response.ok) {
        throw new Error('Drasi Server is not accessible');
      }

      this.connectionStatus = { connected: true };
      console.log('Connected to Drasi Server REST API');

      // Start polling for each query
      for (const queryId of queryIds) {
        this.startPolling(queryId);
      }

    } catch (error) {
      console.error('Failed to connect to Drasi:', error);
      this.connectionStatus = { connected: false, error: String(error) };
      
      // Retry connection after delay
      setTimeout(() => this.connect(queryIds), 5000);
    }
  }

  private startPolling(queryId: string) {
    // Clear existing interval if any
    if (this.pollIntervals.has(queryId)) {
      clearInterval(this.pollIntervals.get(queryId)!);
    }

    // Poll for updates every 2 seconds
    const interval = setInterval(async () => {
      try {
        const response = await fetch(`${this.baseUrl}/queries/${queryId}/results`);
        if (response.ok) {
          const data = await response.json();
          this.handleQueryResult({
            queryId,
            data,
            timestamp: Date.now()
          });
        }
      } catch (error) {
        console.error(`Failed to poll query ${queryId}:`, error);
      }
    }, 2000);

    this.pollIntervals.set(queryId, interval);
  }

  private handleQueryResult(result: QueryResult) {
    const subscribers = this.subscribers.get(result.queryId);
    
    if (subscribers && subscribers.size > 0) {
      subscribers.forEach(callback => callback(result));
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

    // If not already polling this query, start
    if (!this.pollIntervals.has(queryId)) {
      this.startPolling(queryId);
    }
    
    // Return unsubscribe function
    return () => {
      const callbacks = this.subscribers.get(queryId);
      if (callbacks) {
        callbacks.delete(callback);
        if (callbacks.size === 0) {
          this.subscribers.delete(queryId);
          // Stop polling if no more subscribers
          if (this.pollIntervals.has(queryId)) {
            clearInterval(this.pollIntervals.get(queryId)!);
            this.pollIntervals.delete(queryId);
          }
        }
      }
    };
  }

  /**
   * Disconnect from Drasi
   */
  disconnect() {
    // Clear all polling intervals
    for (const interval of this.pollIntervals.values()) {
      clearInterval(interval);
    }
    this.pollIntervals.clear();
    
    this.connectionStatus = { connected: false };
    this.subscribers.clear();
  }

  getConnectionStatus() {
    return { ...this.connectionStatus };
  }
}