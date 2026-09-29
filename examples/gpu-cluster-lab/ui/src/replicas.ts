import type { ResultRow } from '@drasi/react/client';
import type { Views } from './App';
import { correlated, currentFeeds, executionObserved, replicaInputs, replicaReady } from './Evidence';
import { label, reasonText } from './labels';
import { number, records, text } from './rows';

export type ReplicaSection = 'planned' | 'running' | 'paused' | 'stopping' | 'stopped';
export const sectionNames: Record<ReplicaSection, string> = {
  planned: 'Planned', running: 'Running', paused: 'Paused', stopping: 'Stopping', stopped: 'Stopped',
};
export type Replica = {
  id: string; workloadId: string; name: string; index: number; requested: boolean; workloadPresent: boolean;
  planned?: ResultRow; actual?: ResultRow; confirmed: boolean; status: string; detail: string;
};
export type ReplicaView = {
  replicas: Replica[]; gpus: ResultRow[]; current: boolean; executionKnown: boolean;
  epoch: string; live: boolean; available: boolean;
};
export type ReplicaTile = { replica: Replica; gpuId: string; section: ReplicaSection; assignment: ResultRow };

export function gpuLocation(gpus: readonly ResultRow[], id: string | undefined): string {
  if (!id) return 'Not allocated';
  const gpu = gpus.find(gpu => gpu.gpu_id === id);
  return gpu ? `${text(gpu, 'host_id')} / GPU ${number(gpu, 'slot')}` : `Unavailable GPU (${id})`;
}
export function replicaCaption(replica: Replica): string {
  return `${replica.name} · replica ${replica.index + 1}`;
}
export function executionSection(actual: ResultRow): Exclude<ReplicaSection, 'planned'> {
  switch (actual.state) {
    case 'running': return 'running';
    case 'suspended': return 'paused';
    case 'fenced': return 'stopped';
    case 'fencing-pending': return 'stopping';
    default: throw new Error('Unrecognized replica execution state');
  }
}

export function buildReplicaView(views: Views, disconnected = false): ReplicaView {
  const input = replicaInputs(views), placement = input.placement, status = input.status;
  const epoch = typeof placement?.observation_epoch === 'string' ? placement.observation_epoch : '';
  const available = !!placement;
  const actual = new Map((placement ? records(placement, 'actual') : []).map(row => [text(row, 'id'), row]));
  const sameEpoch = [...actual.values()].every(row => row.observation_epoch === undefined || row.observation_epoch === epoch);
  const live = !disconnected && currentFeeds(views) && !!epoch && sameEpoch
    && (['ui-placements', 'ui-gpus', 'ui-workloads', 'ui-status'] as const).every(id =>
      ['live', 'mock snapshot'].includes(views[id].status));
  const current = !disconnected && currentFeeds(views) && !!epoch && sameEpoch
    && correlated(placement, status) && status?.inputs_ready === true;
  const executionKnown = current && !!placement && executionObserved(placement);
  const planned = new Map((placement ? records(placement, 'desired') : []).map(row => [text(row, 'id'), row]));
  const identities = new Map<string, { workloadId: string; index: number }>();
  for (const workload of input.workloads) {
    for (let index = 0; index < number(workload, 'replicas'); index++) {
      identities.set(`${workload.workload_id}/${index}`, { workloadId: text(workload, 'workload_id'), index });
    }
  }
  for (const row of [...planned.values(), ...actual.values()]) {
    identities.set(text(row, 'id'), { workloadId: text(row, 'workload_id'), index: number(row, 'replica_index') });
  }
  const replicas = [...identities].map(([id, identity]): Replica => {
    const workload = input.workloads.find(row => row.workload_id === identity.workloadId);
    const replica: Replica = {
      id, ...identity, name: workload ? text(workload, 'name') : identity.workloadId,
      workloadPresent: !!workload,
      requested: !!workload && identity.index < number(workload, 'replicas'),
      planned: planned.get(id), actual: actual.get(id), confirmed: false, status: 'Unknown',
      detail: 'Waiting for current execution evidence.',
    };
    if (!executionKnown) {
      if (replica.actual) replica.detail = `Last observed: ${label('execution', text(replica.actual, 'state'))} on ${gpuLocation(input.gpus, text(replica.actual, 'gpu_id'))}.`;
      return replica;
    }
    if (replica.actual) {
      const a = replica.actual, location = gpuLocation(input.gpus, text(a, 'gpu_id'));
      replica.confirmed = current && replicaReady(a, input);
      replica.status = label('execution', text(a, 'state'));
      if (a.state === 'running') {
        replica.detail = replica.confirmed ? 'Confirmed by fresh GPU reports.' : 'Running; not yet confirmed by current plan, policy and reports.';
        if (replica.planned && a.gpu_id !== replica.planned.gpu_id) {
          replica.status = 'Awaiting move';
          replica.detail = `Still running on ${location}; planned on ${gpuLocation(input.gpus, text(replica.planned, 'gpu_id'))}.`;
        }
      } else if (a.state === 'suspended') replica.detail = `Paused by policy on ${location}; memory is retained.`;
      else if (a.state === 'fenced') replica.detail = `Stopped by policy on ${location}; resources released.`;
      else replica.detail = `Stop requested · not yet confirmed on ${location}.`;
    } else {
      replica.status = replica.planned ? 'Not running' : 'Awaiting allocation';
      replica.detail = replica.planned ? 'Planned location has not been applied.' : 'No saved allocation for this requested replica.';
      const pairs = input.policies.filter(pair => pair.workload_id === identity.workloadId);
      if (pairs.length && pairs.every(pair => pair.current === true && pair.authorization === 'deny'
        && pair.observation_epoch === epoch && pair.policy_signature === status?.policy_signature)) {
        replica.status = 'Blocked by policy';
        replica.detail = 'Not running; all observed destinations are denied. This does not imply a previously running replica was stopped.';
      } else if (placement?.status === 'blocked') replica.detail = reasonText(text(placement, 'reason'));
    }
    if (!replica.requested) replica.detail += ' No longer requested by the current workload.';
    return replica;
  }).sort((a, b) => a.name.localeCompare(b.name) || a.index - b.index || a.id.localeCompare(b.id));
  return { replicas, gpus: input.gpus, current, executionKnown, epoch, live, available };
}

export function replicaTiles(view: ReplicaView, gpuId?: string): ReplicaTile[] {
  const tiles: ReplicaTile[] = [];
  for (const replica of view.replicas) {
    if (replica.actual) {
      const id = text(replica.actual, 'gpu_id');
      if (gpuId === undefined || gpuId === id) tiles.push({
        replica, gpuId: id, section: executionSection(replica.actual), assignment: replica.actual,
      });
    }
    if (replica.planned && replica.planned.gpu_id !== replica.actual?.gpu_id) {
      const id = text(replica.planned, 'gpu_id');
      if (gpuId === undefined || gpuId === id) tiles.push({
        replica, gpuId: id, section: 'planned', assignment: replica.planned,
      });
    }
  }
  return tiles;
}
