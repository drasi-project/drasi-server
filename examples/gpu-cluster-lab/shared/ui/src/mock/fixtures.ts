import type { ResultRow } from '@drasi/react/client';
import { replicaReady } from '../Evidence';
import { number, records, text, validateRow, type QueryId } from '../rows';

export type MockRows = Record<QueryId, ResultRow[]>;
export const epoch = 'mock-observation-1';
export const snapshotTime = Date.UTC(2026, 8, 27, 12, 0);
export const signatures = {
  observation_epoch: epoch, scheduling_signature: 'mock-scheduling-initial', policy_signature: 'mock-policy-initial',
};
export const profiles: Record<string, { name: string; memory: number; demand: number }> = {
  'assistant-v1': { name: 'Qwen2.5-32B assistant', memory: 76 * 1024, demand: 55 },
  'chat-v1': { name: 'Llama3.1-8B chat', memory: 24 * 1024, demand: 30 },
  'embeddings-v1': { name: 'BGE embeddings', memory: 4 * 1024, demand: 20 },
  'reranker-v1': { name: 'BGE reranker', memory: 4 * 1024, demand: 25 },
};

export function workload(id: string, name: string, profileId: string, replicas: number, regional: boolean): ResultRow {
  const profile = profiles[profileId];
  if (!profile) throw new Error(`Unknown mock profile: ${profileId}`);
  return {
    workload_id: id, name, profile_id: profileId, replicas, running_replicas: 0, ready_replicas: 0,
    suspended_replicas: 0, fenced_replicas: 0, fencing_pending_replicas: 0,
    memory_mib_per_replica: profile.memory, compute_units_per_replica: profile.demand,
    data_profile_id: regional ? 'customer-eu-documents' : 'demo-open',
    purpose: regional ? 'customer-support' : 'demo', spread_across_domains: true, revision: '1',
  };
}

export function gpu(host: string, cluster: string, slot: number, region = 'westeurope'): ResultRow {
  return {
    gpu_id: `mock-${host}-${slot}`, name: `${host} / GPU ${slot}`, host_id: host, cluster_id: cluster, slot, region,
    memory_mib: 81920, inventory_revision: '1', telemetry_revision: '1', health: 'healthy',
    powered_on: true, reporting_enabled: true, scheduling_enabled: true,
    background_compute_units: 10, background_memory_mib: 0, interval_ms: 1000,
    modeled_memory_used_mib: 0, total_compute_units: 10, busy_percent: 10,
    managed_memory_mib: 0, managed_compute_units: 0, reported_background_compute_units: 10, background_memory_allocated_mib: 0,
    observation_epoch: epoch, sample_age_ms: 200, sample_plan_version: '1', report_sequence: '18',
    report_time_ms: snapshotTime - 200, sample_inventory_revision: '1', sample_telemetry_revision: '1',
    reported_background_memory_requested_mib: 0,
  };
}

