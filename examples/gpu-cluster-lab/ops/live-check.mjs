import assert from 'node:assert/strict';
import { setTimeout as delay } from 'node:timers/promises';
import { randomUUID } from 'node:crypto';
import { queryIds, rowKey, validateRow } from '../ui/src/rows.ts';

const demo = process.env.DEMO_URL ?? 'http://localhost:5400';
const drasi = process.env.DRASI_URL ?? 'http://localhost:8080';
const mode = process.argv[2] ?? '--smoke';
assert.ok(['--smoke', '--errors', '--concurrency', '--commands', '--runbook', '--acceptance',
  '--restart-snapshot', '--restart-verify'].includes(mode), 'Unknown live check mode');

async function json(base, path, options = {}) {
  const response = await fetch(`${base}${path}`, { signal: AbortSignal.timeout(15_000), ...options });
  const body = response.status === 204 ? null : await response.json();
  assert.ok(response.ok, `${path}: ${response.status} ${JSON.stringify(body)}`);
  return body;
}
async function rows(query) {
  const body = await json(demo, `/api/v1/instances/gpu-demo/queries/${query}/results`);
  assert.ok(Array.isArray(body.data), `${query}: missing Server result data`);
  const keys = new Set();
  for (const row of body.data) {
    try {
      validateRow(query, row);
    } catch (error) {
      throw new Error(`${query}: invalid live row ${JSON.stringify(row)}`, { cause: error });
    }
    const key = rowKey(query, row);
    assert.ok(!keys.has(key), `${query}: duplicate identity ${key}`);
    keys.add(key);
  }
  return body.data;
}
async function until(description, check, timeout = 30_000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    if (await check()) return;
    await delay(200);
  }
  assert.fail(`Timed out: ${description}`);
}

