// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { DrasiClient, DrasiError, type Component, type QueryConfig } from '@drasi/react';
import { TRADING_QUERIES, TRADING_QUERY_IDS, TRADING_REACTION } from './config';

const SETUP_TIMEOUT_MS = 60000;
const POLL_MS = 200;
const isRecord = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);

/** Only positively identified, known Trading resources are eligible for setup. */
export function canPrepareTrading(error: unknown, instanceId: string): error is DrasiError {
  return error instanceof DrasiError && error.instanceId === instanceId &&
    ['QUERY_NOT_FOUND', 'REACTION_NOT_FOUND', 'RESOURCE_STOPPED', 'RESOURCE_STARTING'].includes(error.code) &&
    ((error.resourceKind === 'query' && TRADING_QUERY_IDS.includes(error.resourceId ?? '')) ||
      (error.resourceKind === 'reaction' && error.resourceId === TRADING_REACTION.id));
}

async function request(
  fetcher: typeof fetch, url: string, signal: AbortSignal, init: RequestInit = {}, instanceId?: string,
): Promise<{ status: number; data: unknown }> {
  const controller = new AbortController();
  const abort = () => controller.abort();
  const timer = setTimeout(abort, 10000);
  signal.addEventListener('abort', abort, { once: true });
  try {
    signal.throwIfAborted();
    const response = await fetcher(url, { ...init, signal: controller.signal });
    const details = { instanceId, status: response.status };
    if (response.status === 409 && init.method === 'POST') return { status: 409, data: null };
    if (!response.ok) {
      throw new DrasiError(response.status === 401 ? 'UNAUTHENTICATED'
        : response.status === 403 ? 'FORBIDDEN'
          : response.status >= 500 ? 'SERVER_UNAVAILABLE' : 'INCOMPATIBLE_RESOURCE', details);
    }
    const body: unknown = await response.json();
    if (!isRecord(body) || body.success !== true || !('data' in body)) {
      throw new DrasiError('INVALID_PAYLOAD', details);
    }
    controller.signal.throwIfAborted();
    return { status: response.status, data: body.data };
  } catch (error) {
    signal.throwIfAborted();
    if (error instanceof DrasiError) throw error;
    throw new DrasiError(error instanceof SyntaxError ? 'INVALID_PAYLOAD' : 'SERVER_UNAVAILABLE', { instanceId });
  } finally {
    clearTimeout(timer);
    signal.removeEventListener('abort', abort);
  }
}

/** Preserve the one-instance demo without making the reusable package guess. */
export async function resolveTradingInstance(
  serverUrl: string, fetcher: typeof fetch, signal: AbortSignal, configuredId?: string,
): Promise<string> {
  if (configuredId !== undefined) {
    if (!configuredId.trim()) throw new DrasiError('INVALID_CONFIGURATION');
    return configuredId;
  }
  const { data } = await request(fetcher, `${serverUrl}/api/v1/instances`, signal);
  if (!Array.isArray(data) || !data.every(item => isRecord(item) && typeof item.id === 'string' && item.id)) {
    throw new DrasiError('INVALID_PAYLOAD');
  }
  if (data.length === 0) throw new DrasiError('INSTANCE_NOT_FOUND', { resourceKind: 'instance' });
  if (data.length !== 1) throw new DrasiError('INVALID_CONFIGURATION');
  return data[0].id;
}

function queryContract(config: QueryConfig): string {
  return JSON.stringify({
    query: config.query.trim(),
    language: config.queryLanguage,
    sources: config.sources.map(source => ({
      sourceId: source.sourceId, pipeline: source.pipeline ?? [],
      nodes: source.nodes ?? [], relations: source.relations ?? [],
    })),
    joins: (config.joins ?? []).map(join => ({
      id: join.id, keys: join.keys.map(key => ({ label: key.label, property: key.property })),
    })),
  });
}

function delay(signal: AbortSignal): Promise<void> {
  return new Promise((resolve, reject) => {
    signal.throwIfAborted();
    const done = () => { signal.removeEventListener('abort', abort); resolve(); };
    const timer = setTimeout(done, POLL_MS);
    const abort = () => {
      clearTimeout(timer);
      signal.removeEventListener('abort', abort);
      reject(signal.reason);
    };
    signal.addEventListener('abort', abort, { once: true });
  });
}

interface SetupOptions {
  client: DrasiClient;
  serverUrl: string;
  fetch: typeof fetch;
}

