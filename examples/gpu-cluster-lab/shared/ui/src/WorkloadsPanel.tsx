import type { ResultRow } from '@drasi/react/client';
import type { View } from './App';
import { count } from './Evidence';
import { label } from './labels';
import { nullableNumber, number, text } from './rows';
import { HierarchyIcon } from './Hierarchy';
import { useState } from 'react';
import { WorkloadReplicas } from './ReplicaPlacements';
import type { ReplicaView } from './replicas';

export function WorkloadsPanel({ query, replicaView, stale, disabled, selectedWorkload, onToggleWorkload, onAdd, onEditPolicy, onEdit, onDelete }: {
  query: View; replicaView: ReplicaView; stale: boolean; disabled: boolean; selectedWorkload: string | null;
  onToggleWorkload: (id: string) => void; onAdd: () => void; onEditPolicy: () => void; onEdit: (row: ResultRow) => void; onDelete: (row: ResultRow) => void;
}) {
  const rows = query.data ?? [];
  const empty = query.error || query.stale ? 'Workload data unavailable' : query.loading || query.data === null ? 'Waiting for workloads' : 'No workloads configured';
  return <section className="panel workload-panel" aria-label="Workloads">
    <div className="workload-heading">
      <h3>Workloads</h3>
      <div className="workload-actions">
        <button type="button" disabled={disabled} onClick={onEditPolicy}>Edit policy</button>
        <button type="button" disabled={disabled} onClick={onAdd}>Add workload</button>
      </div>
    </div>
    <div className="workload-overview">
      <details className="workload-body">
        <summary aria-label="Workload details" aria-controls="workload-details">
          {query.data === null ? 'Workloads' : `${rows.length} workload${rows.length === 1 ? '' : 's'}`}
        </summary>
        <div id="workload-details">
          <p className="muted">Select a workload to highlight its GPU replicas. Resources are planning reservations, not measured model performance. Paused keeps memory; stopped releases it; stop requested is not yet confirmed.</p>
          {stale && <p className="warning">Last received workload settings; current execution is unknown.</p>}
          {rows.length ? <div className="table-scroll"><table><thead><tr><th>Workload / model</th><th>Resources per replica</th><th>Replicas required</th><th>Running</th><th>Confirmed</th><th>Policy: paused / stopped / stop requested</th><th>Data / purpose</th><th>Actions</th></tr></thead>
            <tbody>{rows.map(w => <WorkloadRows key={text(w, 'workload_id')} workload={w} view={replicaView}
              stale={stale} disabled={disabled} selected={selectedWorkload === w.workload_id}
              onToggleWorkload={onToggleWorkload} onEdit={onEdit} onDelete={onDelete}/>)}</tbody></table></div> : <p className="empty">{empty}</p>}
        </div>
      </details>
      <div className="workload-summary" aria-label="Workload summary">
        {rows.length ? rows.map(w => {
          const id = text(w, 'workload_id'), required = number(w, 'replicas');
          const confirmed = stale ? null : nullableNumber(w, 'ready_replicas');
          const running = stale ? null : nullableNumber(w, 'running_replicas');
          const replicas = replicaView.replicas.filter(replica => replica.workloadId === id && replica.requested);
          const blocked = replicaView.executionKnown && required > 0 && replicas.length === required
            && replicas.every(replica => replica.actual?.state === 'fenced' || replica.status === 'Blocked by policy');
          const stopped = blocked && replicas.every(replica => replica.actual?.state === 'fenced');
          const state = blocked ? 'blocked' : confirmed === null ? 'unknown' : required === 0 ? 'idle' : confirmed === required ? 'confirmed' : 'unconfirmed';
          return <button type="button" key={id} className={`workload-chip workload-${state}`} aria-label={text(w, 'name')}
            data-hierarchy-kind="workload" data-hierarchy-id={id}
            aria-describedby={`workload-counts-${id}`} aria-pressed={selectedWorkload === id}
            title={`${text(w, 'name')}: ${required} required, ${count(running)} running, ${count(confirmed)} confirmed.${blocked ? stopped ? ' Stopped by policy.' : ' Blocked by policy.' : ''}${stale ? ' Last received settings; current execution is unknown.' : ''} Select to highlight GPU replicas.`}
            onClick={() => onToggleWorkload(id)}>
            <strong><HierarchyIcon kind="workload"/>{text(w, 'name')}</strong><span id={`workload-counts-${id}`}>{count(confirmed)} / {required} confirmed</span>
            {blocked && <span className="workload-policy-state">{stopped ? 'Stopped by policy' : 'Blocked by policy'}</span>}
            <span className="workload-progress" aria-hidden="true"><span style={{ width: `${confirmed === null || required === 0 ? 0 : Math.min(100, confirmed / required * 100)}%` }}/></span>
          </button>;
        }) : <span className="muted">{empty}</span>}
      </div>
    </div>
  </section>;
}

function WorkloadRows({ workload: w, view, stale, disabled, selected, onToggleWorkload, onEdit, onDelete }: {
  workload: ResultRow; view: ReplicaView; stale: boolean; disabled: boolean; selected: boolean;
  onToggleWorkload: (id: string) => void; onEdit: (row: ResultRow) => void; onDelete: (row: ResultRow) => void;
}) {
  const [open, setOpen] = useState(false);
  const id = `replicas-${encodeURIComponent(text(w, 'workload_id'))}`;
  return <>
    <tr className={`workload-row${selected ? ' selected-workload' : ''}`}
      data-hierarchy-kind="workload" data-hierarchy-id={text(w, 'workload_id')}>
      <td><div className="workload-identity">
        <button type="button" className="replica-expander" aria-label={`${open ? 'Hide' : 'Show'} replicas for ${text(w, 'name')}`}
          aria-expanded={open} aria-controls={id} onClick={() => setOpen(value => !value)}><HierarchyIcon kind="chevron"/></button>
        <div><button type="button" aria-pressed={selected} title={`Highlight ${text(w, 'name')} and its replicas`}
          onClick={() => onToggleWorkload(text(w, 'workload_id'))}>{text(w, 'name')}</button>
          <small>{label('profile', text(w, 'profile_id'))}</small></div>
      </div></td>
      <td>{number(w, 'memory_mib_per_replica') / 1024} GiB · {number(w, 'compute_units_per_replica')} demand units</td>
      <td>{number(w, 'replicas')}</td><td>{count(stale ? null : w.running_replicas)}</td><td>{count(stale ? null : w.ready_replicas)}</td>
      <td>{count(stale ? null : w.suspended_replicas)} / {count(stale ? null : w.fenced_replicas)} / {count(stale ? null : w.fencing_pending_replicas)}</td>
      <td>{label('data', text(w, 'data_profile_id'))}<small>{label('purpose', text(w, 'purpose'))}</small></td>
      <td><button disabled={disabled} onClick={() => onEdit(w)}>Edit workload</button>
        <button disabled={disabled} onClick={() => onDelete(w)}>Delete</button></td>
    </tr>
    {open && <tr className="replica-detail-row" id={id}><td colSpan={8}>
      <WorkloadReplicas workloadId={text(w, 'workload_id')} view={view}/>
    </td></tr>}
  </>;
}
