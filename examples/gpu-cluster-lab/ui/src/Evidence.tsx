import type { ResultRow } from '@drasi/react/client';
import type { Views } from './App';
import { nullableNumber, nullableText, number, records, strings, text } from './rows';
import { label, reasonText } from './labels';

export function correlated(row: ResultRow | undefined, status: ResultRow | undefined): boolean {
  return !!row && !!status && ['observation_epoch', 'scheduling_signature', 'policy_signature']
    .every(key => typeof row[key] === 'string' && row[key] === status[key]);
}
export function currentFeeds(views: Views): boolean {
  return Object.values(views).every(v => v.data !== null && !v.loading && !v.error && !v.stale);
}
export function sumCounts(rows: ResultRow[] | null, field: string): number | null {
  if (!rows || rows.some(r => r[field] === null)) return null;
  return rows.reduce((sum, row) => sum + number(row, field), 0);
}
export const count = (value: unknown) => value === null || value === undefined ? 'Unknown' : String(value);
export const gib = (value: number | null) => value === null ? 'Unknown' : `${value / 1024} GiB`;

type ReplicaInputs = {
  placement: ResultRow | undefined; status: ResultRow | undefined;
  workloads: ResultRow[]; gpus: ResultRow[]; policies: ResultRow[];
};
export function replicaInputs(views: Views): ReplicaInputs {
  return { placement: views['ui-placements'].data?.[0], status: views['ui-status'].data?.[0],
    workloads: views['ui-workloads'].data ?? [], gpus: views['ui-gpus'].data ?? [], policies: views['ui-policy'].data ?? [] };
}
export function replicaReady(a: ResultRow, { placement: p, status, workloads, gpus, policies }: ReplicaInputs): boolean {
  if (!p || !status || !correlated(p, status) || status.inputs_ready !== true
    || !p.desired_plan_version || p.applied_plan_version !== p.desired_plan_version || a.state !== 'running') return false;
  const w = workloads.find(w => w.workload_id === a.workload_id);
  const g = gpus.find(g => g.gpu_id === a.gpu_id);
  const auth = policies.find(pair => pair.workload_id === a.workload_id && pair.cluster_id === g?.cluster_id);
  const desired = records(p, 'desired').find(d => d.id === a.id);
  return !!w && !!g && !!auth && !!desired
    && number(a, 'replica_index') < number(w, 'replicas')
    && a.memory_mib === w.memory_mib_per_replica && a.compute_units === w.compute_units_per_replica
    && ['profile_id', 'data_profile_id', 'purpose'].every(k => a[k] === w[k])
    && ['gpu_id', 'workload_id', 'replica_index', 'profile_id', 'data_profile_id', 'purpose', 'memory_mib', 'compute_units'].every(k => a[k] === desired[k])
    && auth.authorization === 'allow' && auth.current === true && auth.policy_signature === status.policy_signature
    && auth.observation_epoch === status.observation_epoch
    && auth.data_profile_id === w.data_profile_id && auth.purpose === w.purpose && a.input_fingerprint === auth.input_fingerprint
    && g.scheduling_enabled === true
    && g.health === 'healthy' && g.observation_epoch === status.observation_epoch && g.sample_plan_version === p.desired_plan_version
    && typeof g.sample_age_ms === 'number' && g.sample_age_ms < 5000
    && typeof a.acknowledged_at_ms === 'number' && typeof g.report_time_ms === 'number' && g.report_time_ms >= a.acknowledged_at_ms;
}

export function executionObserved(placement: ResultRow): boolean {
  // These query statuses require a current AppliedPlan observation, even if application failed.
  return placement.applied_plan_version !== null || placement.status === 'blocked' || placement.status === 'awaiting-application';
}

