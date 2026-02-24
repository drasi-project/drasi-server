import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  ReactFlow,
  Background,
  Controls,
  MiniMap,
  Panel,
  useNodesState,
  useEdgesState,
  type Node,
  type NodeChange,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { Lock, Unlock, Trash2 } from "lucide-react";

import SourceNode from "./SourceNode";
import QueryNode from "./QueryNode";
import ReactionNode from "./ReactionNode";
import AnimatedEdge from "./AnimatedEdge";
import { buildFlowGraph, type PipelineData } from "@/utils/graph";
import { THEME } from "@/utils/colors";
import { useAutoLayout } from "@/hooks/useAutoLayout";
import { useCanvasPersistence } from "@/hooks/useCanvasPersistence";

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
  instanceId?: string;
  onNodeClick?: (nodeId: string, type: string) => void;
  onDeleteNodes?: (nodeIds: Array<{ id: string; type: string }>) => void;
}

interface CollisionApi {
  clampDragPosition: (id: string, x: number, y: number) => { x: number; y: number };
}

const CANVAS_LOCK_KEY = "drasi-canvas-locked";

function AutoLayout({
  onCollisionRef,
  instanceId,
}: {
  onCollisionRef: React.MutableRefObject<CollisionApi | null>;
  instanceId?: string;
}) {
  const api = useAutoLayout();
  onCollisionRef.current = api;
  useCanvasPersistence(instanceId);
  return null;
}

