import type { ResultRow } from '@drasi/react/client';

export const queryIds = ['ui-gpus', 'ui-workloads', 'ui-placements', 'ui-resilience',
  'ui-decisions', 'ui-status', 'ui-timeline', 'ui-clusters', 'ui-policy'] as const;
export type QueryId = typeof queryIds[number];
const keys: Record<QueryId, string> = {
  'ui-gpus': 'gpu_id', 'ui-workloads': 'workload_id', 'ui-placements': 'fleet_id',
  'ui-resilience': 'fleet_id', 'ui-decisions': 'decision_id', 'ui-status': 'fleet_id',
  'ui-timeline': 'event_id', 'ui-clusters': 'cluster_id', 'ui-policy': 'id',
};
export function rowKey(query: QueryId, row: Readonly<ResultRow>): string {
  const value = row[keys[query]];
  if (typeof value !== 'string' || value.length === 0) throw new Error(`${query}: missing stable ${keys[query]}`);
  return value;
}
export function text(row: Readonly<ResultRow>, field: string): string {
  const value = row[field];
  if (typeof value !== 'string') throw new Error(`Invalid ${field} in query row`);
  return value;
}
export function number(row: Readonly<ResultRow>, field: string): number {
  const value = row[field];
  if (typeof value !== 'number' || !Number.isFinite(value)) throw new Error(`Invalid ${field} in query row`);
  return value;
}
export function boolean(row: Readonly<ResultRow>, field: string): boolean {
  const value = row[field];
  if (typeof value !== 'boolean') throw new Error(`Invalid ${field} in query row`);
  return value;
}
export function nullableNumber(row: Readonly<ResultRow>, field: string): number | null {
  return row[field] === null ? null : number(row, field);
}
export function nullableText(row: Readonly<ResultRow>, field: string): string | null {
  return row[field] === null ? null : text(row, field);
}
export function records(row: Readonly<ResultRow>, field: string): ResultRow[] {
  const value = row[field];
  if (!Array.isArray(value)) throw new Error(`Invalid ${field} array`);
  return value.map(item => {
    if (!item || typeof item !== 'object' || Array.isArray(item)) throw new Error(`Invalid ${field} record`);
    return { ...item };
  });
}
export function strings(row: Readonly<ResultRow>, field: string): string[] {
  const value = row[field];
  if (!Array.isArray(value) || !value.every(v => typeof v === 'string')) throw new Error(`Invalid ${field} strings`);
  return value;
}
export function recordMap(row: Readonly<ResultRow>, field: string, identity: string): ResultRow[] | null {
  const value = row[field];
  if (value === undefined || value === null) return null;
  if (typeof value !== 'object' || Array.isArray(value)) throw new Error(`Invalid ${field} map`);
  return Object.entries(value).map(([key, item]) => {
    if (!item || typeof item !== 'object' || Array.isArray(item)) throw new Error(`Invalid ${field} record`);
    const record = { ...item };
    if (!(identity in record) || record[identity] !== key) throw new Error(`Invalid ${field} identity`);
    return record;
  });
}
function choice(row: Readonly<ResultRow>, field: string, options: readonly string[]) {
  if (!options.includes(text(row, field))) throw new Error(`Invalid ${field} value`);
}
function counter(row: Readonly<ResultRow>, field: string, nullable = false) {
  if (nullable && row[field] === null) return;
  const n = number(row, field);
  if (!Number.isSafeInteger(n) || n < 0) throw new Error(`Invalid ${field} count`);
}
function version(row: Readonly<ResultRow>, field: string, nullable = false) {
  if (nullable && row[field] === null) return;
  if (!/^[1-9]\d*$/.test(text(row, field))) throw new Error(`Invalid ${field} revision`);
}
function signature(row: Readonly<ResultRow>) {
  for (const field of ['observation_epoch', 'scheduling_signature', 'policy_signature']) text(row, field);
}
function assignment(row: Readonly<ResultRow>) {
  for (const field of ['id', 'workload_id', 'gpu_id', 'profile_id', 'data_profile_id', 'purpose']) text(row, field);
  for (const field of ['replica_index', 'memory_mib', 'compute_units']) counter(row, field);
  if (row.id !== `${row.workload_id}/${row.replica_index}`) throw new Error('Invalid replica identity');
}
function unique(items: ResultRow[], key: string) {
  if (new Set(items.map(r => r[key])).size !== items.length) throw new Error(`Duplicate ${key}`);
}
export function validateRow(query: QueryId, row: Readonly<ResultRow>): ResultRow {
  rowKey(query, row);
  switch (query) {
    case 'ui-gpus':
      for (const key of ['name', 'host_id', 'cluster_id', 'region', 'observation_epoch']) text(row, key);
      for (const key of ['inventory_revision', 'telemetry_revision']) version(row, key);
      choice(row, 'health', ['healthy', 'unreachable', 'unknown']);
      for (const key of ['powered_on', 'reporting_enabled', 'scheduling_enabled']) boolean(row, key);
      for (const key of ['memory_mib', 'slot', 'background_compute_units', 'background_memory_mib', 'interval_ms']) counter(row, key);
      for (const key of ['modeled_memory_used_mib', 'total_compute_units', 'busy_percent', 'sample_age_ms',
        'managed_memory_mib', 'managed_compute_units', 'reported_background_compute_units', 'background_memory_allocated_mib',
        'report_time_ms', 'reported_background_memory_requested_mib']) counter(row, key, true);
      version(row, 'sample_plan_version', true);
      version(row, 'sample_inventory_revision', true); version(row, 'sample_telemetry_revision', true);
      version(row, 'report_sequence', true);
      if (row.busy_percent !== null && number(row, 'busy_percent') > 100) throw new Error('Invalid busy percent');
      if (row.health === 'healthy' && (row.sample_age_ms === null || number(row, 'sample_age_ms') >= 5000)) throw new Error('Healthy GPU requires a fresh report');
      if (row.health === 'unreachable' && (row.sample_age_ms === null || number(row, 'sample_age_ms') < 5000)) throw new Error('Unreachable GPU requires heartbeat expiry');
      break;
    case 'ui-clusters':
      for (const key of ['name', 'region']) text(row, key);
      counter(row, 'registered_workers');
      counter(row, 'healthy_gpus', true);
      break;
    case 'ui-workloads':
      for (const key of ['name', 'profile_id', 'data_profile_id', 'purpose']) text(row, key);
      version(row, 'revision');
      for (const key of ['replicas', 'memory_mib_per_replica', 'compute_units_per_replica']) counter(row, key);
      for (const key of ['running_replicas', 'ready_replicas', 'suspended_replicas', 'fenced_replicas', 'fencing_pending_replicas']) counter(row, key, true);
      boolean(row, 'spread_across_domains');
      break;
    case 'ui-placements':
      signature(row);
      text(row, 'decision_id'); text(row, 'config_fingerprint'); text(row, 'reason');
      choice(row, 'status', ['confirmed', 'awaiting-application', 'awaiting-measurements', 'blocked', 'unknown']);
      for (const key of ['desired_plan_version', 'applied_plan_version', 'confirmed_plan_version']) version(row, key, true);
      for (const a of records(row, 'desired')) assignment(a);
      unique(records(row, 'desired'), 'id'); unique(records(row, 'actual'), 'id');
      for (const a of records(row, 'actual')) {
        assignment(a);
        choice(a, 'state', ['running', 'suspended', 'fenced', 'fencing-pending']);
        nullableText(a, 'input_fingerprint'); text(a, 'reason'); counter(a, 'acknowledged_at_ms', true);
        if (a.state === 'fencing-pending' && a.acknowledged_at_ms !== null) throw new Error('Pending fencing cannot be acknowledged');
      }
      break;
    case 'ui-policy':
      for (const key of ['workload_id', 'cluster_id', 'region', 'data_profile_id', 'purpose',
        'policy_id', 'authority_ref', 'input_fingerprint', 'policy_signature', 'observation_epoch']) text(row, key);
      if (row.id !== `${row.workload_id}/${row.cluster_id}`) throw new Error('Policy identity must be workload/cluster');
      version(row, 'policy_revision', true); boolean(row, 'current'); nullableText(row, 'error');
      choice(row, 'authorization', ['allow', 'deny', 'unknown']);
      strings(row, 'reasons'); strings(row, 'allowed_regions');
      if (row.current && row.authorization !== 'unknown' && (row.policy_revision === null || row.error !== null)) {
        throw new Error('Current policy decision requires policy metadata without an error');
      }
      break;
    case 'ui-resilience':
      signature(row);
      choice(row, 'status', ['current', 'checking', 'unknown']);
      for (const key of ['workers', 'regions']) for (const scenario of records(row, key)) {
        text(scenario, 'excluded'); text(scenario, 'detail');
        if (scenario.feasible !== null) boolean(scenario, 'feasible');
      }
      if (row.capacity_only_feasible !== null) boolean(row, 'capacity_only_feasible');
      text(row, 'capacity_only_detail');
      break;
    case 'ui-decisions':
      signature(row);
      text(row, 'summary'); text(row, 'config_fingerprint'); strings(row, 'reason_codes');
      choice(row, 'outcome', ['feasible', 'infeasible', 'unknown']);
      choice(row, 'stage', ['candidate', 'committed', 'rejected', 'diagnostic']);
      version(row, 'plan_version', true);
      for (const key of ['moved_replicas', 'new_replicas', 'retained_replicas', 'pre_plan_free_memory_mib', 'pre_plan_largest_gap_mib']) counter(row, key, true);
      for (const move of records(row, 'moves')) {
        text(move, 'replica_id'); text(move, 'to_gpu_id'); nullableText(move, 'from_gpu_id'); text(move, 'reason');
      }
      break;
    case 'ui-status':
      signature(row);
      if ('policy_rules' in row || 'data_profiles' in row || 'policy_rules_current' in row) {
        boolean(row, 'policy_rules_current');
        const policies = recordMap(row, 'policy_rules', 'policy_id'), profiles = recordMap(row, 'data_profiles', 'data_profile_id');
        if ((policies === null) !== (profiles === null) || (row.policy_rules_current && policies === null)) throw new Error('Incomplete shared data-policy configuration');
        for (const policy of policies ?? []) {
          for (const field of ['name', 'customer_id', 'authority_ref']) text(policy, field);
          version(policy, 'revision');
          for (const field of ['allowed_regions', 'allowed_purposes', 'allowed_classifications']) strings(policy, field);
        }
        for (const profile of profiles ?? []) {
          for (const field of ['customer_id', 'policy_id', 'authority_ref']) text(profile, field);
          version(profile, 'revision');
          choice(profile, 'classification', ['synthetic', 'restricted']);
          if (!policies?.some(policy => policy.policy_id === profile.policy_id)) throw new Error('Data profile references unavailable rules');
        }
      }
      boolean(row, 'scenario_ready'); boolean(row, 'inputs_ready'); text(row, 'scenario'); text(row, 'state'); text(row, 'detail');
      for (const component of records(row, 'components')) {
        text(component, 'component_id'); text(component, 'status'); nullableText(component, 'error');
      }
      break;
    case 'ui-timeline':
      for (const key of ['observation_epoch', 'component_id', 'kind', 'message']) text(row, key);
      version(row, 'event_sequence'); counter(row, 'time_ms');
      nullableText(row, 'decision_id'); version(row, 'plan_version', true);
      if (row.event_id !== `${row.observation_epoch}/${row.event_sequence}`) throw new Error('Invalid timeline identity');
      break;
  }
  return { ...row };
}