async function prepare({ client, serverUrl, fetch: fetcher }: SetupOptions, signal: AbortSignal): Promise<void> {
  const instanceId = client.instanceId;
  const details = (kind: 'query' | 'reaction', id: string) => ({ instanceId, resourceKind: kind, resourceId: id });
  const url = (kind: 'query' | 'reaction', id?: string, start = false) =>
    `${serverUrl}/api/v1/instances/${encodeURIComponent(instanceId)}/${kind === 'query' ? 'queries' : 'reactions'}` +
      (id === undefined ? '' : `/${encodeURIComponent(id)}${start ? '/start' : ''}`);
  const checkState = (component: Component<unknown>, kind: 'query' | 'reaction') => {
    if (!['Running', 'Starting', 'Reconfiguring', 'Stopped', 'Added'].includes(component.status)) {
      throw new DrasiError('RESOURCE_UNAVAILABLE', { ...details(kind, component.id), resourceStatus: component.status });
    }
  };
  const readQuery = async (definition: QueryConfig) => {
    const component = await client.getQuery(definition.id, signal);
    if (queryContract(component.config) !== queryContract(definition)) {
      throw new DrasiError('INCOMPATIBLE_RESOURCE', details('query', definition.id));
    }
    checkState(component, 'query');
    return component;
  };
  const readReaction = async () => {
    const component = await client.getReaction(signal);
    const expected = { ...TRADING_REACTION, queries: TRADING_QUERY_IDS };
    if (Object.entries(expected).some(([key, value]) => JSON.stringify(component.config[key]) !== JSON.stringify(value))) {
      throw new DrasiError('INCOMPATIBLE_RESOURCE', details('reaction', TRADING_REACTION.id));
    }
    checkState(component, 'reaction');
    return component;
  };
  const readOptional = async <T,>(read: () => Promise<T>, code: 'QUERY_NOT_FOUND' | 'REACTION_NOT_FOUND') => {
    try { return await read(); }
    catch (error) {
      if (error instanceof DrasiError && error.code === code) return null;
      throw error;
    }
  };

  // Preflight the whole known bundle before any write. Do not partially create
  // around an authorization, malformed-data or conflicting-definition failure.
  for (const definition of TRADING_QUERIES) await readOptional(() => readQuery(definition), 'QUERY_NOT_FOUND');
  await readOptional(readReaction, 'REACTION_NOT_FOUND');

  const ensure = async (
    kind: 'query' | 'reaction', id: string, body: object,
    read: () => Promise<Component<unknown>>,
  ) => {
    signal.throwIfAborted();
    let component = await readOptional(read, kind === 'query' ? 'QUERY_NOT_FOUND' : 'REACTION_NOT_FOUND');
    let started = false;
    if (!component) {
      const result = await request(fetcher, url(kind), signal, {
        method: 'POST', headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body),
      }, instanceId);
      started = result.status !== 409; // autoStart was requested only on our successful create.
      component = await read(); // Also validates the winner of a concurrent create.
    }
    if (['Stopped', 'Added'].includes(component.status) && !started) {
      component = await read(); // Another tab may already have started it.
      if (['Stopped', 'Added'].includes(component.status)) {
        try {
          await request(fetcher, url(kind, id, true), signal, { method: 'POST' }, instanceId);
        } catch (error) {
          // The server can report a racing start as an operation error rather
          // than 409. Accept it only after a successful, compatible active read.
          if (!(error instanceof DrasiError) || error.status === undefined ||
              error.status === 401 || error.status === 403) throw error;
          component = await read();
          if (!['Running', 'Starting', 'Reconfiguring'].includes(component.status)) throw error;
        }
        component = await read();
      }
    }
    while (component.status !== 'Running') {
      await delay(signal);
      component = await read();
    }
  };
  // Query/source/join order is intentional. Never parallelize creation.
  for (const definition of TRADING_QUERIES) {
    await ensure('query', definition.id, { ...definition, autoStart: true }, () => readQuery(definition));
  }
  await ensure('reaction', TRADING_REACTION.id, {
    ...TRADING_REACTION, queries: TRADING_QUERY_IDS, autoStart: true,
  }, readReaction);
}

interface Flight {
  controller: AbortController;
  promise: Promise<void>;
  users: number;
}
const flights = new WeakMap<typeof fetch, Map<string, Flight>>();

/**
 * Bounded, app-owned recovery after a typed eligible failure. Concurrent
 * consumers share work; Web Locks serialize tabs where available. 409/read
 * handles independent clients otherwise. The last cancellation aborts work.
 */
export function ensureTradingResources(
  options: SetupOptions, error: DrasiError, signal: AbortSignal,
): Promise<void> {
  signal.throwIfAborted();
  if (!canPrepareTrading(error, options.client.instanceId)) return Promise.reject(error);
  let scopes = flights.get(options.fetch);
  if (!scopes) { scopes = new Map(); flights.set(options.fetch, scopes); }
  const key = JSON.stringify([options.serverUrl, options.client.instanceId]);
  let flight = scopes.get(key);
  if (!flight || flight.controller.signal.aborted) {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(new DrasiError('SERVER_UNAVAILABLE', {
      instanceId: options.client.instanceId,
    })), SETUP_TIMEOUT_MS);
    const run = () => prepare(options, controller.signal);
    const promise = (async () => {
      if (globalThis.navigator?.locks) {
        await navigator.locks.request(`trading-setup:${key}`, { signal: controller.signal }, run);
      } else {
        await run();
      }
    })();
    flight = { controller, promise, users: 0 };
    scopes.set(key, flight);
    const current = flight;
    const cleanup = () => {
      clearTimeout(timer);
      if (scopes.get(key) === current) scopes.delete(key);
    };
    void promise.then(cleanup, cleanup);
  }
  const shared = flight;
  shared.users += 1;
  return new Promise((resolve, reject) => {
    let settled = false;
    const finish = (failure?: unknown) => {
      if (settled) return;
      settled = true;
      signal.removeEventListener('abort', abort);
      shared.users -= 1;
      if (shared.users === 0) shared.controller.abort();
      if (failure !== undefined) reject(failure);
      else resolve();
    };
    const abort = () => finish(signal.reason);
    signal.addEventListener('abort', abort, { once: true });
    void shared.promise.then(() => finish(), failure => finish(failure));
  });
}
