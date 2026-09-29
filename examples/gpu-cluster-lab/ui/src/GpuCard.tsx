import { useId, useState, type ReactNode } from 'react';
import type { ResultRow } from '@drasi/react/client';
import { count, gib } from './Evidence';
import { boolean, nullableNumber, number, text } from './rows';
import { label } from './labels';
import { HierarchyIcon } from './Hierarchy';
import { ReplicaPlacements } from './ReplicaPlacements';
import type { ReplicaView } from './replicas';
import type { ReplicaCue } from './replicaMotion';

const icons = {
  expand: <path d="m6 9 6 6 6-6"/>,
  collapse: <path d="m6 15 6-6 6 6"/>,
  pause: <><path d="M8 5v14M16 5v14"/></>,
  resume: <path d="m8 5 11 7-11 7Z"/>,
  power: <><path d="M12 2v9M6 5a9 9 0 1 0 12 0"/></>,
  restore: <><path d="M4 10a8 8 0 1 1 1 8M4 3v7h7"/></>,
  edit: <><path d="M4 7h16M4 17h16M9 4v6M15 14v6"/></>,
  exclude: <><circle cx="12" cy="12" r="9"/><path d="M7 12h10"/></>,
  include: <><circle cx="12" cy="12" r="9"/><path d="M7 12h10M12 7v10"/></>,
  remove: <><path d="M4 6h16M9 6V3h6v3M6 6l1 15h10l1-15M10 10v7M14 10v7"/></>,
} satisfies Record<string, ReactNode>;

function IconButton({ icon, title, onClick, disabled = false, attention = false, expanded, controls }: {
  icon: keyof typeof icons; title: string; onClick: () => void; disabled?: boolean; attention?: boolean;
  expanded?: boolean; controls?: string;
}) {
  return <button type="button" className={`icon-button${attention ? ' attention' : ''}${icon === 'remove' ? ' destructive' : ''}`}
    aria-label={title} title={title} aria-expanded={expanded} aria-controls={controls} disabled={disabled} onClick={onClick}>
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8"
      strokeLinecap="round" strokeLinejoin="round" aria-hidden="true" focusable="false">{icons[icon]}</svg>
  </button>;
}

