// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { act, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { DrasiProvider, useDrasiQuery } from '../src/react/DrasiContext';
import { fakeEventSourceFactory } from './FakeEventSource';
import { pressureDelete, readings, temperatureUpdate } from './fixtures/synthetic/telemetry';
import { component, queryConfig } from './server';

interface Reading {
  device: string;
  metric: string;
  value: number;
}

function Readings({ queryId }: { queryId: string }) {
  const { data, error } = useDrasiQuery<Reading>(queryId, {
    getKey: row => `${row.device}/${row.metric}`,
    transform: row => {
      if (typeof row.device !== 'string' || typeof row.metric !== 'string') {
        throw new TypeError('Invalid synthetic reading identity');
      }
      return { device: row.device, metric: row.metric, value: Number(row.value) };
    },
  });
  return <output aria-label={queryId}>{error?.message ?? JSON.stringify(data)}</output>;
}

describe('non-Trading synthetic contracts', () => {
  it('accumulates custom-key rows without id/symbol and keeps identical-shaped queries separate', async () => {
    const factory = fakeEventSourceFactory();
    const fetcher: typeof fetch = async input => {
      const url = String(input);
      const json = (body: unknown) => new Response(JSON.stringify({ success: true, data: body }));
      if (url.includes('?view=full')) {
        const id = new URL(url).pathname.split('/').pop()!;
        return json(id === 'building-events'
          ? component('reactions', id, {
            id, kind: 'sse', queries: ['building-readings', 'archive-readings'],
            host: '0.0.0.0', port: 9999, ssePath: '/events', heartbeatIntervalMs: 15000,
          }, 'Running', 'building-a')
          : component('queries', id, queryConfig(id, {
            query: 'MATCH (r:Reading) RETURN r',
          }), 'Running', 'building-a'));
      }
      if (url.endsWith('/building-readings/results')) return json(readings);
      if (url.endsWith('/archive-readings/results')) return json(readings);
      throw new Error(`Unexpected non-Trading fixture request ${url}`);
    };
    render(
      <DrasiProvider
        serverUrl="http://building.invalid:8080"
        instanceId="building-a"
        queryIds={['building-readings', 'archive-readings']}
        reaction={{ id: 'building-events', endpoint: 'https://building.invalid/events' }}
        fetch={fetcher}
        eventSourceFactory={factory.create}
      >
        <Readings queryId="building-readings" />
        <Readings queryId="archive-readings" />
      </DrasiProvider>,
    );
    await waitFor(() => expect(factory.instances).toHaveLength(1));
    act(() => factory.instances[0].open());
    const initial = '[{"device":"boiler-7","metric":"temperature","value":21.5},{"device":"boiler-7","metric":"pressure","value":4}]';
    await waitFor(() => expect(screen.getByLabelText('building-readings').textContent).toBe(initial));
    act(() => factory.instances[0].message(temperatureUpdate));
    act(() => factory.instances[0].message(pressureDelete));
    expect(screen.getByLabelText('building-readings').textContent).toBe('[{"device":"boiler-7","metric":"temperature","value":22}]');
    expect(screen.getByLabelText('archive-readings').textContent).toBe(initial);
  });
});
