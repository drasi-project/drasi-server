import type { ResultRow } from '@drasi/react/client';
import { number, records, text, validateRow, type QueryId } from '../rows';
import { allocation, bindFixtureAuthorizations, executions, findRow, fixture, gpu, measure, snapshotTime, summarize, timelineEvent, workload, type MockRows } from './fixtures';

export const scenarios = [
  { id: 'healthy', fixture: 'baseline', title: 'Baseline · all replicas confirmed', detail: 'All eight service replicas are running and confirmed by GPU reports. The fleet could recover from one VM failure, but not the loss of its only region.' },
  { id: 'reports-paused', fixture: 'baseline', title: 'Reports paused · still within five seconds', detail: 'Processing continues and the last report is retained. It is 4.8 seconds old, so it has not yet passed the five-second deadline.' },
  { id: 'reports-expired', fixture: 'baseline', title: 'Reports paused · now overdue', detail: 'Two GPUs have not reported for more than five seconds. Eight replicas still run, but only six are confirmed by recent reports. A missing report does not prove a GPU has stopped.' },
  { id: 'power-off', fixture: 'baseline', title: 'VM powered off · loss not yet detected', detail: 'Processing on inference-a stops immediately. Its last reports are still recent, so placement decisions have not yet detected the VM failure.' },
  { id: 'worker-lost', fixture: 'baseline', title: 'VM failure · waiting for a new plan', detail: 'The five-second report deadline has passed. Six replicas are still running; a complete replacement plan is needed.' },
  { id: 'worker-recovered', fixture: 'baseline', title: 'VM failure · all replicas recovered', detail: 'All eight replicas now run on inference-b and inference-c. Another VM failure would leave too little capacity. Recovery does not mean uninterrupted service.' },
  { id: 'fragmented', fixture: 'fragmentation', title: 'Memory fragmentation · starting layout', detail: 'Six chat replicas each reserve 24 GiB, one per GPU. There is 336 GiB free overall, but only 56 GiB free on any single GPU—not enough for a 76 GiB assistant.' },
  { id: 'fragmentation-candidate', fixture: 'fragmentation', title: 'Memory fragmentation · plan proposed', detail: 'One assistant replica is requested. The proposed plan fits everything by moving one existing chat replica. It has not been saved or applied.' },
  { id: 'plan-conflict', fixture: 'fragmentation', title: 'Plan rejected · inputs changed', detail: 'The proposed plan used out-of-date inputs and was not saved (409 conflict). The previous saved plan remains; the optimizer must make a new decision.' },
  { id: 'plan-committed', fixture: 'fragmentation', title: 'Memory fragmentation · plan saved', detail: 'Plan v2 is saved to the database. Processing and GPU reports still reflect v1. Dashed tiles show planned replicas, not replicas confirmed running there.' },
  { id: 'plan-applied', fixture: 'fragmentation', title: 'Memory fragmentation · plan applied', detail: 'The simulator has applied v2, but the latest GPU reports still show v1. Applying a plan is not the same as confirming it through new reports.' },
  { id: 'plan-confirmed', fixture: 'fragmentation', title: 'Memory fragmentation · all replicas confirmed', detail: 'New GPU reports confirm v2. All seven replicas are confirmed after moving exactly one existing replica.' },
  { id: 'tenant-demand', fixture: 'baseline', title: 'More demand · no VM failure margin', detail: 'Two tenant-chat replicas raise workload demand to 320 units. Everything fits now, but one VM failure would leave only 300 units available for workloads.' },
  { id: 'tenant-restored', fixture: 'baseline', title: 'More demand · ready VM added', detail: 'Adding a fourth ready VM restores the ability to recover from one VM failure. The demo adds simulated ready capacity; it does not create a real Azure VM.' },
  { id: 'regional', fixture: 'regional-boundary', title: 'Regional policy · all replicas confirmed', detail: 'Policy allows these workloads in West Europe and North Europe, but not East US. East US GPUs still report normally; hardware availability does not override policy.' },
  { id: 'region-recovered', fixture: 'regional-boundary', title: 'West Europe failure · recovered in North Europe', detail: 'All required replicas now run on four North Europe GPUs. Another VM failure there would leave too little capacity in allowed regions.' },
  { id: 'regional-infeasible', fixture: 'regional-boundary', title: 'Another VM failure · cannot recover all replicas', detail: 'Only two allowed GPUs remain: 150 demand units for workloads needing 260. East US capacity is available but prohibited. The previous saved plan is retained; only surviving, allowed replicas run.' },
  { id: 'regional-restored', fixture: 'regional-boundary', title: 'Ready VM added · all replicas recovered', detail: 'Adding recovery-c restores four available GPUs in an allowed region. All eight replicas recover, but there is still not enough capacity for another VM failure there.' },
  { id: 'policy-unknown', fixture: 'regional-boundary', title: 'Policy unavailable · processing paused', detail: 'When policy information becomes unavailable, processing pauses and memory stays reserved. Old permission is not reused. The pause confirmation does not count as a new GPU report.' },
  { id: 'fencing-pending', fixture: 'regional-boundary', title: 'Policy changed · stop requested', detail: 'The new policy allows no regions. Stops have been requested but not confirmed, so they cannot yet be counted as stopped replicas.' },
  { id: 'policy-fenced', fixture: 'regional-boundary', title: 'Policy changed · processing stopped', detail: 'All eight policy stops are confirmed and their resources released, even though no allowed replacement is available. The displayed GPU measurements are still the previous reports.' },
  { id: 'assessment-stale', fixture: 'baseline', title: 'Recovery analysis · out-of-date inputs', detail: 'The recovery result used older placement inputs. It cannot confirm whether the current fleet could recover from a failure.' },
  { id: 'solver-timeout', fixture: 'baseline', title: 'Optimizer time limit · result unknown', detail: 'The optimizer ran out of time. This does not prove a plan is impossible. No new plan is saved; previously allowed work continues.' },
  { id: 'feed-stale', fixture: 'baseline', title: 'Updates disconnected · previous data shown', detail: 'Previously received data stays visible but is marked out of date. Actions are disabled until current data is available.' },
  { id: 'query-error', fixture: 'baseline', title: 'Policy updates unavailable', detail: 'Policy updates failed. Other data stays visible, but previous policy decisions cannot confirm current permission.' },
  { id: 'bootstrap', fixture: 'baseline', title: 'Startup · waiting for initial data', detail: 'The saved plan remains, but processing, GPU report status and policy permission are unknown until initialization completes.' },
] as const;

