// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

// This portable public-API regression also runs against exact P3 source.
import React from 'react';
import { act, cleanup, render } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { DrasiProvider, useDrasiQuery } from '../src/react/DrasiContext';
import { fakeEventSourceFactory } from './FakeEventSource';
import { ReadServer, deferred, json, refs } from './server';

beforeEach(() => vi.useFakeTimers());
afterEach(() => { cleanup(); vi.useRealTimers(); });
const flush = () => act(async () => { await vi.advanceTimersByTimeAsync(0); });

function fixture(options) {
  const server = new ReadServer(), factory = fakeEventSourceFactory();
  server.snapshot = async () => json([{ code: 'old', value: 1 }]);
  let latest;
  function Probe() {
    latest = useDrasiQuery('stocks', options);
    return <output>{JSON.stringify(latest.data)}</output>;
  }
  render(<DrasiProvider {...refs} fetch={server.fetch} eventSourceFactory={factory.create}
    reconnect={{ maxReconnectAttempts: 1, initialReconnectDelayMs: 10 }}>
    <Probe />
  </DrasiProvider>);
  return {
    server, factory, result: () => latest,
    async open() {
      await flush();
      act(() => factory.instances[0].open());
      await flush();
    },
    emit(results) {
      act(() => factory.instances[0].message({ queryId: 'stocks', results, timestamp: 1 }));
    },
  };
}

describe('P3-to-P4 observable regressions', () => {
  it('extracts identity before a projection which deliberately omits the raw key', async () => {
    const f = fixture({
      getKey: row => {
        if (typeof row.code !== 'string') throw new Error('Missing raw key');
        return row.code;
      },
      transform: row => ({ reading: row.value }),
    });
    await f.open();
    expect(f.result().data).toEqual([{ reading: 1 }]);
    f.emit([{ type: 'DELETE', data: { code: 'old' } }]);
    expect(f.result().data).toEqual([]);
  });

  it('removes the previous identity on an official before/after key-changing update', async () => {
    const f = fixture({ getKey: row => row.code, transform: row => row });
    await f.open();
    const next = { code: 'new', value: 2 };
    f.emit([{ type: 'UPDATE', before: { code: 'old', value: 1 }, after: next, data: next }]);
    expect(f.result().data).toEqual([next]);
  });

  it('refreshes an ambiguous newer snapshot rather than blindly replaying an older buffered delta', async () => {
    const f = fixture({ getKey: row => row.code, transform: row => row });
    const pending = deferred();
    f.server.snapshot = () => pending.promise;
    await f.open();
    f.emit([{ type: 'ADD', data: { code: 'old', value: 1 } }]);
    pending.resolve(json([{ code: 'old', value: 2 }]));
    await flush();
    f.server.snapshot = async () => json([{ code: 'old', value: 3 }]);
    await act(async () => { await vi.advanceTimersByTimeAsync(10); });
    expect(f.result().data).toEqual([{ code: 'old', value: 3 }]);
  });
});
