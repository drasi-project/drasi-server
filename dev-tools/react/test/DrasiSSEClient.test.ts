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
import { fakeEventSourceFactory } from './FakeEventSource';

afterEach(() => {
  vi.useRealTimers();
});

describe('DrasiSSEClient', () => {
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
      data: { id: 'A', price: 10 },
      timestamp: '2026-08-12T00:00:00Z',
    });

    expect(results).toEqual([
      expect.objectContaining({
        queryId: 'stocks',
        data: [{ id: 'A', price: 10 }],
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
