import type { ResultRow } from '@drasi/react/client';
export const queryIds = ['scene-inputs','journey-footprints','obstacle-footprints','obstructions','affected-journeys','scene-status'] as const;
export type QueryId = typeof queryIds[number];
export type Point = [number, number];
export type Shape =
  | { kind: 'cart'; radius: number; clearance: number }
  | { kind: 'journey'; cart_id: string; destination_id: string; points: Point[] }
  | { kind: 'destination'; point: Point }
  | { kind: 'obstacle'; vertices: Point[] };
export type Entity = { id: string; name: string; active: boolean; shape: Shape };
export type Status = { revision: number; command: string; changed: string[]; objects: number };
export function object(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('Expected an object');
  return value as Record<string, unknown>;
}
export function text(value: unknown): string {
  if (typeof value !== 'string') throw new Error('Expected a string');
  return value;
}
export function number(value: unknown): number {
  if (typeof value !== 'number' || !Number.isFinite(value)) throw new Error('Expected a finite number');
  return value;
}
function bool(value: unknown): boolean {
  if (typeof value !== 'boolean') throw new Error('Expected a boolean');
  return value;
}
function point(value: unknown): Point {
  if (!Array.isArray(value) || value.length !== 2) throw new Error('Expected [x,y]');
  return [number(value[0]),number(value[1])];
}
function points(value: unknown): Point[] {
  if (!Array.isArray(value)) throw new Error('Expected an array of points');
  return value.map(point);
}
export function entity(value: unknown): Entity {
  const v = object(value), s = object(v.shape);
  let shape: Shape;
  switch (s.kind) {
    case 'cart': shape = { kind:s.kind, radius:number(s.radius), clearance:number(s.clearance) }; break;
    case 'journey': shape = { kind:s.kind, cart_id:text(s.cart_id), destination_id:text(s.destination_id), points:points(s.points) }; break;
    case 'destination': shape = { kind:s.kind, point:point(s.point) }; break;
    case 'obstacle': shape = { kind:s.kind, vertices:points(s.vertices) }; break;
    default: throw new Error(`Unsupported entity kind ${String(s.kind)}`);
  }
  return { id:text(v.id), name:text(v.name), active:bool(v.active), shape };
}
/** One scene-inputs row: a typed scene node's plain properties. */
export function sceneEntity(row: ResultRow): Entity {
  const { id, kind, name, active, ...fields } = row;
  return entity({ id, name, active, shape:{ kind, ...fields } });
}
export function status(row: ResultRow): Status {
  if (!Array.isArray(row.changed)) throw new Error('Scene status has no changed array');
  return { revision:number(row.revision), command:text(row.command), changed:row.changed.map(text), objects:number(row.objects) };
}
export function validate(id: QueryId, row: ResultRow): ResultRow {
  switch (id) {
    case 'scene-inputs': sceneEntity(row); break;
    case 'scene-status': status(row); break;
    case 'journey-footprints':
      for (const key of ['journey_id','cart_id']) text(row[key]);
      points(row.points); number(row.radius); number(row.clearance); break;
    case 'obstacle-footprints': text(row.obstacle_id); points(row.vertices); break;
    case 'affected-journeys':
      text(row.id);
      for (const key of ['cart_id','journey_id','destination_id','obstacle_id','cart','task','destination','obstacle']) text(row[key]);
      break;
    case 'obstructions':
      text(row.id);
      for (const key of ['journey_id','cart_id','obstacle_id']) text(row[key]);
      number(row.distance_m); number(row.required_m); break;
  }
  return row;
}
export function translate(e: Entity, dx: number, dy: number): Entity {
  const move = (p: Point): Point => [Number((p[0]+dx).toFixed(3)),Number((p[1]+dy).toFixed(3))];
  if (e.shape.kind === 'obstacle') return { ...e, shape:{ ...e.shape,vertices:e.shape.vertices.map(move) } };
  if (e.shape.kind === 'journey') return { ...e, shape:{ ...e.shape,points:e.shape.points.map(move) } };
  if (e.shape.kind === 'destination') return { ...e, shape:{ ...e.shape,point:move(e.shape.point) } };
  return e;
}
