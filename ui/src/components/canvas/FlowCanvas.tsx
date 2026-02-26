import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  ReactFlow,
  Background,
  Controls,
  ControlButton,
  MiniMap,
  Panel,
  useNodesState,
  useEdgesState,
  type Node,
  type NodeChange,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { Trash2, Pin, Lock, LockOpen } from "lucide-react";

import SourceNode from "./SourceNode";
import QueryNode from "./QueryNode";
import ReactionNode from "./ReactionNode";
import AnimatedEdge from "./AnimatedEdge";
import { buildFlowGraph, type PipelineData } from "@/utils/graph";
import { THEME } from "@/utils/colors";
import { useAutoLayout } from "@/hooks/useAutoLayout";
import { useCanvasPersistence, loadPersistedState } from "@/hooks/useCanvasPersistence";

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
  onPaneClick?: () => void;
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

export default function FlowCanvas({ data, instanceId, onNodeClick, onPaneClick, onDeleteNodes }: FlowCanvasProps) {
  // Build initial nodes and pre-apply any persisted state (positions, expanded,
  // locked) so that nodes mount at their correct size and position. This avoids
  // a visible shrink/grow animation when Framer Motion transitions from the
  // default collapsed width to the persisted expanded width.
  const { nodes: initialNodes, edges: initialEdges } = useMemo(() => {
    const graph = buildFlowGraph(data);
    if (!instanceId) return graph;
    const persisted = loadPersistedState(instanceId);
    if (!persisted) return graph;
    return {
      edges: graph.edges,
      nodes: graph.nodes.map((n) => {
        const pos = persisted.positions[n.id];
        const exp = persisted.expanded[n.id];
        const lock = persisted.locked?.[n.id];
        return {
          ...n,
          position: pos ?? n.position,
          draggable: lock ? false : undefined,
          data: {
            ...n.data,
            expanded: exp ?? false,
            locked: lock ?? false,
          },
        };
      }),
    };
  }, [data, instanceId]);

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

  // Toggle lock on all selected nodes
  const toggleSelectedNodesLock = useCallback(() => {
    if (canvasLocked) return;
    const selected = nodes.filter((n) => n.selected);
    if (selected.length === 0) return;
    // If any selected node is unlocked, lock all; otherwise unlock all
    const shouldLock = selected.some((n) => !n.data?.locked);
    setNodes((prev) =>
      prev.map((n) =>
        n.selected
          ? {
              ...n,
              draggable: !shouldLock,
              data: { ...n.data, locked: shouldLock },
            }
          : n,
      ),
    );
  }, [nodes, canvasLocked, setNodes]);

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
        onPaneClick={onPaneClick}
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
        <Background color="var(--drasi-border)" gap={24} size={1} />
        <Controls showInteractive={false} className="!bg-drasi-card !border-drasi-border !rounded-lg [&>button]:!bg-drasi-card [&>button]:!border-drasi-border [&>button]:!text-drasi-text-secondary [&>button:hover]:!bg-drasi-surface [&>button]:!w-8 [&>button]:!h-8">
          <ControlButton
            onClick={toggleCanvasLock}
            title={canvasLocked ? "Unlock canvas" : "Lock canvas"}
            className={canvasLocked ? "!text-drasi-warning !bg-drasi-warning/20" : ""}
          >
            {canvasLocked ? <Lock size={18} /> : <LockOpen size={18} />}
          </ControlButton>
        </Controls>

        {/* Canvas toolbar — visible when nodes are selected */}
        {!canvasLocked && nodes.some((n) => n.selected) && (
          <Panel position="top-right" className="flex gap-1.5">
            <button
              onClick={toggleSelectedNodesLock}
              className="p-2 rounded-lg border bg-drasi-card border-drasi-border text-drasi-text-secondary hover:text-drasi-warning hover:bg-drasi-warning/10 transition-colors"
              title={
                nodes.filter((n) => n.selected).some((n) => !n.data?.locked)
                  ? "Pin selected nodes"
                  : "Unpin selected nodes"
              }
            >
              {nodes.filter((n) => n.selected).some((n) => !n.data?.locked) ? (
                <Pin size={16} className="-rotate-45" />
              ) : (
                <Pin size={16} />
              )}
            </button>
            <button
              onClick={deleteSelectedNodes}
              className="p-2 rounded-lg border bg-drasi-card border-drasi-border text-drasi-error/70 hover:text-drasi-error hover:bg-drasi-error/10 transition-colors"
              title="Delete selected nodes"
            >
              <Trash2 size={16} />
            </button>
          </Panel>
        )}

        <MiniMap
          nodeColor={(node) => {
            if (node.type === "sourceNode") return THEME.source;
            if (node.type === "queryNode") return THEME.query;
            if (node.type === "reactionNode") return THEME.reaction;
            return THEME.stopped;
          }}
          maskColor="var(--drasi-minimap-mask)"
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