export function fixture(name: string): MockRows {
  if (!['baseline', 'fragmentation', 'regional-boundary'].includes(name)) throw new Error(`Unknown preview scenario: ${name}`);
  const regional = name === 'regional-boundary';
  const fragmented = name === 'fragmentation';
  const topology = [
    { id: 'eu-primary', name: 'EU Primary', region: 'westeurope', hosts: ['inference-a', 'inference-b', 'inference-c'] },
    ...(regional ? [
      { id: 'eu-recovery', name: 'EU Recovery', region: 'northeurope', hosts: ['recovery-a', 'recovery-b'] },
      { id: 'us-spare', name: 'US Spare', region: 'eastus', hosts: ['us-a', 'us-b'] },
    ] : []),
  ];
  const gpus = topology.flatMap(c => c.hosts.flatMap(h => [gpu(h, c.id, 0, c.region), gpu(h, c.id, 1, c.region)]));
  const specs = fragmented
    ? [['chat-alpha', 'chat-v1'], ['chat-bravo', 'chat-v1'], ['chat-charlie', 'chat-v1']]
    : [['assistant', 'assistant-v1'], ['chat', 'chat-v1'], ['embeddings', 'embeddings-v1'], ['reranker', 'reranker-v1']];
  const workloads = specs.map(([id, profile]) => workload(`mock-${id}`, id, profile, 2, regional));
  const layout: [string, number, string, number][] = fragmented ? [
    ['chat-alpha', 0, 'inference-a', 0], ['chat-bravo', 0, 'inference-a', 1],
    ['chat-alpha', 1, 'inference-b', 0], ['chat-charlie', 0, 'inference-b', 1],
    ['chat-bravo', 1, 'inference-c', 0], ['chat-charlie', 1, 'inference-c', 1],
  ] : [
    ['assistant', 0, 'inference-a', 0], ['chat', 0, 'inference-a', 1],
    ['assistant', 1, 'inference-b', 0], ['embeddings', 0, 'inference-b', 1],
    ['reranker', 0, 'inference-b', 1], ['chat', 1, 'inference-c', 0],
    ['reranker', 1, 'inference-c', 0], ['embeddings', 1, 'inference-c', 1],
  ];
  const assignments = layout.map(([name, replica, host, slot]) => {
    const w = workloads.find(w => w.workload_id === `mock-${name}`);
    const g = gpus.find(g => g.gpu_id === `mock-${host}-${slot}`);
    if (!w || !g) throw new Error('Invalid mock layout');
    g.modeled_memory_used_mib = number(g, 'modeled_memory_used_mib') + number(w, 'memory_mib_per_replica');
    g.total_compute_units = number(g, 'total_compute_units') + number(w, 'compute_units_per_replica');
    g.busy_percent = Math.min(100, number(g, 'total_compute_units'));
    w.running_replicas = number(w, 'running_replicas') + 1;
    w.ready_replicas = number(w, 'ready_replicas') + 1;
    return allocation(w, replica, text(g, 'gpu_id'));
  });
  const policyId = regional ? 'customer-eu-processing' : 'demo-permissive';
  const rows: MockRows = {
    'ui-gpus': gpus,
    'ui-workloads': workloads,
    'ui-clusters': topology.map(c => ({ cluster_id: c.id, name: c.name, region: c.region, registered_workers: c.hosts.length, healthy_gpus: c.hosts.length * 2 })),
    'ui-placements': [{
      fleet_id: 'demo', ...signatures, status: 'confirmed', reason: 'All required replicas match the applied plan, current policy permission and recent GPU reports.',
      desired: assignments, actual: executions(assignments), desired_plan_version: '1', applied_plan_version: '1',
      confirmed_plan_version: '1', decision_id: 'mock-initial-layout', config_fingerprint: 'mock-config-initial',
    }],
    'ui-resilience': [{
      fleet_id: 'demo', ...signatures, status: 'current',
      workers: topology.flatMap(c => c.hosts.map(excluded => ({ excluded, feasible: true, detail: 'All required replicas can run again in allowed regions after this VM fails.' }))),
      regions: topology.map(c => ({ excluded: c.region, feasible: regional,
        detail: regional ? 'A permitted region has enough capacity for all required replicas.' : 'Loss of the only region leaves no capacity.' })),
      capacity_only_feasible: null, capacity_only_detail: 'Not needed for this result: all replicas already fit within policy.',
    }],
    'ui-decisions': [{
      decision_id: 'mock-initial-layout', ...signatures, outcome: 'feasible', stage: 'committed', plan_version: '1',
      config_fingerprint: 'mock-config-initial', reason_codes: ['fixture-setup'], moves: [],
      summary: 'Prepared starting plan for this scenario, not a new optimizer result.',
      moved_replicas: 0, new_replicas: fragmented ? 6 : 8, retained_replicas: 0,
      pre_plan_free_memory_mib: gpus.length * 81920, pre_plan_largest_gap_mib: 81920,
    }],
    'ui-status': [{
      fleet_id: 'demo', ...signatures, scenario_ready: true, inputs_ready: true, scenario: name, state: 'ready',
      detail: 'Prepared example of component status, not activity observed from a running system.',
      components: ['postgres-source', 'telemetry-simulator', 'regorus-policy', 'placement-solver', 'resilience-assessor', 'plan-writer']
        .map(component_id => ({ component_id, status: 'ready', error: null })),
    }],
    'ui-timeline': [timelineEvent(1, 'confirmed', 'Recent GPU reports confirm the starting plan.', 'mock-initial-layout', '1')],
    'ui-policy': workloads.flatMap(w => topology.map(c => ({
      id: `${w.workload_id}/${c.id}`, workload_id: w.workload_id, purpose: w.purpose,
      observation_epoch: epoch,
      policy_id: policyId, policy_revision: '1', cluster_id: c.id, region: c.region,
      authorization: regional && c.region === 'eastus' ? 'deny' : 'allow', current: true,
      reasons: regional && c.region === 'eastus' ? ['region-not-permitted'] : ['policy-permitted'],
      error: null, authority_ref: regional ? 'customer-eu-contract-v1' : 'demo-fixture',
      allowed_regions: regional ? ['westeurope', 'northeurope'] : ['*'],
      input_fingerprint: `mock-input-${w.workload_id}-${c.id}`, policy_signature: signatures.policy_signature,
      data_profile_id: w.data_profile_id,
    }))),
  };
  bindFixtureAuthorizations(rows);
  measure(rows);
  summarize(rows);
  for (const query of Object.keys(rows) as QueryId[]) rows[query] = rows[query].map(row => validateRow(query, row));
  return rows;
}

