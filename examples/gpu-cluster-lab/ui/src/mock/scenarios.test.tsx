import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import MockApp, { mockViews } from './MockApp';
import { scenarios, snapshot } from './scenarios';
import { applyCommand } from './model';
import { clusterReplicaCounts, confirmed, PolicyEvidence, sumCounts } from '../Evidence';
import { number, queryIds, records, rowKey, text, validateRow } from '../rows';
import { bindFixtureAuthorizations, measure, summarize, type MockRows } from './fixtures';

describe('shared replica confirmation evidence', () => {
  it('renders missing policy metadata as unknown rather than a fabricated revision or empty authorization', () => {
    const rows = snapshot('healthy');
    const pair = { ...rows['ui-policy'][0], current: false, authorization: 'unknown',
      policy_revision: null, authority_ref: '', input_fingerprint: '', allowed_regions: [], error: 'Policy record missing' };
    const html = renderToStaticMarkup(<PolicyEvidence row={pair} views={views(rows)}/>);
    expect(html).toContain('Policy settings · unavailable');
    expect(html).toContain('Allowed regions: Unknown');
    expect(html).toContain('Policy record missing');
    expect(html).not.toContain('Allowed regions: None');
  });
  const mutations: [string, (rows: MockRows) => void][] = [
    ['report predates acknowledgement', rows => {
      const a = records(placement(rows), 'actual')[0];
      const g = rows['ui-gpus'].find(g => g.gpu_id === a.gpu_id)!;
      placement(rows).actual = records(placement(rows), 'actual').map((a, index) =>
        index === 0 ? { ...a, acknowledged_at_ms: number(g, 'report_time_ms') + 1 } : a);
    }],
    ['different sample epoch', rows => { rows['ui-gpus'][0].observation_epoch = 'previous-run'; }],
    ['different policy epoch', rows => { rows['ui-policy'][0].observation_epoch = 'previous-run'; }],
    ['excluded GPU', rows => { rows['ui-gpus'][0].scheduling_enabled = false; }],
    ['different authorization input', rows => {
      placement(rows).actual = records(placement(rows), 'actual').map((a, index) =>
        index === 0 ? { ...a, input_fingerprint: 'obsolete-policy-input' } : a);
    }],
    ['changed workload resources', rows => { rows['ui-workloads'][0].memory_mib_per_replica = 1; }],
    ['plan not applied', rows => { placement(rows).applied_plan_version = null; }],
    ['different scheduling generation', rows => { placement(rows).scheduling_signature = 'previous-input'; }],
  ];
  it.each(mutations)('workload and regional counts agree when %s', (_name, mutate) => {
    const rows = snapshot('healthy');
    mutate(rows);
    summarize(rows);
    const ready = total(rows, 'ready_replicas');
    expect(ready).toBeLessThan(8);
    const regional = clusterReplicaCounts(views(rows), 'eu-primary');
    if (regional.ready !== null) expect(ready).toBe(regional.ready);
    expect(confirmed(views(rows))).toBe(false);
  });
});

const views = (rows: MockRows, scene = 'healthy') => mockViews(rows, scene, () => {});
const total = (rows: MockRows, field: string) => sumCounts(rows['ui-workloads'], field);
const placement = (rows: MockRows) => rows['ui-placements'][0];
function checkPlan(rows: MockRows) {
  const assignments = records(placement(rows), 'desired');
  expect(new Set(assignments.map(a => a.id)).size).toBe(assignments.length);
  expect(assignments.length).toBe(total(rows, 'replicas'));
  for (const w of rows['ui-workloads']) {
    const assigned = assignments.filter(a => a.workload_id === w.workload_id);
    expect(assigned.length).toBe(w.replicas);
    if (w.spread_across_domains) expect(new Set(assigned.map(a => rows['ui-gpus'].find(g => g.gpu_id === a.gpu_id)?.host_id)).size).toBe(w.replicas);
    for (const a of assigned) {
      expect(a.memory_mib).toBe(w.memory_mib_per_replica); expect(a.compute_units).toBe(w.compute_units_per_replica);
      for (const key of ['profile_id', 'data_profile_id', 'purpose']) expect(a[key]).toBe(w[key]);
    }
  }
  for (const g of rows['ui-gpus']) {
    const onGpu = assignments.filter(a => a.gpu_id === g.gpu_id);
    if (!onGpu.length) continue;
    expect(g.health).toBe('healthy');
    expect(g.powered_on).toBe(true);
    expect(onGpu.reduce((sum, a) => sum + number(a, 'memory_mib'), 0) + number(g, 'background_memory_mib')).toBeLessThanOrEqual(81920);
    expect(onGpu.reduce((sum, a) => sum + number(a, 'compute_units'), 0) + number(g, 'background_compute_units')).toBeLessThanOrEqual(85);
    for (const a of onGpu) expect(rows['ui-policy'].find(p => p.workload_id === a.workload_id && p.cluster_id === g.cluster_id)?.authorization).toBe('allow');
  }
}

