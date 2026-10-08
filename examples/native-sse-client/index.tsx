import React, { useCallback, useEffect, useState, type ReactNode } from 'react';
import { DrasiClient, DrasiError, sse034ResultAdapter } from '@drasi/react/client';
import { DrasiClientProvider } from '@drasi/react/react';

function record(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function requireRunning(node: Record<string, unknown>): void {
  if (node.lifecycle === 'Starting') throw new DrasiError('RESOURCE_STARTING');
  // Native components need not publish a separate health probe. Unknown is not
  // evidence of health, but a completed start with no reported failure is usable.
  if (node.lifecycle !== 'Running' || node.failurePhase !== null ||
      (node.health !== 'Healthy' && node.health !== 'Unknown')) {
    throw new DrasiError('RESOURCE_UNAVAILABLE');
  }
}

export function validateNativeSseGraph(value: unknown, queryIds: readonly string[], sinkId: string): void {
  if (!record(value) || value.success !== true || !record(value.data) ||
      !record(value.data.graph) || !Array.isArray(value.data.components) ||
      !Array.isArray(value.data.relationships)) {
    throw new DrasiError('INVALID_PAYLOAD');
  }
  if (value.data.graph.state !== 'Running' || value.data.graph.driverFailed !== false) {
    throw new DrasiError('RESOURCE_UNAVAILABLE');
  }
  const components: unknown[] = value.data.components;
  const edges: unknown[] = value.data.relationships;
  const sink = components.find(node => record(node) && node.id === sinkId);
  if (!record(sink)) throw new DrasiError('REACTION_NOT_FOUND');
  if (sink.role !== 'Sink' || !record(sink.implementation) ||
      sink.implementation.name !== 'drasi.network/sse-sink') {
    throw new DrasiError('INCOMPATIBLE_RESOURCE');
  }
  requireRunning(sink);
  for (const id of queryIds) {
    const query = components.find(node => record(node) && node.id === id);
    if (!record(query)) throw new DrasiError('QUERY_NOT_FOUND', { resourceId: id });
    if (query.role !== 'Query') throw new DrasiError('INCOMPATIBLE_RESOURCE', { resourceId: id });
    requireRunning(query);
    if (!edges.some(edge => record(edge) && edge.representation === 'NativeProvider' &&
        edge.binding === 'Bound' && record(edge.from) && edge.from.component === id &&
        edge.from.port === 'out' && record(edge.to) && edge.to.component === sinkId &&
        edge.to.port === 'in')) {
      throw new DrasiError('INCOMPATIBLE_RESOURCE', { resourceId: id });
    }
  }
}

// The pinned SDK's default validator expects a legacy Reaction DTO. Its public
// client/provider binding lets this native example validate actual graph edges.
type NativeOptions = { origin: string; instanceId: string; queryIds: readonly string[]; sinkId: string; endpoint: string };

class NativeSseClient extends DrasiClient {
  constructor(private readonly nativeOptions: NativeOptions) {
    super({
      serverUrl: nativeOptions.origin, instanceId: nativeOptions.instanceId, queryIds: [...nativeOptions.queryIds],
      reaction: { id: nativeOptions.sinkId, endpoint: nativeOptions.endpoint },
      resultAdapter: sse034ResultAdapter,
    });
  }

  override async validateResources(signal?: AbortSignal): Promise<void> {
    const deadline = AbortSignal.timeout(10_000);
    const requestSignal = signal ? AbortSignal.any([signal, deadline]) : deadline;
    try {
      const response = await fetch(`${this.nativeOptions.origin}/api/v1/instances/${encodeURIComponent(this.nativeOptions.instanceId)}/computation`, {
        signal: requestSignal, credentials: 'same-origin', redirect: 'error',
      });
      if (!response.ok) {
        throw new DrasiError(
          response.status === 401 ? 'UNAUTHENTICATED' :
          response.status === 403 ? 'FORBIDDEN' :
          response.status === 404 ? 'INSTANCE_NOT_FOUND' : 'SERVER_UNAVAILABLE',
          { instanceId: this.nativeOptions.instanceId, status: response.status },
        );
      }
      validateNativeSseGraph(await response.json(), this.nativeOptions.queryIds, this.nativeOptions.sinkId);
    } catch (error) {
      signal?.throwIfAborted();
      if (error instanceof DrasiError) throw error;
      if (error instanceof SyntaxError) throw new DrasiError('INVALID_PAYLOAD');
      throw new DrasiError('SERVER_UNAVAILABLE', { instanceId: this.nativeOptions.instanceId });
    }
  }
}

export function NativeSseProvider({ children, origin, instanceId, queryIds, sinkId, endpoint }: NativeOptions & { children: ReactNode }) {
  const [attempt, setAttempt] = useState(0);
  const [state, setState] = useState<{
    client: DrasiClient | null; initialized: boolean; error: DrasiError | null;
  }>({ client: null, initialized: false, error: null });
  const retry = useCallback(() => {
    setState(current => ({ ...current, initialized: false, error: null }));
    setAttempt(current => current + 1);
  }, []);
  useEffect(() => {
    const client = new NativeSseClient({ origin, instanceId, queryIds, sinkId, endpoint });
    let active = true;
    setState({ client, initialized: false, error: null });
    const unsubscribe = client.onConnectionStatusChange(status => {
      if (active && status.error && !status.reconnecting) {
        setState({ client, initialized: false, error: status.error });
      }
    });
    void client.initialize().then(
      () => { if (active) setState({ client, initialized: true, error: null }); },
      error => {
        if (active) setState({
          client, initialized: false,
          error: error instanceof DrasiError ? error : new DrasiError('SERVER_UNAVAILABLE'),
        });
      },
    );
    return () => { active = false; unsubscribe(); void client.disconnect(); };
  }, [attempt, origin, instanceId, queryIds, sinkId, endpoint]);
  return <DrasiClientProvider value={{ ...state, retry }}>{children}</DrasiClientProvider>;
}