export function invalidateEvidence(rows: MockRows, path: string, method: string): void {
  rows['ui-status'][0].state = 'unevaluated';
  rows['ui-status'][0].scheduling_signature = 'mock-scheduling-unevaluated';
  rows['ui-status'][0].policy_signature = 'mock-policy-unevaluated';
  rows['ui-status'][0].detail = 'Preview settings changed. Replica activity, GPU measurements and placement decisions have not been recalculated.';
  rows['ui-status'][0].components = records(rows['ui-status'][0], 'components').map(c => ({ ...c, status: 'not evaluated' }));
  for (const g of rows['ui-gpus']) {
    for (const key of ['modeled_memory_used_mib', 'total_compute_units', 'busy_percent', 'sample_age_ms', 'sample_plan_version',
      'report_sequence', 'report_time_ms', 'sample_inventory_revision', 'sample_telemetry_revision', 'reported_background_memory_requested_mib',
      'managed_memory_mib', 'managed_compute_units', 'reported_background_compute_units', 'background_memory_allocated_mib']) g[key] = null;
    g.health = 'unknown';
  }
  Object.assign(rows['ui-placements'][0], { status: 'unknown', actual: [], confirmed_plan_version: null, applied_plan_version: null,
    reason: 'The previous saved plan is shown for reference only. No new plan or replica activity has been evaluated after this preview edit.' });
  Object.assign(rows['ui-resilience'][0], { status: 'unknown', workers: [], regions: [], capacity_only_feasible: null, capacity_only_detail: 'Not evaluated' });
  rows['ui-policy'] = rows['ui-workloads'].flatMap(w => rows['ui-clusters'].map(c => {
    const old = rows['ui-policy'].find(p => p.workload_id === w.workload_id && p.cluster_id === c.cluster_id && p.data_profile_id === w.data_profile_id)
      ?? rows['ui-policy'].find(p => p.data_profile_id === w.data_profile_id);
    return { ...old, id: `${w.workload_id}/${c.cluster_id}`, workload_id: w.workload_id, cluster_id: c.cluster_id,
      observation_epoch: rows['ui-status'][0].observation_epoch,
      region: c.region, data_profile_id: w.data_profile_id, purpose: w.purpose, policy_id: old?.policy_id ?? 'unresolved',
      policy_revision: old?.policy_revision ?? null, authority_ref: old?.authority_ref ?? 'unresolved',
      allowed_regions: old?.allowed_regions ?? [], policy_signature: 'mock-policy-unevaluated',
      input_fingerprint: 'mock-input-unevaluated', authorization: 'unknown', current: false,
      reasons: ['preview-not-evaluated'], error: 'No policy evaluator is connected.' };
  }));
  for (const w of rows['ui-workloads']) for (const key of ['running_replicas', 'ready_replicas', 'suspended_replicas', 'fenced_replicas', 'fencing_pending_replicas']) w[key] = null;
  for (const c of rows['ui-clusters']) {
    c.healthy_gpus = null;
    c.registered_workers = new Set(rows['ui-gpus'].filter(g => g.cluster_id === c.cluster_id).map(g => g.host_id)).size;
  }
  const sequence = Number(rows['ui-timeline'][0].event_sequence) + 1;
  const event = timelineEvent(sequence, 'preview-edit', `Local ${method} ${path}; no backend request.`, null, null);
  event.time_ms = number(rows['ui-timeline'][0], 'time_ms') + 1000;
  rows['ui-timeline'] = [event, ...rows['ui-timeline']].slice(0, 32);
}

