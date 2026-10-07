// Copyright 2026 The Drasi Authors. Licensed under the Apache License, Version 2.0.
import assert from 'node:assert/strict';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

export const probes = [
  { id: 'north-probe', probeId: 'probe-1001', room: 'North', temperatureC: 3 },
  { id: 'south-probe', probeId: 'probe-2001', room: 'South', temperatureC: 5 },
];

export async function sendReading(endpoint, probe, operation = 'update') {
  assert(/^http:\/\/127\.0\.0\.1:\d+$/.test(endpoint), 'Use the owned loopback feed URL printed by npm start');
  assert(['insert', 'update', 'delete'].includes(operation), 'Unsupported feed operation');
  assert(probes.some(item => item.id === probe.id && item.probeId === probe.probeId && item.room === probe.room),
    'Only this example owns these two probe identities');
  assert(Number.isFinite(probe.temperatureC), 'Expected a finite temperature');
  const response = await fetch(`${endpoint}/sources/probe-feed/events`, {
    method: 'POST', headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      operation,
      ...(operation === 'delete' ? { id: probe.id, labels: ['Probe'] } : { element: {
        type: 'node', id: probe.id, labels: ['Probe'],
        properties: {
          probeId: probe.probeId, room: probe.room, temperatureC: probe.temperatureC,
        },
      } }),
      timestamp: Date.now() * 1_000_000,
    }),
    signal: AbortSignal.timeout(5000),
  });
  assert(response.ok, `Feed ${operation} failed (${response.status}); see the owned server log`);
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  const [endpoint, command] = process.argv.slice(2);
  assert(['warm', 'delete', 'restore'].includes(command),
    'Usage: npm run feed -- http://127.0.0.1:9180 warm|delete|restore');
  if (command === 'warm') await sendReading(endpoint, { ...probes[0], temperatureC: 8 });
  if (command === 'delete') await sendReading(endpoint, probes[1], 'delete');
  if (command === 'restore') {
    for (const probe of probes) await sendReading(endpoint, probe, 'insert');
  }
  console.log(`Applied ${command} to the example-owned probe feed.`);
}
