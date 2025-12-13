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
import { DrasiRestClient } from '../core/DrasiRestClient';
import { RestClientConfig, QueryDefinition } from '../core/types';

const RestClientContext = createContext<DrasiRestClient | null>(null);

interface RestProviderProps {
  config: RestClientConfig;
  queries?: QueryDefinition[];
  children: ReactNode;
}

/**
 * Provider for Drasi REST API client
 */
export function RestProvider({ config, queries, children }: RestProviderProps) {
  const [client] = useState(() => new DrasiRestClient(config));
  const [initialized, setInitialized] = useState(false);

  useEffect(() => {
    const init = async () => {
      try {
        const healthy = await client.checkHealth();
        if (!healthy) {
          throw new Error('REST API health check failed');
        }

        if (queries) {
          for (const query of queries) {
            await client.createQuery(query);
            await client.startQuery(query.id);
          }
        }

        setInitialized(true);
      } catch (err) {
        console.error('[@drasi/react] Failed to initialize REST client:', err);
      }
    };
    init();
  }, [client, queries]);

  if (!initialized) {
    return null;
  }

  return (
    <RestClientContext.Provider value={client}>
      {children}
    </RestClientContext.Provider>
  );
}

/**
 * Hook to access the REST client
 */
export function useRestClient(): DrasiRestClient {
  const client = useContext(RestClientContext);
  if (!client) {
    throw new Error('[@drasi/react] useRestClient must be used within a RestProvider');
  }
  return client;
}