describe('regional replica summaries', () => {
  it.each([
    ['healthy', 8, 8, 8], ['reports-paused', 8, 8, 8], ['reports-expired', 8, 8, 6],
    ['power-off', 8, 6, 6], ['worker-lost', 8, 6, 6], ['worker-recovered', 8, 8, 8],
    ['fragmentation-candidate', 6, 6, 6], ['plan-conflict', 6, 6, 6], ['plan-committed', 7, 6, 0],
    ['plan-applied', 7, 7, 0], ['plan-confirmed', 7, 7, 7], ['solver-timeout', 8, 8, 8],
  ] as const)('%s separates saved targets, actual replicas and individual confirmation', (scene, planned, running, ready) => {
    expect(clusterReplicaCounts(views(snapshot(scene)), 'eu-primary')).toEqual({ planned, running, ready });
  });
  it.each([
    ['regional', [{ planned: 8, running: 8, ready: 8 }, { planned: 0, running: 0, ready: 0 }, { planned: 0, running: 0, ready: 0 }]],
    ['region-recovered', [{ planned: 0, running: 0, ready: 0 }, { planned: 8, running: 8, ready: 8 }, { planned: 0, running: 0, ready: 0 }]],
    ['regional-infeasible', [{ planned: 0, running: 0, ready: 0 }, { planned: 8, running: 4, ready: 4 }, { planned: 0, running: 0, ready: 0 }]],
    ['regional-restored', [{ planned: 0, running: 0, ready: 0 }, { planned: 8, running: 8, ready: 8 }, { planned: 0, running: 0, ready: 0 }]],
    ['policy-unknown', [{ planned: 8, running: 0, ready: 0 }, { planned: 0, running: 0, ready: 0 }, { planned: 0, running: 0, ready: 0 }]],
    ['fencing-pending', [{ planned: 8, running: null, ready: 0 }, { planned: 0, running: 0, ready: 0 }, { planned: 0, running: 0, ready: 0 }]],
    ['policy-fenced', [{ planned: 8, running: 0, ready: 0 }, { planned: 0, running: 0, ready: 0 }, { planned: 0, running: 0, ready: 0 }]],
  ] as const)('%s keeps activity in its actual regional cluster, including empty and failed regions', (scene, expected) => {
    const rows = snapshot(scene);
    expect(rows['ui-clusters'].map(c => clusterReplicaCounts(views(rows), text(c, 'cluster_id')))).toEqual(expected);
  });
  it('updates saved regional targets without moving running counts', () => {
    const rows = snapshot('regional');
    Object.assign(placement(rows), {
      desired: placement(snapshot('region-recovered')).desired, desired_plan_version: '2',
      status: 'awaiting-application', confirmed_plan_version: null,
    });
    expect(clusterReplicaCounts(views(rows), 'eu-primary')).toEqual({ planned: 0, running: 8, ready: 0 });
    expect(clusterReplicaCounts(views(rows), 'eu-recovery')).toEqual({ planned: 8, running: 0, ready: 0 });
  });
  it('shows a saved target before execution is known, but not when the saved plan is missing', () => {
    const rows = snapshot('healthy'), p = placement(rows);
    Object.assign(p, { actual: [], applied_plan_version: null, confirmed_plan_version: null, status: 'awaiting-application' });
    expect(clusterReplicaCounts(views(rows), 'eu-primary')).toEqual({ planned: 8, running: null, ready: null });
    p.desired_plan_version = null;
    expect(clusterReplicaCounts(views(rows), 'eu-primary')).toEqual({ planned: null, running: null, ready: null });
  });
  it('does not turn an unresolvable saved destination into zero planned replicas', () => {
    const rows = snapshot('healthy'), p = placement(rows);
    p.desired = records(p, 'desired').map((a, i) => i === 0 ? { ...a, gpu_id: 'removed-gpu' } : a);
    expect(clusterReplicaCounts(views(rows), 'eu-primary')).toEqual({ planned: null, running: 8, ready: 7 });
  });
  it('distinguishes an empty saved plan from a missing saved plan', () => {
    const rows = snapshot('healthy');
    rows['ui-workloads'] = []; rows['ui-policy'] = [];
    Object.assign(placement(rows), { desired: [], actual: [] });
    expect(clusterReplicaCounts(views(rows), 'eu-primary')).toEqual({ planned: 0, running: 0, ready: 0 });
  });
  it('marks only the region with a pending stop as unknown', () => {
    const rows = snapshot('regional'), p = placement(rows);
    const actual = records(p, 'actual'), desired = records(p, 'desired');
    actual[0].gpu_id = desired[0].gpu_id = 'mock-recovery-a-0';
    p.actual = actual; p.desired = desired;
    bindFixtureAuthorizations(rows); measure(rows);
    expect(clusterReplicaCounts(views(rows), 'eu-primary')).toEqual({ planned: 7, running: 7, ready: 7 });
    expect(clusterReplicaCounts(views(rows), 'eu-recovery')).toEqual({ planned: 1, running: 1, ready: 1 });
    p.actual = records(p, 'actual').map((a, i) => i === 0 ? { ...a, state: 'fencing-pending', acknowledged_at_ms: null } : a);
    expect(clusterReplicaCounts(views(rows), 'eu-primary')).toEqual({ planned: 7, running: 7, ready: 7 });
    expect(clusterReplicaCounts(views(rows), 'eu-recovery')).toEqual({ planned: 1, running: null, ready: 0 });
  });
  it.each(['feed-stale', 'query-error', 'bootstrap'])('%s produces unknown counts, not zero or old success', scene => {
    expect(clusterReplicaCounts(views(snapshot(scene), scene), 'eu-primary')).toEqual({ planned: null, running: null, ready: null });
  });
  it('does not use historical counts after a local edit or incomplete inventory update', () => {
    const edited = applyCommand(snapshot('healthy'), '/api/gpus/mock-inference-a-0/telemetry', 'PATCH',
      { expected_revision: '1', changes: { reporting_enabled: false } });
    expect(clusterReplicaCounts(views(edited), 'eu-primary')).toEqual({ planned: null, running: null, ready: null });
    const partial = snapshot('regional');
    partial['ui-gpus'] = partial['ui-gpus'].slice(1);
    for (const c of partial['ui-clusters']) expect(clusterReplicaCounts(views(partial), text(c, 'cluster_id'))).toEqual({ planned: null, running: null, ready: null });
    expect(clusterReplicaCounts(views(snapshot('regional')), 'not-a-cluster')).toEqual({ planned: null, running: null, ready: null });
  });
  it.each(['observation_epoch', 'scheduling_signature', 'policy_signature', 'applied_plan_version'])('rejects missing or mismatched %s evidence', key => {
    const rows = snapshot('healthy');
    placement(rows)[key] = key === 'applied_plan_version' ? null : 'older';
    expect(clusterReplicaCounts(views(rows), 'eu-primary')).toEqual({ planned: key === 'applied_plan_version' ? 8 : null, running: null, ready: null });
  });
  it('uses the same per-replica readiness checks as full-plan confirmation', () => {
    for (const change of ['expired', 'plan', 'epoch', 'report-before-application', 'authorization', 'unacknowledged', 'context', 'replica-index']) {
      const rows = snapshot('healthy'), p = placement(rows), actual = records(p, 'actual');
      const g = rows['ui-gpus'][0];
      if (change === 'expired') Object.assign(g, { health: 'unreachable', sample_age_ms: 5000 });
      if (change === 'plan') g.sample_plan_version = '2';
      if (change === 'epoch') g.observation_epoch = 'previous';
      if (change === 'report-before-application') g.report_time_ms = number(actual[0], 'acknowledged_at_ms') - 1;
      if (change === 'authorization') actual[0].input_fingerprint = 'old-permission';
      if (change === 'unacknowledged') actual[0].acknowledged_at_ms = null;
      if (change === 'context') actual[0].purpose = 'previous-purpose';
      if (change === 'replica-index') actual[0].replica_index = 99;
      p.actual = actual;
      expect(clusterReplicaCounts(views(rows), 'eu-primary'), change).toEqual({ planned: 8, running: 8, ready: 7 });
      expect(confirmed(views(rows)), change).toBe(false);
    }
    const boundary = snapshot('healthy');
    boundary['ui-gpus'][0].sample_age_ms = 4999;
    expect(clusterReplicaCounts(views(boundary), 'eu-primary')).toEqual({ planned: 8, running: 8, ready: 8 });
  });
});

