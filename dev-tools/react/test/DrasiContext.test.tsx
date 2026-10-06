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

import React from 'react';
import { render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import {
  DrasiProvider,
  useDrasiQuery,
} from '../src/react/DrasiContext';
import { fakeEventSourceFactory } from './FakeEventSource';

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });
}

function QueryProbe(): React.ReactElement {
  const { data, loading, error } = useDrasiQuery<{
    id: string;
    value: number;
  }>('stocks', {
    getKey: (row) => row.id,
    transform: (row) => ({
      id: row.id,
      value: Number(row.value),
    }),
  });

  return (
    <div>
      <span data-testid="loading">{String(loading)}</span>
      <span data-testid="error">{error ?? ''}</span>
      <span data-testid="data">{JSON.stringify(data)}</span>
    </div>
  );
}

describe('DrasiProvider and useDrasiQuery', () => {
  it('propagates initialization failures instead of loading forever', async () => {
    const consoleError = vi.spyOn(console, 'error').mockImplementation(() => {});
    const fetcher = vi.fn(async () =>
      jsonResponse({ message: 'unhealthy' }, 503),
    );

    render(
      <DrasiProvider
        queries={[]}
        reaction={{ port: 8281 }}
        fetch={fetcher as typeof fetch}
      >
        <QueryProbe />
      </DrasiProvider>,
    );

    await waitFor(() =>
      expect(screen.getByTestId('loading').textContent).toBe('false'),
    );
    expect(screen.getByTestId('error').textContent).toContain(
      'health check failed (503)',
    );
    expect(consoleError).toHaveBeenCalledWith(
      'Failed to initialize Drasi client:',
      expect.any(Error),
    );
  });

  it('preserves delete markers through transforms', async () => {
    const factory = fakeEventSourceFactory();
    const fetcher = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.endsWith('/health')) return jsonResponse({});
      if (url.endsWith('/api/v1/instances')) return jsonResponse([]);
      if (url.includes('/api/v1/queries/stocks?view=full')) {
        return jsonResponse({ status: 'running', config: {} });
      }
      if (url.includes('/api/v1/reactions/stream?view=full')) {
        return jsonResponse({ status: 'running', config: {} });
      }
      if (url.endsWith('/api/v1/queries/stocks/results')) {
        return jsonResponse([{ id: 'A', value: '10' }]);
      }
      throw new Error(`Unexpected request: ${url}`);
    });

    render(
      <DrasiProvider
        queries={[
          {
            id: 'stocks',
            query: 'MATCH (n) RETURN n',
            sources: [],
          },
        ]}
        reaction={{
          id: 'stream',
          port: 8281,
          endpoint: 'http://localhost:8281/events',
        }}
        fetch={fetcher as typeof fetch}
        eventSourceFactory={factory.create}
      >
        <QueryProbe />
      </DrasiProvider>,
    );

    await waitFor(() => expect(factory.instances).toHaveLength(1));
    factory.instances[0].open();
    await waitFor(() =>
      expect(screen.getByTestId('data').textContent).toBe(
        '[{"id":"A","value":10}]',
      ),
    );

    factory.instances[0].message({
      queryId: 'stocks',
      data: { id: 'A', value: '10', _deleted: true },
    });
    await waitFor(() =>
      expect(screen.getByTestId('data').textContent).toBe('[]'),
    );
  });

  it('closes the EventSource when StrictMode effects are cleaned up', async () => {
    const factory = fakeEventSourceFactory();
    const fetcher = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.endsWith('/health')) return jsonResponse({});
      if (url.endsWith('/api/v1/instances')) return jsonResponse([]);
      if (url.includes('/api/v1/queries/stocks?view=full')) {
        return jsonResponse({ status: 'running', config: {} });
      }
      if (url.includes('/api/v1/reactions/stream?view=full')) {
        return jsonResponse({ status: 'running', config: {} });
      }
      if (url.endsWith('/api/v1/queries/stocks/results')) {
        return jsonResponse([]);
      }
      throw new Error(`Unexpected request: ${url}`);
    });

    const rendered = render(
      <React.StrictMode>
        <DrasiProvider
          queries={[
            {
              id: 'stocks',
              query: 'MATCH (n) RETURN n',
              sources: [],
            },
          ]}
          reaction={{
            id: 'stream',
            port: 8281,
            endpoint: 'http://localhost:8281/events',
          }}
          fetch={fetcher as typeof fetch}
          eventSourceFactory={factory.create}
        >
          <QueryProbe />
        </DrasiProvider>
      </React.StrictMode>,
    );

    await waitFor(() => expect(factory.instances.length).toBeGreaterThan(0));
    factory.instances[factory.instances.length - 1]?.open();
    rendered.unmount();

    expect(factory.instances.every((source) => source.closed)).toBe(true);
  });
});
