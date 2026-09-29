import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { setTimeout as delay } from 'node:timers/promises';

const demo = process.env.DEMO_URL;
const drasi = process.env.DRASI_URL;
assert.ok(demo && drasi && process.env.INTERNAL_TOKEN);
const started = performance.now();
async function json(base, path, options = {}) {
  const response = await fetch(`${base}${path}`, { signal: AbortSignal.timeout(15_000), ...options });
  const body = response.status === 204 ? null : await response.json();
  assert.ok(response.ok, `${path}: ${response.status} ${JSON.stringify(body)}`);
  return body;
}
async function rows(query) {
  return (await json(drasi, `/api/v1/instances/gpu-demo/queries/${query}/results`)).data;
}
async function command(path, body, method = 'PATCH', headers = {}) {
  return json(demo, path, {
    method, headers: { authorization: `Bearer ${process.env.INTERNAL_TOKEN}`,
      'content-type': 'application/json', 'idempotency-key': randomUUID(), ...headers },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
}
async function snapshot(stage) {
  const queries = ['ui-placements', 'ui-workloads', 'ui-gpus', 'ui-status',
    'diagnostic-contexts', 'diagnostic-workload-predicates', 'diagnostic-placement-predicates'];
  const data = Object.fromEntries(await Promise.all(queries.map(async q => [q, await rows(q)])));
  const metrics = await json(drasi, '/internal/diagnostics/metrics',
    { headers: { authorization: `Bearer ${process.env.INTERNAL_TOKEN}` } });
  console.log(JSON.stringify({ stage, elapsed_ms: performance.now() - started, data, metrics }));
  return data;
}
async function settled(required, timeout) {
  const deadline = performance.now() + timeout;
  do {
    const [[p], workloads] = await Promise.all([rows('ui-placements'), rows('ui-workloads')]);
    if (p?.status === 'confirmed' && p.desired.length === required
      && workloads.reduce((n, w) => n + w.ready_replicas, 0) === required
      && workloads.every(w => w.ready_replicas === w.replicas)) return true;
    await delay(200);
  } while (performance.now() < deadline);
  return false;
}
await snapshot('baseline');
assert.ok(await settled(8, 30_000), 'Diagnostic requires an initially confirmed baseline');
const name = `drain-canary-${randomUUID()}`;
await command('/api/workloads', { name, profile_id: 'embeddings-v1', replicas: 1,
  spread_across_domains: true, data_profile_id: 'demo-open', purpose: 'demo' }, 'POST');
assert.ok(await settled(9, 30_000), 'Canary creation must first reach ordinary confirmation');
const created = (await rows('ui-workloads')).find(w => w.name === name);
assert.ok(created);
await command(`/api/workloads/${created.workload_id}`, undefined, 'DELETE', { 'if-match': created.revision });
const convergedBeforePause = await settled(8, 30_000);
await snapshot('before-pause');
const graph = await json(drasi, '/api/v1/instances/gpu-demo/computation');
console.log(JSON.stringify({ stage: 'graph-before-pause', graph }));
const gpus = await rows('ui-gpus');
assert.ok(gpus.length === 6 && gpus.every(g => g.region === 'westeurope' && g.reporting_enabled));
await command('/api/regions/westeurope/telemetry', {
  gpu_revisions: Object.fromEntries(gpus.map(g => [g.gpu_id, g.telemetry_revision])),
  changes: { reporting_enabled: false },
});
const pauseStarted = performance.now();
try {
  for (let observation = 0; observation < 4; observation++) {
    await delay(Math.max(0, pauseStarted + (observation + 1) * 5000 - performance.now()));
    await snapshot(`paused-${(observation + 1) * 5}s`);
  }
} finally {
  const current = await rows('ui-gpus');
  await command('/api/regions/westeurope/telemetry', {
    gpu_revisions: Object.fromEntries(current.map(g => [g.gpu_id, g.telemetry_revision])),
    changes: { reporting_enabled: true },
  });
}
await snapshot('reporting-restore-requested');
console.log(JSON.stringify({ diagnostic_complete: true, converged_before_pause: convergedBeforePause,
  pause_elapsed_ms: performance.now() - pauseStarted,
  acceptance_pass: false, note: 'Bounded 20-second report pause is diagnostic only; heartbeat expiry and zero confirmed replicas are expected during the drain.' }));
