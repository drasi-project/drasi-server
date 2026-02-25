import type { Node, Edge } from "@xyflow/react";

export interface PipelineData {
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

export const COLUMN_X = { source: 50, query: 400, reaction: 750 };
export const NODE_SPACING_Y = 140;
export const NODE_START_Y = 60;

export function buildFlowGraph(data: PipelineData): {
  nodes: Node[];
  edges: Edge[];
} {
  const nodes: Node[] = [];
  const edges: Edge[] = [];

  data.sources.forEach((src, i) => {
    nodes.push({
      id: `source-${src.id}`,
      type: "sourceNode",
      position: { x: COLUMN_X.source, y: NODE_START_Y + i * NODE_SPACING_Y },
      data: { ...src, componentType: "source" as const },
    });
  });

  data.queries.forEach((q, i) => {
    nodes.push({
      id: `query-${q.id}`,
      type: "queryNode",
      position: { x: COLUMN_X.query, y: NODE_START_Y + i * NODE_SPACING_Y },
      data: { ...q, componentType: "query" as const },
    });

    q.sourceIds.forEach((srcId) => {
      const source = data.sources.find((s) => s.id === srcId);
      const bothRunning = source?.status === "Running" && q.status === "Running";
      edges.push({
        id: `e-source-${srcId}-query-${q.id}`,
        source: `source-${srcId}`,
        target: `query-${q.id}`,
        type: "animatedEdge",
        animated: bothRunning,
        style: {
          stroke: bothRunning ? "#10b981" : "var(--drasi-border)",
          strokeWidth: 2,
        },
      });
    });
  });

  data.reactions.forEach((r, i) => {
    nodes.push({
      id: `reaction-${r.id}`,
      type: "reactionNode",
      position: {
        x: COLUMN_X.reaction,
        y: NODE_START_Y + i * NODE_SPACING_Y,
      },
      data: { ...r, componentType: "reaction" as const },
    });

    r.queryIds.forEach((qId) => {
      const query = data.queries.find((q) => q.id === qId);
      const bothRunning = query?.status === "Running" && r.status === "Running";
      edges.push({
        id: `e-query-${qId}-reaction-${r.id}`,
        source: `query-${qId}`,
        target: `reaction-${r.id}`,
        type: "animatedEdge",
        animated: bothRunning,
        style: {
          stroke: bothRunning ? "#10b981" : "var(--drasi-border)",
          strokeWidth: 2,
        },
      });
    });
  });

  return { nodes, edges };
}
