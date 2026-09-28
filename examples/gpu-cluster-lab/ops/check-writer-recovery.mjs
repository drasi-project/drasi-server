import assert from 'node:assert/strict';
import http from 'node:http';
import { setTimeout as delay } from 'node:timers/promises';
import { validateRow } from '../ui/src/rows.ts';

const token = process.env.INTERNAL_TOKEN;
assert.ok(token, 'Internal test authorization is required');
const headers = { authorization: `Bearer ${token}`, 'content-type': 'application/json' };
const control = 'http://control:5400';
const faults = 'http://writer-faults:8082';

async function json(base, path, options = {}) {
  const response = await fetch(`${base}${path}`, { headers, signal: AbortSignal.timeout(15000), ...options });
  const body = await response.json();
  assert.ok(response.ok, `${path}: ${response.status} ${JSON.stringify(body)}`);
  return body;
}
async function rows(query) {
  const { data } = await json(control, `/api/v1/instances/gpu-demo/queries/${query}/results`);
  return data.map(row => validateRow(query, row));
}
async function until(description, check) {
  const deadline = Date.now() + 30000;
  while (Date.now() < deadline) {
    if (await check()) return;
    await delay(200);
  }
  assert.fail(description);
}

async function check(fault) {
  const [before] = await rows('ui-placements');
  assert.equal(before.status, 'confirmed');
  await json(faults, '/arm', { method: 'POST', body: JSON.stringify({ fault }) });
  const gpu = (await rows('ui-gpus')).find(g => g.host_id === 'inference-a' && g.slot === 0);
  assert.ok(gpu);
  await json(control, `/api/gpus/${gpu.gpu_id}/telemetry`, {
    method: 'PATCH',
    body: JSON.stringify({ expected_revision: gpu.telemetry_revision, changes: { background_compute_units: 35 } }),
  });
  await until('lost HTTP receipt must still converge through authoritative database feedback', async () => {
    const [plan] = await rows('ui-placements');
    return plan.status === 'confirmed' && BigInt(plan.desired_plan_version) > BigInt(before.desired_plan_version)
      && (await rows('ui-workloads')).every(w => w.ready_replicas === w.replicas)
      && (await fetch(`${control}/health/ready`, { signal: AbortSignal.timeout(15000) })).status === 200;
  });
  const [plan] = await rows('ui-placements');
  assert.ok(plan.desired.every(assignment => assignment.gpu_id !== gpu.gpu_id));
  assert.equal(plan.desired.filter(a => before.desired.some(old => old.id === a.id && old.gpu_id !== a.gpu_id)).length, 1);
  const measured = (await rows('ui-gpus')).find(g => g.gpu_id === gpu.gpu_id);
  assert.equal(measured.reported_background_compute_units, 35);
  assert.equal(measured.sample_telemetry_revision, measured.telemetry_revision);
  const { calls } = await json(faults, '/observed');
  const injected = calls.find(call => call.fault === fault);
  assert.ok(injected, 'Test must actually inject the requested HTTP failure');
  const committed = calls.find(call => call.receipt?.status === 'committed');
  assert.ok(committed);
  assert.equal(committed.candidate.decision_id, plan.decision_id);
  if (fault === 'reject-before-commit') {
    assert.equal(injected.status, 503);
    assert.equal(injected.receipt, null);
    assert.equal(calls.length, 2, 'Native writer must retry the transient failure once with the same decision');
  } else {
    assert.equal(injected.receipt.status, 'committed');
  }
  assert.equal(BigInt(plan.desired_plan_version), BigInt(before.desired_plan_version) + 1n,
    'A lost acknowledgement must not duplicate the committed plan');
  for (const call of calls) assert.deepEqual(call.candidate, committed.candidate, 'Retries must preserve the entire candidate');
  const retry = await json(control, '/internal/placement-plans', {
    method: 'POST', body: JSON.stringify(committed.candidate),
  });
  assert.equal(retry.status, 'already_committed');
  assert.equal(retry.plan_version, plan.desired_plan_version);
  assert.equal(retry.current_plan_version, plan.desired_plan_version);
  assert.equal((await rows('ui-placements'))[0].desired_plan_version, plan.desired_plan_version);
  console.log(`Writer recovery passed (${fault}): ${calls.length} native request(s), one saved version, fresh confirmation and durable idempotent replay.`);
}

async function readBody(request) {
  const chunks = [];
  let bytes = 0;
  for await (const chunk of request) {
    bytes += chunk.length;
    assert.ok(bytes <= 256 * 1024, 'Request exceeds the production plan limit');
    chunks.push(chunk);
  }
  return Buffer.concat(chunks);
}