export function allocation(w: ResultRow, index: number, gpuId: string): ResultRow {
  return { id: `${w.workload_id}/${index}`, workload_id: w.workload_id, replica_index: index, gpu_id: gpuId,
    profile_id: w.profile_id, data_profile_id: w.data_profile_id, purpose: w.purpose,
    memory_mib: w.memory_mib_per_replica, compute_units: w.compute_units_per_replica };
}
export function executions(assignments: ResultRow[]): ResultRow[] {
  return assignments.map(a => ({ ...a, state: 'running', reason: 'current-plan-authorized',
    input_fingerprint: `mock-authorized-${a.id}`, acknowledged_at_ms: snapshotTime - 1000 }));
}
export function bindFixtureAuthorizations(rows: MockRows): void {
  rows['ui-placements'][0].actual = records(rows['ui-placements'][0], 'actual').map(a => {
    if (a.state !== 'running') return a;
    const g = findRow(rows['ui-gpus'], 'gpu_id', text(a, 'gpu_id'));
    const pair = rows['ui-policy'].find(p => p.workload_id === a.workload_id && p.cluster_id === g.cluster_id);
    if (!pair || pair.authorization !== 'allow') throw new Error('Illustrated running allocation requires an explicit allow pair');
    return { ...a, input_fingerprint: pair.input_fingerprint };
  });
}
export function timelineEvent(sequence: number, kind: string, message: string, decision: string | null, version: string | null): ResultRow {
  return { event_id: `${epoch}/${sequence}`, observation_epoch: epoch, event_sequence: String(sequence),
    time_ms: snapshotTime + sequence * 1000, component_id: kind === 'preview-edit' ? 'preview' : 'coordinator',
    kind, message, decision_id: decision, plan_version: version };
}
export function measure(rows: MockRows, version = '1', reportTime = number(rows['ui-timeline'][0], 'time_ms') - 200): void {
  for (const g of rows['ui-gpus']) {
    if (!g.powered_on || !g.reporting_enabled) continue;
    const actual = records(rows['ui-placements'][0], 'actual').filter(a => a.gpu_id === g.gpu_id);
    g.managed_memory_mib = actual.filter(a => a.state !== 'fenced').reduce((sum, a) => sum + number(a, 'memory_mib'), 0);
    g.managed_compute_units = actual.filter(a => a.state === 'running').reduce((sum, a) => sum + number(a, 'compute_units'), 0);
    g.reported_background_compute_units = g.background_compute_units;
    g.reported_background_memory_requested_mib = g.background_memory_mib;
    g.background_memory_allocated_mib = Math.min(number(g, 'background_memory_mib'), 81920 - number(g, 'managed_memory_mib'));
    g.modeled_memory_used_mib = number(g, 'managed_memory_mib') + number(g, 'background_memory_allocated_mib');
    g.total_compute_units = number(g, 'background_compute_units') + number(g, 'managed_compute_units');
    g.busy_percent = Math.min(100, number(g, 'total_compute_units'));
    g.sample_age_ms = 200; g.sample_plan_version = version;
    g.report_time_ms = reportTime;
    g.sample_inventory_revision = g.inventory_revision; g.sample_telemetry_revision = g.telemetry_revision;
  }
}
export function summarize(rows: MockRows): void {
  const placement = rows['ui-placements'][0];
  const actual = records(placement, 'actual');
  for (const w of rows['ui-workloads']) {
    const replicas = actual.filter(a => a.workload_id === w.workload_id);
    const pending = replicas.filter(a => a.state === 'fencing-pending').length;
    w.running_replicas = pending ? null : replicas.filter(a => a.state === 'running').length;
    w.suspended_replicas = replicas.filter(a => a.state === 'suspended').length;
    w.fenced_replicas = replicas.filter(a => a.state === 'fenced').length;
    w.fencing_pending_replicas = pending;
    w.ready_replicas = replicas.filter(a => replicaReady(a, {
      placement, status: rows['ui-status'][0], workloads: rows['ui-workloads'],
      gpus: rows['ui-gpus'], policies: rows['ui-policy'],
    })).length;
  }
  for (const c of rows['ui-clusters']) {
    const gpus = rows['ui-gpus'].filter(g => g.cluster_id === c.cluster_id);
    c.registered_workers = new Set(gpus.map(g => g.host_id)).size;
    c.healthy_gpus = gpus.filter(g => g.health === 'healthy').length;
  }
}

export function findRow(rows: ResultRow[], field: string, id: string): ResultRow {
  const row = rows.find(r => text(r, field) === id);
  if (!row) throw new Error(`Mock ${field} not found: ${id}`);
  return row;
}
