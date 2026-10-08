import { describe, expect, it } from 'vitest';
import { createElement } from 'react';
import { renderToString } from 'react-dom/server';
import { NativeSseProvider, validateNativeSseGraph as validateGraph } from '@drasi/example-native-sse';
import { queryIds } from './records';

const validateNativeSseGraph = (value: unknown) => validateGraph(value, queryIds, 'wall-ui');

function graph() {
  return {
    success: true,
    data: {
      graph: { state: 'Running', driverFailed: false },
      components: [
        { id: 'wall-ui', role: 'Sink', lifecycle: 'Running', health: 'Unknown', failurePhase: null,
          implementation: { name: 'drasi.network/sse-sink' } },
        ...queryIds.map(id => ({
          id, role: 'Query', lifecycle: 'Running', health: 'Unknown', failurePhase: null,
          implementation: { name: 'drasi/continuous-query' },
        })),
      ],
      relationships: queryIds.map(id => ({
        representation: 'NativeProvider', binding: 'Bound',
        from: { component: id, port: 'out' }, to: { component: 'wall-ui', port: 'in' },
      })),
    },
  };
}

describe('native SSE graph validation', () => {
  it('renders the shared package without relying on the consuming app JSX runtime', () => {
    expect(renderToString(createElement(NativeSseProvider, {
      origin: 'http://localhost', instanceId: 'move-a-wall', queryIds,
      sinkId: 'wall-ui', endpoint: 'http://localhost/events', children: 'native',
    }))).toBe('native');
  });
  it('accepts actual native query-to-sink wiring without a legacy Reaction DTO', () => {
    expect(() => validateNativeSseGraph(graph())).not.toThrow();
  });
  it('rejects malformed inspection rather than fabricating a resource', () => {
    for (const value of [null, {}, { success: true, data: {} }, { success: false }]) {
      expect(() => validateNativeSseGraph(value)).toThrow();
    }
  });
  it('rejects missing, incompatible and unhealthy native components', () => {
    for (const change of [
      (value: ReturnType<typeof graph>) => { value.data.components.shift(); },
      (value: ReturnType<typeof graph>) => { value.data.components.pop(); },
      (value: ReturnType<typeof graph>) => { value.data.components[0].role = 'Reaction'; },
      (value: ReturnType<typeof graph>) => { value.data.components[0].implementation.name = 'unknown'; },
      (value: ReturnType<typeof graph>) => { value.data.components[0].health = 'Unavailable'; },
      (value: ReturnType<typeof graph>) => { value.data.components[1].lifecycle = 'Stopped'; },
      (value: ReturnType<typeof graph>) => { value.data.graph.driverFailed = true; },
    ]) {
      const value = graph();
      change(value);
      expect(() => validateNativeSseGraph(value)).toThrow();
    }
  });
  it('requires a bound native edge from every selected query', () => {
    for (const change of [
      (value: ReturnType<typeof graph>) => { value.data.relationships.pop(); },
      (value: ReturnType<typeof graph>) => { value.data.relationships[0].binding = 'Binding'; },
      (value: ReturnType<typeof graph>) => { value.data.relationships[0].representation = 'HostSubscription'; },
      (value: ReturnType<typeof graph>) => { value.data.relationships[0].to.port = 'wrong'; },
    ]) {
      const value = graph();
      change(value);
      expect(() => validateNativeSseGraph(value)).toThrow();
    }
  });
});