async function command(path, body, method = 'PATCH', extraHeaders = {}) {
  assert.ok(process.env.INTERNAL_TOKEN, 'Internal command authorization is required');
  return json(demo, path, {
    method, headers: { authorization: `Bearer ${process.env.INTERNAL_TOKEN}`, 'content-type': 'application/json',
      'idempotency-key': randomUUID(), ...extraHeaders },
    body: body === undefined ? undefined : JSON.stringify(body),
    signal: AbortSignal.timeout(300_000),
  });
}
async function confirmed() {
  await until('saved, applied and freshly confirmed plan agree', async () => {
    const [plan] = await rows('ui-placements');
    return plan?.status === 'confirmed'
      && plan.desired_plan_version === plan.applied_plan_version
      && plan.applied_plan_version === plan.confirmed_plan_version;
  });
}
async function telemetry(id, changes) {
  const gpu = (await rows('ui-gpus')).find(row => row.gpu_id === id);
  assert.ok(gpu, `GPU ${id} is absent`);
  await command(`/api/gpus/${encodeURIComponent(id)}/telemetry`, {
    expected_revision: gpu.telemetry_revision, changes,
  });
}
async function reset(scenario) {
  const [before] = await rows('ui-placements');
  const oldIds = new Set((await rows('ui-gpus')).map(g => g.gpu_id));
  const result = await command(`/api/demo/presets/${scenario}`, undefined, 'POST');
  assert.equal(result.status, 'ready');
  assert.equal(result.scenario, scenario);
  assert.notEqual(result.observation_epoch, before.observation_epoch);
  assert.ok(BigInt(result.plan_version) > BigInt(before.desired_plan_version));
  assert.ok((await rows('ui-gpus')).every(g => !oldIds.has(g.gpu_id)), 'Reset must use fresh identities');
  await confirmed();
}
async function groupPower(kind, name, powered_on) {
  const members = (await rows('ui-gpus')).filter(g => (kind === 'hosts' ? g.host_id : g.region) === name);
  assert.ok(members.length > 0);
  await command(`/api/${kind}/${encodeURIComponent(name)}/telemetry`, {
    gpu_revisions: Object.fromEntries(members.map(g => [g.gpu_id, g.telemetry_revision])),
    changes: { powered_on },
  });
  await until(`${kind}/${name} power setting observed`, async () =>
    (await rows('ui-gpus')).filter(g => members.some(m => m.gpu_id === g.gpu_id)).every(g => g.powered_on === powered_on));
  return members;
}
async function healthyReports(ids, healthy) {
  await until(`GPU reports become ${healthy ? 'fresh' : 'overdue'}`, async () => {
    const members = (await rows('ui-gpus')).filter(g => ids.includes(g.gpu_id));
    return members.length === ids.length && members.every(g => g.health === (healthy ? 'healthy' : 'unreachable'));
  });
}
async function replannedAway(ids, previousVersion) {
  await until('a new complete saved plan excludes unavailable GPUs', async () => {
    const [plan] = await rows('ui-placements');
    return plan && BigInt(plan.desired_plan_version) > BigInt(previousVersion)
      && plan.desired.every(a => !ids.includes(a.gpu_id));
  });
}
async function resilience(check) {
  await until('current resilience matches the expected constraint outcome', async () => {
    const [[assessment], [status]] = await Promise.all([rows('ui-resilience'), rows('ui-status')]);
    return assessment?.status === 'current' && status
      && assessment.scheduling_signature === status.scheduling_signature
      && assessment.policy_signature === status.policy_signature && check(assessment);
  }, 60_000);
}
async function replicas(count) {
  await until(`${count} required replicas are freshly confirmed`, async () => {
    const [workloads, [plan]] = await Promise.all([rows('ui-workloads'), rows('ui-placements')]);
    return plan?.status === 'confirmed' && plan.desired.length === count
      && workloads.reduce((sum, w) => sum + w.ready_replicas, 0) === count
      && workloads.every(w => w.ready_replicas === w.replicas);
  });
}
async function addWorkload(name, profile_id, replicas) {
  await command('/api/workloads', { name, profile_id, replicas, spread_across_domains: true,
    data_profile_id: 'demo-open', purpose: 'demo' }, 'POST');
  await until(`${name} appears in the workload query`, async () =>
    (await rows('ui-workloads')).some(w => w.name === name));
}
async function addHost(host_id, cluster_id) {
  const result = await command('/api/hosts', { host_id, cluster_id, hardware_profile_id: 'h100-nvl-pair-v1' }, 'POST');
  assert.equal(result.gpu_ids.length, 2);
  await until('both ready-worker GPUs appear with fresh reports', async () => {
    const gpus = await rows('ui-gpus');
    return result.gpu_ids.every(id => gpus.some(g => g.gpu_id === id && g.health === 'healthy'));
  });
}
async function infeasible(savedVersion) {
  await until('current placement is explicitly infeasible', async () => {
    const [[status], decisions] = await Promise.all([rows('ui-status'), rows('ui-decisions')]);
    return status && decisions.some(d => d.outcome === 'infeasible' && d.stage === 'diagnostic'
      && d.scheduling_signature === status.scheduling_signature);
  });
  assert.equal((await rows('ui-placements'))[0].desired_plan_version, savedVersion,
    'Infeasibility must not write a partial or empty replacement plan');
}
async function onlyRegion(region) {
  await until(`all eight replicas are confirmed in ${region}`, async () => {
    const [[plan], gpus] = await Promise.all([rows('ui-placements'), rows('ui-gpus')]);
    return plan?.status === 'confirmed' && plan.desired.length === 8
      && plan.desired.every(a => gpus.some(g => g.gpu_id === a.gpu_id && g.region === region));
  });
}
async function policyRegions(allowed_regions) {
  const policies = await rows('ui-policy');
  const policy = policies.find(p => p.policy_id === 'customer-eu-processing');
  assert.ok(policy);
  await command(`/api/policies/${policy.policy_id}`, {
    expected_revision: policy.policy_revision, changes: { allowed_regions },
  });
  await until('new policy revision is current', async () => {
    const current = await rows('ui-policy');
    return current.length > 0 && current.every(p => p.current && BigInt(p.policy_revision) > BigInt(policy.policy_revision));
  });
}
async function rejectedCommands() {
  assert.ok(process.env.INTERNAL_TOKEN, 'Internal command authorization is required');
  const [gpu] = await rows('ui-gpus');
  const [before] = await rows('ui-placements');
  const path = `/api/gpus/${gpu.gpu_id}/telemetry`;
  const headers = { authorization: `Bearer ${process.env.INTERNAL_TOKEN}`, 'content-type': 'application/json' };
  const valid = { expected_revision: gpu.telemetry_revision, changes: { background_compute_units: 10 } };
  for (const [url, options, status] of [
    [path, { method: 'PATCH', headers: { 'content-type': 'application/json' }, body: JSON.stringify(valid) }, 403],
    [path, { method: 'PATCH', headers: { ...headers, origin: 'https://untrusted.invalid' }, body: JSON.stringify(valid) }, 403],
    [path, { method: 'PATCH', headers, body: JSON.stringify({ ...valid,
      expected_revision: (BigInt(gpu.telemetry_revision) + 100n).toString() }) }, 409],
    [path, { method: 'PATCH', headers, body: JSON.stringify({ ...valid, changes: { instant_health: 'healthy' } }) }, 422],
    [path, { method: 'PATCH', headers, body: JSON.stringify({ ...valid, changes: { background_compute_units: -1 } }) }, 422],
    [`/api/gpus/${randomUUID()}/telemetry`, { method: 'PATCH', headers, body: JSON.stringify(valid) }, 404],
    ['/api/demo/presets/not-a-fixture', { method: 'POST', headers }, 422],
    ['/api/v1/instances/gpu-demo/queries/input-settings/results', {}, 404],
    ['/api/v1/instances/gpu-demo/queries/ui-gpus', { method: 'POST', headers }, 405],
  ]) {
    const response = await fetch(`${demo}${url}`, { signal: AbortSignal.timeout(15_000), ...options });
    assert.equal(response.status, status, `${url} must reject the invalid request`);
  }
  const current = (await rows('ui-gpus')).find(g => g.gpu_id === gpu.gpu_id);
  assert.equal(current.telemetry_revision, gpu.telemetry_revision);
  assert.equal(current.background_compute_units, gpu.background_compute_units);
  const [after] = await rows('ui-placements');
  assert.equal(after.observation_epoch, before.observation_epoch);
  assert.equal(after.desired_plan_version, before.desired_plan_version);
  assert.equal((await json(demo, '/health/ready')).ready, true);
  console.log('Live rejection checks passed: authorization/origin, revisions, invalid fields/ranges, missing IDs and read-route isolation; no state mutation.');
}

