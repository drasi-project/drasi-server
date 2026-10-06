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

import { afterEach, describe, expect, it, vi } from 'vitest';
import { DrasiSSEClient } from '../src/client/DrasiSSEClient';
import { DrasiError } from '../src/client/errors';
import { fakeEventSourceFactory } from './FakeEventSource';

afterEach(() => {
  vi.useRealTimers();
});

describe('DrasiSSEClient', () => {
  it.each([null, [], {}, { queryId: 3 }, { queryId: 'q' }, { addedResults: 'bad' }])(
    'terminates malformed live payload %j with an actionable typed error', async payload => {
      const factory = fakeEventSourceFactory();
      const client = new DrasiSSEClient({ eventSourceFactory: factory.create });
      const connected = client.connect(['q'], 'https://events.invalid');
      factory.instances[0].open();
      await connected;
      factory.instances[0].message(payload);
      const keyed = payload !== null && typeof payload === 'object' && 'queryId' in payload && payload.queryId === 'q';
      const error = keyed ? client.getQueryError('q') : client.getConnectionStatus().error;
      expect(error).toBeInstanceOf(DrasiError);
      expect(error?.code).toBe('INVALID_PAYLOAD');
      expect(client.getConnectionStatus().reconnecting).toBe(false);
      expect(factory.instances[0].closed).toBe(!keyed);
      await client.disconnect();
    },
  );

  it('cancels a pending connection via AbortSignal and ignores obsolete open/events', async () => {
    const factory = fakeEventSourceFactory(), controller = new AbortController();
    const client = new DrasiSSEClient({ eventSourceFactory: factory.create });
    const connected = client.connect(['q'], 'https://events.invalid', controller.signal);
    controller.abort();
    await expect(connected).rejects.toMatchObject({ name: 'AbortError' });
    factory.instances[0].open();
    factory.instances[0].message({ queryId: 'q', data: [] });
    factory.instances[0].fail();
    expect(client.isConnected()).toBe(false);
    await client.disconnect();
  });

  it('bounds constructor/factory failures without claiming absent resources', async () => {
    expect(() => new DrasiSSEClient({ connectionTimeoutMs: 0 })).toThrow(DrasiError);
    const client = new DrasiSSEClient({
      maxReconnectAttempts: 0, eventSourceFactory: () => { throw new Error('private failure'); },
    });
    await expect(client.connect(['q'], 'https://events.invalid')).rejects.toMatchObject({ code: 'STREAM_UNAVAILABLE' });
    expect(client.getConnectionStatus().error?.message).not.toContain('private');
    await client.disconnect();
  });

  it('multiplexes query batches after the connection opens', async () => {
    const factory = fakeEventSourceFactory();
    const client = new DrasiSSEClient({
      eventSourceFactory: factory.create,
    });
    const results: unknown[] = [];
    client.subscribe('stocks', (result) => results.push(result));

    const connected = client.connect(
      ['stocks'],
      'http://localhost:8281/events',
    );
    factory.instances[0].open();
    await connected;

    factory.instances[0].message({
      queryId: 'stocks',
      results: [{ type: 'ADD', data: { id: 'A', price: 10 } }],
      timestamp: Date.parse('2026-08-12T00:00:00Z'),
    });

    expect(results).toEqual([
      expect.objectContaining({
        queryId: 'stocks',
        kind: 'delta',
        changes: [{ kind: 'upsert', after: { id: 'A', price: 10 } }],
      }),
    ]);
    expect(client.getConnectionStatus().connected).toBe(true);
  });

  it('closes a failed source and cancels manual reconnect on disconnect', async () => {
    vi.useFakeTimers();
    const factory = fakeEventSourceFactory();
    const client = new DrasiSSEClient({
      eventSourceFactory: factory.create,
      initialReconnectDelayMs: 10,
    });

    const connected = client.connect(
      ['stocks'],
      'http://localhost:8281/events',
    );
    factory.instances[0].open();
    await connected;

    factory.instances[0].fail();
    expect(factory.instances[0].closed).toBe(true);
    expect(client.getConnectionStatus().reconnecting).toBe(true);

    await client.disconnect();
    await vi.advanceTimersByTimeAsync(1000);

    expect(factory.instances).toHaveLength(1);
    expect(client.getConnectionStatus()).toEqual({
      connected: false,
      reconnecting: false,
    });
  });

  it('rejects an initial connection that is disconnected before open', async () => {
    const factory = fakeEventSourceFactory();
    const client = new DrasiSSEClient({
      eventSourceFactory: factory.create,
    });

    const connected = client.connect(
      ['stocks'],
      'http://localhost:8281/events',
    );
    await client.disconnect();

    await expect(connected).rejects.toMatchObject({ name: 'AbortError' });
    expect(factory.instances[0].closed).toBe(true);
  });
});