export default function FlowCanvas({ data, instanceId, onNodeClick, onDeleteNodes }: FlowCanvasProps) {
  const { nodes: initialNodes, edges: initialEdges } = useMemo(
    () => buildFlowGraph(data),
    [data],
  );

  const [nodes, setNodes, onNodesChange] = useNodesState(initialNodes);
  const [edges, , onEdgesChange] = useEdgesState(initialEdges);
  const collisionRef = useRef<CollisionApi | null>(null);
  const [canvasLocked, setCanvasLocked] = useState(() => {
    try { return localStorage.getItem(CANVAS_LOCK_KEY) === "true"; } catch { return false; }
  });
  const [pendingDelete, setPendingDelete] = useState<Node[] | null>(null);

  const toggleCanvasLock = useCallback(() => {
    setCanvasLocked((prev) => {
      const next = !prev;
      try { localStorage.setItem(CANVAS_LOCK_KEY, String(next)); } catch { /* ignore */ }
      return next;
    });
  }, []);

  const handleNodesChange = useCallback(
    (changes: NodeChange[]) => {
      const clampedChanges = changes.map((c) => {
        if (c.type === "position" && c.position && collisionRef.current) {
          const clamped = collisionRef.current.clampDragPosition(
            c.id,
            c.position.x,
            c.position.y,
          );
          return { ...c, position: clamped };
        }
        return c;
      });
      onNodesChange(clampedChanges);
    },
    [onNodesChange],
  );

  // Sync when pipeline data changes
  useEffect(() => {
    const { nodes: newNodes, edges: newEdges } = buildFlowGraph(data);
    setNodes((prev) =>
      newNodes.map((n) => {
        const existing = prev.find((p) => p.id === n.id);
        if (existing) {
          return {
            ...n,
            position: existing.position,
            draggable: !existing.data?.locked,
            data: {
              ...n.data,
              expanded: existing.data.expanded,
              locked: existing.data.locked,
              canvasLocked: canvasLocked,
            },
          };
        }
        return { ...n, data: { ...n.data, canvasLocked } };
      }),
    );
    onEdgesChange(
      newEdges.map((e) => ({ type: "add" as const, item: e })),
    );
  }, [data, setNodes, onEdgesChange, canvasLocked]);

  // Propagate canvasLocked to all nodes when it changes
  useEffect(() => {
    setNodes((prev) =>
      prev.map((n) => ({
        ...n,
        draggable: canvasLocked ? false : !n.data?.locked,
        data: { ...n.data, canvasLocked },
      })),
    );
  }, [canvasLocked, setNodes]);

  const handleNodeClick = useCallback(
    (event: React.MouseEvent, node: Node) => {
      // Don't open inspector when shift-clicking (multi-select)
      if (event.shiftKey) return;
      const [type, ...idParts] = node.id.split("-");
      onNodeClick?.(idParts.join("-"), type);
    },
    [onNodeClick],
  );

  // Delete selected nodes (respecting locks)
  const deleteSelectedNodes = useCallback(() => {
    if (canvasLocked) return;
    const selected = nodes.filter(
      (n) => n.selected && !n.data?.locked,
    );
    if (selected.length === 0) return;
    setPendingDelete(selected);
  }, [nodes, canvasLocked]);

  const confirmDelete = useCallback(() => {
    if (!pendingDelete) return;
    const deleteIds = new Set(pendingDelete.map((n) => n.id));

    if (onDeleteNodes) {
      onDeleteNodes(
        pendingDelete.map((n) => {
          const [type, ...idParts] = n.id.split("-");
          return { id: idParts.join("-"), type };
        }),
      );
    }

    setNodes((prev) => prev.filter((n) => !deleteIds.has(n.id)));
    setPendingDelete(null);
  }, [pendingDelete, onDeleteNodes, setNodes]);

  // Keyboard shortcut for delete
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Delete" || e.key === "Backspace") {
        // Don't intercept if user is typing in an input
        if (
          e.target instanceof HTMLInputElement ||
          e.target instanceof HTMLTextAreaElement
        ) return;
        deleteSelectedNodes();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [deleteSelectedNodes]);

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
        onNodesChange={handleNodesChange}
        onEdgesChange={onEdgesChange}
        onNodeClick={handleNodeClick}
        nodeTypes={nodeTypes}
        edgeTypes={edgeTypes}
        nodesDraggable={!canvasLocked}
        nodesConnectable={false}
        multiSelectionKeyCode="Shift"
        selectionKeyCode="Shift"
        deleteKeyCode={null}
        fitView
        minZoom={0.3}
        maxZoom={2}
        proOptions={{ hideAttribution: true }}
      >
        <AutoLayout onCollisionRef={collisionRef} instanceId={instanceId} />
        <Background color={THEME.border} gap={24} size={1} />
        <Controls className="!bg-drasi-card !border-drasi-border !rounded-lg [&>button]:!bg-drasi-card [&>button]:!border-drasi-border [&>button]:!text-drasi-text-secondary [&>button:hover]:!bg-drasi-surface" />

        {/* Canvas toolbar */}
        <Panel position="top-right" className="flex gap-1.5">
          <button
            onClick={toggleCanvasLock}
            className={`p-2 rounded-lg border transition-colors ${
              canvasLocked
                ? "bg-drasi-warning/20 border-drasi-warning/40 text-drasi-warning"
                : "bg-drasi-card border-drasi-border text-drasi-text-secondary hover:text-drasi-text-primary hover:bg-drasi-surface"
            }`}
            title={canvasLocked ? "Unlock canvas" : "Lock canvas"}
          >
            {canvasLocked ? <Lock size={14} /> : <Unlock size={14} />}
          </button>
          {!canvasLocked && nodes.some((n) => n.selected) && (
            <button
              onClick={deleteSelectedNodes}
              className="p-2 rounded-lg border bg-drasi-card border-drasi-border text-drasi-error/70 hover:text-drasi-error hover:bg-drasi-error/10 transition-colors"
              title="Delete selected nodes"
            >
              <Trash2 size={14} />
            </button>
          )}
        </Panel>

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

      {/* Delete confirmation dialog */}
      {pendingDelete && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
          <div className="bg-drasi-card border border-drasi-border rounded-xl p-5 max-w-sm space-y-4">
            <h3 className="text-sm font-semibold text-drasi-text-primary">
              Delete {pendingDelete.length} component{pendingDelete.length > 1 ? "s" : ""}?
            </h3>
            <p className="text-xs text-drasi-text-secondary">
              This will remove the selected component{pendingDelete.length > 1 ? "s" : ""} from the server. This action cannot be undone.
            </p>
            <div className="flex gap-2 justify-end">
              <button
                onClick={() => setPendingDelete(null)}
                className="action-btn-ghost text-xs"
              >
                Cancel
              </button>
              <button
                onClick={confirmDelete}
                className="action-btn-danger text-xs"
              >
                Delete
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
