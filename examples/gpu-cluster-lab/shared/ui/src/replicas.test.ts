import { describe, expect, it } from 'vitest';
import { buildReplicaView, replicaTiles } from './replicas';
import { replicaChanges } from './replicaMotion';
import { mockViews } from './mock/MockApp';
import { snapshot } from './mock/scenarios';
import { records } from './rows';

const view = (scene: string) => buildReplicaView(mockViews(snapshot(scene), scene, () => {}));

describe('replica sections and workload detail', () => {
  it('renders one running tile per replica with no duplicate planned copy at the same GPU', () => {
    const state = view('healthy');
    expect(state.replicas).toHaveLength(8);
    expect(replicaTiles(state)).toHaveLength(8);
    expect(replicaTiles(state).every(tile => tile.section === 'running')).toBe(true);
    expect(state.replicas.every(replica => replica.confirmed && replica.planned && replica.actual)).toBe(true);
  });
  it('retains both locations when a running replica has a different planned GPU', () => {
    const state = view('plan-committed');
    const moved = state.replicas.find(replica => replica.id === 'mock-chat-alpha/0')!;
    expect(moved.status).toBe('Awaiting move');
    expect(moved.actual?.gpu_id).toBe('mock-inference-a-0');
    expect(moved.planned?.gpu_id).toBe('mock-inference-c-1');
    expect(replicaTiles(state).filter(tile => tile.replica.id === moved.id).map(tile => tile.section))
      .toEqual(['running', 'planned']);
    const added = state.replicas.find(replica => replica.id === 'mock-assistant/0')!;
    expect(added.actual).toBeUndefined();
    expect(added.status).toBe('Not running');
  });
  it.each([
    ['policy-unknown', 'paused', 'Paused by policy'],
    ['fencing-pending', 'stopping', 'Stop requested'],
    ['policy-fenced', 'stopped', 'Stopped by policy'],
  ] as const)('places %s in its own section without duplicating saved intent', (scene, section, status) => {
    const state = view(scene), tiles = replicaTiles(state);
    expect(tiles).toHaveLength(8);
    expect(tiles.every(tile => tile.section === section && tile.replica.planned)).toBe(true);
    expect(state.replicas.every(replica => replica.status === status && !replica.confirmed)).toBe(true);
  });
  it('does not call never-started denied replicas stopped or fabricate their GPU location', () => {
    const rows = snapshot('policy-fenced');
    Object.assign(rows['ui-placements'][0], { actual: [], applied_plan_version: null });
    const state = buildReplicaView(mockViews(rows, 'policy-fenced', () => {}));
    expect(state.executionKnown).toBe(true);
    expect(state.replicas.every(replica => replica.status === 'Blocked by policy' && !replica.actual)).toBe(true);
    expect(replicaTiles(state).every(tile => tile.section === 'planned')).toBe(true);
  });
  it('includes requested replicas with no saved allocation', () => {
    const state = view('fragmentation-candidate');
    const added = state.replicas.find(replica => replica.id === 'mock-assistant/0')!;
    expect(added.status).toBe('Awaiting allocation');
    expect(added.requested).toBe(true);
    expect(added.planned).toBeUndefined();
    expect(added.actual).toBeUndefined();
  });
  it('keeps historical replicas separate from the current requirement and labels unknown execution', () => {
    const rows = snapshot('healthy');
    rows['ui-workloads'][0].replicas = 0;
    const state = buildReplicaView(mockViews(rows, 'healthy', () => {}));
    expect(state.replicas.filter(replica => !replica.requested)).toHaveLength(2);
    const stale = buildReplicaView(mockViews(rows, 'feed-stale', () => {}));
    expect(stale.executionKnown).toBe(false);
    expect(stale.replicas.every(replica => replica.status === 'Unknown' && !replica.confirmed)).toBe(true);
  });
});

describe('observed replica movement', () => {
  it('does not move when the plan is merely saved, and only fades cross-GPU moves after application', () => {
    expect(replicaChanges(view('fragmented'), view('plan-committed'))).toEqual([]);
    const changes = replicaChanges(view('plan-committed'), view('plan-applied'));
    expect(changes).toHaveLength(2);
    expect(changes.find(change => change.id === 'mock-chat-alpha/0')).toMatchObject({
      kind: 'move', fromGpu: 'mock-inference-a-0', toGpu: 'mock-inference-c-1',
      fromSection: 'running', toSection: 'running',
    });
    expect(changes.find(change => change.id === 'mock-assistant/0')).toMatchObject({
      kind: 'section', fromGpu: 'mock-inference-a-0', toGpu: 'mock-inference-a-0',
      fromSection: 'planned', toSection: 'running',
    });
    expect(replicaChanges(view('plan-applied'), view('plan-confirmed'))).toEqual([]);
  });
  it('moves between acknowledged policy-state sections without pretending the GPU changed', () => {
    const changes = replicaChanges(view('regional'), view('policy-fenced'));
    expect(changes).toHaveLength(8);
    expect(changes.every(change => change.kind === 'section' && change.fromGpu === change.toGpu
      && change.fromSection === 'running' && change.toSection === 'stopped')).toBe(true);
    expect(replicaChanges(view('regional'), view('fencing-pending'))).toEqual([]);
  });
  it('fences reconnect, epoch changes, and older application evidence', () => {
    const before = view('plan-committed'), after = view('plan-applied');
    expect(replicaChanges({ ...before, live: false }, after)).toEqual([]);
    expect(replicaChanges(before, { ...after, live: false })).toEqual([]);
    expect(replicaChanges(before, { ...after, epoch: 'new-run' })).toEqual([]);
    const rows = snapshot('plan-applied');
    rows['ui-placements'][0].actual = records(rows['ui-placements'][0], 'actual').map(row =>
      ({ ...row, acknowledged_at_ms: 0 }));
    const older = buildReplicaView(mockViews(rows, 'plan-applied', () => {}));
    expect(replicaChanges(before, older).some(change => change.id === 'mock-chat-alpha/0')).toBe(false);
  });
});