function setChecking(rows: MockRows) {
  Object.assign(rows['ui-resilience'][0], { status: 'checking', workers: [], regions: [] });
}
function event(rows: MockRows, kind: string, message: string, decision: string | null, version: string | null, earliest = 0) {
  const previous = rows['ui-timeline'][0];
  const next = timelineEvent(Number(previous.event_sequence) + 1, kind, message, decision, version);
  next.time_ms = Math.max(number(previous, 'time_ms') + 1000, earliest);
  rows['ui-timeline'].unshift(next);
  return number(next, 'time_ms');
}
function lose(rows: MockRows, host: string, poweredOff: boolean, expired: boolean) {
  for (const g of rows['ui-gpus'].filter(g => g.host_id === host)) {
    if (poweredOff) g.powered_on = false;
    else g.reporting_enabled = false;
    g.telemetry_revision = '2'; g.sample_age_ms = expired ? 5200 : 4800;
    g.health = expired ? 'unreachable' : 'healthy';
  }
  if (poweredOff) rows['ui-placements'][0].actual = records(rows['ui-placements'][0], 'actual')
    .filter(a => findRow(rows['ui-gpus'], 'gpu_id', text(a, 'gpu_id')).host_id !== host);
  Object.assign(rows['ui-placements'][0], { status: 'blocked', confirmed_plan_version: null,
    reason: expired ? 'GPU reports are overdue; waiting for a replacement plan.' : poweredOff ? 'Processing stopped, but the last GPU reports are still recent.' : 'Reports paused; previous measurements remain.' });
  if (expired) setChecking(rows);
  const g = rows['ui-gpus'].find(g => g.host_id === host)!;
  event(rows, expired ? 'heartbeat-expired' : poweredOff ? 'power-off' : 'reports-paused',
    expired ? `${host}: no GPU reports for five seconds; these GPUs are excluded from new placement plans.`
      : poweredOff ? `${host}: processing stopped; placement decisions still see recent GPU reports.` : `${host}: reports paused; processing continues and the previous reports are retained.`,
    null, null, number(g, 'report_time_ms') + number(g, 'sample_age_ms'));
}
function fullPlan(rows: MockRows, layout: [string, number, string, number][]): ResultRow[] {
  return layout.map(([name, index, host, slot]) => allocation(findRow(rows['ui-workloads'], 'name', name), index, `mock-${host}-${slot}`));
}
function recoveredLayout(a: string, b: string): [string, number, string, number][] {
  return [
    ['assistant', 0, a, 0], ['chat', 0, a, 1], ['embeddings', 0, a, 1], ['reranker', 0, a, 1],
    ['assistant', 1, b, 0], ['chat', 1, b, 1], ['embeddings', 1, b, 1], ['reranker', 1, b, 1],
  ];
}
function commit(rows: MockRows, desired: ResultRow[], reason: string, version = '2') {
  const before = records(rows['ui-placements'][0], 'desired');
  const moved = desired.filter(a => before.some(b => b.id === a.id && b.gpu_id !== a.gpu_id));
  const added = desired.filter(a => !before.some(b => b.id === a.id));
  const free = rows['ui-gpus'].filter(g => g.health === 'healthy' && g.scheduling_enabled).map(g =>
    Math.max(0, number(g, 'memory_mib') - number(g, 'background_memory_mib')
      - before.filter(a => a.gpu_id === g.gpu_id).reduce((sum, a) => sum + number(a, 'memory_mib'), 0)));
  const decisionId = `mock-decision-${version}`;
  Object.assign(rows['ui-placements'][0], { desired, desired_plan_version: version, decision_id: decisionId,
    status: 'awaiting-application', confirmed_plan_version: null, reason, config_fingerprint: 'mock-config-updated' });
  rows['ui-decisions'] = [{
    ...rows['ui-decisions'][0], decision_id: decisionId, plan_version: version, stage: 'committed', outcome: 'feasible',
    config_fingerprint: 'mock-config-updated', reason_codes: [reason], summary: 'Example plan that fits all required replicas. It was prepared for this preview, not calculated in the browser.',
    moved_replicas: moved.length, new_replicas: added.length, retained_replicas: desired.length - moved.length - added.length,
    pre_plan_free_memory_mib: free.reduce((sum, memory) => sum + memory, 0),
    pre_plan_largest_gap_mib: Math.max(0, ...free),
    moves: [...moved, ...added].map(a => ({ replica_id: a.id, from_gpu_id: before.find(b => b.id === a.id)?.gpu_id ?? null,
      to_gpu_id: a.gpu_id, reason })),
  }, ...rows['ui-decisions']];
  event(rows, 'committed', `Complete plan v${version} saved to PostgreSQL.`, decisionId, version);
}
function apply(rows: MockRows, fresh: boolean) {
  const p = rows['ui-placements'][0];
  const version = text(p, 'desired_plan_version');
  p.actual = executions(records(p, 'desired')); p.applied_plan_version = version;
  p.status = fresh ? 'confirmed' : 'awaiting-measurements'; p.confirmed_plan_version = fresh ? version : null;
  p.reason = fresh ? 'Recent GPU reports confirm the full plan, with current policy permission.' : 'The simulator applied the plan. Waiting for GPU reports that confirm this version.';
  const appliedAt = event(rows, 'applied', `The simulator confirmed that plan v${version} was applied.`, text(p, 'decision_id'), version);
  p.actual = records(p, 'actual').map(a => ({ ...a, acknowledged_at_ms: appliedAt }));
  if (fresh) {
    measure(rows, version, appliedAt + 800);
    for (const g of rows['ui-gpus'].filter(g => g.powered_on && g.reporting_enabled)) g.report_sequence = String(BigInt(text(g, 'report_sequence')) + 1n);
    event(rows, 'confirmed', `New GPU reports confirm plan v${version}.`, text(p, 'decision_id'), version);
  }
}
function recoveryEvidence(rows: MockRows, permittedHosts: string[]) {
  const hosts = [...new Set(rows['ui-gpus'].filter(g => g.health === 'healthy').map(g => text(g, 'host_id')))];
  const regions = [...new Set(rows['ui-gpus'].filter(g => g.health === 'healthy').map(g => text(g, 'region')))];
  Object.assign(rows['ui-resilience'][0], { status: 'current',
    workers: hosts.map(excluded => ({ excluded, feasible: !permittedHosts.includes(excluded),
      detail: permittedHosts.includes(excluded) ? 'Losing this VM leaves 150 demand units for workloads needing 260.' : 'Losing this spare VM does not reduce capacity in the allowed regions.' })),
    regions: regions.map(excluded => ({ excluded, feasible: excluded === 'eastus',
      detail: excluded === 'eastus' ? 'The capacity in allowed North Europe remains available.' : 'No other allowed region has available capacity.' })),
  });
}

