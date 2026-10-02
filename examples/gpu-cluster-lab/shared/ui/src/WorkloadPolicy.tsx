import type { ResultRow } from '@drasi/react/client';
import type { Views } from './App';
import { policyCurrent } from './Evidence';
import { dataProfile } from './DataPolicy';
import { label } from './labels';
import { strings, text } from './rows';

export function workloadPolicyRows(workload: ResultRow, rows: ResultRow[]): ResultRow[] {
  return rows.filter(row => row.workload_id === workload.workload_id && row.data_profile_id === workload.data_profile_id);
}

export function policyMetadata(rows: ResultRow[]): ResultRow | undefined {
  const first = rows[0];
  if (!first || !first.policy_id || first.policy_id === 'unresolved' || first.policy_revision === null) return undefined;
  return rows.every(row => ['policy_id', 'policy_revision', 'authority_ref'].every(key => row[key] === first[key])
    && JSON.stringify([...strings(row, 'allowed_regions')].sort()) === JSON.stringify([...strings(first, 'allowed_regions')].sort()))
    ? first : undefined;
}

export function WorkloadPolicySummary({ workload, views, stale, onInspect }: {
  workload: ResultRow; views: Views; stale: boolean; onInspect: (trigger: HTMLButtonElement) => void;
}) {
  const rows = workloadPolicyRows(workload, views['ui-policy'].data ?? []);
  const destinations = (views['ui-clusters'].data ?? []).map(cluster => {
    const row = rows.find(row => row.cluster_id === cluster.cluster_id);
    return { region: label('region', text(cluster, 'region')),
      authorization: row && policyCurrent(row, views, stale) ? row.authorization : 'unknown' };
  });
  const allowed = [...new Set(destinations.filter(d => d.authorization === 'allow').map(d => d.region))];
  const denied = [...new Set(destinations.filter(d => d.authorization === 'deny').map(d => d.region))];
  const unknown = !destinations.length || destinations.some(d => d.authorization === 'unknown');
  return <div className="workload-policy-summary">
    <button type="button" className="workload-policy-link" aria-haspopup="dialog"
      aria-label={`Policy results for ${text(workload, 'name')}`} onClick={event => onInspect(event.currentTarget)}>
      Policy results <span aria-hidden="true">Details</span>
    </button>
    {allowed.length > 0 && <span className="status-text good">Allowed: {allowed.join(', ')}</span>}
    {denied.length > 0 && <span className="status-text danger">Blocked: {denied.join(', ')}</span>}
    {unknown && <span className="status-text warning">{stale ? 'Policy results out of date' : 'Policy results pending or unavailable'}</span>}
  </div>;
}

export function WorkloadPolicyContext({ workload, views, stale, onShowRules }: {
  workload: ResultRow; views: Views; stale: boolean; onShowRules: () => void;
}) {
  const profile = dataProfile(views, workload);
  return <section className="workload-policy-context" aria-label="Workload facts">
    <p><strong>Workload facts, shared rules, destination results.</strong> Drasi checks these facts against the shared data policy.
      Changing a workload's facts does not change the rules.</p>
    <dl className="policy-relationship">
      <dt>Data classification</dt><dd>{profile ? label('classification', text(profile, 'classification')) : 'Unavailable in current query results'}</dd>
      <dt>Data being processed</dt><dd>{label('data', text(workload, 'data_profile_id'))}</dd>
      <dt>Customer</dt><dd>{profile ? text(profile, 'customer_id') : 'Unavailable in current query results'}</dd>
      <dt>Purpose</dt><dd>{label('purpose', text(workload, 'purpose'))}</dd>
    </dl>
    {stale && <p className="status-text warning">Last received facts. Current permission is unknown.</p>}
    <p><strong>How this affects allocation:</strong> Drasi evaluates whether this data may be processed for this purpose in each destination.
      The optimizer chooses GPUs only in allowed destinations, subject to available memory, compute capacity and replica separation.
      Allowed does not mean allocated; blocked destinations cannot receive a new allocation even if their GPUs are free.</p>
    <button type="button" onClick={onShowRules}>View shared data policy</button>
    <p className="muted">Edit rules in the shared Data policy panel. Rule changes can affect multiple workloads;
      the destination results below come from the evaluator, not from this page predicting an outcome.</p>
  </section>;
}

export function WorkloadPolicyBinding({ dataId, purpose, views }: { dataId: string; purpose: string; views: Views }) {
  const profile = dataProfile(views, { data_profile_id: dataId });
  return <div className="workload-policy-binding" role="status">
    <strong>Data classification: {profile ? label('classification', text(profile, 'classification')) : 'Unavailable in current query results'}</strong>
    <p>The selected data determines its classification. Choosing Demo as the purpose does not turn EU user data into synthetic data.</p>
    <p>Purpose: {label('purpose', purpose)}. These are workload facts, not rule settings. Drasi checks them against the shared data policy after saving.
      Changing the purpose does not override the rules. This form does not preview a new permission decision.</p>
  </div>;
}
