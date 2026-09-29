import type { ResultRow } from '@drasi/react/client';
import { number, text, validateRow, type QueryId } from '../rows';
import { findRow, fixture, gpu, invalidateEvidence, profiles, workload, type MockRows } from './fixtures';

function object(value: unknown): ResultRow {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('Invalid mock command body');
  return { ...value };
}

function patch(row: ResultRow, revision: string, expected: unknown, changes: ResultRow): void {
  if (row[revision] !== expected) throw new Error('Mock revision changed. Close and reopen the dialog.');
  Object.assign(row, changes, { [revision]: String(BigInt(text(row, revision)) + 1n) });
}
function allowed(changes: ResultRow, fields: string[]): ResultRow {
  if (!Object.keys(changes).length || Object.keys(changes).some(key => !fields.includes(key))) throw new Error('Unsupported mock change field');
  return changes;
}

export function applyCommand(current: MockRows, path: string, method: string, body?: unknown, expectedRevision?: string): MockRows {
  const parts = path.split('/').filter(Boolean).map(decodeURIComponent);
  if (parts[0] !== 'api') throw new Error('Unsupported mock command');
  if (method === 'POST' && parts.length === 4 && parts[1] === 'demo' && parts[2] === 'presets') return fixture(parts[3]);
  const rows = structuredClone(current);
  const data = body === undefined ? {} : object(body);
  const [, kind, id, subresource] = parts;
  if (method === 'POST' && kind === 'hosts' && parts.length === 2) {
    const host = text(data, 'host_id').trim();
    if (!host || rows['ui-gpus'].some(g => g.host_id === host)) throw new Error('Enter a nonempty VM name that is not already in use');
    const cluster = text(data, 'cluster_id');
    const region = text(findRow(rows['ui-clusters'], 'cluster_id', cluster), 'region');
    if (new Set(rows['ui-gpus'].map(g => g.host_id)).size >= 8) throw new Error('The preview supports at most eight VMs');
    rows['ui-gpus'].push(gpu(host, cluster, 0, region), gpu(host, cluster, 1, region));
  } else if (method === 'POST' && kind === 'workloads' && parts.length === 2) {
    const w = workload(`mock-workload-${crypto.randomUUID()}`, text(data, 'name'), text(data, 'profile_id'), number(data, 'replicas'), false);
    for (const key of ['data_profile_id', 'purpose', 'spread_across_domains']) w[key] = data[key];
    rows['ui-workloads'].push(w);
  } else if ((kind === 'gpus' || kind === 'workloads') && id && (method === 'PATCH' || method === 'DELETE')) {
    if (parts.length !== 3 && !(parts.length === 4 && kind === 'gpus' && subresource === 'telemetry' && method === 'PATCH')) throw new Error('Unsupported mock resource path');
    const query = kind === 'gpus' ? 'ui-gpus' : 'ui-workloads';
    const field = kind === 'gpus' ? 'gpu_id' : 'workload_id';
    const revision = kind === 'gpus' ? subresource === 'telemetry' ? 'telemetry_revision' : 'inventory_revision' : 'revision';
    const row = findRow(rows[query], field, id);
    if (method === 'DELETE') {
      if (row[revision] !== expectedRevision) throw new Error('Mock revision changed. Refresh the preview.');
      rows[query] = rows[query].filter(r => r[field] !== id);
    } else {
      const changes = allowed(object(data.changes), kind === 'workloads'
        ? ['name', 'profile_id', 'replicas', 'data_profile_id', 'purpose', 'spread_across_domains']
        : subresource === 'telemetry' ? ['powered_on', 'reporting_enabled', 'background_compute_units', 'background_memory_mib', 'interval_ms']
          : ['scheduling_enabled']);
      if (kind === 'workloads' && changes.profile_id !== undefined) {
        const profile = profiles[text(changes, 'profile_id')];
        if (!profile) throw new Error('This model resource profile is not available in the preview');
        changes.memory_mib_per_replica = profile.memory;
        changes.compute_units_per_replica = profile.demand;
      }
      patch(row, revision, data.expected_revision, changes);
    }
  } else if ((kind === 'hosts' || kind === 'regions') && subresource === 'telemetry' && method === 'PATCH') {
    const members = rows['ui-gpus'].filter(g => kind === 'hosts' ? g.host_id === id
      : findRow(rows['ui-clusters'], 'cluster_id', text(g, 'cluster_id')).region === id);
    const revisions = object(data.gpu_revisions);
    if (!members.length || Object.keys(revisions).length !== members.length) throw new Error('VM or region inventory changed. Reload the scenario before editing.');
    const changes = allowed(object(data.changes), ['powered_on']);
    for (const row of members) patch(row, 'telemetry_revision', revisions[text(row, 'gpu_id')], changes);
  } else if (kind === 'policies' && id && method === 'PATCH' && parts.length === 3) {
    const policies = rows['ui-policy'].filter(p => p.policy_id === id);
    if (!policies.length) throw new Error('This policy is not available in the preview');
    const changes = allowed(object(data.changes), ['allowed_regions']);
    if (!Array.isArray(changes.allowed_regions) || !changes.allowed_regions.every(r => ['*', 'westeurope', 'northeurope', 'eastus'].includes(r))) throw new Error('Invalid mock regions');
    for (const row of policies) patch(row, 'policy_revision', data.expected_revision, changes);
  } else {
    throw new Error(`Mock preview does not implement ${method} ${path}`);
  }
  for (const g of rows['ui-gpus']) {
    if (number(g, 'interval_ms') < 250 || number(g, 'interval_ms') > 2000 || number(g, 'background_compute_units') > 200
      || number(g, 'background_memory_mib') > 81920) throw new Error('GPU report settings are out of range');
  }
  for (const w of rows['ui-workloads']) {
    if (!text(w, 'name').trim() || !Number.isInteger(w.replicas) || number(w, 'replicas') < 0 || number(w, 'replicas') > 32) {
      throw new Error('Mock workloads require a name and 0–32 replicas');
    }
  }
  invalidateEvidence(rows, path, method);
  for (const query of Object.keys(rows) as QueryId[]) rows[query] = rows[query].map(row => validateRow(query, row));
  return rows;
}
