const labels = {
  region: { westeurope: 'West Europe', northeurope: 'North Europe', eastus: 'East US', '*': 'All regions' },
  scenario: { baseline: 'Baseline fleet', fragmentation: 'Memory fragmentation', 'regional-boundary': 'Regional policy' },
  profile: {
    'assistant-v1': 'Qwen2.5-32B assistant', 'chat-v1': 'Llama3.1-8B chat',
    'embeddings-v1': 'BGE embeddings', 'reranker-v1': 'BGE reranker', custom: 'Custom resource requirements',
  },
  data: { 'demo-open': 'Synthetic demo data', 'customer-eu-documents': 'Example customer EU documents' },
  purpose: { demo: 'Demo', 'customer-support': 'Customer support' },
  policy: { 'demo-permissive': 'Demo processing policy', 'customer-eu-processing': 'Customer processing policy', unresolved: 'Policy not identified' },
  health: { healthy: 'Recent report', unreachable: 'Report overdue', unknown: 'Report status unknown' },
  authorization: { allow: 'Allowed', deny: 'Not allowed', unknown: 'Unknown' },
  execution: { running: 'Running', suspended: 'Paused by policy', fenced: 'Stopped by policy', 'fencing-pending': 'Stop requested' },
  placement: {
    confirmed: 'Confirmed by GPU reports', 'awaiting-application': 'Saved; waiting to apply',
    'awaiting-measurements': 'Applied; waiting for GPU reports', blocked: 'Not fully confirmed', unknown: 'Plan status unknown',
  },
  outcome: { feasible: 'Complete plan found', infeasible: 'No complete plan fits', unknown: 'Outcome unknown' },
  stage: { candidate: 'Proposed, not saved', committed: 'Saved to database', rejected: 'Rejected, not saved', diagnostic: 'Analysis only' },
  event: {
    candidate: 'Plan proposed', committed: 'Plan saved', applied: 'Plan applied', confirmed: 'Confirmed by GPU reports',
    'heartbeat-expired': 'GPU reports overdue', 'power-off': 'VM powered off', 'reports-paused': 'GPU reports paused',
    'write-rejected': 'Plan not saved', infeasible: 'No complete plan fits', suspended: 'Processing paused by policy',
    'fencing-requested': 'Policy stop requested', fenced: 'Processing stopped by policy', 'solver-timeout': 'Optimizer time limit reached',
    'preview-edit': 'Preview settings changed',
    'execution-running': 'Replica running', 'write-receipt': 'Plan write receipt received',
    'replica-moved': 'Replica moved',
    'write-unknown': 'Plan write outcome unknown', 'solver-unknown': 'Optimizer result unknown',
    'component-status': 'Component status changed',
  },
  component: {
    postgres: 'Database change feed', simulator: 'GPU simulator', policy: 'Policy evaluator (Regorus)',
    placement: 'Placement optimizer', resilience: 'Failure recovery analysis', 'runtime-status': 'Runtime status and activity',
    'postgres-source': 'Database change feed', 'telemetry-simulator': 'GPU simulator',
    'regorus-policy': 'Policy evaluator (Regorus)', 'placement-solver': 'Placement optimizer',
    'resilience-assessor': 'Failure recovery analysis', 'plan-writer': 'Plan saving',
  },
  componentStatus: {
    ready: 'Ready', current: 'Up to date', unavailable: 'Unavailable', 'not evaluated': 'Not evaluated',
    starting: 'Starting', stopped: 'Stopped', failed: 'Failed', running: 'Running',
    'policy-current': 'Policy current', 'plan-current': 'Plan current', 'resilience-current': 'Analysis current',
    'candidate-produced': 'Candidate produced', pending: 'Working', 'policy-pending': 'Evaluating policy',
    'awaiting-source-bootstrap': 'Awaiting database bootstrap', 'awaiting-policy-bootstrap': 'Awaiting initial policy',
    'awaiting-policy': 'Awaiting policy',
  },
  query: {
    'ui-gpus': 'GPUs', 'ui-workloads': 'Workloads', 'ui-placements': 'Placement plan', 'ui-resilience': 'Failure recovery',
    'ui-decisions': 'Decisions', 'ui-status': 'Demo components', 'ui-timeline': 'Timeline', 'ui-clusters': 'Regional clusters', 'ui-policy': 'Policy',
  },
} as const;

// Unknown identifiers remain visible; presentation must not invent a successful state.
export function label(category: keyof typeof labels, value: string): string {
  const values: Readonly<Record<string, string>> = labels[category];
  return values[value] ?? value;
}

export type StatusTone = 'good' | 'warning' | 'danger';

export function reportTone(health: string, stale = false): StatusTone {
  if (stale || health === 'unknown') return 'warning';
  return health === 'healthy' ? 'good' : health === 'unreachable' ? 'danger' : 'warning';
}

export function componentTone(status: string, hasError: boolean, stale = false): StatusTone {
  if (stale) return 'warning';
  if (hasError || ['failed', 'stopped', 'unavailable', 'infeasible', 'application-rejected', 'initialization-error', 'error', 'rejected'].includes(status)) return 'danger';
  if (['ready', 'running', 'current', 'policy-current', 'plan-current', 'resilience-current', 'candidate-produced', 'committed', 'applied'].includes(status)) return 'good';
  return 'warning';
}

export function queryRetrying(status: string): boolean {
  return ['initial-loading', 'resynchronizing', 'reconnecting', 'stale-last-good-data'].includes(status);
}

const reasons: Readonly<Record<string, string>> = {
  'fixture-setup': 'Starting scenario',
  'workload-added': 'New workload requested',
  'requirements-changed': 'Workload requirements changed',
  'device-unreachable': 'No recent GPU report',
  'administratively-disabled': 'GPU excluded from placement plans',
  'capacity-pressure': 'Not enough spare capacity',
  'memory-fragmentation': 'Enough memory overall, but not enough free on one GPU',
  'policy-changed': 'Processing policy changed',
  'policy-restricted-capacity': 'Policy excludes the capacity needed for a complete plan',
  'policy-fenced': 'Processing stopped by policy',
  'region-not-permitted': 'This region is not allowed by the policy',
  'policy-permitted': 'The policy allows processing in this region',
  'policy-input-unavailable': 'Policy information is unavailable',
  'policy-unavailable': 'Policy information is unavailable',
  'preview-not-evaluated': 'Preview edits have not been evaluated',
  'solver-timeout': 'Optimizer reached its time limit; the result is unknown',
  'current-plan-authorized': 'Current plan is allowed by policy',
  'scheduling-input-changed': 'Configuration, policy or observed capacity changed',
  'policy-input-pending': 'Waiting for current policy evaluation',
  'no-complete-plan': 'No complete plan satisfies the current constraints',
  'solver-unknown': 'Optimizer did not establish a complete result',
};
export function reasonText(value: string): string {
  return reasons[value] ?? (/^[a-z][a-z0-9-]+$/.test(value) ? `Reason code: ${value}` : value);
}
