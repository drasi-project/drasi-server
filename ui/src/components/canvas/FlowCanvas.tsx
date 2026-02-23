import { useCallback, useMemo } from "react";
import {
  ReactFlow,
  Background,
  Controls,
  MiniMap,
  useNodesState,
  useEdgesState,
  type Node,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";

import SourceNode from "./SourceNode";
import QueryNode from "./QueryNode";
import ReactionNode from "./ReactionNode";
import AnimatedEdge from "./AnimatedEdge";
import { buildFlowGraph, type PipelineData } from "@/utils/graph";
import { THEME } from "@/utils/colors";

const nodeTypes = {
  sourceNode: SourceNode,
  queryNode: QueryNode,
  reactionNode: ReactionNode,
};

const edgeTypes = {
  animatedEdge: AnimatedEdge,
};

interface FlowCanvasProps {
  data: PipelineData;
  onNodeClick?: (nodeId: string, type: string) => void;
}

export default function FlowCanvas({ data, onNodeClick }: FlowCanvasProps) {
  const { nodes: initialNodes, edges: initialEdges } = useMemo(
    () => buildFlowGraph(data),
    [data],
  );

  const [nodes, setNodes, onNodesChange] = useNodesState(initialNodes);
  const [edges, , onEdgesChange] = useEdgesState(initialEdges);

  // Sync when pipeline data changes
  useMemo(() => {
    const { nodes: newNodes, edges: newEdges } = buildFlowGraph(data);
    setNodes((prev) =>
      newNodes.map((n) => {
        const existing = prev.find((p) => p.id === n.id);
        return existing ? { ...n, position: existing.position } : n;
      }),
    );
    onEdgesChange(
      newEdges.map((e) => ({ type: "add" as const, item: e })),
    );
  }, [data, setNodes, onEdgesChange]);

  const handleNodeClick = useCallback(
    (_: React.MouseEvent, node: Node) => {
      const [type, ...idParts] = node.id.split("-");
      onNodeClick?.(idParts.join("-"), type);
    },
    [onNodeClick],
  );

  return (
    <div className="w-full h-full" style={{ minHeight: "100%" }}>
      <svg width="0" height="0">
        <defs>
          <filter id="glow" x="-50%" y="-50%" width="200%" height="200%">
            <feGaussianBlur stdDeviation="3" result="blur" />
            <feComposite in="SourceGraphic" in2="blur" operator="over" />
          </filter>
        </defs>
      </svg>
      <ReactFlow
        nodes={nodes}
        edges={edges}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        onNodeClick={handleNodeClick}
        nodeTypes={nodeTypes}
        edgeTypes={edgeTypes}
        fitView
        minZoom={0.3}
        maxZoom={2}
        proOptions={{ hideAttribution: true }}
      >
        <Background color={THEME.border} gap={24} size={1} />
        <Controls className="!bg-drasi-card !border-drasi-border !rounded-lg [&>button]:!bg-drasi-card [&>button]:!border-drasi-border [&>button]:!text-drasi-text-secondary [&>button:hover]:!bg-drasi-surface" />
        <MiniMap
          nodeColor={(node) => {
            if (node.type === "sourceNode") return THEME.source;
            if (node.type === "queryNode") return THEME.query;
            if (node.type === "reactionNode") return THEME.reaction;
            return THEME.border;
          }}
          maskColor="rgba(10, 14, 23, 0.8)"
          className="!bg-drasi-surface !border-drasi-border !rounded-lg"
        />
      </ReactFlow>
    </div>
  );
}
