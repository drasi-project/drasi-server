// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { vi } from 'vitest';
import type { DrasiClientOptions } from '../src/client/DrasiClient';
import type { Component, ComponentStatus, QueryConfig } from '../src/client/types';

export const refs: DrasiClientOptions = {
  serverUrl: 'https://drasi.invalid',
  instanceId: 'selected / instance',
  queryIds: ['stocks'],
  reaction: { id: 'stream', endpoint: 'https://events.invalid/proxy/events' },
  reconnect: { maxReconnectAttempts: 0, initialReconnectDelayMs: 10, maxReconnectDelayMs: 20 },
};

export const json = (data: unknown) => new Response(JSON.stringify({ success: true, data, error: null }));
export const failure = (status: number, code = 'INTERNAL_ERROR') =>
  new Response(JSON.stringify({ code, message: 'Sensitive server detail must not escape' }), { status });

export function queryConfig(id: string, overrides: Partial<QueryConfig> = {}): QueryConfig {
  return {
    id, query: 'MATCH (n) RETURN n', queryLanguage: 'Cypher', sources: [],
    autoStart: true, middleware: [], enableBootstrap: true,
    bootstrapBufferSize: 10000, outboxCapacity: 1000, bootstrapTimeoutSecs: 300,
    ...overrides,
  };
}

export function component<T>(
  kind: 'queries' | 'reactions', id: string, config: T,
  status: ComponentStatus = 'Running', instanceId = refs.instanceId,
): Component<T> {
  const self = `/api/v1/instances/${encodeURIComponent(instanceId)}/${kind}/${encodeURIComponent(id)}`;
  return { id, status, config, links: { self, full: `${self}?view=full` } };
}

export function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>(done => { resolve = done; });
  return { promise, resolve };
}

export class ReadServer {
  queryStatus: ComponentStatus = 'Running';
  reactionStatus: ComponentStatus = 'Running';
  missing: 'query' | 'reaction' | 'instance' | null = null;
  reaction = {
    id: 'stream', kind: 'sse', queries: ['stocks'], host: '0.0.0.0',
    port: 9999, ssePath: '/internal', heartbeatIntervalMs: 15000,
  };
  snapshot = () => Promise.resolve(json([{ id: 'A', value: '10', price: 10 }]));
  fetch = vi.fn(async (input: RequestInfo | URL, init?: RequestInit): Promise<Response> => {
    init?.signal?.throwIfAborted();
    const url = new URL(String(input));
    const prefix = `/api/v1/instances/${encodeURIComponent(refs.instanceId)}/`;
    if (!url.pathname.startsWith(prefix)) throw new Error(`Unscoped request: ${url}`);
    if (this.missing === 'instance') return failure(404, 'INSTANCE_NOT_FOUND');
    if (url.pathname.includes('/queries/')) {
      if (this.missing === 'query') return failure(404, 'QUERY_NOT_FOUND');
      if (url.pathname.endsWith('/results')) return this.snapshot();
      const id = decodeURIComponent(url.pathname.split('/').pop()!);
      return json(component('queries', id, queryConfig(id), this.queryStatus));
    }
    if (url.pathname.endsWith('/reactions/stream')) {
      return this.missing === 'reaction' ? failure(404, 'REACTION_NOT_FOUND')
        : json(component('reactions', 'stream', this.reaction, this.reactionStatus));
    }
    throw new Error(`Unexpected request: ${url}`);
  });
}