async function commandLifecycle() {
  await addWorkload('lifecycle-service', 'embeddings-v1', 1);
  await replicas(9);
  for (const changes of [{ replicas: 2 }, { profile_id: 'reranker-v1' }]) {
    const workload = (await rows('ui-workloads')).find(w => w.name === 'lifecycle-service');
    assert.ok(workload);
    await command(`/api/workloads/${workload.workload_id}`, { expected_revision: workload.revision, changes });
    await until('updated workload parameters are observed', async () =>
      (await rows('ui-workloads')).some(w => w.workload_id === workload.workload_id
        && BigInt(w.revision) > BigInt(workload.revision)
        && Object.entries(changes).every(([key, value]) => w[key] === value)));
    await replicas(10);
  }
  console.log('Workload commands passed: create, scale and serving-profile update reach fresh confirmation.');
  let workload = (await rows('ui-workloads')).find(w => w.name === 'lifecycle-service');
  const saved = (await rows('ui-placements'))[0].desired_plan_version;
  const denial = await command(`/api/workloads/${workload.workload_id}`, {
    expected_revision: workload.revision, changes: { purpose: 'customer-support' },
  });
  await until('denied workload is acknowledged fenced while allowed workloads keep running', async () => {
    const workloads = await rows('ui-workloads');
    return workloads.length === 5 && workloads.every(w => w.workload_id === workload.workload_id
      ? w.revision === denial.revision && w.purpose === 'customer-support'
        && w.fenced_replicas === 2 && w.running_replicas === 0 && w.fencing_pending_replicas === 0
      : w.running_replicas === w.replicas);
  });
  await infeasible(saved);
  workload = (await rows('ui-workloads')).find(w => w.workload_id === workload.workload_id);
  await command(`/api/workloads/${workload.workload_id}`, undefined, 'DELETE', { 'if-match': workload.revision });
  await until('deleted workload is removed from the query', async () =>
    !(await rows('ui-workloads')).some(w => w.workload_id === workload.workload_id));
  await replicas(8);
  console.log('Workload denial/deletion passed: acknowledged fencing, no partial plan, and restored baseline execution.');
  const workloads = await rows('ui-workloads');
  let remaining = 8;
  for (const workload of workloads) {
    await command(`/api/workloads/${workload.workload_id}`, undefined, 'DELETE', { 'if-match': workload.revision });
    remaining -= workload.replicas;
    await until('deleted workload is absent', async () =>
      !(await rows('ui-workloads')).some(w => w.workload_id === workload.workload_id));
    await replicas(remaining);
  }
  assert.equal((await rows('ui-workloads')).length, 0);
  await until('zero-requirement scenario is observed ready', async () =>
    (await rows('ui-status'))[0]?.scenario_ready === true);
  console.log('Empty workload set passed: a complete empty saved/applied plan, no active execution, observed readiness.');
  for (const gpu of await rows('ui-gpus')) {
    await command(`/api/gpus/${gpu.gpu_id}`, undefined, 'DELETE', { 'if-match': gpu.inventory_revision });
    await until('deleted GPU is absent', async () =>
      !(await rows('ui-gpus')).some(g => g.gpu_id === gpu.gpu_id));
  }
  await until('empty fleet has zero registered workers and current readiness', async () => {
    const [clusters, [status]] = await Promise.all([rows('ui-clusters'), rows('ui-status')]);
    return clusters.length === 1 && clusters[0].registered_workers === 0 && status?.scenario_ready === true;
  });
  await replicas(0);
  await addHost('empty-fleet-recovery', 'eu-primary');
  await addWorkload('restored-service', 'chat-v1', 1);
  await replicas(1);
  console.log('Empty fleet recovery passed: inventory removal, ready VM registration and new workload confirmation.');
  await reset('regional-boundary');
  const original = (await rows('ui-placements'))[0].desired_plan_version;
  await policyRegions([]);
  await until('all execution is acknowledged fenced after complete policy revocation', async () => {
    const workloads = await rows('ui-workloads');
    return workloads.length === 4 && workloads.every(w => w.fenced_replicas === w.replicas
      && w.running_replicas === 0 && w.ready_replicas === 0 && w.fencing_pending_replicas === 0);
  });
  await infeasible(original);
  await until('read-only capacity diagnostic completes', async () =>
    (await rows('ui-resilience'))[0]?.capacity_only_feasible === true);
  assert.equal((await rows('ui-placements'))[0].desired_plan_version, original);
  await policyRegions(['northeurope']);
  await onlyRegion('northeurope');
  const [north] = await rows('ui-placements');
  await policyRegions(['westeurope', 'northeurope']);
  await until('policy relaxation is committed as reauthorization', async () => {
    const [plan] = await rows('ui-placements');
    return plan?.status === 'confirmed' && BigInt(plan.desired_plan_version) > BigInt(north.desired_plan_version);
  });
  assert.deepEqual((await rows('ui-placements'))[0].desired, north.desired);
  await reset('baseline');
  console.log('Policy lifecycle passed: full revocation/fencing, non-writing capacity diagnostic, permitted recovery and zero-move reauthorization.');
}