export function clusterReplicaCounts(views: Views, clusterId: string): { planned: number | null; running: number | null; ready: number | null } {
  const p = views['ui-placements'].data?.[0], status = views['ui-status'].data?.[0];
  const unknown = { planned: null, running: null, ready: null };
  if (!currentFeeds(views) || !p || !correlated(p, status) || !status?.inputs_ready) return unknown;
  const gpus = views['ui-gpus'].data ?? [], clusters = views['ui-clusters'].data ?? [];
  if (!clusters.some(c => c.cluster_id === clusterId)) return unknown;
  const located = (a: ResultRow) => {
    const g = gpus.find(g => g.gpu_id === a.gpu_id);
    return !!g && clusters.some(c => c.cluster_id === g.cluster_id);
  };
  const inCluster = (a: ResultRow) => gpus.some(g => g.gpu_id === a.gpu_id && g.cluster_id === clusterId);
  const desired = records(p, 'desired'), actual = records(p, 'actual');
  const planned = p.desired_plan_version !== null && desired.every(located) ? desired.filter(inCluster).length : null;
  if (!executionObserved(p) || !actual.every(located)) return { ...unknown, planned };
  const local = actual.filter(inCluster);
  return {
    planned,
    running: local.some(a => a.state === 'fencing-pending') ? null : local.filter(a => a.state === 'running').length,
    ready: local.filter(a => replicaReady(a, replicaInputs(views))).length,
  };
}

export function confirmed(views: Views): boolean {
  const p = views['ui-placements'].data?.[0], status = views['ui-status'].data?.[0];
  if (!currentFeeds(views) || !correlated(p, status) || !p || !status?.inputs_ready || p.status !== 'confirmed'
    || !p.desired_plan_version || p.desired_plan_version !== p.applied_plan_version
    || p.desired_plan_version !== p.confirmed_plan_version) return false;
  const desired = records(p, 'desired'), actual = records(p, 'actual'), workloads = views['ui-workloads'].data ?? [];
  if (desired.length !== sumCounts(workloads, 'replicas') || sumCounts(workloads, 'ready_replicas') !== desired.length) return false;
  if (actual.filter(a => a.state !== 'fenced').length !== desired.length) return false;
  return workloads.every(w => desired.filter(a => a.workload_id === w.workload_id).length === w.replicas)
    && actual.filter(a => a.state !== 'fenced').every(a => replicaReady(a, replicaInputs(views)));
}

