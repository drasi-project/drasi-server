import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { architectureEdges as edges, architectureNodes as nodes, databaseInputs } from './architectureData';
import { queryIds } from './rows';

const source = (path: string) => readFileSync(fileURLToPath(new URL(path, import.meta.url)), 'utf8');
const node = (id: string) => {
  const found = nodes.find(n => n.id === id);
  if (!found) throw new Error(`Missing architecture node ${id}`);
  return found;
};

describe('embedded architecture inventory and contracts', () => {
  it('has distinct identities, meaningful details and resolvable connection endpoints', () => {
    const ids = [...nodes, ...edges].map(item => item.id);
    expect(new Set(ids).size).toBe(ids.length);
    for (const component of nodes) {
      expect(component.title.trim().length).toBeGreaterThan(0);
      for (const value of [component.implementation, component.detail,
        component.input, component.output, component.example, component.note]) expect(value.trim().length).toBeGreaterThan(15);
      expect(component.x).toBeGreaterThan(0);
      expect(component.y).toBeGreaterThan(0);
      expect(component.x).toBeLessThan(1360);
      expect(component.y).toBeLessThan(850);
    }
    for (const edge of edges) {
      expect(edge.sources.length).toBeGreaterThan(0);
      expect(edge.targets.length).toBeGreaterThan(0);
      for (const id of [...edge.sources, ...edge.targets]) expect(nodes.some(n => n.id === id)).toBe(true);
      for (const value of [edge.title, edge.implementation, edge.detail, edge.schema, edge.changes, edge.note]) {
        expect(value.trim().length).toBeGreaterThan(15);
        expect(value).not.toMatch(/undefined|\[object Object\]/);
      }
      expect(edge.path).toMatch(/^M/);
    }
  });

  it('covers all fourteen real queries once and the nine UI subscriptions', () => {
    const inputs = source('../../crates/native/src/inputs.rs');
    const declared = nodes.flatMap(n => n.queryIds ?? []);
    const expected = ['input-configuration', ...queryIds,
      'simulation-inputs', 'scheduling-inputs', 'plan-output', 'runtime-context'];
    expect(declared).toHaveLength(14);
    expect(new Set(declared).size).toBe(14);
    expect([...declared].sort()).toEqual(expected.sort());
    expect(node('views').queryIds).toEqual(queryIds);
    for (const [id, table] of databaseInputs) {
      expect(inputs).toContain(`"${id}"`);
      expect(source('../../../embedded/src/database.rs')).toContain(`"${table}"`);
    }
    for (const id of ['simulation-inputs', 'scheduling-inputs']) {
      const detail = nodes.find(n => n.queryIds?.includes(id));
      expect(detail?.schema).toBe(source(`../../queries/${id}.cypher`));
    }
  });

  it('matches native implementations and meaningful assembly connections without a hypothetical writer path', () => {
    const assembly = source('../../../embedded/src/main.rs');
    const processors = ['policy', 'simulator', 'placement', 'resilience'];
    for (const id of processors) {
      const implementation = node(id).implementation.match(/gpu\.lab\/[\w-]+/)?.[0];
      expect(implementation).toBeDefined();
      expect(assembly).toContain(`("${id}", "${implementation}")`);
    }
    for (const [from, to] of [
      ['policy', 'simulation-inputs'], ['policy', 'scheduling-inputs'],
      ['simulation-inputs', 'simulator'], ['simulator', 'scheduling-inputs'],
      ['scheduling-inputs', 'placement'], ['scheduling-inputs', 'resilience'],
      ['placement', 'plan-output'], ['plan-output', 'plan-writer'], ['placement', 'runtime-context'],
    ]) expect(assembly).toContain(`("${from}", "${to}")`);
    expect(edges.find(e => e.id === 'evidence')?.sources).toEqual([...processors, 'status']);
    expect(edges.filter(e => e.sources.includes('resilience')).every(e => e.targets.includes('views'))).toBe(true);
    expect(edges.filter(e => e.targets.includes('writer')).map(e => e.id)).toEqual(['candidate']);
    expect(edges.find(e => e.id === 'write-outcomes')?.kind).toBe('lifecycle');
  });

  it('documents actual candidate and report fields, query payload and browser routes', () => {
    const contracts = source('../../crates/contracts/src/lib.rs');
    const candidate = contracts.split('pub struct Candidate {')[1].split('\n}')[0];
    const fields = [...candidate.matchAll(/pub (\w+):/g)].map(match => match[1]);
    const writer = edges.find(e => e.id === 'write-plan');
    for (const field of fields) expect(writer?.schema).toContain(`${field}:`);
    expect(contracts).toContain('pub const MEMORY_MIB: u32 = 81920;');
    expect(contracts).toContain('pub const PLANNING_UNITS: u32 = 85;');
    const samples = source('../../crates/simulator/src/lib.rs').split('pub struct Sample {')[1].split('\n}')[0];
    const report = edges.find(e => e.id === 'reports');
    for (const field of report?.schema.matchAll(/^\s+(\w+):/gm) ?? []) expect(samples).toContain(`pub ${field[1]}:`);
    expect(source('../../crates/native/src/inputs.rs')).toContain('MATCH (p:CandidatePlan) RETURN p.payload AS payload');
    const control = source('../../../embedded/control/main.rs');
    expect(control).toContain('.route("/internal/placement-plans", post(write_plan))');
    expect(control).toContain('.route("/events/gpu-demo", get(events))');
    expect(source('./main.tsx')).toContain('NativeSseProvider');
    expect(node('delivery').output).toContain('/events/gpu-demo');
    expect(node('postgres').note).toContain('WAL acknowledgement follows the atomic query commit');
  });
});
