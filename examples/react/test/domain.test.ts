// Copyright 2026 The Drasi Authors. Licensed under the Apache License, Version 2.0.
import assert from 'node:assert/strict';
import test from 'node:test';
import { accumulateResult } from '@drasi/react/client';
import { connectionOptions, probeKey, readingOptions } from '../src/readings.ts';
import { simulatedQuery, simulatedRows, simulatedStates } from '../src/showcaseState.ts';

test('explicit references contain no definitions or instance-discovery defaults', () => {
  const options = connectionOptions('http://127.0.0.1:5373');
  assert.equal(options.instanceId, 'cold-chain');
  assert.deepEqual(options.queryIds, ['north-room', 'south-room']);
  assert.deepEqual(options.reaction, { id: 'cold-chain-events', endpoint: 'http://127.0.0.1:5373/events' });
});

test('raw identity remains separate from validated render projection and handles sparse deletes', () => {
  const raw = { probeId: 'probe-1001', room: 'North', temperatureC: 3 };
  assert.equal(probeKey(raw), 'probe-1001');
  assert.deepEqual(readingOptions.transform(raw), { key: 'probe-1001', room: 'North', celsius: 3 });
  assert.equal(probeKey({ probeId: 'probe-1001' }), 'probe-1001');
  assert.throws(() => readingOptions.transform({ probeId: 'probe-1001' }));
  assert.deepEqual(accumulateResult([raw], {
    kind: 'delta', queryId: 'north-room', receivedAt: 1,
    changes: [{ kind: 'delete', before: { probeId: 'probe-1001' } }],
  }, probeKey), []);
});

test('invalid identity or measurement is rejected, not silently dropped', () => {
  for (const probeId of [undefined, '', ' ', 1]) assert.throws(() => probeKey({ probeId }));
  for (const temperatureC of [undefined, '3', NaN, Infinity]) {
    assert.throws(() => readingOptions.transform({ probeId: 'probe-1001', room: 'North', temperatureC }));
  }
  assert.throws(() => readingOptions.transform({ probeId: 'probe-1001', room: 'West', temperatureC: 3 }));
});

test('simulated states retain useful data and retry is explicit and deterministic', () => {
  let retries = 0;
  for (const status of simulatedStates) {
    const query = simulatedQuery(status, simulatedRows, () => { retries += 1; });
    assert.equal(query.status, status);
    assert.equal(query.data === null, status === 'initial-loading');
    if (status === 'empty') assert.deepEqual(query.data, []);
    if (query.stale) assert.equal(query.data, simulatedRows);
    if (status === 'terminal-error') assert.equal(query.error?.code, 'INVALID_PAYLOAD');
    if (status === 'stale-last-good-data') assert.equal(query.error?.code, 'SERVER_UNAVAILABLE');
    query.retry();
  }
  assert.equal(retries, simulatedStates.length);
});