async function concurrentCommands() {
  const [gpu] = await rows('ui-gpus');
  const headers = { authorization: `Bearer ${process.env.INTERNAL_TOKEN}`, 'content-type': 'application/json' };
  const requests = [11, 12].map(background_compute_units => fetch(`${demo}/api/gpus/${gpu.gpu_id}/telemetry`, {
    method: 'PATCH', headers, signal: AbortSignal.timeout(15_000),
    body: JSON.stringify({ expected_revision: gpu.telemetry_revision, changes: { background_compute_units } }),
  }));
  const responses = await Promise.all(requests);
  assert.deepEqual(responses.map(r => r.status).sort(), [200, 409], 'Only one concurrent revision-checked edit may commit');
  const winner = responses.findIndex(r => r.status === 200);
  const committed = await responses[winner].json();
  await until('only the winning edit reaches fresh measurements', async () => {
    const current = (await rows('ui-gpus')).find(g => g.gpu_id === gpu.gpu_id);
    return current.telemetry_revision === committed.revision
      && current.sample_telemetry_revision === committed.revision
      && current.background_compute_units === [11, 12][winner]
      && current.reported_background_compute_units === [11, 12][winner];
  });
  await confirmed();
  await telemetry(gpu.gpu_id, { background_compute_units: gpu.background_compute_units });
  await until('restored demand is measured', async () =>
    (await rows('ui-gpus')).find(g => g.gpu_id === gpu.gpu_id).reported_background_compute_units === gpu.background_compute_units);

  const key = randomUUID();
  const payload = { name: 'concurrent-service', profile_id: 'embeddings-v1', replicas: 1,
    spread_across_domains: true, data_profile_id: 'demo-open', purpose: 'demo' };
  const [first, retry] = await Promise.all([0, 1].map(() =>
    command('/api/workloads', payload, 'POST', { 'idempotency-key': key })));
  assert.deepEqual(first, retry, 'Concurrent retries must return the same committed identity');
  await replicas(9);
  const created = (await rows('ui-workloads')).filter(w => w.name === payload.name);
  assert.equal(created.length, 1, 'Concurrent retry must not create duplicate work');
  const conflict = await fetch(`${demo}/api/workloads`, {
    method: 'POST', headers: { ...headers, 'idempotency-key': key },
    body: JSON.stringify({ ...payload, replicas: 2 }), signal: AbortSignal.timeout(15_000),
  });
  assert.equal(conflict.status, 409, 'An idempotency key cannot be reused for different input');
  assert.equal((await rows('ui-workloads')).find(w => w.workload_id === created[0].workload_id).replicas, 1);
  await command(`/api/workloads/${created[0].workload_id}`, undefined, 'DELETE', { 'if-match': created[0].revision });
  await replicas(8);
  await reset('baseline');
  const historical = await command('/api/workloads', payload, 'POST', { 'idempotency-key': key });
  assert.deepEqual(historical, first, 'Reset must preserve the receipt of an old command rather than replay its mutation');
  assert.ok((await rows('ui-workloads')).every(w => w.name !== payload.name), 'An old retry must not resurrect deleted work after reset');
  await replicas(8);
  console.log('Live concurrency passed: one revision winner, exactly-once create retries, conflicting retry rejection, cleanup and non-replaying historical receipts after reset.');
}