export function snapshot(id: string): MockRows {
  const scene = scenarios.find(s => s.id === id);
  if (!scene) throw new Error(`Unknown preview snapshot: ${id}`);
  const rows = fixture(scene.fixture);
  const p = rows['ui-placements'][0];
  const r = rows['ui-resilience'][0];
  const status = rows['ui-status'][0];
  status.state = id; status.detail = scene.detail;
  if (['reports-paused', 'reports-expired', 'power-off', 'worker-lost', 'worker-recovered'].includes(id)) {
    lose(rows, 'inference-a', !id.startsWith('reports'), ['reports-expired', 'worker-lost', 'worker-recovered'].includes(id));
    if (id === 'reports-paused') Object.assign(p, { status: 'confirmed', confirmed_plan_version: '1' });
    if (id === 'worker-recovered') {
      const desired = fullPlan(rows, [
        ['assistant', 1, 'inference-b', 0], ['assistant', 0, 'inference-c', 1],
        ['chat', 0, 'inference-b', 1], ['chat', 1, 'inference-c', 0],
        ['embeddings', 0, 'inference-b', 1], ['embeddings', 1, 'inference-c', 0],
        ['reranker', 0, 'inference-b', 1], ['reranker', 1, 'inference-c', 0],
      ]);
      commit(rows, desired, 'device-unreachable'); apply(rows, true);
      recoveryEvidence(rows, ['inference-b', 'inference-c']);
    }
  }
  if (['fragmentation-candidate', 'plan-conflict', 'plan-committed', 'plan-applied', 'plan-confirmed'].includes(id)) {
    const assistant = workload('mock-assistant', 'assistant', 'assistant-v1', 1, false);
    rows['ui-workloads'].push(assistant);
    rows['ui-policy'].push({ ...rows['ui-policy'][0], id: 'mock-assistant/eu-primary', workload_id: 'mock-assistant',
      input_fingerprint: 'mock-input-assistant' });
    const original = structuredClone(p);
    const desired = records(p, 'desired').map(a => a.id === 'mock-chat-alpha/0' ? { ...a, gpu_id: 'mock-inference-c-1' } : a);
    desired.push(allocation(assistant, 0, 'mock-inference-a-0'));
    commit(rows, desired, 'memory-fragmentation');
    setChecking(rows);
    if (id === 'fragmentation-candidate' || id === 'plan-conflict') {
      Object.assign(p, original, { status: 'blocked', confirmed_plan_version: null, reason: 'A new plan is proposed, but not yet saved. The new assistant replica has no assigned GPU.' });
      rows['ui-decisions'][0].stage = 'candidate'; rows['ui-decisions'][0].plan_version = null;
      rows['ui-timeline'][0] = timelineEvent(2, 'candidate', 'A proposed plan fits everything by moving one existing chat replica.', 'mock-decision-2', null);
      if (id === 'plan-conflict') {
        p.reason = scene.detail;
        rows['ui-decisions'][0].stage = 'rejected';
        rows['ui-decisions'][0].summary = 'Inputs changed before the plan could be saved (409 conflict). No new plan was saved.';
        event(rows, 'write-rejected', 'The plan used older inputs and was rejected (409 conflict). The optimizer must make a new decision.', 'mock-decision-2', null);
      }
    }
    if (id === 'plan-applied' || id === 'plan-confirmed') apply(rows, id === 'plan-confirmed');
  }
  if (id === 'tenant-demand' || id === 'tenant-restored') {
    const w = workload('mock-tenant-chat', 'tenant-chat', 'chat-v1', 2, false);
    rows['ui-workloads'].push(w);
    rows['ui-policy'].push({ ...rows['ui-policy'][0], id: `${w.workload_id}/eu-primary`, workload_id: w.workload_id, input_fingerprint: 'mock-tenant-input' });
    const desired = records(p, 'desired');
    desired.push(allocation(w, 0, 'mock-inference-a-1'), allocation(w, 1, 'mock-inference-c-1'));
    if (id === 'tenant-restored') rows['ui-gpus'].push(gpu('inference-d', 'eu-primary', 0), gpu('inference-d', 'eu-primary', 1));
    commit(rows, desired, 'workload-added'); apply(rows, true);
    r.workers = ['inference-a', 'inference-b', 'inference-c', ...(id === 'tenant-restored' ? ['inference-d'] : [])]
      .map(excluded => ({ excluded, feasible: id === 'tenant-restored', detail: id === 'tenant-demand'
        ? 'Workloads need 320 demand units, but only 300 would remain after this VM failure.' : 'Six surviving GPUs can fit 320 demand units while keeping replicas on different VMs.' }));
  }
  if (['region-recovered', 'regional-infeasible', 'regional-restored'].includes(id)) {
    for (const host of ['inference-a', 'inference-b', 'inference-c']) lose(rows, host, true, true);
    commit(rows, fullPlan(rows, recoveredLayout('recovery-a', 'recovery-b')), 'device-unreachable');
    apply(rows, true);
    recoveryEvidence(rows, ['recovery-a', 'recovery-b']);
    if (id !== 'region-recovered') {
      lose(rows, 'recovery-a', true, true);
      Object.assign(p, { status: 'blocked', reason: 'No complete plan fits in allowed regions: workloads need 260 demand units, but only 150 are available.' });
      Object.assign(r, { status: 'current', workers: ['recovery-b', 'us-a', 'us-b'].map(excluded => ({ excluded, feasible: false, detail: 'There is already too little capacity in allowed regions. Another failure cannot improve that.' })),
        regions: [{ excluded: 'northeurope', feasible: false, detail: 'No GPU in an allowed region would remain.' }, { excluded: 'eastus', feasible: false, detail: 'There is already too little capacity in allowed regions.' }],
        capacity_only_feasible: true, capacity_only_detail: 'All replicas would fit if region restrictions were removed. This is a comparison only, not an approved plan.' });
      rows['ui-decisions'].unshift({ ...rows['ui-decisions'][0], decision_id: 'mock-infeasible', outcome: 'infeasible', stage: 'diagnostic', plan_version: null,
        summary: 'US capacity is available but not allowed by policy. The previous saved plan remains; only surviving, allowed replicas continue running.',
        reason_codes: ['policy-restricted-capacity'], moved_replicas: null, retained_replicas: null, new_replicas: null, moves: [] });
      event(rows, 'infeasible', 'VM recovery-a failed. There is no way to fit all required replicas in allowed regions.', 'mock-infeasible', null);
    }
    if (id === 'regional-restored') {
      rows['ui-gpus'].push(gpu('recovery-c', 'eu-recovery', 0, 'northeurope'), gpu('recovery-c', 'eu-recovery', 1, 'northeurope'));
      commit(rows, fullPlan(rows, recoveredLayout('recovery-c', 'recovery-b')), 'device-unreachable', '3');
      apply(rows, true); recoveryEvidence(rows, ['recovery-b', 'recovery-c']);
      r.capacity_only_feasible = null; r.capacity_only_detail = 'Not needed for this result: all replicas now fit in allowed regions.';
    }
  }
  if (['policy-unknown', 'fencing-pending', 'policy-fenced', 'bootstrap'].includes(id)) {
    const unknown = id === 'policy-unknown' || id === 'bootstrap';
    for (const pair of rows['ui-policy']) Object.assign(pair, { authorization: unknown ? 'unknown' : 'deny',
      current: !unknown, error: unknown ? 'Policy input unavailable' : null, reasons: [unknown ? 'policy-input-unavailable' : 'region-not-permitted'],
      policy_revision: '2', allowed_regions: unknown ? pair.allowed_regions : [], policy_signature: 'mock-policy-updated' });
    p.actual = records(p, 'actual').map(a => ({ ...a,
      state: unknown ? 'suspended' : id === 'fencing-pending' ? 'fencing-pending' : 'fenced',
      acknowledged_at_ms: id === 'fencing-pending' ? null : snapshotTime + 2000,
      reason: unknown ? 'policy-unavailable' : 'region-not-permitted', input_fingerprint: unknown ? null : 'mock-denied-input',
    }));
    Object.assign(p, { status: 'blocked', confirmed_plan_version: null, reason: scene.detail });
    Object.assign(r, { status: unknown ? 'unknown' : 'current', workers: [], regions: [], capacity_only_feasible: unknown ? null : true,
      capacity_only_detail: unknown ? 'Missing policy information is different from a policy that prohibits processing.' : 'All replicas would fit without the policy restriction, but that does not give permission to run them.' });
    if (!unknown) {
      r.workers = rows['ui-gpus'].filter(g => g.slot === 0).map(g => ({ excluded: g.host_id, feasible: false, detail: 'Current policy allows no region.' }));
      r.regions = rows['ui-clusters'].map(c => ({ excluded: c.region, feasible: false, detail: 'Current policy allows no region.' }));
    }
    status.policy_signature = 'mock-policy-updated';
    status.components = records(status, 'components').map(c => c.component_id === 'regorus-policy'
      ? { ...c, status: unknown ? 'unavailable' : 'current', error: unknown ? 'Policy input unavailable' : null } : c);
    event(rows, id === 'fencing-pending' ? 'fencing-requested' : unknown ? 'suspended' : 'fenced', scene.detail, null, '1');
    rows['ui-decisions'].unshift({ ...rows['ui-decisions'][0], decision_id: `mock-${id}`, stage: 'diagnostic',
      outcome: unknown ? 'unknown' : 'infeasible', plan_version: null, summary: scene.detail,
      reason_codes: [unknown ? 'policy-input-unavailable' : 'policy-restricted-capacity'],
      moved_replicas: null, new_replicas: null, retained_replicas: null, pre_plan_free_memory_mib: null, pre_plan_largest_gap_mib: null });
    if (id === 'bootstrap') {
      status.scenario_ready = false;
      status.inputs_ready = false;
      p.actual = []; p.applied_plan_version = null;
      for (const g of rows['ui-gpus']) {
        g.health = 'unknown';
        for (const field of ['sample_age_ms', 'sample_plan_version', 'report_sequence', 'managed_memory_mib', 'managed_compute_units',
          'reported_background_compute_units', 'background_memory_allocated_mib', 'modeled_memory_used_mib', 'total_compute_units', 'busy_percent',
          'report_time_ms', 'sample_inventory_revision', 'sample_telemetry_revision', 'reported_background_memory_requested_mib']) g[field] = null;
      }
    }
  }
  if (id === 'solver-timeout') {
    Object.assign(p, { status: 'unknown', confirmed_plan_version: null, reason: scene.detail });
    Object.assign(r, { status: 'unknown', workers: [], regions: [], capacity_only_detail: 'No capacity-only comparison completed after the time limit.' });
    rows['ui-decisions'].unshift({ ...rows['ui-decisions'][0], decision_id: 'mock-timeout', stage: 'diagnostic',
      outcome: 'unknown', plan_version: null, summary: scene.detail, reason_codes: ['solver-timeout'],
      moved_replicas: null, new_replicas: null, retained_replicas: null, pre_plan_free_memory_mib: null, pre_plan_largest_gap_mib: null });
    event(rows, 'solver-timeout', scene.detail, 'mock-timeout', null);
  }
  for (const result of [p, r, status]) {
    result.scheduling_signature = ['healthy', 'fragmented', 'regional'].includes(id) ? 'mock-scheduling-initial' : `mock-scheduling-${id}`;
    result.policy_signature = text(status, 'policy_signature');
  }
  for (const pair of rows['ui-policy']) pair.policy_signature = status.policy_signature;
  for (const d of rows['ui-decisions'].slice(0, 1).filter(d => d.decision_id !== 'mock-initial-layout')) {
    d.scheduling_signature = status.scheduling_signature; d.policy_signature = status.policy_signature;
  }
  if (id === 'assessment-stale') r.scheduling_signature = 'mock-scheduling-older';
  bindFixtureAuthorizations(rows);
  summarize(rows);
  if (id === 'bootstrap') for (const w of rows['ui-workloads']) {
    for (const key of ['running_replicas', 'ready_replicas', 'suspended_replicas', 'fenced_replicas', 'fencing_pending_replicas']) w[key] = null;
  }
  for (const key of Object.keys(rows) as QueryId[]) rows[key] = rows[key].map(row => validateRow(key, row));
  return rows;
}
