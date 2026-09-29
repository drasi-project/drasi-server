import { useId } from 'react';
import { count, gib } from './Evidence';
import { HierarchyIcon } from './Hierarchy';
import { gpuLocation, replicaCaption, replicaTiles, sectionNames, type ReplicaSection, type ReplicaView } from './replicas';
import { number, text } from './rows';
import type { ReplicaCue } from './replicaMotion';

const sections: readonly ReplicaSection[] = ['planned', 'running', 'paused', 'stopping', 'stopped'];

export function ReplicaPlacements({ gpuId, view, compact, selectedWorkload, onToggleWorkload, cues }: {
  gpuId: string; view: ReplicaView; compact: boolean; selectedWorkload: string | null;
  onToggleWorkload: (id: string) => void; cues: readonly ReplicaCue[];
}) {
  const heading = useId();
  if (!view.available) return <p className="muted">Replica placement data is unavailable.</p>;
  const tiles = replicaTiles(view, gpuId);
  const move = cues.find(cue => cue.kind === 'move' && (cue.fromGpu === gpuId || cue.toGpu === gpuId));
  return <div className={`allocations split-allocations${!view.current ? ' stale-evidence' : ''}${compact ? ' compact-allocations' : ''}`}
    aria-label="Replica activity">
    {!view.current && <p className="muted">Replica activity (last received data); current execution is unknown.</p>}
    {sections.map(section => {
      const members = tiles.filter(tile => tile.section === section);
      if (!members.length && section !== 'planned' && section !== 'running') return null;
      const title = `${heading}-${section}`;
      return <section className={`replica-section replica-section-${section}`} key={section} aria-labelledby={title}>
        <h5 id={title} title={section === 'planned' ? 'Saved destinations not already represented by an observed replica on this GPU.' : undefined}>
          {sectionNames[section]}{!view.current && section !== 'planned' && ' · last observed'}
          <span>{section !== 'planned' && !view.executionKnown ? 'Unknown' : members.length}</span></h5>
        <div className="replica-section-items">
          {members.map(({ replica, assignment }) => {
            const current = view.executionKnown;
            const pendingHere = replica.planned?.gpu_id === gpuId;
            const description = section === 'planned'
              ? replica.actual?.state === 'running'
                ? `Running on ${gpuLocation(view.gpus, text(replica.actual, 'gpu_id'))}; planned here.`
                : current ? replica.status === 'Blocked by policy' ? 'Blocked by policy · not running' : 'Planned here · not running'
                  : 'Planned here · execution unknown'
              : replica.detail;
            const cue = cues.find(cue => cue.id === replica.id && cue.toGpu === gpuId);
            return <div key={replica.id} className={`allocation ${section === 'planned' ? 'desired' : text(assignment, 'state')}${replica.workloadId === selectedWorkload ? ' selected-replica' : ''}${cue ? ' replica-cued' : ''}`}
              data-hierarchy-kind="replica" data-hierarchy-id={replica.id}
              data-allocation-gpu={gpuId} data-allocation-section={section}
              title={`${description} · ${gib(number(assignment, 'memory_mib'))} · ${number(assignment, 'compute_units')} demand units`}>
              <strong><HierarchyIcon kind="replica"/>{replica.workloadPresent ? <button type="button" className="replica-link"
                aria-pressed={selectedWorkload === replica.workloadId} title={`Highlight ${replica.name} and its replicas`}
                onClick={() => onToggleWorkload(replica.workloadId)}>{replicaCaption(replica)}</button> : replicaCaption(replica)}</strong>
              <span>{compact && current && section === 'running' && replica.status === 'Running'
                ? replica.confirmed ? 'Confirmed by GPU report' : 'Not yet confirmed' : description}</span>
              {section !== 'planned' && replica.planned && <span className="planned-reference">{pendingHere ? 'Also planned here' : `Planned on ${gpuLocation(view.gpus, text(replica.planned, 'gpu_id'))}`}</span>}
              {!compact && section !== 'planned' && typeof assignment.acknowledged_at_ms === 'number'
                && <span>Simulator confirmed this action at {new Date(assignment.acknowledged_at_ms).toISOString().slice(11, 19)} UTC</span>}
              {!compact && <small>Requirements: {gib(number(assignment, 'memory_mib'))} · {number(assignment, 'compute_units')} demand units</small>}
            </div>;
          })}
          {!members.length && <p className="muted">{section === 'planned' ? 'No pending placement here'
            : view.executionKnown ? 'No running replicas' : 'Replica activity unknown'}</p>}
        </div>
      </section>;
    })}
    <p className="replica-move-note" title={move ? `${move.caption} moved from ${move.fromName} to ${move.toName}.` : undefined}>
      {move && (move.fromGpu === gpuId ? `${move.caption} moved to ${move.toName}.` : `${move.caption} moved here from ${move.fromName}.`)}
    </p>
  </div>;
}

export function WorkloadReplicas({ workloadId, view }: { workloadId: string; view: ReplicaView }) {
  const replicas = view.replicas.filter(replica => replica.workloadId === workloadId);
  return <div className="workload-replica-detail">
    {!view.current && <p className="warning">Last received replica information; current execution is unknown.</p>}
    {replicas.length ? <ul className="workload-replica-tree">{replicas.map(replica =>
      <li key={replica.id} data-replica-detail={replica.id}>
        <div className="replica-tree-title"><HierarchyIcon kind="replica"/><strong>Replica {replica.index + 1}</strong>
          <span>{replica.status}</span>{!replica.requested && <span>No longer requested</span>}</div>
        <dl className="replica-locations">
          <dt>Planned</dt><dd>{gpuLocation(view.gpus, replica.planned && text(replica.planned, 'gpu_id'))}</dd>
          <dt>{!view.executionKnown ? 'Running on' : replica.actual?.state === 'fenced' ? 'Stopped on'
            : replica.actual?.state === 'suspended' ? 'Paused on' : replica.actual?.state === 'fencing-pending' ? 'Stop requested on' : 'Running on'}</dt>
          <dd>{!view.executionKnown ? 'Unknown' : replica.actual
            ? gpuLocation(view.gpus, text(replica.actual, 'gpu_id')) : 'Not running'}</dd>
          <dt>Confirmed</dt><dd>{!view.executionKnown ? 'Unknown' : replica.confirmed ? 'Yes · fresh GPU report' : 'No'}</dd>
        </dl><p>{replica.detail}</p>
      </li>)}</ul> : <p className="muted">{count(replicas.length)} replicas requested or observed.</p>}
  </div>;
}
