import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { createServer } from 'node:net';
import { once } from 'node:events';
import { setTimeout as delay } from 'node:timers/promises';

for (const name of ['DEMO_TEST_CONFIG_URL', 'DEMO_TEST_PLAN_URL']) {
  assert(process.env[name], `${name} is required`);
  assert.equal(new URL(process.env[name]).pathname, '/gpu_demo_test', 'Use only the dedicated integration database');
}
const reservation = createServer();
reservation.listen(0, '127.0.0.1');
await once(reservation, 'listening');
const port = reservation.address().port;
await new Promise(resolve => reservation.close(resolve));
const origin = `http://127.0.0.1:${port}`;
const child = spawn('./target/debug/gpu-control', {
  cwd: new URL('../../shared/', import.meta.url),
  env: { ...process.env, DATABASE_URL: process.env.DEMO_TEST_CONFIG_URL, PLAN_DATABASE_URL: process.env.DEMO_TEST_PLAN_URL,
    DRASI_API_URL: 'http://127.0.0.1:1', DRASI_SSE_URL: 'http://127.0.0.1:1/events',
    PUBLIC_ORIGIN: origin, INTERNAL_TOKEN: 'integration-only-token', LISTEN_ADDRESS: `127.0.0.1:${port}`, RUST_LOG: 'error' },
  stdio: ['ignore', 'pipe', 'pipe'],
});
let output = '';
child.stdout.on('data', data => { output += data; });
child.stderr.on('data', data => { output += data; });
const exited = once(child, 'exit');
try {
  let live = false;
  for (let i = 0; i < 100; i++) {
    if (child.exitCode !== null) throw new Error(`Control exited: ${output}`);
    try {
      live = (await fetch(`${origin}/health/live`, { signal: AbortSignal.timeout(250) })).ok;
    } catch (error) {
      if (!(error instanceof TypeError) && error.name !== 'TimeoutError') throw error;
    }
    if (live) break;
    await delay(50);
  }
  assert(live, `Control did not start: ${output}`);
  const ready = await fetch(`${origin}/health/ready`);
  assert.equal(ready.status, 200);
  assert.equal((await ready.json()).scope, 'database-and-command-service-only');
  const html = await fetch(origin);
  assert.equal(html.status, 200);
  assert.match(await html.text(), /GPU Cluster Lab/);
  assert.equal((await fetch(`${origin}/api/state`)).status, 405);
  assert.equal((await fetch(`${origin}/api/v1/instances/gpu-demo/secrets?view=full`)).status, 404);
  assert.equal((await fetch(`${origin}/api/v1/instances/gpu-demo/queries/ui-gpus/results`)).status, 503);
  assert.equal((await fetch(`${origin}/events/gpu-demo`)).status, 503);
  const csrf = await (await fetch(`${origin}/api/csrf`)).json();
  const body = { name: 'http-check', profile_id: 'chat-v1', data_profile_id: 'customer-eu-documents',
    purpose: 'customer-support', replicas: 1, spread_across_domains: true };
  const key = crypto.randomUUID();
  const headers = { 'Content-Type': 'application/json', 'Origin': origin, 'X-CSRF-Token': csrf.token, 'Idempotency-Key': key };
  assert.equal((await fetch(`${origin}/api/workloads`, { method: 'POST', headers: { ...headers, Origin: 'http://untrusted.invalid' }, body: JSON.stringify(body) })).status, 403);
  const first = await fetch(`${origin}/api/workloads`, { method: 'POST', headers, body: JSON.stringify(body) });
  assert.equal(first.status, 201, await first.clone().text());
  const ack = await first.json();
  assert.equal(ack.status, 'committed');
  assert.equal(ack.revision, '1');
  const retry = await fetch(`${origin}/api/workloads`, { method: 'POST', headers, body: JSON.stringify(body) });
  assert.equal(retry.status, 200);
  assert.equal((await retry.json()).id, ack.id);
  assert.equal((await fetch(`${origin}/api/demo/presets/baseline`, { method: 'POST', headers })).status, 503);
  assert.equal((await fetch(`${origin}/api/workloads/${ack.id}`, { method: 'DELETE',
    headers: { ...headers, 'If-Match': '1' } })).status, 204);
  console.log('HTTP smoke checks passed: scoped reads, explicit unavailable integration, CSRF, durable command retry.');
} finally {
  child.kill('SIGINT');
  await Promise.race([exited, delay(5000).then(() => { child.kill('SIGKILL'); })]);
}
