// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { vi } from 'vitest';
import type { DrasiClientOptions } from '../src/client/DrasiClient';
import type { Component, ComponentStatus, QueryConfig, ReactionConfig } from '../src/client/types';

export const ids = {
  stocks: 'b20c42ee-59a1-441f-a549-87a72be1f1e9',
  other: 'e4ba7cee-4b9b-4b24-ad4e-a58f0e04d481',
  stream: 'ee84d7f5-adf8-4f1f-95e7-b67a67f39a0d',
};

export const refs: DrasiClientOptions = {
  serverUrl: 'https://drasi.invalid',
  instanceId: 'selected / instance',
  outputIds: [ids.stocks],
  reaction: { id: ids.stream, endpoint: 'https://events.invalid/proxy/events' },
  reconnect: { maxReconnectAttempts: 0, initialReconnectDelayMs: 10, maxReconnectDelayMs: 20 },
};

export const json = (data: unknown) => new Response(JSON.stringify({ success: true, data, error: null }));
export const failure = (status: number, code = 'INTERNAL_ERROR') =>
  new Response(JSON.stringify({ code, message: 'Sensitive server detail must not escape' }), { status });

export function queryConfig(id: string, overrides: Partial<QueryConfig> = {}): QueryConfig {
  return {
    id, name: `component-${id}`, query: 'MATCH (n) RETURN n', queryLanguage: 'Cypher', inputs: [],
    autoStart: true, middleware: [], enableBootstrap: true,
    outboxCapacity: 1000, bootstrapTimeoutSecs: 300,
    ...overrides,
  };
}

export function component<T extends { name: string }>(
  kind: 'queries' | 'reactions', id: string, config: T,
  status: ComponentStatus = 'Running', instanceId = refs.instanceId,
): Component<T> {
  const self = `/api/v1/instances/${encodeURIComponent(instanceId)}/${kind}/${encodeURIComponent(id)}`;
  return { id, name: config.name, status, config, links: { self, full: `${self}?view=full` } };
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
  reaction: { -readonly [K in keyof ReactionConfig]: ReactionConfig[K] } = {
    id: ids.stream, name: 'events', kind: 'sse', queries: [{ id: ids.stocks }], host: '0.0.0.0',
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
    if (url.pathname.endsWith(`/reactions/${ids.stream}`)) {
      return this.missing === 'reaction' ? failure(404, 'REACTION_NOT_FOUND')
        : json(component('reactions', ids.stream, this.reaction, this.reactionStatus));
    }
    throw new Error(`Unexpected request: ${url}`);
  });
}