describe('contract-faithful snapshots', () => {
  it.each(scenarios)('$id validates every projection, pair and stable identity', scene => {
    const rows = snapshot(scene.id);
    for (const query of queryIds) {
      expect(new Set(rows[query].map(row => rowKey(query, row))).size, query).toBe(rows[query].length);
      for (const row of rows[query]) expect(validateRow(query, row)).toEqual(row);
    }
    expect(rows['ui-policy'].length).toBe(rows['ui-clusters'].length * rows['ui-workloads'].length);
    for (const w of rows['ui-workloads']) for (const c of rows['ui-clusters']) {
      const pair = rows['ui-policy'].find(p => p.id === `${w.workload_id}/${c.cluster_id}`)!;
      expect(pair.data_profile_id).toBe(w.data_profile_id); expect(pair.purpose).toBe(w.purpose);
    }
    for (const event of rows['ui-timeline'].filter(e => e.decision_id)) expect(rows['ui-decisions'].some(d => d.decision_id === event.decision_id)).toBe(true);
    for (let i = 1; i < rows['ui-timeline'].length; i++) expect(number(rows['ui-timeline'][i - 1], 'time_ms')).toBeGreaterThan(number(rows['ui-timeline'][i], 'time_ms'));
  });
  it.each(scenarios)('$id renders as mock evidence without any HTTP call', scene => {
    const network = vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('Unexpected network'));
    try {
      const html = renderToStaticMarkup(<MockApp initialSnapshot={scene.id}/>);
      expect(html).toContain('MOCK DATA'); expect(html).toContain('Prepared example data, not live results');
      expect(html).not.toContain('Live updates connected');
      expect(network).not.toHaveBeenCalled();
    } finally { network.mockRestore(); }
  });
  it.each(['healthy', 'fragmented', 'worker-recovered', 'plan-confirmed', 'tenant-demand', 'tenant-restored', 'regional', 'region-recovered', 'regional-restored'])
  ('%s complete plans obey per-GPU capacity, policy and worker separation', id => {
    const rows = snapshot(id);
    checkPlan(rows); expect(confirmed(views(rows))).toBe(true);
  });
  it('separates report loss from execution loss exactly at the heartbeat boundary', () => {
    const start = snapshot('healthy'), paused = snapshot('reports-paused'), expired = snapshot('reports-expired');
    expect(total(paused, 'running_replicas')).toBe(8); expect(total(paused, 'ready_replicas')).toBe(8);
    expect(total(expired, 'running_replicas')).toBe(8); expect(total(expired, 'ready_replicas')).toBe(6);
    for (const after of [paused, expired]) {
      expect(placement(after).actual).toEqual(placement(start).actual);
      for (const field of ['report_sequence', 'report_time_ms', 'modeled_memory_used_mib', 'total_compute_units', 'sample_telemetry_revision']) {
        expect(after['ui-gpus'][0][field]).toEqual(start['ui-gpus'][0][field]);
      }
    }
    expect(paused['ui-gpus'][0].health).toBe('healthy'); expect(expired['ui-gpus'][0].health).toBe('unreachable');
    const g = start['ui-gpus'][0];
    expect(() => validateRow('ui-gpus', { ...g, sample_age_ms: 4999 })).not.toThrow();
    expect(() => validateRow('ui-gpus', { ...g, sample_age_ms: 5000 })).toThrow();
    expect(() => validateRow('ui-gpus', { ...g, health: 'unreachable', sample_age_ms: 5000 })).not.toThrow();
    const off = snapshot('power-off');
    expect(off['ui-gpus'][0].health).toBe('healthy'); expect(total(off, 'running_replicas')).toBe(6);
    expect(total(off, 'ready_replicas')).toBe(6); expect(confirmed(views(off))).toBe(false);
  });
  it('moves exactly one existing chat and distinguishes candidate, commit, application and measurement', () => {
    const start = snapshot('fragmented'), candidate = snapshot('fragmentation-candidate');
    const committed = snapshot('plan-committed'), applied = snapshot('plan-applied'), measured = snapshot('plan-confirmed');
    expect(placement(candidate).desired).toEqual(placement(start).desired);
    expect(candidate['ui-decisions'][0].stage).toBe('candidate');
    expect(candidate['ui-decisions'][0].plan_version).toBeNull();
    expect(placement(committed).actual).toEqual(placement(start).actual);
    expect(placement(committed).desired_plan_version).toBe('2'); expect(placement(committed).applied_plan_version).toBe('1');
    expect(placement(applied).applied_plan_version).toBe('2'); expect(applied['ui-gpus'].every(g => g.sample_plan_version === '1')).toBe(true);
    expect(applied['ui-gpus']).toEqual(committed['ui-gpus']);
    expect(confirmed(views(applied))).toBe(false); expect(total(applied, 'running_replicas')).toBe(7); expect(total(applied, 'ready_replicas')).toBe(0);
    expect(confirmed(views(measured))).toBe(true); expect(total(measured, 'ready_replicas')).toBe(7);
    const ack = number(records(placement(measured), 'actual')[0], 'acknowledged_at_ms');
    expect(measured['ui-gpus'].every(g => number(g, 'report_time_ms') > ack)).toBe(true);
    const d = measured['ui-decisions'][0];
    expect(d.moved_replicas).toBe(1); expect(d.new_replicas).toBe(1); expect(d.retained_replicas).toBe(5);
    expect(d.pre_plan_free_memory_mib).toBe(336 * 1024); expect(d.pre_plan_largest_gap_mib).toBe(56 * 1024);
    const before = records(placement(start), 'desired');
    const after = records(placement(measured), 'desired');
    expect(after.filter(a => before.some(b => a.id === b.id && a.gpu_id !== b.gpu_id))).toHaveLength(1);
  });
  it('treats stale-candidate rejection and solver timeout as no new plan, not partial success', () => {
    const conflict = snapshot('plan-conflict'), timeout = snapshot('solver-timeout');
    expect(placement(conflict).desired_plan_version).toBe('1');
    expect(conflict['ui-decisions'][0].stage).toBe('rejected');
    expect(conflict['ui-timeline'].some(e => e.kind === 'committed')).toBe(false);
    expect(total(conflict, 'running_replicas')).toBe(6);
    expect(placement(timeout).desired).toEqual(placement(snapshot('healthy')).desired);
    expect(timeout['ui-decisions'][0].outcome).toBe('unknown');
    expect(timeout['ui-resilience'][0].capacity_only_feasible).toBeNull();
    expect(confirmed(views(timeout))).toBe(false);
  });
  it('reports current tenant demand separately from loss recovery capacity', () => {
    const demand = snapshot('tenant-demand'), restored = snapshot('tenant-restored');
    expect(demand['ui-workloads'].reduce((sum, w) => sum + number(w, 'compute_units_per_replica') * number(w, 'replicas'), 0)).toBe(320);
    expect(records(demand['ui-resilience'][0], 'workers').every(s => s.feasible === false)).toBe(true);
    expect(records(restored['ui-resilience'][0], 'workers').every(s => s.feasible === true)).toBe(true);
    expect(restored['ui-gpus']).toHaveLength(8);
  });
  it('retains a complete old regional plan on infeasibility and never assigns prohibited US GPUs', () => {
    const recovered = snapshot('region-recovered'), failed = snapshot('regional-infeasible'), restored = snapshot('regional-restored');
    expect(placement(failed).desired).toEqual(placement(recovered).desired);
    expect(total(failed, 'running_replicas')).toBe(4); expect(total(failed, 'ready_replicas')).toBe(4);
    expect(failed['ui-resilience'][0].capacity_only_feasible).toBe(true);
    expect(failed['ui-decisions'][0].outcome).toBe('infeasible');
    expect(records(placement(failed), 'actual')).toHaveLength(4);
    for (const rows of [recovered, failed, restored]) {
      expect(rows['ui-gpus'].filter(g => g.region === 'eastus').every(g => g.health === 'healthy')).toBe(true);
      expect(records(placement(rows), 'desired').every(a => !text(a, 'gpu_id').includes('us-'))).toBe(true);
    }
    expect(restored['ui-gpus']).toHaveLength(16); expect(total(restored, 'ready_replicas')).toBe(8);
    expect(records(restored['ui-resilience'][0], 'workers').find(s => s.excluded === 'recovery-c')?.feasible).toBe(false);
  });
  it('distinguishes unknown suspension, pending stop and acknowledged fencing without rewriting the last sample', () => {
    const start = snapshot('regional');
    const unknown = snapshot('policy-unknown'), pending = snapshot('fencing-pending'), fenced = snapshot('policy-fenced');
    expect(total(unknown, 'suspended_replicas')).toBe(8); expect(total(unknown, 'running_replicas')).toBe(0);
    expect(total(unknown, 'fenced_replicas')).toBe(0); expect(unknown['ui-decisions'][0].outcome).toBe('unknown');
    expect(total(pending, 'fencing_pending_replicas')).toBe(8); expect(total(pending, 'running_replicas')).toBeNull();
    expect(records(placement(pending), 'actual').every(a => a.acknowledged_at_ms === null)).toBe(true);
    expect(total(fenced, 'fenced_replicas')).toBe(8); expect(total(fenced, 'running_replicas')).toBe(0);
    for (const rows of [unknown, pending, fenced]) {
      expect(total(rows, 'ready_replicas')).toBe(0); expect(rows['ui-gpus']).toEqual(start['ui-gpus']);
      expect(placement(rows).desired).toEqual(placement(start).desired); expect(confirmed(views(rows))).toBe(false);
    }
  });
  it('never shows convergence or current green assessments for stale, partial or mismatched evidence', () => {
    const rows = snapshot('healthy');
    for (const scene of ['feed-stale', 'query-error', 'bootstrap']) expect(confirmed(views(snapshot(scene), scene))).toBe(false);
    const partial = structuredClone(rows); partial['ui-policy'] = [];
    expect(confirmed(views(partial))).toBe(false);
    const contextChanged = structuredClone(rows);
    contextChanged['ui-workloads'][0].purpose = 'changed-purpose';
    expect(confirmed(views(contextChanged))).toBe(false);
    const wrongAuthorization = structuredClone(rows);
    placement(wrongAuthorization).actual = records(placement(wrongAuthorization), 'actual').map(a => ({ ...a, input_fingerprint: 'older-input' }));
    expect(confirmed(views(wrongAuthorization))).toBe(false);
    for (const key of ['observation_epoch', 'scheduling_signature', 'policy_signature', 'applied_plan_version', 'confirmed_plan_version']) {
      const mismatch = structuredClone(rows); placement(mismatch)[key] = 'different';
      expect(confirmed(views(mismatch)), key).toBe(false);
    }
    const html = renderToStaticMarkup(<MockApp initialSnapshot="assessment-stale"/>);
    expect(html).toContain('Not current'); expect(html).not.toContain('badge good">Every tested case can recover');
    expect(renderToStaticMarkup(<MockApp initialSnapshot="policy-unknown"/>)).toContain('Not current');
    const bootstrap = snapshot('bootstrap');
    expect(total(bootstrap, 'running_replicas')).toBeNull();
    expect(bootstrap['ui-gpus'].every(g => g.health === 'unknown' && g.report_time_ms === null)).toBe(true);
  });
  it('keeps empty clusters and unknown counts without fabricating topology or success', () => {
    let rows = snapshot('healthy');
    for (const g of [...rows['ui-gpus']]) rows = applyCommand(rows, `/api/gpus/${g.gpu_id}`, 'DELETE', undefined, text(g, 'inventory_revision'));
    expect(rows['ui-clusters']).toHaveLength(1); expect(rows['ui-gpus']).toHaveLength(0);
    expect(rows['ui-clusters'][0].registered_workers).toBe(0); expect(total(rows, 'ready_replicas')).toBeNull();
    expect(confirmed(views(rows))).toBe(false);
  });
  it('validates identities, nested evidence, revisions, nullability and mock command fields', () => {
    const rows = snapshot('healthy');
    expect(() => validateRow('ui-policy', { ...rows['ui-policy'][0], id: 'policy/cluster' })).toThrow('identity');
    expect(() => validateRow('ui-placements', { ...placement(rows), actual: [{}] })).toThrow();
    expect(() => validateRow('ui-status', { ...rows['ui-status'][0], fleet_id: undefined })).toThrow();
    expect(() => validateRow('ui-workloads', { ...rows['ui-workloads'][0], running_replicas: -1 })).toThrow();
    expect(() => validateRow('ui-workloads', { ...rows['ui-workloads'][0], running_replicas: undefined })).toThrow();
    expect(() => validateRow('ui-gpus', { ...rows['ui-gpus'][0], busy_percent: 101 })).toThrow();
    expect(() => applyCommand(rows, '/api/gpus/mock-inference-a-0/telemetry', 'PATCH', { expected_revision: '1', changes: { health: 'healthy' } })).toThrow('field');
    expect(() => applyCommand(rows, '/api/gpus/mock-inference-a-0/telemetry', 'PATCH', { expected_revision: '1', changes: { interval_ms: 2001 } })).toThrow('range');
    expect(() => snapshot('invented')).toThrow('Unknown');
  });
});