async function restartSnapshot() {
  await replicas(8);
  const [[plan], gpus, workloads] = await Promise.all([
    rows('ui-placements'), rows('ui-gpus'), rows('ui-workloads'),
  ]);
  const fields = ['gpu_id', 'name', 'host_id', 'cluster_id', 'region', 'slot', 'memory_mib',
    'inventory_revision', 'telemetry_revision', 'powered_on', 'reporting_enabled',
    'scheduling_enabled', 'background_compute_units', 'background_memory_mib', 'interval_ms'];
  return {
    epoch: plan.observation_epoch,
    version: plan.desired_plan_version,
    desired: plan.desired.toSorted((a, b) => a.id.localeCompare(b.id)),
    gpus: gpus.map(g => Object.fromEntries(fields.map(field => [field, g[field]])))
      .sort((a, b) => a.gpu_id.localeCompare(b.gpu_id)),
    workloads: workloads.toSorted((a, b) => a.workload_id.localeCompare(b.workload_id)),
  };
}
async function prepareRestart() {
  const key = randomUUID();
  const payload = { name: 'restart-receipt-service', profile_id: 'embeddings-v1', replicas: 1,
    spread_across_domains: true, data_profile_id: 'demo-open', purpose: 'demo' };
  const receipt = await command('/api/workloads', payload, 'POST', { 'idempotency-key': key });
  await replicas(9);
  const created = (await rows('ui-workloads')).find(w => w.name === payload.name);
  assert.ok(created);
  await command(`/api/workloads/${created.workload_id}`, undefined, 'DELETE', { 'if-match': created.revision });
  await replicas(8);
  const [gpu] = await rows('ui-gpus');
  await telemetry(gpu.gpu_id, { background_compute_units: 20 });
  await until('non-default restart setting is measured', async () => {
    const current = (await rows('ui-gpus')).find(g => g.gpu_id === gpu.gpu_id);
    return current.reported_background_compute_units === 20
      && current.sample_telemetry_revision === current.telemetry_revision;
  });
  return { ...await restartSnapshot(), command: { key, payload, receipt } };
}
async function verifyRestart() {
  assert.ok(process.env.RESTART_SNAPSHOT, 'Restart verification requires the pre-shutdown snapshot');
  const before = JSON.parse(process.env.RESTART_SNAPSHOT);
  const after = await restartSnapshot();
  assert.notEqual(after.epoch, before.epoch, 'A new runtime must own a fresh observation epoch');
  for (const field of ['version', 'desired', 'gpus', 'workloads']) {
    assert.deepEqual(after[field], before[field], `Restart must preserve ${field}`);
  }
  const historical = await command('/api/workloads', before.command.payload, 'POST',
    { 'idempotency-key': before.command.key });
  assert.deepEqual(historical, before.command.receipt, 'Restart must preserve the original command receipt');
  assert.ok((await rows('ui-workloads')).every(w => w.name !== before.command.payload.name),
    'Replaying the receipt must not recreate deleted work');
  await replicas(8);
  console.log('Full stack restart passed: fresh epoch, unchanged saved plan/identities/non-default settings, fresh confirmation and durable non-replaying receipts.');
}

