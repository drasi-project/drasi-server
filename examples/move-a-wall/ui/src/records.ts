import type { ResultRow } from '@drasi/react/client';
export const queryIds = ['scene-inputs','geometry-context','obstructions','affected-journeys','geometry-status'] as const;
export type QueryId = typeof queryIds[number];
export type Point = [number, number];
export type Shape =
  | { kind: 'cart'; radius: number; clearance: number }
  | { kind: 'journey'; cart_id: string; destination_id: string; points: Point[] }
  | { kind: 'destination'; point: Point }
  | { kind: 'obstacle'; vertices: Point[] };
export type Entity = { id: string; name: string; active: boolean; shape: Shape };
export type Clock = { type: 'clock'; revision: number; members: Record<string,number>; changed: string[]; command: string };
export type Input = Clock | { type: 'entity'; revision: number; entity: Entity };
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
export function inputs(row: ResultRow): Input[] {
  if (!Array.isArray(row.objects)) throw new Error('Query returned no objects array');
  return row.objects.map(payload => {
    const r = object(JSON.parse(text(payload)));
    if (r.type === 'entity') return { type:'entity', revision:number(r.revision), entity:entity(r.entity) };
    if (r.type !== 'clock' || !Array.isArray(r.changed)) throw new Error('Invalid source revision record');
    const members = Object.fromEntries(Object.entries(object(r.members)).map(([key,v]) => [key,number(v)]));
    return { type:'clock', revision:number(r.revision), members, changed:r.changed.map(text), command:text(r.command) };
  });
}
export function validate(id: QueryId, row: ResultRow): ResultRow {
  if (id === 'scene-inputs' || id === 'geometry-context') inputs(row);
  else {
    text(row.id);
    if (id === 'geometry-status') { number(row.revision); number(row.obstructions); }
    if (id === 'affected-journeys') for (const key of ['cart_id','journey_id','destination_id','obstacle_id','cart','task','destination','obstacle']) text(row[key]);
    if (id === 'obstructions') for (const key of ['journey_id','cart_id','obstacle_id']) text(row[key]);
  }
  return row;
}
export function key(id: QueryId, row: ResultRow): string {
  return id === 'scene-inputs' || id === 'geometry-context' ? id : text(row.id);
}
export function translate(e: Entity, dx: number, dy: number): Entity {
  const move = (p: Point): Point => [Number((p[0]+dx).toFixed(3)),Number((p[1]+dy).toFixed(3))];
  if (e.shape.kind === 'obstacle') return { ...e, shape:{ ...e.shape,vertices:e.shape.vertices.map(move) } };
  if (e.shape.kind === 'journey') return { ...e, shape:{ ...e.shape,points:e.shape.points.map(move) } };
  if (e.shape.kind === 'destination') return { ...e, shape:{ ...e.shape,point:move(e.shape.point) } };
  return e;
}
