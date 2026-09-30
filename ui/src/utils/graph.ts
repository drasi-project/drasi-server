import { MarkerType, type Node, type Edge } from "@xyflow/react";
import type { ComputationComponent, ComputationInspection } from "../api/types";
import { computationStatus, isInternalComponent, relationshipEndpoints, relationshipLabel } from "./computation.ts";

export type CanvasComponentType = "source" | "query" | "reaction" | "computation";

// Match the dimensions passed to NodeShell by each renderer.
const NODE_DIMENSIONS: Record<string, { collapsed: [number, number]; expanded: [number, number] }> = {
  sourceNode: { collapsed: [180, 92], expanded: [320, 250] },
  queryNode: { collapsed: [180, 92], expanded: [420, 280] },
  reactionNode: { collapsed: [180, 92], expanded: [300, 180] },
  computationNode: { collapsed: [220, 122], expanded: [220, 122] },
};

export function getNodeDimensions(node: Node): { width: number; height: number } {
  const spec = NODE_DIMENSIONS[node.type ?? ""];
  const [width, height] = spec
    ? (node.data.expanded ? spec.expanded : spec.collapsed)
    : [180, 92];
  return { width, height };
}

export interface PipelineData {
  computation?: ComputationInspection;
  sources: SourceInfo[];
  queries: QueryInfo[];
  reactions: ReactionInfo[];
}

export interface SourceInfo {
  id: string;
  kind: string;
  status: string;
  autoStart: boolean;
  properties?: Record<string, unknown>;
  instanceId?: string;
  error?: string;
}

export interface QueryInfo {
  id: string;
  status: string;
  sourceIds: string[];
  resultCount?: number;
  query?: string;
  queryLanguage?: string;
  error?: string;
  instanceId?: string;
}

export interface ReactionInfo {
  id: string;
  kind: string;
  status: string;
  queryIds: string[];
  properties?: Record<string, unknown>;
  error?: string;
}

export function canvasComponentType(component: ComputationComponent, data: PipelineData): CanvasComponentType {
  if (!isInternalComponent(component)) {
    if (component.role === "Source" && data.sources.some((s) => s.id === component.id)) return "source";
    if (component.role === "Query" && data.queries.some((q) => q.id === component.id)) return "query";
    if (component.role === "Reaction" && data.reactions.some((r) => r.id === component.id)) return "reaction";
  }
  return "computation";
}

export function buildFlowGraph(data: PipelineData, showInternal = false): {
  nodes: Node[];
  edges: Edge[];
  unresolvedRelationships: number;
} {
  const components = data.computation?.components ?? [];
  const nodes: Node[] = components.map((component) => {
    const componentType = canvasComponentType(component, data);
    const details = componentType === "source" ? data.sources.find((s) => s.id === component.id)
      : componentType === "query" ? data.queries.find((q) => q.id === component.id)
      : componentType === "reaction" ? data.reactions.find((r) => r.id === component.id)
      : undefined;
    const internal = isInternalComponent(component);
    return {
      id: `component-${component.id}`,
      type: componentType === "computation" ? "computationNode" : `${componentType}Node`,
      position: { x: 0, y: 0 },
      hidden: internal && !showInternal,
      data: {
        ...details,
        id: component.id,
        role: component.role,
        componentType,
        status: computationStatus(component),
        inspection: component,
        internal,
        graphHandles: "both",
      },
    };
  });
  const byId = new Map(nodes.map((n) => [n.data.id, n]));
  const edges: Edge[] = [];
  let unresolvedRelationships = 0;
  for (const relationship of data.computation?.relationships ?? []) {
    const { from, to } = relationshipEndpoints(relationship);
    const source = byId.get(from);
    const target = byId.get(to);
    if (!source || !target) {
      unresolvedRelationships++;
      continue;
    }
    const active = relationship.representation === "NativeProvider"
      && relationship.binding === "Bound" && relationship.availability === "Available";
    edges.push({
      id: JSON.stringify([relationship.representation, relationship.from, relationship.to]),
      source: source.id,
      target: target.id,
      hidden: source.hidden || target.hidden,
      type: "animatedEdge",
      label: relationshipLabel(relationship),
      animated: active,
      markerEnd: { type: MarkerType.ArrowClosed },
      style: {
        stroke: active ? "#10b981" : "var(--drasi-edge)",
        strokeWidth: 2,
        strokeDasharray: relationship.representation === "ControlConnection" ? "5 5" : undefined,
      },
    });
  }
  return { nodes: layoutFlowNodes(nodes, edges), edges, unresolvedRelationships };
}

export function layoutFlowNodes(
  nodes: Node[],
  edges: Edge[],
  dimensions: (node: Node) => { width: number; height: number } = getNodeDimensions,
): Node[] {
  const visible = nodes.filter((n) => !n.hidden);
  const isProcessing = (node: Node) => node.data.role === "Query" || node.data.role === "Transformer";
  const processingGroup = visible.find(isProcessing)?.id;
  // Treat queries and transformers as one layout group, without changing real edges.
  const groups = new Map(visible.map((node) => [
    node.id,
    isProcessing(node) ? processingGroup ?? node.id : node.id,
  ]));
  const outgoing = new Map([...groups.values()].map((id) => [id, new Set<string>()]));
  const incoming = new Set<string>();
  for (const edge of edges) {
    const from = groups.get(edge.source);
    const to = groups.get(edge.target);
    if (!edge.hidden && from !== undefined && to !== undefined && from !== to) {
      outgoing.get(from)!.add(to);
      incoming.add(to);
    }
  }
  const levels = new Map<string, number>();
  const queue: string[] = [];
  for (const id of outgoing.keys()) {
    if (!incoming.has(id)) {
      levels.set(id, 0);
      queue.push(id);
    }
  }
  let cursor = 0;
  const visitQueued = () => {
    while (cursor < queue.length) {
      const id = queue[cursor++];
      for (const next of outgoing.get(id) ?? []) {
        if (!levels.has(next)) {
          levels.set(next, levels.get(id)! + 1);
          queue.push(next);
        }
      }
    }
  };
  visitQueued();
  // Seed disconnected cycles as well as roots; each component is visited once.
  for (const id of outgoing.keys()) {
    if (!levels.has(id)) {
      levels.set(id, 0);
      queue.push(id);
      visitQueued();
    }
  }
  const columns = new Map<number, Node[]>();
  for (const node of visible) {
    const level = levels.get(groups.get(node.id)!) ?? 0;
    columns.set(level, [...(columns.get(level) ?? []), node]);
  }
  const positions = new Map<string, { x: number; y: number }>();
  let x = 50;
  for (const [, column] of [...columns].sort(([a], [b]) => a - b)) {
    let y = 60;
    for (const node of column) {
      positions.set(node.id, { x, y });
      y += dimensions(node).height + 60;
    }
    x += Math.max(...column.map((n) => dimensions(n).width)) + 100;
  }
  return nodes.map((node) => ({ ...node, position: positions.get(node.id) ?? node.position }));
}
