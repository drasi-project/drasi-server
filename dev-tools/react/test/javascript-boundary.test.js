// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

// JavaScript callers have no compiler to enforce declared callback boundaries.
import { afterEach, describe, expect, it, vi } from 'vitest';
import { createElement } from 'react';
import { act, renderHook, waitFor } from '@testing-library/react';
import { DrasiSSEClient } from '../src/client/DrasiSSEClient';
import { accumulateResult } from '../src/client/accumulation';
import { createLegacyResultAdapter } from '../src/client/results';
import { fakeEventSourceFactory } from './FakeEventSource';
import { DrasiProvider, useDrasiQuery } from '../src/react/DrasiContext';
import { ReadServer, refs } from './server';

const clients = [];
afterEach(async () => {
  await Promise.all(clients.splice(0).map(client => client.disconnect()));
});

describe('untyped client callback boundaries', () => {
  it.each([null, undefined, 3, {}, false])('rejects a non-string raw identity %j without fallback', key => {
    expect(() => accumulateResult([], {
      kind: 'snapshot', queryId: 'q', rows: [{ symbol: 'not-a-default' }], receivedAt: 1,
    }, () => key)).toThrow(expect.objectContaining({ code: 'INVALID_ROW_KEY' }));
  });

  it('rejects malformed custom adapter output as a visible nonretryable protocol failure', async () => {
    const factory = fakeEventSourceFactory();
    const client = new DrasiSSEClient({ eventSourceFactory: factory.create, resultAdapter: () => ({ bad: true }) });
    clients.push(client);
    const connected = client.connect(['q'], 'https://stream.invalid');
    factory.instances[0].open();
    await connected;
    factory.instances[0].message({ privateBody: 'never logged' });
    expect(client.getConnectionStatus().error).toMatchObject({ code: 'INVALID_PAYLOAD', retryable: false });
    expect(factory.instances[0].closed).toBe(true);
  });

  it('rejects an invalid legacy router delivery rather than leaking arbitrary data', () => {
    const adapter = createLegacyResultAdapter({ routeUnidentified: (_rows, deliver) => deliver('q', null) });
    expect(() => adapter({ opaque: 'sensitive fixture' }, { receivedAt: 1 }))
      .toThrow(expect.objectContaining({ code: 'UNROUTABLE_RESULT' }));
  });

  it('exposes safe callback failures through getQueryError even without an error observer', async () => {
    const factory = fakeEventSourceFactory(), log = vi.spyOn(console, 'error').mockImplementation(() => {});
    const client = new DrasiSSEClient({ eventSourceFactory: factory.create });
    clients.push(client);
    client.subscribe('q', () => { throw new Error('secret in callback'); });
    const connected = client.connect(['q'], 'https://stream.invalid');
    factory.instances[0].open();
    await connected;
    factory.instances[0].message({ queryId: 'q', timestamp: 1, results: [{ type: 'ADD', data: {} }] });
    expect(client.getQueryError('q')).toMatchObject({ code: 'RESULT_PROCESSING_FAILED' });
    expect(log.mock.calls).toEqual([['Drasi result subscriber failed:', 'RESULT_PROCESSING_FAILED']]);
  });

  it.each([
    { transform: () => false },
    { postProcess: () => null },
    { postProcess: () => [null] },
  ])('rejects invalid JavaScript projection output instead of publishing a success-shaped value', async invalid => {
    const server = new ReadServer(), factory = fakeEventSourceFactory();
    const view = renderHook(() => useDrasiQuery('stocks', {
      getKey: row => String(row.id), transform: row => row, ...invalid,
    }), {
      wrapper: ({ children }) => createElement(DrasiProvider, {
        ...refs, fetch: server.fetch, eventSourceFactory: factory.create,
      }, children),
    });
    await waitFor(() => expect(factory.instances).toHaveLength(1));
    act(() => factory.instances[0].open());
    await waitFor(() => expect(view.result.current.status).toBe('terminal-error'));
    expect(view.result.current).toMatchObject({
      data: null, stale: false, error: { code: 'RESULT_PROCESSING_FAILED' }, errorScope: 'query',
    });
    view.unmount();
    expect(factory.instances[0].closed).toBe(true);
  });
});
