// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import React, { Suspense, startTransition, useLayoutEffect, useState } from 'react';
import { act, cleanup, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { DrasiProvider, useDrasiQuery } from '../src/react/DrasiContext';
import type { ResultRow } from '../src/client/types';
import type { UseDrasiQueryOptions, UseDrasiQueryResult } from '../src/react/types';
import { fakeEventSourceFactory } from './FakeEventSource';
import { ReadServer, deferred, json, refs } from './server';

afterEach(cleanup);

interface Reading { value: number }
const rows = [
  { id: 'A', alternateId: 'future-A', value: 10 },
  { id: 'B', alternateId: 'future-B', value: 20 },
  { id: 'C', alternateId: 'future-C', value: 30 },
];

function fixture({ strict = false, deliverInLayout = false } = {}) {
  const server = new ReadServer(), factory = fakeEventSourceFactory(), gate = deferred<void>();
  server.snapshot = async () => json(rows);
  let futureReady = false;
  let attempted = 0;
  let setFuture: React.Dispatch<React.SetStateAction<boolean>> | undefined;
  let committed: UseDrasiQueryResult<Reading> | undefined;
  const currentTransform = vi.fn((row: Readonly<ResultRow>): Reading => {
    if (typeof row.value !== 'number') throw new Error('Expected a complete reading');
    return { value: row.value };
  });
  const futureTransform = vi.fn((row: Readonly<ResultRow>): Reading | null => {
    const value = currentTransform(row).value;
    return value === 10 ? null : { value: value * 10 };
  });
  const currentOptions: UseDrasiQueryOptions<Reading> = {
    getKey: row => typeof row.id === 'string' ? row.id : '',
    transform: currentTransform,
    postProcess: values => values.slice().sort((a, b) => a.value - b.value),
  };
  const futureOptions: UseDrasiQueryOptions<Reading> = {
    getKey: row => typeof row.alternateId === 'string' ? row.alternateId : '',
    transform: futureTransform,
    postProcess: values => values.slice().sort((a, b) => b.value - a.value),
  };

  function FutureContent({ future }: { future: boolean }) {
    if (future && !futureReady) {
      attempted += 1;
      throw gate.promise;
    }
    return null;
  }
  function CommitDelivery({ future }: { future: boolean }) {
    useLayoutEffect(() => {
      if (future && deliverInLayout) {
        factory.instances[0].message({
          queryId: 'stocks', timestamp: 1,
          results: [{ type: 'DELETE', data: { alternateId: 'future-B' } }],
        });
      }
    }, [future]);
    return null;
  }
  function Probe({ future }: { future: boolean }) {
    const result = useDrasiQuery('stocks', future ? futureOptions : currentOptions);
    useLayoutEffect(() => { committed = result; });
    return <>
      <output data-testid="mode">{future ? 'future' : 'current'}</output>
      <output data-testid="state">{result.status}:{result.error?.code ?? ''}</output>
      <output data-testid="data">{JSON.stringify(result.data)}</output>
      <FutureContent future={future} />
      <CommitDelivery future={future} />
    </>;
  }
  function Example() {
    const [future, update] = useState(false);
    useLayoutEffect(() => { setFuture = update; });
    return <DrasiProvider {...refs} fetch={server.fetch} eventSourceFactory={factory.create}>
      <Suspense fallback={<p>Waiting for future options</p>}><Probe future={future} /></Suspense>
    </DrasiProvider>;
  }
  const view = render(strict ? <React.StrictMode><Example /></React.StrictMode> : <Example />);
  const update = (value: boolean) => {
    if (!setFuture) throw new Error('Example has not committed');
    setFuture(value);
  };
  return {
    view, server, factory, currentTransform, futureTransform,
    result: () => {
      if (!committed) throw new Error('Probe has not committed');
      return committed;
    },
    attempted: () => attempted,
    async open() {
      await waitFor(() => expect(factory.instances).toHaveLength(1));
      act(() => factory.instances[0].open());
      await waitFor(() => expect(committed?.status).toBe('live'));
    },
    async beginFuture() {
      await act(async () => { startTransition(() => update(true)); });
      expect(attempted).toBeGreaterThan(0);
      expect(screen.getByTestId('mode').textContent).toBe('current');
    },
    async resolveFuture() {
      await act(async () => { futureReady = true; gate.resolve(); });
    },
    setCurrent() { act(() => update(false)); },
    emitDelete(data: ResultRow) {
      act(() => factory.instances[0].message({
        queryId: 'stocks', timestamp: 2, results: [{ type: 'DELETE', data }],
      }));
    },
  };
}

describe('query options are published by committed React work only', () => {
  it.each([false, true])('keeps suspended and later abandoned options out of the active subscription (StrictMode=%s)', async strict => {
    const f = fixture({ strict });
    await f.open();
    const reads = f.server.fetch.mock.calls.length;
    await f.beginFuture();
    expect(f.result()).toMatchObject({ status: 'live', error: null, data: [{ value: 10 }, { value: 20 }, { value: 30 }] });
    f.emitDelete({ id: 'A' });
    expect(screen.getByTestId('mode').textContent).toBe('current');
    expect(f.result()).toMatchObject({ status: 'live', error: null, data: [{ value: 20 }, { value: 30 }] });

    f.setCurrent();
    await f.resolveFuture();
    expect(screen.getByTestId('mode').textContent).toBe('current');
    f.emitDelete({ id: 'B' });
    expect(f.result()).toMatchObject({ status: 'live', error: null, data: [{ value: 30 }] });
    expect(f.factory.instances).toHaveLength(1);
    expect(f.factory.instances[0].closed).toBe(false);
    expect(f.server.fetch).toHaveBeenCalledTimes(reads);
    expect(f.currentTransform.mock.calls.every(([row]) => typeof row.value === 'number')).toBe(true);
  });

  it('applies later committed keys, transforms and post-processing without losing hidden raw rows or resubscribing', async () => {
    const f = fixture();
    await f.open();
    const reads = f.server.fetch.mock.calls.length;
    await f.beginFuture();
    await f.resolveFuture();
    expect(screen.getByTestId('mode').textContent).toBe('future');
    expect(f.result()).toMatchObject({ status: 'live', error: null, data: [{ value: 300 }, { value: 200 }] });
    f.emitDelete({ alternateId: 'future-B' });
    expect(f.result()).toMatchObject({ status: 'live', error: null, data: [{ value: 300 }] });
    f.setCurrent();
    expect(f.result()).toMatchObject({ status: 'live', error: null, data: [{ value: 10 }, { value: 30 }] });
    f.emitDelete({ id: 'A' });
    expect(f.result().data).toEqual([{ value: 30 }]);
    expect(f.futureTransform.mock.calls.every(([row]) => typeof row.value === 'number')).toBe(true);
    expect(f.factory.instances).toHaveLength(1);
    expect(f.server.fetch).toHaveBeenCalledTimes(reads);
    f.view.unmount();
    expect(f.factory.instances[0].closed).toBe(true);
  });

  it('publishes committed keys before a descendant layout effect can deliver a stream event', async () => {
    const f = fixture({ deliverInLayout: true });
    await f.open();
    const reads = f.server.fetch.mock.calls.length;
    await f.beginFuture();
    await f.resolveFuture();
    expect(screen.getByTestId('mode').textContent).toBe('future');
    expect(f.result()).toMatchObject({ status: 'live', error: null, data: [{ value: 300 }] });
    expect(f.factory.instances).toHaveLength(1);
    expect(f.server.fetch).toHaveBeenCalledTimes(reads);
  });
});