export function GpuCard({ gpu: g, replicaView, cues, stale, permission, disabled, actions, selectedWorkload, onToggleWorkload }: {
  gpu: ResultRow; replicaView: ReplicaView; cues: readonly ReplicaCue[]; stale: boolean;
  permission?: { workloadName: string; authorization: string; inspect: () => void };
  disabled: boolean;
  selectedWorkload: string | null; onToggleWorkload: (id: string) => void;
  actions: { reports: () => void; power: () => void; edit: () => void; scheduling: () => void; remove: () => void };
}) {
  const [expanded, setExpanded] = useState(false);
  const detailsId = useId(), headingId = useId();
  const reporting = boolean(g, 'reporting_enabled'), powered = boolean(g, 'powered_on'), scheduling = boolean(g, 'scheduling_enabled');
  const memory = nullableNumber(g, 'modeled_memory_used_mib');
  const policyTitle = permission ? `${permission.workloadName} · policy: ${label('authorization', permission.authorization)}` : undefined;
  const reportAge = g.sample_age_ms === null ? 'No report available' : `${number(g, 'sample_age_ms') / 1000}s ago`;
  const reportPlan = g.sample_plan_version === null ? 'No plan applied' : `Reported plan v${g.sample_plan_version}`;
  const reportSummary = g.sample_age_ms === null ? 'Awaiting first report' : `${reportAge} · ${reportPlan}`;
  return <article aria-labelledby={headingId} className={`gpu gpu-card ${expanded ? 'gpu-expanded' : 'gpu-condensed'}${permission ? ` destination-${permission.authorization}` : ''}`}
    data-hierarchy-kind="gpu" data-hierarchy-id={text(g, 'gpu_id')}>
    <div className="gpu-title">
      <HierarchyIcon kind="gpu"/>
      <strong id={headingId}><span className="hierarchy-type">GPU {number(g, 'slot')}</span>{' '}<span className="gpu-name">{text(g, 'name')}</span></strong>
      <button type="button" className={`policy-indicator${permission ? ` policy-${permission.authorization}` : ''}`}
        disabled={!permission} aria-hidden={!permission} aria-haspopup="dialog"
        aria-label={policyTitle ? `Policy details: ${policyTitle}` : undefined} title={policyTitle} onClick={permission?.inspect}>
        {permission && <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor"
          strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true" focusable="false">
          {permission.authorization === 'allow' ? <path d="m5 12 4 4L19 6"/> :
            permission.authorization === 'deny' ? <path d="m6 6 12 12M18 6 6 18"/> :
              <><circle cx="12" cy="12" r="9"/><path d="M9 9a3 3 0 0 1 6 0c0 2-3 2-3 4M12 17h.01"/></>}
        </svg>}
      </button>
      <IconButton icon={expanded ? 'collapse' : 'expand'} title={expanded ? 'Collapse GPU details' : 'Expand GPU details'}
        expanded={expanded} controls={detailsId} onClick={() => setExpanded(value => !value)}/>
    </div>
    <div className="gpu-status-line">
      <span className={`badge ${!stale && g.health === 'healthy' ? 'good' : 'warning'}`}>{stale && 'Previously: '}{label('health', text(g, 'health'))}</span>
      <span className="muted gpu-report-summary" title={`Latest GPU report: ${reportSummary}`}>{reportSummary}</span>
    </div>
    {(!powered || !reporting || !scheduling) && <p className="gpu-setting-state">
      {[!powered && 'Power setting: off', !reporting && 'Reports paused', !scheduling && 'Excluded from plans'].filter(Boolean).join(' · ')}
    </p>}
    <div className="gpu-meters">
      <Gauge label="Memory" value={memory === null ? null : memory / 1024} max={number(g, 'memory_mib') / 1024} unit="GiB"
        description="Last reported GPU memory use within the demo budget"/>
      <Gauge label="Compute demand" value={nullableNumber(g, 'total_compute_units')} max={85} unit="units"
        description="Last reported compute demand / planning limit, in illustrative demand units, not percent"/>
    </div>
    <ReplicaPlacements gpuId={text(g, 'gpu_id')} view={replicaView} compact={!expanded} cues={cues}
      selectedWorkload={selectedWorkload} onToggleWorkload={onToggleWorkload}/>
    <div id={detailsId} hidden={!expanded} className="gpu-details">
      {expanded && <>
        <p>Reported demand: {count(g.managed_compute_units)} from workloads + {count(g.reported_background_compute_units)} from background activity.
          {' '}Simulated busy time: {g.busy_percent === null ? 'Unknown' : `${number(g, 'busy_percent')}%`}.</p>
        <p>Reported memory: {gib(nullableNumber(g, 'managed_memory_mib'))} for workloads + {gib(nullableNumber(g, 'background_memory_allocated_mib'))} for background activity.</p>
        <p>Power setting: {powered ? 'on' : 'off'} · reports: {reporting ? 'enabled' : 'paused'} · {scheduling ? 'included in plans' : 'excluded from plans'}</p>
        <p>Background activity requests {number(g, 'background_compute_units')} demand units and {gib(number(g, 'background_memory_mib'))}.
          {' '}That leaves {gib(number(g, 'memory_mib') - number(g, 'background_memory_mib'))} for all workload reservations on this GPU.</p>
        <p>Report interval: {number(g, 'interval_ms')} ms · inventory revision {text(g, 'inventory_revision')} / report-settings revision {text(g, 'telemetry_revision')}.</p>
        <p>Latest report: {reportPlan}; sequence {count(g.report_sequence)}, inventory revision {count(g.sample_inventory_revision)}, report-settings revision {count(g.sample_telemetry_revision)};
          {' '}background memory requested {gib(nullableNumber(g, 'reported_background_memory_requested_mib'))}.
          {' '}Timestamp: {g.report_time_ms === null ? 'none' : new Date(number(g, 'report_time_ms')).toISOString()}.</p>
        <p>Reports become overdue at five seconds. Settings changes and action confirmations do not count as new reports.
          {' '}Values above are the last reported use, not a live measurement.</p>
      </>}
    </div>
    <div className="gpu-controls" role="group" aria-label={`Actions for ${text(g, 'name')}`}>
      <IconButton icon={reporting ? 'pause' : 'resume'} title={reporting ? 'Pause reports' : 'Resume reports'}
        disabled={disabled} attention={!reporting} onClick={actions.reports}/>
      <IconButton icon={powered ? 'power' : 'restore'} title={powered ? 'Simulate GPU failure' : 'Restore GPU'}
        disabled={disabled} attention={!powered} onClick={actions.power}/>
      <IconButton icon="edit" title="Edit background load" disabled={disabled} onClick={actions.edit}/>
      <IconButton icon={scheduling ? 'exclude' : 'include'} title={scheduling ? 'Exclude GPU from plans' : 'Include GPU in plans'}
        disabled={disabled} attention={!scheduling} onClick={actions.scheduling}/>
      <IconButton icon="remove" title="Remove GPU" disabled={disabled} onClick={actions.remove}/>
    </div>
  </article>;
}

function Gauge({ label, value, max, unit, description }: { label: string; value: number | null; max: number; unit: string; description: string }) {
  return <div className="gauge" title={description}><div><span>{label}</span><strong>{value === null ? 'Unknown' : `${value} / ${max} ${unit}`}</strong></div>
    {value === null ? <div className="meter-unknown" aria-hidden="true"/> :
      <meter min={0} max={max} value={Math.min(value, max)} aria-label={`${label}: ${value} ${unit}`}
        aria-valuetext={`${value} of ${max} ${unit}`}/>}
  </div>;
}
