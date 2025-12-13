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

import React, { createContext, useContext, ReactNode, useState, useEffect } from 'react';
import { DrasiSSEClient } from '../core/DrasiSSEClient';
import { SSEClientConfig } from '../core/types';

const SSEClientContext = createContext<DrasiSSEClient | null>(null);

interface SSEProviderProps {
  config: SSEClientConfig;
  queryIds?: string[];
  initialResults?: Record<string, any[]>;
  children: ReactNode;
}

/**
 * Provider for Drasi SSE client
 */
export function SSEProvider({ config, queryIds = [], initialResults, children }: SSEProviderProps) {
  const [client] = useState(() => new DrasiSSEClient(config));
  const [initialized, setInitialized] = useState(false);

  useEffect(() => {
    const init = async () => {
      try {
        await client.connect(queryIds, initialResults);
        setInitialized(true);
      } catch (err) {
        console.error('[@drasi/react] Failed to initialize SSE client:', err);
      }
    };
    init();

    return () => {
      client.disconnect();
    };
  }, [client, queryIds, initialResults]);

  if (!initialized) {
    return null;
  }

  return (
    <SSEClientContext.Provider value={client}>
      {children}
    </SSEClientContext.Provider>
  );
}

/**
 * Hook to access the SSE client
 */
export function useSSEClient(): DrasiSSEClient {
  const client = useContext(SSEClientContext);
  if (!client) {
    throw new Error('[@drasi/react] useSSEClient must be used within an SSEProvider');
  }
  return client;
}