if (mode === '--commands' || mode === '--concurrency') await reset('baseline');
const health = await json(demo, '/health/ready');
assert.equal(health.ready, true);
const instances = await json(drasi, '/api/v1/instances');
assert.deepEqual(instances.data.map(instance => instance.id), ['gpu-demo']);
for (const query of queryIds) {
  const config = await json(demo, `/api/v1/instances/gpu-demo/queries/${query}?view=full`);
  assert.equal(config.data.id, query);
  assert.equal(config.data.status.toLowerCase(), 'running');
  assert.ok((await rows(query)).length > 0, `${query}: no live rows`);
}
await confirmed();
await until('current resilience assessment', async () => {
  const [assessment] = await rows('ui-resilience');
  return assessment?.status === 'current';
});
const stream = await fetch(`${demo}/events/gpu-demo`, { signal: AbortSignal.timeout(10_000) });
assert.equal(stream.status, 200);
assert.match(stream.headers.get('content-type'), /text\/event-stream/);
const reader = stream.body.getReader();
let buffer = '';
let observed = false;
try {
  while (!observed) {
    const { done, value } = await reader.read();
    assert.equal(done, false, 'SSE ended before a live query update');
    buffer += new TextDecoder().decode(value);
    let end;
    while ((end = buffer.indexOf('\n\n')) >= 0) {
      const event = buffer.slice(0, end); buffer = buffer.slice(end + 2);
      const data = event.split('\n').find(line => line.startsWith('data:'));
      if (data) {
        const message = JSON.parse(data.slice(5));
        if (queryIds.includes(message.queryId)) observed = true;
      }
    }
  }
} finally { await reader.cancel(); }
if (mode === '--restart-snapshot') {
  console.log(JSON.stringify(await prepareRestart()));
} else {
  console.log('Live smoke passed: one instance, nine validated query feeds, confirmed placement, resilience and actual SSE updates.');
}
if (mode === '--restart-verify') await verifyRestart();
if (['--errors', '--concurrency', '--commands', '--runbook', '--acceptance'].includes(mode)) await rejectedCommands();
if (mode === '--concurrency' || mode === '--acceptance') await concurrentCommands();
if (mode === '--commands' || mode === '--acceptance') await commandLifecycle();
if (mode === '--runbook' || mode === '--acceptance') {
  await reset('baseline');
  const [originalPlan] = await rows('ui-placements');
  const originalGpu = (await rows('ui-gpus')).find(row => row.host_id === 'inference-a' && row.slot === 0);
  assert.ok(originalGpu);
  const target = originalGpu.gpu_id;
  await telemetry(target, { background_compute_units: 35 });
  await until('command commit appears in fresh generated reports', async () => {
    const gpu = (await rows('ui-gpus')).find(row => row.gpu_id === target);
    return gpu.background_compute_units === 35 && gpu.reported_background_compute_units === 35
      && gpu.sample_telemetry_revision === gpu.telemetry_revision;
  });
  await until('the measured capacity change produces a new saved fleet plan', async () => {
    const [plan] = await rows('ui-placements');
    return BigInt(plan.desired_plan_version) > BigInt(originalPlan.desired_plan_version)
      && plan.desired.every(assignment => assignment.gpu_id !== target);
  });
  await confirmed();
  const [moved] = await rows('ui-placements');
  assert.equal(moved.desired.filter(a => originalPlan.desired.some(old => old.id === a.id && old.gpu_id !== a.gpu_id)).length, 1);
  console.log('Live feedback passed: command -> PostgreSQL -> reports -> solver -> HTTP plan writer -> PostgreSQL -> application -> fresh confirmation.');
  await telemetry(target, { background_compute_units: originalGpu.background_compute_units });
  await until('restored background load is measured', async () =>
    (await rows('ui-gpus')).find(row => row.gpu_id === target).reported_background_compute_units === originalGpu.background_compute_units);
  await confirmed();
  assert.deepEqual((await rows('ui-placements'))[0].desired, moved.desired, 'Restored capacity must not cause gratuitous moves');

  await reset('fragmentation');
  await addWorkload('document-assistant', 'assistant-v1', 1);
  await replicas(7);
  const [fragmentedPlan] = await rows('ui-placements');
  const decision = (await rows('ui-decisions')).find(d => d.decision_id === fragmentedPlan.decision_id);
  assert.ok(decision);
  assert.equal(decision.moved_replicas, 1);
  assert.equal(decision.new_replicas, 1);
  assert.equal(decision.pre_plan_free_memory_mib, 336 * 1024);
  assert.equal(decision.pre_plan_largest_gap_mib, 56 * 1024);
  assert.equal((await rows('ui-gpus')).length, 6);
  console.log('Fragmentation passed: one existing move, one new replica, preserved pre-plan evidence, no added GPUs.');

  await reset('baseline');
  await resilience(r => r.workers.length === 3 && r.workers.every(w => w.feasible));
  await addWorkload('tenant-chat', 'chat-v1', 2);
  await replicas(10);
  await resilience(r => r.workers.length === 3 && r.workers.every(w => w.feasible === false));
  const [beforeCapacity] = await rows('ui-placements');
  await addHost('inference-d', 'eu-primary');
  await replicas(10);
  await resilience(r => r.workers.length === 4 && r.workers.every(w => w.feasible));
  assert.deepEqual((await rows('ui-placements'))[0].desired, beforeCapacity.desired);
  console.log('Resilience passed: ten healthy replicas, real worker-loss infeasibility, and ready capacity restoring protection without moves.');

  await reset('baseline');
  const paused = (await rows('ui-gpus')).find(g => g.host_id === 'inference-a' && g.slot === 0);
  const [beforePause] = await rows('ui-placements');
  await telemetry(paused.gpu_id, { reporting_enabled: false });
  await until('reporting pause is observed before heartbeat expiry', async () =>
    (await rows('ui-gpus')).some(g => g.gpu_id === paused.gpu_id && !g.reporting_enabled));
  const latest = (await rows('ui-gpus')).find(g => g.gpu_id === paused.gpu_id);
  assert.equal(latest.health, 'healthy');
  assert.ok(latest.sample_age_ms < 5000);
  const [stillRunning] = await rows('ui-placements');
  assert.equal(stillRunning.desired_plan_version, beforePause.desired_plan_version);
  assert.ok(stillRunning.actual.filter(a => a.gpu_id === paused.gpu_id).every(a => a.state === 'running'));
  await healthyReports([paused.gpu_id], false);
  assert.equal((await rows('ui-gpus')).find(g => g.gpu_id === paused.gpu_id).report_sequence, latest.report_sequence);
  await replannedAway([paused.gpu_id], beforePause.desired_plan_version);
  await replicas(8);
  assert.ok((await rows('ui-placements'))[0].desired.every(a => a.gpu_id !== paused.gpu_id));
  await telemetry(paused.gpu_id, { reporting_enabled: true });
  await healthyReports([paused.gpu_id], true);
  await replicas(8);
  console.log('Non-event detection passed: paused reporting preserves execution/sample, then the real five-second deadline drives recovery.');

  await reset('baseline');
  const [beforePower] = await rows('ui-placements');
  const failedA = await groupPower('hosts', 'inference-a', false);
  assert.equal((await rows('ui-placements'))[0].desired_plan_version, beforePower.desired_plan_version);
  await healthyReports(failedA.map(g => g.gpu_id), false);
  await replannedAway(failedA.map(g => g.gpu_id), beforePower.desired_plan_version);
  await replicas(8);
  const [survivingPlan] = await rows('ui-placements');
  const failedB = await groupPower('hosts', 'inference-b', false);
  await healthyReports(failedB.map(g => g.gpu_id), false);
  await infeasible(survivingPlan.desired_plan_version);
  await groupPower('hosts', 'inference-b', true);
  await healthyReports(failedB.map(g => g.gpu_id), true);
  await replicas(8);
  await groupPower('hosts', 'inference-a', true);
  await healthyReports(failedA.map(g => g.gpu_id), true);
  await resilience(r => r.workers.every(w => w.feasible));
  console.log('Worker failure passed: expiry-based recovery, explicit infeasibility without a partial plan, and real restoration.');

  await reset('regional-boundary');
  assert.equal((await rows('ui-gpus')).length, 14);
  await onlyRegion('westeurope');
  await resilience(r => r.workers.every(w => w.feasible) && r.regions.every(r => r.feasible));
  const west = await groupPower('regions', 'westeurope', false);
  await healthyReports(west.map(g => g.gpu_id), false);
  await onlyRegion('northeurope');
  const [northPlan] = await rows('ui-placements');
  const recovery = await groupPower('hosts', 'recovery-a', false);
  await healthyReports(recovery.map(g => g.gpu_id), false);
  await infeasible(northPlan.desired_plan_version);
  await until('capacity-only analysis proves spare capacity without authorizing a plan', async () =>
    (await rows('ui-resilience'))[0].capacity_only_feasible === true);
  assert.equal((await rows('ui-placements'))[0].desired_plan_version, northPlan.desired_plan_version);
  await addHost('recovery-c', 'eu-recovery');
  await onlyRegion('northeurope');
  assert.equal((await rows('ui-gpus')).length, 16);
  console.log('Regional recovery passed: permitted EU recovery, unused US capacity, non-writing diagnostics and added permitted capacity.');

  await reset('regional-boundary');
  const failedWest = await groupPower('regions', 'westeurope', false);
  await healthyReports(failedWest.map(g => g.gpu_id), false);
  await onlyRegion('northeurope');
  const [beforeFence] = await rows('ui-placements');
  await policyRegions(['westeurope']);
  await until('all prohibited execution is acknowledged fenced', async () => {
    const workloads = await rows('ui-workloads');
    return workloads.every(w => w.fenced_replicas === w.replicas && w.running_replicas === 0
      && w.ready_replicas === 0 && w.fencing_pending_replicas === 0);
  });
  await infeasible(beforeFence.desired_plan_version);
  await groupPower('regions', 'westeurope', true);
  await healthyReports(failedWest.map(g => g.gpu_id), true);
  await onlyRegion('westeurope');
  await policyRegions(['westeurope', 'northeurope']);
  await onlyRegion('westeurope');
  await reset('baseline');
  console.log('Policy revocation passed: acknowledged fencing without an empty success plan, permitted recovery and reauthorization.');
  console.log('Scenario resets passed with fresh epochs/identities, increasing versions and complete query-backed rebootstrap.');
}
if (mode === '--acceptance') {
  assert.equal(health.transaction_completion, 'supported',
    'Full acceptance still requires the user-approved PostgreSQL policy-context consistency contract.');
}
