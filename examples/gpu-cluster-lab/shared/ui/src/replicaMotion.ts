import { useEffect, useRef, useState, type RefObject } from 'react';
import { useReducedMotion } from '@drasi/react/react';
import { gpuLocation, replicaCaption, replicaTiles, type ReplicaSection, type ReplicaView } from './replicas';
import { text } from './rows';

export type ReplicaCue = {
  id: string; kind: 'section' | 'move'; caption: string;
  fromGpu: string; toGpu: string; fromName: string; toName: string;
  fromSection: ReplicaSection; toSection: ReplicaSection;
};
const locationKey = (gpu: string, replica: string) => JSON.stringify([gpu, replica]);

export function replicaChanges(before: ReplicaView, after: ReplicaView): ReplicaCue[] {
  if (!before.live || !after.live || before.epoch !== after.epoch) return [];
  const oldTiles = replicaTiles(before);
  const nextTiles = replicaTiles(after);
  const changes: ReplicaCue[] = [];
  for (const replica of after.replicas) {
    if (!replica.actual || replica.actual.acknowledged_at_ms === null) continue;
    const previous = before.replicas.find(row => row.id === replica.id);
    if (!previous) continue;
    const toGpu = text(replica.actual, 'gpu_id');
    const destination = nextTiles.find(tile => tile.replica.id === replica.id && tile.gpuId === toGpu);
    const fromGpu = previous.actual ? text(previous.actual, 'gpu_id') : toGpu;
    const origin = oldTiles.find(tile => tile.replica.id === replica.id && tile.gpuId === fromGpu);
    if (!origin || !destination || (origin.section === destination.section && fromGpu === toGpu)) continue;
    if (fromGpu !== toGpu && replica.actual.state !== 'running') continue;
    if (typeof previous.actual?.acknowledged_at_ms === 'number'
      && typeof replica.actual.acknowledged_at_ms === 'number'
      && replica.actual.acknowledged_at_ms < previous.actual.acknowledged_at_ms) continue;
    changes.push({
      id: replica.id, kind: fromGpu === toGpu ? 'section' : 'move', caption: replicaCaption(replica),
      fromGpu, toGpu, fromName: gpuLocation(before.gpus, fromGpu), toName: gpuLocation(after.gpus, toGpu),
      fromSection: origin.section, toSection: destination.section,
    });
  }
  return changes;
}

type TilePosition = { element: HTMLElement; x: number; y: number };
function positions(root: HTMLElement | null): Map<string, TilePosition> {
  const values = new Map<string, TilePosition>();
  for (const element of root?.querySelectorAll<HTMLElement>('[data-allocation-gpu]') ?? []) {
    const rect = element.getBoundingClientRect();
    if (!rect.width || !rect.height) continue;
    const card = element.closest('.gpu-card')?.getBoundingClientRect();
    values.set(locationKey(element.dataset.allocationGpu!, element.dataset.hierarchyId!), {
      element, x: rect.x - (card?.x ?? 0), y: rect.y - (card?.y ?? 0),
    });
  }
  return values;
}

export function useReplicaMotion(root: RefObject<HTMLElement>, view: ReplicaView, enabled: boolean): readonly ReplicaCue[] {
  const reducedMotion = useReducedMotion();
  const previous = useRef<{ view: ReplicaView; positions: Map<string, TilePosition> } | null>(null);
  const animations = useRef(new Map<string, Animation>());
  const timers = useRef(new Map<string, ReturnType<typeof setTimeout>>());
  const [cues, setCues] = useState<ReplicaCue[]>([]);
  const cancelAnimations = () => {
    for (const animation of animations.current.values()) animation.cancel();
    animations.current.clear();
  };
  useEffect(() => {
    if (!enabled || reducedMotion) cancelAnimations();
    const measured = positions(root.current), prior = previous.current;
    previous.current = view.live ? { view, positions: measured } : null;
    if (!view.live || !enabled || (prior && prior.view.epoch !== view.epoch)) {
      cancelAnimations();
      timers.current.forEach(clearTimeout);
      timers.current.clear();
      setCues(current => current.length ? [] : current);
      return;
    }
    if (!prior) return;
    const present = new Set(view.replicas.map(replica => replica.id));
    for (const [id, timer] of timers.current) {
      if (present.has(id)) continue;
      clearTimeout(timer);
      timers.current.delete(id);
      animations.current.get(id)?.cancel();
      animations.current.delete(id);
    }
    const changed = replicaChanges(prior.view, view);
    if (!changed.length) {
      setCues(current => current.some(cue => !present.has(cue.id))
        ? current.filter(cue => present.has(cue.id)) : current);
      return;
    }
    const ids = new Set(changed.map(change => change.id));
    setCues(current => [...current.filter(cue => present.has(cue.id) && !ids.has(cue.id)), ...changed]);
    for (const change of changed) {
      clearTimeout(timers.current.get(change.id));
      animations.current.get(change.id)?.cancel();
      animations.current.delete(change.id);
      timers.current.set(change.id, setTimeout(() => {
        timers.current.delete(change.id);
        setCues(current => current.filter(cue => cue.id !== change.id));
      }, 4000));
      const destination = measured.get(locationKey(change.toGpu, change.id));
      const origin = prior.positions.get(locationKey(change.fromGpu, change.id));
      if (reducedMotion || !destination?.element.animate) continue;
      const slide = change.kind === 'section' && origin;
      const keyframes: Keyframe[] = slide
        ? [{ transform: `translate(${origin.x - destination.x}px, ${origin.y - destination.y}px)` }, { transform: 'translate(0, 0)' }]
        : [{ opacity: 0.35, boxShadow: '0 0 0 0 transparent' },
          { opacity: 1, boxShadow: '0 0 0 5px #92eaff66', offset: 0.5 },
          { opacity: 1, boxShadow: '0 0 0 0 transparent' }];
      const animation = destination.element.animate(keyframes, {
        duration: slide ? 380 : 1100, easing: 'ease-out', iterations: slide ? 1 : 2,
      });
      animations.current.set(change.id, animation);
      const finished = () => { if (animations.current.get(change.id) === animation) animations.current.delete(change.id); };
      animation.onfinish = finished;
      animation.oncancel = finished;
    }
  });
  useEffect(() => () => {
    cancelAnimations();
    timers.current.forEach(clearTimeout);
    timers.current.clear();
  }, []);
  return view.live && enabled ? cues : [];
}
