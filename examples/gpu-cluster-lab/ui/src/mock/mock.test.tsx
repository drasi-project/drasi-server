import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import MockApp from './MockApp';
import { fixture } from './fixtures';
import { applyCommand } from './model';
import { number, queryIds, rowKey, validateRow } from '../rows';

describe('isolated UI mock preview', () => {
  it.each(['baseline', 'fragmentation', 'regional-boundary'])('validates all nine %s projections and stable identities', name => {
    const rows = fixture(name);
    for (const query of queryIds) {
      expect(new Set(rows[query].map(r => rowKey(query, r))).size).toBe(rows[query].length);
      for (const row of rows[query]) expect(validateRow(query, row)).toEqual(row);
    }
    expect(rows['ui-gpus']).toHaveLength(name === 'regional-boundary' ? 14 : 6);
    expect(rows['ui-workloads'].reduce((sum, w) => sum + number(w, 'replicas') * number(w, 'memory_mib_per_replica'), 0))
      .toBe((name === 'fragmentation' ? 144 : 216) * 1024);
  });
  it('keeps fragmentation totals and gap consistent with the displayed GPU rows', () => {
    const gpus = fixture('fragmentation')['ui-gpus'];
    expect(gpus.reduce((free, g) => free + 80 - number(g, 'modeled_memory_used_mib') / 1024, 0)).toBe(336);
    expect(Math.max(...gpus.map(g => 80 - number(g, 'modeled_memory_used_mib') / 1024))).toBe(56);
  });
  it('renders a populated, explicitly mock UI without making network calls', () => {
    const network = vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('Unexpected backend request'));
    try {
      const html = renderToStaticMarkup(<MockApp/>);
      expect(html).toContain('MOCK DATA');
      expect(html).toContain('inference-a / GPU 0');
      expect(html).toContain('<strong>8</strong> planned');
      expect(html).toContain('<strong>8</strong> running');
      expect(html).toContain('<strong>8</strong> confirmed');
      expect(html).not.toContain('class="metrics"');
      expect(html).not.toContain('Live updates connected');
      expect(network).not.toHaveBeenCalled();
    } finally { network.mockRestore(); }
  });
  it('edits local settings without changing the fixture input or inventing computed outcomes', () => {
    const initial = fixture('baseline');
    const next = applyCommand(initial, '/api/gpus/mock-inference-a-0/telemetry', 'PATCH',
      { expected_revision: '1', changes: { reporting_enabled: false } });
    expect(initial['ui-gpus'][0].reporting_enabled).toBe(true);
    expect(next['ui-gpus'][0].reporting_enabled).toBe(false);
    expect(next['ui-gpus'][0].telemetry_revision).toBe('2');
    expect(next['ui-gpus'][0].modeled_memory_used_mib).toBeNull();
    expect(next['ui-status'][0].state).toBe('unevaluated');
    expect(next['ui-placements'][0].status).toBe('unknown');
    expect(next['ui-workloads'].every(w => w.running_replicas === null && w.ready_replicas === null)).toBe(true);
    expect(() => applyCommand(next, '/api/gpus/mock-inference-a-0/telemetry', 'PATCH',
      { expected_revision: '1', changes: { reporting_enabled: true } })).toThrow('revision');
  });
  it('supports workload and worker forms, deletes, group edits and fixture reset', () => {
    let rows = applyCommand(fixture('baseline'), '/api/workloads', 'POST', {
      name: 'tenant-chat', profile_id: 'chat-v1', replicas: 2,
      data_profile_id: 'demo-open', purpose: 'demo', spread_across_domains: true,
    });
    const workload = rows['ui-workloads'].at(-1)!;
    expect(workload.compute_units_per_replica).toBe(30);
    rows = applyCommand(rows, `/api/workloads/${workload.workload_id}`, 'PATCH',
      { expected_revision: '1', changes: { replicas: 3 } });
    expect(rows['ui-workloads'].at(-1)!.replicas).toBe(3);
    rows = applyCommand(rows, `/api/workloads/${workload.workload_id}`, 'DELETE', undefined, '2');
    expect(rows['ui-workloads']).toHaveLength(4);
    rows = applyCommand(rows, '/api/hosts', 'POST', { host_id: 'inference-d', cluster_id: 'eu-primary' });
    expect(rows['ui-gpus']).toHaveLength(8);
    rows = applyCommand(rows, '/api/hosts/inference-d/telemetry', 'PATCH', {
      gpu_revisions: { 'mock-inference-d-0': '1', 'mock-inference-d-1': '1' }, changes: { powered_on: false },
    });
    expect(rows['ui-gpus'].slice(-2).every(g => g.powered_on === false)).toBe(true);
    rows = applyCommand(rows, '/api/demo/presets/regional-boundary', 'POST');
    expect(rows['ui-gpus']).toHaveLength(14);
    expect(rows['ui-status'][0].state).toBe('ready');
  });
  it('updates policy parameters without claiming to evaluate authorization', () => {
    const rows = applyCommand(fixture('regional-boundary'), '/api/policies/customer-eu-processing', 'PATCH',
      { expected_revision: '1', changes: { allowed_regions: ['northeurope'] } });
    expect(rows['ui-policy'].every(p => p.policy_revision === '2' && p.authorization === 'unknown' && !p.current)).toBe(true);
    expect(rows['ui-policy'][0].allowed_regions).toEqual(['northeurope']);
    expect(() => applyCommand(rows, '/api/unsupported', 'POST', {})).toThrow('does not implement');
  });
});