async function checkExhaustion() {
  await json(control, '/api/demo/presets/baseline', { method: 'POST', signal: AbortSignal.timeout(300000) });
  const [before] = await rows('ui-placements');
  await json(faults, '/arm', { method: 'POST', body: JSON.stringify({ fault: 'reject-all' }) });
  let gpu = (await rows('ui-gpus')).find(g => g.host_id === 'inference-a' && g.slot === 0);
  await json(control, `/api/gpus/${gpu.gpu_id}/telemetry`, {
    method: 'PATCH', body: JSON.stringify({ expected_revision: gpu.telemetry_revision, changes: { background_compute_units: 35 } }),
  });
  await until('exhausted retries must become a visible writer error', async () =>
    (await rows('ui-status'))[0]?.components.some(c => c.component_id === 'plan-writer'
      && c.status === 'write-error' && c.error.includes('exhausted five attempts')));
  const { calls } = await json(faults, '/observed');
  assert.equal(calls.length, 5);
  for (const call of calls) {
    assert.deepEqual(call.candidate, calls[0].candidate);
    assert.equal(call.receipt, null);
  }
  assert.equal((await rows('ui-placements'))[0].desired_plan_version, before.desired_plan_version);
  assert.equal((await rows('ui-status'))[0].inputs_ready, true,
    'A failed write must not disable corrective edits while input/lifecycle evidence remains valid');
  assert.equal((await fetch(`${control}/health/ready`)).status, 503);
  gpu = (await rows('ui-gpus')).find(g => g.gpu_id === gpu.gpu_id);
  const sequence = BigInt(gpu.report_sequence);
  await until('reports must continue while the failed candidate remains blocked', async () =>
    BigInt((await rows('ui-gpus')).find(g => g.gpu_id === gpu.gpu_id).report_sequence) >= sequence + 2n);
  assert.equal((await json(faults, '/observed')).calls.length, 5, 'Unchanged input must not spin on an exhausted candidate');
  await json(faults, '/disarm', { method: 'POST' });
  await json(control, `/api/gpus/${gpu.gpu_id}/telemetry`, {
    method: 'PATCH', body: JSON.stringify({ expected_revision: gpu.telemetry_revision, changes: { background_compute_units: 36 } }),
  });
  await until('a new measured input must allow a new complete decision to recover', async () => {
    const [plan] = await rows('ui-placements');
    return plan.status === 'confirmed' && plan.decision_id !== calls[0].candidate.decision_id
      && BigInt(plan.desired_plan_version) === BigInt(before.desired_plan_version) + 1n
      && (await fetch(`${control}/health/ready`)).status === 200;
  });
  console.log('Writer exhaustion passed: five identical attempts, visible failure, no saved mutation or retry spin, then recovery from a genuinely new measured input.');
}

function serve() {
  let armed = null;
  const calls = [];
  const server = http.createServer((request, response) => {
    const handle = async () => {
      if (request.headers.authorization !== headers.authorization) {
        response.writeHead(403).end();
        return;
      }
      response.setHeader('content-type', 'application/json');
      if (request.method === 'POST' && request.url === '/arm') {
        const { fault } = JSON.parse((await readBody(request)).toString('utf8'));
        assert.ok(['drop-after-commit', 'reject-before-commit', 'reject-all'].includes(fault));
        calls.length = 0;
        armed = fault;
        response.end(JSON.stringify({ armed }));
        return;
      }
      if (request.method === 'POST' && request.url === '/disarm') {
        armed = null;
        response.end(JSON.stringify({ armed }));
        return;
      }
      if (request.method === 'GET' && request.url === '/observed') {
        response.end(JSON.stringify({ calls }));
        return;
      }
      if (request.method !== 'POST' || request.url !== '/plans') {
        response.writeHead(404).end();
        return;
      }
      const body = await readBody(request);
      const candidate = JSON.parse(body.toString('utf8'));
      if (armed === 'reject-before-commit' || armed === 'reject-all') {
        calls.push({ candidate, receipt: null, status: 503, fault: armed });
        assert.ok(calls.length <= 10, 'Unexpected unbounded writer retries');
        if (armed === 'reject-before-commit') armed = null;
        response.writeHead(503).end(JSON.stringify({ error: 'Injected transient failure before forwarding to control' }));
        return;
      }
      const upstream = await fetch(`${control}/internal/placement-plans`, {
        method: 'POST', headers, body, signal: AbortSignal.timeout(15000),
      });
      const receipt = await upstream.json();
      const dropped = armed === 'drop-after-commit' && upstream.ok;
      calls.push({ candidate, receipt, status: upstream.status, fault: dropped ? armed : null });
      assert.ok(calls.length <= 10, 'Unexpected unbounded writer retries');
      if (dropped) {
        armed = null;
        console.log(`Dropping successful acknowledgement for actual decision ${candidate.decision_id}, plan ${receipt.plan_version}`);
        response.destroy();
      } else {
        response.writeHead(upstream.status).end(JSON.stringify(receipt));
      }
    };
    handle().catch(error => {
      console.error('Writer fault test failed:', error);
      response.destroy(error);
    });
  });
  server.listen(8082, '0.0.0.0');
}

if (process.argv[2] === 'serve') serve();
else {
  for (const fault of ['drop-after-commit', 'reject-before-commit']) {
    await json(control, '/api/demo/presets/baseline', { method: 'POST', signal: AbortSignal.timeout(300000) });
    await check(fault);
  }
  await checkExhaustion();
}