export function PlacementEvidence({ row, views, onDecision }: { row: ResultRow; views: Views; onDecision?: (id: string) => void }) {
  const synced = correlated(row, views['ui-status'].data?.[0]) && currentFeeds(views);
  const complete = confirmed(views);
  return <div className="evidence">
    <span className={`badge ${complete ? 'good' : 'warning'}`}>{complete ? 'Confirmed by GPU reports' : !synced ? 'Data is out of date or does not match' : row.status === 'confirmed' ? 'Waiting for matching reports' : label('placement', text(row, 'status'))}</span>
    <dl className="facts">
      <dt>Saved to database</dt><dd>{version(row, 'desired_plan_version')}</dd>
      <dt>Applied by simulator</dt><dd>{version(row, 'applied_plan_version')}</dd>
      <dt>Confirmed by GPU reports</dt><dd>{complete ? version(row, 'confirmed_plan_version') : 'Not confirmed'}</dd>
      <dt>Replicas in saved plan</dt><dd>{records(row, 'desired').length}</dd>
    </dl>
    <p>{reasonText(text(row, 'reason'))}</p>
    <p className="muted">Proposed → saved → applied → confirmed. Accepting a save request is not the same as saving it. Saving a plan does not mean workloads have moved.</p>
    <a href={`#decision-${text(row, 'decision_id')}`} onClick={event => {
      if (onDecision) { event.preventDefault(); onDecision(text(row, 'decision_id')); }
    }}>View the decision behind this plan</a>
  </div>;
}
function version(row: ResultRow, key: string) {
  const value = nullableText(row, key); return value === null ? 'Not confirmed' : `v${value}`;
}
function Assessment({ title, scenarios, current }: { title: string; scenarios: ResultRow[]; current: boolean }) {
  const unknown = scenarios.some(s => s.feasible === null);
  const failed = scenarios.filter(s => s.feasible === false).length;
  const result = !current ? 'Not current' : !scenarios.length ? 'Nothing to assess' : failed ? 'Some cases cannot recover' : unknown ? 'Unknown' : 'Every tested case can recover';
  return <div className="assessment"><h4>{title} <span className={`badge ${current && scenarios.length && !failed && !unknown ? 'good' : 'warning'}`}>{result}</span></h4>
    {scenarios.map(s => <details key={text(s, 'excluded')}><summary>{label('region', text(s, 'excluded'))} · {s.feasible === null ? 'Unknown' : s.feasible ? 'Can recover all replicas' : 'Cannot recover all replicas'}{!current && ' (previous result)'}</summary><p>{text(s, 'detail')}</p></details>)}
  </div>;
}
export function ResilienceEvidence({ row, views }: { row: ResultRow; views: Views }) {
  const status = views['ui-status'].data?.[0];
  const current = currentFeeds(views) && correlated(row, status) && row.status === 'current';
  return <div className="evidence">
    <p>Could <strong>all required replicas</strong> run again after <strong>one more failure</strong>, using only allowed regions? Recovery may interrupt service.</p>
    {row.status !== 'current' && <p className="warning">{row.status === 'checking' ? 'Checking recovery options…' : 'Recovery result unknown'}</p>}
    <Assessment title="One more VM fails" scenarios={records(row, 'workers')} current={current}/>
    <Assessment title="An entire region fails" scenarios={records(row, 'regions')} current={current}/>
    <details className="technical-details"><summary>Technical details: are the inputs up to date?</summary>
      <p>Placement input signatures (analyzed / current): {text(row, 'scheduling_signature')} / {status ? text(status, 'scheduling_signature') : 'unknown'}</p>
      <p>Policy input signatures (analyzed / current): {text(row, 'policy_signature')} / {status ? text(status, 'policy_signature') : 'unknown'}</p>
    </details>
    <p><strong>Would everything fit without the policy restriction? </strong>{!current ? 'Not current' : row.capacity_only_feasible === null ? 'Not evaluated' : row.capacity_only_feasible ? 'Yes' : 'No'}</p>
    <p className="muted">{text(row, 'capacity_only_detail')} This comparison never places workloads or bypasses policy.</p>
  </div>;
}
export function PolicyEvidence({ row, views }: { row: ResultRow; views: Views }) {
  const status = views['ui-status'].data?.[0];
  const current = currentFeeds(views) && row.current && row.policy_signature === status?.policy_signature
    && row.observation_epoch === status?.observation_epoch;
  const workload = views['ui-workloads'].data?.find(w => w.workload_id === row.workload_id);
  const cluster = views['ui-clusters'].data?.find(c => c.cluster_id === row.cluster_id);
  return <div className="eligibility evidence">
    <strong>{String(workload?.name ?? row.workload_id)} → {String(cluster?.name ?? row.cluster_id)}</strong>
    <p><span className={`badge ${current && row.authorization === 'allow' ? 'good' : current && row.authorization === 'deny' ? 'danger' : 'warning'}`}>{current ? label('authorization', text(row, 'authorization')) : 'Unknown / not current'}</span> {strings(row, 'reasons').map(reasonText).join('; ')}</p>
    <p>{label('data', text(row, 'data_profile_id'))} / {label('purpose', text(row, 'purpose'))}</p>
    <details><summary>Policy settings · {row.policy_revision === null ? 'unavailable' : `revision ${text(row, 'policy_revision')}`}</summary>
      <p>{label('policy', text(row, 'policy_id'))}</p>
      <p>Allowed regions: {row.policy_revision === null ? 'Unknown' : strings(row, 'allowed_regions').map(r => label('region', r)).join(', ') || 'None'}</p>
      <details className="technical-details"><summary>Technical details: policy source and input ID</summary>
        <p>Policy source: {text(row, 'authority_ref') || 'Unknown'}</p><p>Input fingerprint: {text(row, 'input_fingerprint') || 'Unavailable'}</p>
      </details>
      {!!row.error && <p className="warning">{text(row, 'error')}</p>}
    </details>
  </div>;
}
export function DecisionEvidence({ row, views }: { row: ResultRow; views: Views }) {
  const current = currentFeeds(views) && correlated(row, views['ui-status'].data?.[0]);
  const plan = views['ui-placements'].data?.[0];
  const saved = plan && row.stage === 'committed' && row.plan_version !== null
    && row.decision_id === plan.decision_id && row.plan_version === plan.desired_plan_version && row.observation_epoch === plan.observation_epoch;
  const context = saved ? `Saved plan v${row.plan_version}`
    : row.stage === 'committed' ? `Saved decision${row.plan_version === null ? '' : ` · plan v${row.plan_version}`}`
      : row.stage === 'diagnostic' ? 'Placement assessment · no new saved plan' : 'Separate placement attempt · not saved';
  return <div className="evidence decision-evidence" id={`decision-${text(row, 'decision_id')}`} tabIndex={-1}>
    <p className="decision-context">{context}</p>
    <p><strong>{label('outcome', text(row, 'outcome'))} · {label('stage', text(row, 'stage'))}</strong>{!current && ' · previous result'}</p>
    <p>{text(row, 'summary')}</p>
    <p>{strings(row, 'reason_codes').map(reasonText).join(' · ')}</p>
    <dl className="facts"><dt>Existing replicas moved</dt><dd>{count(row.moved_replicas)}</dd>
      <dt>New / unchanged replicas</dt><dd>{count(row.new_replicas)} / {count(row.retained_replicas)}</dd>
      <dt>Free memory before planning</dt><dd>{gib(nullableNumber(row, 'pre_plan_free_memory_mib'))}</dd>
      <dt>Most free on a single GPU</dt><dd>{gib(nullableNumber(row, 'pre_plan_largest_gap_mib'))}</dd></dl>
    <p className="muted">Free memory excludes existing reservations and background requests on available GPUs. It does not yet exclude regions prohibited for a particular workload.</p>
    {records(row, 'moves').map(m => <p key={text(m, 'replica_id')} className="move"><strong>{replicaName(text(m, 'replica_id'), views)}</strong><br/>
      {gpuName(nullableText(m, 'from_gpu_id'), views)} → {gpuName(text(m, 'to_gpu_id'), views)}</p>)}
  </div>;
}
export function timelineMessage(row: ResultRow): string {
  return row.kind === 'preview-edit' ? 'Preview settings changed in this tab only. No backend request was sent.' : text(row, 'message');
}
export function TimelineEvidence({ row, onDecision, previous = false }: { row: ResultRow; onDecision?: (id: string) => void; previous?: boolean }) {
  return <div className="timeline-event evidence"><time>{new Date(number(row, 'time_ms')).toISOString().slice(11, 19)} UTC</time>
    <strong>{label('event', text(row, 'kind'))}{previous && ' · previous run'}</strong><p>{timelineMessage(row)}</p>
    {!!row.decision_id && <a href={`#decision-${text(row, 'decision_id')}`} onClick={event => {
      if (onDecision) { event.preventDefault(); onDecision(text(row, 'decision_id')); }
    }}>View decision</a>}
    {!!row.plan_version && <small> · plan v{text(row, 'plan_version')}</small>}
  </div>;
}
export function StatusEvidence({ row, stale = false }: { row: ResultRow; stale?: boolean }) {
  return <div className="evidence"><p>{text(row, 'detail')}</p>
    {stale && <p className="warning">Last received component statuses; current state is unknown.</p>}
    {records(row, 'components').map(c => <div className="component" key={text(c, 'component_id')}><span>{label('component', text(c, 'component_id'))}</span><b>{label('componentStatus', text(c, 'status'))}{stale && ' (last reported)'}</b>
      {!!c.error && <p className="warning">{text(c, 'error')}</p>}</div>)}
  </div>;
}
function gpuName(id: string | null, views: Views): string {
  return id === null ? 'New replica' : String(views['ui-gpus'].data?.find(g => g.gpu_id === id)?.name ?? id);
}
function replicaName(id: string, views: Views): string {
  const separator = id.lastIndexOf('/');
  const index = id.slice(separator + 1);
  const w = views['ui-workloads'].data?.find(w => w.workload_id === id.slice(0, separator));
  return w && /^\d+$/.test(index) && Number.isSafeInteger(Number(index))
    ? `${w.name} · replica ${Number(index) + 1}` : id;
}
